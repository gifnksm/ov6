//! File system implementation.
//!
//! Five layers:
//!   + Blocks: allocator for raw disk blocks.
//!   + Log: crash recovery for multi-step updates.
//!   + Files: inode allocator, reading, writing, metadata.
//!   + Directories: inode with special contents (list of other inodes!)
//!   + Names: paths like `/usr/rtm/xv6/fs.c` for convenient naming.
//!
//! This file contains the low-level file system manipulation
//! routines. The (higher-level) system call implementations
//! are in `syscall/file.rs`

use dataview::Pod;
use once_init::OnceInit;
use ov6_fs_types::{self as repr, SuperBlock};
pub use repr::{BlockNo, FS_BLOCK_SIZE, InodeNo, T_DEVICE, T_DIR, T_FILE};

pub use self::{
    inode::{Inode, LockedTxInode},
    log::{Tx, begin_readonly_tx, begin_tx},
};

mod block_io;
mod data_block;
mod inode;
mod log;
pub mod ops;
pub mod path;
mod virtio;
pub mod virtio_disk;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Pod)]
#[repr(transparent)]
pub struct DeviceNo(u32);

impl DeviceNo {
    pub const CONSOLE: Self = Self(1);
    /// Device number of file system root disk.
    pub const ROOT: Self = Self(0);

    pub const fn new(n: u32) -> Self {
        Self(n)
    }

    pub const fn value(self) -> u32 {
        self.0
    }

    pub fn as_index(self) -> usize {
        usize::try_from(self.0).unwrap()
    }
}

pub fn init() {
    inode::init();
    block_io::init();
    virtio_disk::init();
}

// there should be one superblock per disk device, but we run with
// only one device
static SUPER_BLOCK: OnceInit<SuperBlock> = OnceInit::new();

/// Reads the super block.
fn init_superblock(tx: &Tx<true>, dev: DeviceNo) {
    let mut br = tx.get_block(dev, SuperBlock::SUPER_BLOCK_NO);
    let Ok(bg) = br.lock().read();
    SUPER_BLOCK.init_by_ref(bg.data::<SuperBlock>());
}

pub fn init_in_proc(dev: DeviceNo) {
    let tx = log::begin_readonly_tx();
    init_superblock(&tx, dev);

    let sb = SUPER_BLOCK.get();
    assert_eq!(sb.magic, SuperBlock::FS_MAGIC);
    log::init(dev, sb);
}
