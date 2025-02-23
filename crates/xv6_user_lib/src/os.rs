use core::{ffi::CStr, mem::MaybeUninit};

use xv6_syscall::{OpenFlags, StatType};

use crate::{error::Error, fs::Metadata, syscall};

pub(crate) fn fd_open(path: &CStr, flags: OpenFlags) -> Result<i32, Error> {
    let fd = unsafe { syscall::open(path.as_ptr(), flags) };
    if fd < 0 {
        return Err(Error::Unknown);
    }
    Ok(fd)
}

pub(crate) fn fd_read(fd: i32, buf: &mut [u8]) -> Result<usize, Error> {
    let read = unsafe { syscall::read(fd, buf.as_mut_ptr(), buf.len()) };
    if read < 0 {
        return Err(Error::Unknown);
    }
    Ok(read as usize)
}

pub(crate) fn fd_write(fd: i32, buf: &[u8]) -> Result<usize, Error> {
    let written = unsafe { syscall::write(fd, buf.as_ptr(), buf.len()) };
    if written < 0 {
        return Err(Error::Unknown);
    }
    Ok(written as usize)
}

pub(crate) fn fd_close(fd: i32) -> Result<(), Error> {
    if syscall::close(fd) < 0 {
        return Err(Error::Unknown);
    }
    Ok(())
}

pub(crate) fn fd_dup(fd: i32) -> Result<i32, Error> {
    let fd = syscall::dup(fd);
    if fd < 0 {
        return Err(Error::Unknown);
    }
    Ok(fd)
}

pub(crate) fn fd_stat(fd: i32) -> Result<Metadata, Error> {
    unsafe {
        let mut stat = MaybeUninit::uninit();
        if syscall::fstat(fd, stat.as_mut_ptr()) < 0 {
            return Err(Error::Unknown);
        }
        let stat = stat.assume_init();
        let ty = StatType::from_repr(stat.ty).ok_or(Error::Unknown)?;
        Ok(Metadata {
            dev: stat.dev.cast_unsigned(),
            ino: stat.ino,
            ty,
            nlink: stat.nlink.cast_unsigned(),
            size: stat.size,
        })
    }
}
