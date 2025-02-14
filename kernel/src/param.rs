use crate::fs::DeviceNo;

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

/// Device number of file system root disk.
pub const ROOT_DEV: DeviceNo = DeviceNo::new(1).unwrap();

/// Max exec arguments
pub const MAX_ARG: usize = 32;

/// Max # of blocks any FS op writes.
pub const MAX_OP_BLOCKS: usize = 10;

/// Max data blocks in on-disk log
pub const LOG_SIZE: usize = MAX_OP_BLOCKS * 3;

/// Size of disk block cache.
pub const NBUF: usize = MAX_OP_BLOCKS * 3;

/// User stack pages
pub const USER_STACK: usize = 1;
