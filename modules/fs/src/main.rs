use block_adapter::BlockAdapter;
use process::ipc::sm;
use process::ipc::*;
use process::ipc_server::{IPCServer, ServerImpl};
use process::os_error::{Module, OSError, OSResult, Reason};
use process::syscalls;
use process::{define_server, define_session};
use process::{Handle, INVALID_HANDLE};
use std::sync::Arc;
use std::sync::Mutex;

mod virtio_pci;

mod block;
mod block_adapter;
mod block_virtio;

use std::io::Read;

include!(concat!(env!("OUT_DIR"), "/fs_server_impl.rs"));

type FatFilesystem = fatfs::FileSystem<
    fatfs::StdIoWrapper<BlockAdapter>,
    fatfs::DefaultTimeProvider,
    fatfs::LossyOemCpConverter,
>;

type FatFile<'a> = fatfs::File<'a, fatfs::StdIoWrapper<BlockAdapter>,
    fatfs::DefaultTimeProvider,
    fatfs::LossyOemCpConverter,
>;

define_server! {
    FSServerStruct {
        // todo: hold multiple filesystems and implement some VFS stuff
        fs: Box<FatFilesystem>,
    }
}

define_session! {
    FSSession {},
    FSServerStruct
}

struct IFileSession<'a> {
    __server: Arc<FSServerStruct>,
    file: FatFile<'a>
}

define_session! {
    IDirectorySession {},
    FSServerStruct
}

fn map_fatfs_error(e: fatfs::Error<std::io::Error>) -> OSError {
    match e {
        fatfs::Error::NotFound => OSError::new(Module::Fs, Reason::NotFound),
        _ => OSError::new(Module::Fs, Reason::Unknown),
    }
}

impl FSServerStruct {
    fn accept_main_session(self: &Arc<FSServerStruct>) -> Arc<FSSession> {
        Arc::new(FSSession {
            __server: self.clone(),
        })
    }
}

impl FSSession {
    fn open_file(&self, file_name: String) -> OSResult<TranslateMoveHandle> {
        println!("Hi from open_file!");

        let server = self.get_server();
        let mut file = server.fs
            .root_dir()
            .open_file(&file_name)
            .map_err(map_fatfs_error)?;

        let server_session: Handle = INVALID_HANDLE;
        let client_session: Handle = INVALID_HANDLE;
        let (server_session, client_session) = syscalls::create_session().unwrap();

        server.get_server_impl().register_session(
            server_session,
            Arc::new(IFileSession {
                __server: server.clone(),
                file: file
            }),
        );

        println!("got file handle {:?}", client_session);
        Ok(TranslateMoveHandle(client_session))
    }
}

impl<'a> IFileSession<'a> {
    fn read_file(&self, length: usize) -> OSResult<usize> {
        Ok(0)
    }
}

#[tokio::main]
async fn main() {
    println!("Hello from fs!");

    let port = syscalls::create_port("").unwrap();

    sm::register_port(syscalls::make_tag("fs"), TranslateCopyHandle(port)).unwrap();

    let mut blocks = block_virtio::scan();
    let first_block = blocks.pop().unwrap();

    let adapted = Box::new(BlockAdapter::new(first_block.clone(), 0));

    let cfg = gpt::GptConfig::new().writable(false);
    let gpt_disk = cfg.open_from_device(adapted).unwrap();

    println!("Got partition table: {:?}", gpt_disk.partitions());

    let first_partition = gpt_disk.partitions().get(&1).unwrap();
    let partition_start = first_partition.first_lba;

    let adapted_partition = BlockAdapter::new(first_block, partition_start);

    let fs = fatfs::FileSystem::new(
        fatfs::StdIoWrapper::new(adapted_partition),
        fatfs::FsOptions::new(),
    )
    .unwrap();
    let first_fs = fs;

    let server = Arc::new(FSServerStruct {
        __server_impl: Mutex::new(ServerImpl::new(port)),
        fs: Box::new(first_fs),
    });

    println!("fs: processing");
    server.process_forever().await;

    syscalls::close_handle(port).unwrap();
    println!("FS exiting!");

    syscalls::exit_process();
}
