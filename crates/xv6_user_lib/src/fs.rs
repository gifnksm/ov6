use core::ffi::CStr;

use crate::{
    error::Error,
    io::{Read, Write},
    os, syscall,
};

pub use syscall::OpenFlags;

pub struct File {
    fd: i32,
}

impl File {
    pub fn open(path: &CStr, flags: OpenFlags) -> Result<Self, Error> {
        let fd = unsafe { syscall::open(path.as_ptr(), flags) };
        if fd < 0 {
            return Err(Error::Unknown);
        }
        Ok(Self { fd })
    }

    pub fn try_clone(&self) -> Result<Self, Error> {
        let fd = os::fd_dup(self.fd)?;
        Ok(File { fd })
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
