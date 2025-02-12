/// Maximum number of processes.
pub const NPROC: usize = 64;

/// Maximum number of CPUs.
pub const NCPU: usize = 8;

/// Open files per process.
pub const NOFILE: usize = 16;

/// Device number of file system root disk.
pub const ROOTDEV: usize = 1;

/// Max # of blocks any FS op writes.
pub const MAX_OP_BLOCKS: usize = 10;

/// Size of disk block cache.
pub const NBUF: usize = MAX_OP_BLOCKS * 3;
