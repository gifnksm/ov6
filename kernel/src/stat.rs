#[repr(C)]
pub struct Stat {
    /// File system's disk device
    dev: i32,
    /// Inode number
    ino: u32,
    /// Type of file
    ty: i16,
    /// Number of links to file
    nlink: i16,
    /// Size of file in bytes
    size: u64,
}

impl Stat {
    pub const fn zero() -> Self {
        Self {
            dev: 0,
            ino: 0,
            ty: 0,
            nlink: 0,
            size: 0,
        }
    }
}
