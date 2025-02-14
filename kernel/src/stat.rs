use crate::fs::{DeviceNo, InodeNo};

/// Directory
pub const T_DIR: i16 = 1;
/// File
pub const T_FILE: i16 = 2;
/// Device
pub const T_DEVICE: i16 = 3;

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
