use crate::handle::Handle;
use common::os_error::{RESULT_OK, ResultCode, Module, Reason};

// for now i will just fix the handle table size.
// todo: dynamic
const MAX_HANDLES: usize = 256;
pub struct HandleTable {
	handles: [Handle; MAX_HANDLES],
}

impl core::fmt::Debug for HandleTable {
	fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
		f.debug_struct("HandleTable").finish()
	}
}

impl HandleTable {
	pub fn new() -> HandleTable {
		const INVALID_HANDLE: Handle = Handle::Invalid;
		HandleTable {
			handles: [INVALID_HANDLE; MAX_HANDLES]
		}
	}

	pub fn get_object(&self, handle: u32) -> Handle {
		if (handle as usize) < MAX_HANDLES {
			self.handles[handle as usize].clone()
		} else {
			Handle::Invalid
		}
	}

	pub fn get_handle(&mut self, handle_obj: Handle) -> u32 {
		for (index, obj) in self.handles.iter().enumerate() {
			match obj {
				Handle::Invalid => {
					self.handles[index] = handle_obj;
					return index as u32;
				},
				_ => continue
			}
		}
		panic!("handle table is exhausted!");
	}

	pub fn close(&mut self, handle: u32) -> ResultCode {
		if (handle as usize) < MAX_HANDLES {
			match self.handles[handle as usize] {
				Handle::Invalid => ResultCode::new(Module::Kernel, Reason::InvalidHandle),
				_ => {
					self.handles[handle as usize] = Handle::Invalid;
					RESULT_OK
				}
			}
		} else {
			ResultCode::new(Module::Kernel, Reason::InvalidHandle)
		}
	}
}