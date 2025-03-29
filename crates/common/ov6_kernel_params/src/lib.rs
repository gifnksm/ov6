#![no_std]

/// Maximum number of processes.
pub const NPROC: usize = 64;

/// Maximum number of CPUs.
pub const NCPU: usize = 8;

/// Maximum major device number
pub const NDEV: usize = 10;

/// Open files per process.
pub const NOFILE: usize = 16;

/// Open files per system.
pub const NFILE: usize = 100;

/// Maximum number of active i-nodes
pub const NINODE: usize = 50;

/// Max # of blocks any FS op writes.
pub const MAX_OP_BLOCKS: usize = 10;

/// Max data blocks in on-disk log.
pub const LOG_SIZE: usize = MAX_OP_BLOCKS * 3;

/// Size of disk block cache.
pub const NBUF: usize = MAX_OP_BLOCKS * 3;

/// Maximum file path name.
pub const MAX_PATH: usize = 128;

/// User stack pages
pub const USER_STACK_PAGES: usize = 2;

/// Size of file system image in blocks
pub const FS_SIZE: usize = 2000;

/// Maximum number of i-nodes on file system.
pub const NUM_FS_INODES: usize = 200;
/// Maximum number of logs on file system.
pub const FS_LOG_SIZE: usize = MAX_OP_BLOCKS * 3;
