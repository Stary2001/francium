use crate::scheduler;
use crate::handle;
use crate::handle::Handle;
use crate::process::Thread;
use crate::waitable;
use crate::waitable::{Waiter, Waitable};
use spin::Mutex;
use core::cell::Cell;
use alloc::collections::BTreeMap;
use alloc::vec::Vec;
use alloc::sync::{Arc, Weak};

use smallvec::SmallVec;

#[derive(Debug)]
pub struct ServerSession {
	wait: Waiter,
	connect_wait: Waiter,
	queue: Mutex<SmallVec<[Arc<Thread>; 1]>>,
	client: Mutex<Weak<ClientSession>>,
	client_thread: Mutex<Option<Arc<Thread>>>
}

#[derive(Debug)]
pub struct ClientSession {
	wait: Waiter,
	server: Arc<ServerSession>
}

#[derive(Debug)]
pub struct Port {
	wait: Waiter,
	// todo: queue default length
	queue: Mutex<SmallVec<[Arc<ServerSession>; 1]>>
}

impl Port {
	fn new() -> Port {
		Port {
			wait: Waiter::new(),
			queue: Mutex::new(SmallVec::new())
		}
	}
}

impl Waitable for Port { fn get_waiter(&self) -> &Waiter { &self.wait } }

impl ServerSession {
	fn new() -> ServerSession {
		ServerSession {
			wait: Waiter::new(),
			connect_wait: Waiter::new(),
			queue: Mutex::new(SmallVec::new()),
			client: Mutex::new(Weak::new()),
			client_thread: Mutex::new(None)
		}
	}
}
impl Waitable for ServerSession { fn get_waiter(&self) -> &Waiter { &self.wait } }

impl ClientSession {
	fn new(server: Arc<ServerSession>) -> ClientSession {
		ClientSession {
			wait: Waiter::new(),
			server: server
		}
	}
}
impl Waitable for ClientSession { fn get_waiter(&self) -> &Waiter { &self.wait } }

lazy_static! {
	static ref PORT_LIST: Mutex<BTreeMap<u64, Arc<Port>>> = Mutex::new(BTreeMap::new());
	static ref PORT_WAITERS: Mutex<Vec<(u64, Arc<Thread>)>> = Mutex::new(Vec::new());
}

pub fn svc_create_port(tag: u64) -> (u32, u32) {
	let server_port = Port::new();
	let server_port_handle = Arc::new(server_port);

	// if not a private port
	if tag != 0 {
		let mut ports = PORT_LIST.lock();
		if ports.contains_key(&tag) {
			panic!("panik");
		}

		let mut port_waiters = PORT_WAITERS.lock();
		port_waiters.retain( |x| {
			if x.0 == tag {
				scheduler::wake_thread(x.1.clone(), 0);
				false
			} else {
				true
			}
		});

		ports.insert(tag, server_port_handle.clone());
	}

	let proc_locked = scheduler::get_current_process();
	let mut process = proc_locked.lock();

	let handle_value = process.handle_table.get_handle(Handle::Port(server_port_handle));
	(0, handle_value)
}

pub fn svc_connect_to_port(tag: u64) -> (u32, u32) {
	let port = {
		let ports = PORT_LIST.lock();
		if let Some(server_port) = ports.get(&tag) {
			server_port.clone()
		} else {
			// make sure to drop the lock guard before suspending ourselves!
			drop(ports);

			PORT_WAITERS.lock().push((tag, scheduler::get_current_thread()));
			scheduler::suspend_current_thread();

			// oops, try again
			{
				let ports = PORT_LIST.lock();
				ports.get(&tag).unwrap().clone()
			}
		}
	};

	let server_session = Arc::new(ServerSession::new());
	let client_session = Arc::new(ClientSession::new(server_session.clone()));

	// TODO: ugh, i really wanted OnceCell here
	*server_session.client.lock() = Arc::downgrade(&client_session);

	// create the session, and wait for it to be accepted by the server
	port.queue.lock().push(server_session.clone());
	port.signal_one();
	server_session.connect_wait.wait();

	// return session
	{
		let current_process = scheduler::get_current_process();
		let mut process = current_process.lock();
		let handle_value = process.handle_table.get_handle(Handle::ClientSession(client_session));
		(0, handle_value)
	}
}

// x0: ipc session
pub fn svc_ipc_request(session_handle: u32) -> u32 {
	if let Handle::ClientSession(client_session) = handle::get_handle(session_handle) {
		// signal, then wait for reply
		let current_thread = scheduler::get_current_thread();

		client_session.server.queue.lock().push(current_thread);
		client_session.server.signal_one();
		client_session.wait();

		// [this thread will have its TLS filled by the server]

		0
	} else {
		// error
		1
	}
}

const MAX_HANDLES: usize = 128;
pub fn svc_ipc_receive(handles_ptr: *const u32, handle_count: usize) -> (u32, usize) {
	let mut handles: [u32; MAX_HANDLES] = [ 0xffffffff ; MAX_HANDLES];

	unsafe {
		core::ptr::copy_nonoverlapping(handles_ptr, &mut handles as *mut u32, handle_count);
	}

	let index = waitable::wait_handles(&handles[..handle_count]);

	if let Handle::ServerSession(server_session) = handle::get_handle(handles[index]) {
		let client_thread = server_session.queue.lock().pop().unwrap();
		let current_thread = scheduler::get_current_thread();
		/* Process IPC message here! */
		println!("Copy A from {} to {}", client_thread.id, current_thread.id);

		*server_session.client_thread.lock() = Some(client_thread);
	}

	(0, index)
}

// x0: session handle
pub fn svc_ipc_reply(session_handle: u32) -> u32 {
	if let Handle::ServerSession(server_session) = handle::get_handle(session_handle) {
		// TODO: wtf?
		let current_thread = scheduler::get_current_thread();
		let thread_lock = server_session.client_thread.lock();
		let client_thread = thread_lock.as_ref().unwrap();
		println!("Copy B from {} to {}", current_thread.id, client_thread.id);
		*server_session.client_thread.lock() = None;
		server_session.client.lock().upgrade().unwrap().signal_one();
		0
	} else {
		1
	}
}

// x0: port
// x1: session handle out
pub fn svc_ipc_accept(port_handle: u32) -> (u32, u32) {
	if let Handle::Port(port) = handle::get_handle(port_handle) {
		let server_session = port.queue.lock().pop().unwrap();

		// wake the client
		server_session.connect_wait.signal_one();

		let current_process = scheduler::get_current_process();
		let mut process = current_process.lock();
		let handle_value = process.handle_table.get_handle(Handle::ServerSession(server_session));
		(0, handle_value)
	} else {
		(1, 0xffffffff)
	}
}