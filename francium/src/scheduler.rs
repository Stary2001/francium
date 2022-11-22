use alloc::boxed::Box;
use alloc::sync::Arc;
use smallvec::SmallVec;
use spin::{Mutex, MutexGuard};
use core::ptr::NonNull;
use core::sync::atomic::Ordering;

use crate::process::{Thread, ThreadState, Process};
use crate::arch::context::ThreadContext;

pub struct Scheduler {
	pub threads: SmallVec<[Arc<Thread>; 4]>,
	pub runnable_threads: SmallVec<[Arc<Thread>; 4]>,
	pub current_thread_index: usize
}

lazy_static! {
	static ref SCHEDULER: Mutex<Scheduler> = Mutex::new(Scheduler::new());
}

extern "C" {
	fn switch_thread_asm(from_context: *mut ThreadContext, to_context: *const ThreadContext, from: usize, to: usize) -> usize;
}

#[cfg(target_arch = "x86_64")]
extern "C" {
	#[link_name = "current_thread_kernel_stack"]
	static mut CURRENT_THREAD_KERNEL_STACK: usize;
}

#[no_mangle]
pub extern "C" fn force_unlock_mutex(mutex: NonNull<Mutex<ThreadContext>>) {
	unsafe {
		mutex.as_ref().force_unlock();
	}
}

#[cfg(target_arch = "aarch64")]
fn set_thread_context_tag(p: &Arc<Thread>, tag: usize) {
	p.context.lock().regs[0] = tag;
}

#[cfg(target_arch = "x86_64")]
fn set_thread_context_tag(p: &Arc<Thread>, tag: usize) {
	p.context.lock().regs.rax = tag;
}

#[cfg(target_arch = "x86_64")]
use crate::arch;

#[cfg(target_arch = "x86_64")]
pub unsafe fn set_current_thread_state(kernel_stack: usize, tls: usize) {
	CURRENT_THREAD_KERNEL_STACK = kernel_stack;
	arch::msr::write_fs_base(tls);
}

impl Scheduler {
	fn new() -> Scheduler {
		Scheduler {
			threads: SmallVec::new(),
			runnable_threads: SmallVec::new(),
			current_thread_index: 0
		}
	}

	fn get_current_thread(&self) -> Arc<Thread> {
		self.runnable_threads[self.current_thread_index].clone()
	}

	fn switch_thread(&mut self, from: &Arc<Thread>, to: &Arc<Thread>) -> usize {
		//println!("Switch from {} to {}", from.process.lock().name, to.process.lock().name);
		if from.id == to.id {
			// don't do this, it'll deadlock
			//panic!("Trying to switch to the same thread!");
			return 0
		}

		// TODO: wow, this sucks
		{
			unsafe {
				// TODO: lol
				SCHEDULER.force_unlock();
			}

			{
				to.process.lock().use_pages();
			}

			let from_context_locked = MutexGuard::leak(from.context.lock());
			let to_context_locked = MutexGuard::leak(to.context.lock());

			let from_context_ptr = &from.context as *const Mutex<ThreadContext>;
			let to_context_ptr = &to.context as *const Mutex<ThreadContext>;

			unsafe {
				#[cfg(target_arch = "x86_64")]
				set_current_thread_state(to.kernel_stack_top, to_context_locked.regs.fs);

				return switch_thread_asm(from_context_locked, to_context_locked, from_context_ptr as usize, to_context_ptr as usize)
			}
		}
	}

	pub fn get_next_thread(&mut self) -> Arc<Thread> {
		if self.current_thread_index == self.runnable_threads.len() - 1 {
			self.current_thread_index = 0;
		} else {
			self.current_thread_index += 1;
		}
		self.runnable_threads[self.current_thread_index].clone()
	}

	pub fn tick(&mut self) {
		//println!("Tick!");
		if self.runnable_threads.len() == 0 {
			return
		}

		// do the thing
		let this_thread = self.get_current_thread();
		let next = self.get_next_thread();
		self.switch_thread(&this_thread, &next);
	}

	pub fn suspend(&mut self, p: &Arc<Thread>) -> usize {
		if let Some(runnable_index) = self.runnable_threads.iter().position(|x| x.id == p.id) {
			let idx = self.current_thread_index;
			self.runnable_threads[runnable_index].state.store(ThreadState::Suspended, Ordering::Release);

			self.runnable_threads.remove(runnable_index);
			
			if self.runnable_threads.len() == 0 {
				panic!("Trying to suspend everything!");
			}

			// adjust for threads that shifted
			if self.current_thread_index > runnable_index {
				self.current_thread_index -= 1;
			} else if self.current_thread_index > self.runnable_threads.len() - 1 {
				self.current_thread_index = 0;
			}

			assert!(self.current_thread_index < self.runnable_threads.len());

			// If we got switched out, switch to the new current process.
			if runnable_index == idx {
				let next = self.get_current_thread();
				return self.switch_thread(p, &next)
			}
		}

		0
	}

	pub fn wake(&mut self, p: Arc<Thread>, tag: usize) {
		if let Some(_runnable_index) = self.runnable_threads.iter().position(|x| x.id == p.id) {
			// wtf
			panic!("Trying to re-wake a thread!");
		} else {
			// set x0 of the thread context
			set_thread_context_tag(&p, tag);
			self.runnable_threads.insert(self.current_thread_index+1, p);
		}
	}

	pub fn terminate_current_thread(&mut self) {
		let this_thread = self.get_current_thread();

		let thread_index = self.threads.iter().position(|x| x.id == this_thread.id).unwrap();
		self.threads.remove(thread_index);

		self.suspend(&this_thread);
	}
}

pub fn tick() {
	let mut sched = SCHEDULER.lock();
	sched.tick();
}

pub fn register_thread(p: Arc<Thread>) {
	let mut sched = SCHEDULER.lock();
	p.state.store(ThreadState::Runnable, Ordering::Release);
	sched.threads.push(p.clone());
	sched.runnable_threads.push(p.clone());
}

pub fn get_current_thread() -> Arc<Thread> {
	let sched = SCHEDULER.lock();
	sched.get_current_thread()
}

pub fn get_current_process() -> Arc<Mutex<Box<Process>>> {
	get_current_thread().process.clone()
}

pub fn suspend_process(p: Arc<Thread>) {
	let mut sched = SCHEDULER.lock();
	sched.suspend(&p);
}

pub fn suspend_current_thread() -> usize {
	let mut sched = SCHEDULER.lock();
	let curr = sched.get_current_thread();

	return sched.suspend(&curr)
}

pub fn wake_thread(p: Arc<Thread>, tag: usize) {
	let mut sched = SCHEDULER.lock();
	sched.wake(p, tag);
}

pub fn terminate_current_thread() {
	let mut sched = SCHEDULER.lock();
	sched.terminate_current_thread();
}

pub fn terminate_current_process() {
	let mut sched = SCHEDULER.lock();
	let current_thread = sched.get_current_thread();
	let current_process = current_thread.process.clone();
	let process = current_process.lock();

	for thread in &process.threads {
		if thread.id != current_thread.id {
			sched.suspend(&thread);
		}
	}

	sched.terminate_current_thread();
}

// see also: force_unlock_mutex
extern "C" {
	fn setup_initial_thread_context(ctx: &ThreadContext, mutex: usize);
}

pub fn force_switch_to(thread: Arc<Thread>) {
	{
		let mut sched = SCHEDULER.lock();

		sched.current_thread_index = sched.runnable_threads.iter().position(|x| x.id == thread.id).unwrap();
	}

	thread.process.lock().use_pages();

	let thread_context = MutexGuard::leak(thread.context.lock());
	unsafe {
		#[cfg(target_arch = "x86_64")]
		set_current_thread_state(thread.kernel_stack_top, 0);
		setup_initial_thread_context(thread_context, &thread.context as *const Mutex<ThreadContext> as usize);
	}
}
