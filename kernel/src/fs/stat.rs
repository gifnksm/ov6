use crate::fs::{DeviceNo, InodeNo};

#[repr(C)]
pub struct Stat {
    /// File system's disk device
    pub dev: DeviceNo,
    /// Inode number
    pub ino: InodeNo,
    /// Type of file
    pub ty: i16,
    /// Number of links to file
    pub nlink: i16,
    /// Size of file in bytes
    pub size: u64,
}
