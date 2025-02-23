use crate::{error::Error, syscall};

pub(crate) fn fd_read(fd: i32, buf: &mut [u8]) -> Result<usize, crate::error::Error> {
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
