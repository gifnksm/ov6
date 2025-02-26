use core::ffi::CStr;

use dataview::PodMethods as _;

use crate::{
    error::Error,
    io::{Read, Write},
    os::{
        fd::{AsFd as _, OwnedFd},
        xv6::syscall,
    },
};

pub use syscall::{OpenFlags, StatType};

pub struct Metadata {
    dev: u32,
    ino: u32,
    ty: StatType,
    nlink: u16,
    size: u64,
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
    fd: OwnedFd,
}

impl File {
    pub fn open(path: &CStr, flags: OpenFlags) -> Result<Self, Error> {
        let fd = syscall::open(path, flags)?;
        Ok(Self { fd })
    }

    pub fn try_clone(&self) -> Result<Self, Error> {
        let fd = syscall::dup(self.fd.as_fd())?;
        Ok(File { fd })
    }

    pub fn metadata(&self) -> Result<Metadata, Error> {
        let stat = syscall::fstat(self.fd.as_fd())?;
        Ok(Metadata {
            dev: stat.dev.cast_unsigned(),
            ino: stat.ino,
            ty: stat.ty,
            nlink: stat.nlink.cast_unsigned(),
            size: stat.size,
        })
    }
}

impl Write for File {
    fn write(&mut self, buf: &[u8]) -> Result<usize, Error> {
        syscall::write(self.fd.as_fd(), buf)
    }
}

impl Write for &'_ File {
    fn write(&mut self, buf: &[u8]) -> Result<usize, Error> {
        syscall::write(self.fd.as_fd(), buf)
    }
}

impl Read for File {
    fn read(&mut self, buf: &mut [u8]) -> Result<usize, Error> {
        syscall::read(self.fd.as_fd(), buf)
    }
}

impl Read for &'_ File {
    fn read(&mut self, buf: &mut [u8]) -> Result<usize, Error> {
        syscall::read(self.fd.as_fd(), buf)
    }
}

pub fn mknod(path: &CStr, major: i16, minor: i16) -> Result<(), Error> {
    syscall::mknod(path, major, minor)
}

pub fn link(old: &CStr, new: &CStr) -> Result<(), Error> {
    syscall::link(old, new)
}

pub fn metadata(path: &CStr) -> Result<Metadata, Error> {
    let fd = syscall::open(path, OpenFlags::READ_ONLY)?;
    let stat = syscall::fstat(fd.as_fd())?;
    Ok(Metadata {
        dev: stat.dev.cast_unsigned(),
        ino: stat.ino,
        ty: stat.ty,
        nlink: stat.nlink.cast_unsigned(),
        size: stat.size,
    })
}

pub fn remove_file(path: &CStr) -> Result<(), Error> {
    syscall::unlink(path)
}

pub fn create_dir(path: &CStr) -> Result<(), Error> {
    syscall::mkdir(path)
}

pub fn read_dir(path: &CStr) -> Result<ReadDir, Error> {
    let fd = syscall::open(path, OpenFlags::READ_ONLY)?;
    let st = syscall::fstat(fd.as_fd())?;
    if st.ty != StatType::Dir {
        return Err(Error::NotADirectory);
    }
    Ok(ReadDir { fd })
}

pub struct ReadDir {
    fd: OwnedFd,
}

impl Iterator for ReadDir {
    type Item = Result<DirEntry, Error>;

    fn next(&mut self) -> Option<Self::Item> {
        loop {
            let mut ent = xv6_fs_types::DirEntry::zeroed();
            let Ok(size) = syscall::read(self.fd.as_fd(), ent.as_bytes_mut()) else {
                return Some(Err(Error::Unknown));
            };
            if size == 0 {
                return None;
            }
            if ent.ino().is_none() {
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
