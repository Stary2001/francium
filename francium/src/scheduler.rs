use alloc::boxed::Box;
use alloc::sync::Arc;
use smallvec::SmallVec;
use spin::{Mutex, MutexGuard};
use core::ptr::NonNull;

use crate::process::{Thread, Process, ThreadState};
use crate::aarch64::context::ThreadContext;

pub struct Scheduler {
	pub threads: SmallVec<[Arc<Box<Thread>>; 4]>,
	pub runnable_threads: SmallVec<[Arc<Box<Thread>>; 4]>,
	pub current_thread_index: usize
}

lazy_static! {
	static ref SCHEDULER: Mutex<Scheduler> = Mutex::new(Scheduler::new());
}

// rust says these are ffi unsafe
// they're right but shut
extern "C" {
	fn switch_thread_asm(from_context: *mut ThreadContext, to_context: *const ThreadContext, from: *const Mutex<ThreadContext>, to: *const Mutex<ThreadContext>);
}

#[no_mangle]
pub extern "C" fn force_unlock_mutex(mutex: NonNull<Mutex<ThreadContext>>) {
	unsafe {
		mutex.as_ref().force_unlock();
	}
}

impl Scheduler {
	fn new() -> Scheduler {
		Scheduler {
			threads: SmallVec::new(),
			runnable_threads: SmallVec::new(),
			current_thread_index: 0,
		}
	}

	fn get_current_thread(&self) -> Arc<Box<Thread>> {
		self.runnable_threads[self.current_thread_index].clone()
	}

	fn switch_thread(&mut self, from: &Arc<Box<Thread>>, to: &Arc<Box<Thread>>) {
		// TODO: wow, this sucks
		{
			unsafe {
				// TODO: lol
				SCHEDULER.force_unlock();
			}

			to.process.lock().use_pages();

			let from_context_locked = MutexGuard::leak(from.context.lock());
			let to_context_locked = MutexGuard::leak(to.context.lock());

			unsafe {
				switch_thread_asm(from_context_locked, to_context_locked, &from.context, &to.context);
			}
		}
	}

	pub fn get_next_thread(&mut self) -> Arc<Box<Thread>> {
		if self.current_thread_index == self.runnable_threads.len() - 1 {
			self.current_thread_index = 0;
		} else {
			self.current_thread_index += 1;
		}
		self.runnable_threads[self.current_thread_index].clone()
	}

	pub fn tick(&mut self) {
		// todo: process things
		if self.runnable_threads.len() == 0 {
			return
		}

		// do the thing
		let this_thread = self.get_current_thread();
		let next = self.get_next_thread();
		self.switch_thread(&this_thread, &next);
	}

	pub fn suspend(&mut self, p: &Arc<Box<Thread>>) {
		//p.state = ThreadState::Suspended;
		if let Some(runnable_index) = self.runnable_threads.iter().position(|x| x.id == p.id) {
			if runnable_index < self.current_thread_index {
				self.current_thread_index -= 1;
			}
			self.runnable_threads.remove(runnable_index);

			if runnable_index == self.current_thread_index {
				if self.runnable_threads.len() == 0 {
					panic!("Trying to suspend everything.");
				} else {
					let next = self.get_next_thread();
					self.switch_thread(p, &next);
				}
			}
		}
	}

	pub fn wake(&mut self, p: Arc<Box<Thread>>) {
		if let Some(_runnable_index) = self.runnable_threads.iter().position(|x| x.id == p.id) {
			// wtf
			panic!("Trying to re-wake a thread!");
		} else {
			self.runnable_threads.push(p);
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

pub fn register_thread(p: Arc<Box<Thread>>) {
	let mut sched = SCHEDULER.lock();
	sched.threads.push(p.clone());
	sched.runnable_threads.push(p.clone());
}

pub fn get_current_thread() -> Arc<Box<Thread>> {
	let sched = SCHEDULER.lock();
	sched.get_current_thread()
}

pub fn get_current_process() -> Arc<Mutex<Box<Process>>> {
	get_current_thread().process.clone()
}

pub fn suspend_process(p: Arc<Box<Thread>>) {
	let mut sched = SCHEDULER.lock();
	sched.suspend(&p);
}

pub fn suspend_current_thread() {
	let mut sched = SCHEDULER.lock();
	let curr = sched.get_current_thread();

	sched.suspend(&curr);
}

pub fn wake_thread(p: Arc<Box<Thread>>) {
	let mut sched = SCHEDULER.lock();
	sched.wake(p);
}

pub fn terminate_current_thread() {
	let mut sched = SCHEDULER.lock();
	sched.terminate_current_thread();
}

pub fn terminate_current_process() {
	unimplemented!();
}
