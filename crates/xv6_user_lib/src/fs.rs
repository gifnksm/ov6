use core::ffi::CStr;

use dataview::PodMethods as _;

use crate::{
    error::Error,
    io::{Read, Write},
    os, syscall,
};

pub use syscall::{OpenFlags, StatType};

pub struct Metadata {
    pub(crate) dev: u32,
    pub(crate) ino: u32,
    pub(crate) ty: StatType,
    pub(crate) nlink: u16,
    pub(crate) size: u64,
}

impl Metadata {
    pub fn ty(&self) -> StatType {
        self.ty
    }

    pub fn is_file(&self) -> bool {
        self.ty == StatType::File
    }

    pub fn is_device(&self) -> bool {
        self.ty == StatType::Dev
    }

    pub fn is_dir(&self) -> bool {
        self.ty == StatType::Dir
    }

    pub fn dev(&self) -> u32 {
        self.dev
    }

    pub fn ino(&self) -> u32 {
        self.ino
    }

    pub fn nlink(&self) -> u16 {
        self.nlink
    }

    pub fn size(&self) -> u64 {
        self.size
    }
}

pub struct File {
    fd: i32,
}

impl File {
    pub fn open(path: &CStr, flags: OpenFlags) -> Result<Self, Error> {
        let fd = os::fd_open(path, flags)?;
        Ok(Self { fd })
    }

    pub fn try_clone(&self) -> Result<Self, Error> {
        let fd = os::fd_dup(self.fd)?;
        Ok(File { fd })
    }

    pub fn stat(&self) -> Result<Metadata, Error> {
        os::fd_stat(self.fd)
    }
}

impl Drop for File {
    fn drop(&mut self) {
        let _ = os::fd_close(self.fd); // ignore error here
    }
}

impl Write for File {
    fn write(&mut self, buf: &[u8]) -> Result<usize, Error> {
        os::fd_write(self.fd, buf)
    }
}

impl Write for &'_ File {
    fn write(&mut self, buf: &[u8]) -> Result<usize, Error> {
        os::fd_write(self.fd, buf)
    }
}

impl Read for File {
    fn read(&mut self, buf: &mut [u8]) -> Result<usize, Error> {
        os::fd_read(self.fd, buf)
    }
}

impl Read for &'_ File {
    fn read(&mut self, buf: &mut [u8]) -> Result<usize, Error> {
        os::fd_read(self.fd, buf)
    }
}

pub fn mknod(path: &CStr, major: i16, minor: i16) -> Result<(), Error> {
    if unsafe { syscall::mknod(path.as_ptr(), major, minor) } < 0 {
        return Err(Error::Unknown);
    }
    Ok(())
}

pub fn link(old: &CStr, new: &CStr) -> Result<(), Error> {
    if unsafe { syscall::link(old.as_ptr(), new.as_ptr()) } < 0 {
        return Err(Error::Unknown);
    }
    Ok(())
}

pub fn metadata(path: &CStr) -> Result<Metadata, Error> {
    let file = File::open(path, OpenFlags::READ_ONLY)?;
    file.stat()
}

pub fn create_dir(path: &CStr) -> Result<(), Error> {
    let res = unsafe { syscall::mkdir(path.as_ptr()) };
    if res < 0 {
        return Err(Error::Unknown);
    }
    Ok(())
}

pub fn read_dir(path: &CStr) -> Result<ReadDir, Error> {
    let fd = os::fd_open(path, OpenFlags::READ_ONLY)?;
    let st = os::fd_stat(fd)?;
    if !st.is_dir() {
        return Err(Error::NotADirectory);
    }
    Ok(ReadDir { fd })
}

pub struct ReadDir {
    fd: i32,
}

impl Drop for ReadDir {
    fn drop(&mut self) {
        let _ = os::fd_close(self.fd); // ignore error here
    }
}

impl Iterator for ReadDir {
    type Item = Result<DirEntry, Error>;

    fn next(&mut self) -> Option<Self::Item> {
        loop {
            let mut ent = xv6_fs_types::DirEntry::zeroed();
            let Ok(size) = os::fd_read(self.fd, ent.as_bytes_mut()) else {
                return Some(Err(Error::Unknown));
            };
            if size == 0 {
                return None;
            }
            if ent.inum().is_none() {
                continue;
            }
            assert_eq!(size, size_of::<xv6_fs_types::DirEntry>());
            return Some(Ok(DirEntry { ent }));
        }
    }
}

pub struct DirEntry {
    ent: xv6_fs_types::DirEntry,
}

impl DirEntry {
    pub fn name(&self) -> &str {
        str::from_utf8(self.ent.name()).unwrap()
    }
}
