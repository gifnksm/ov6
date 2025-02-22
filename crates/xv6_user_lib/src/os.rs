use crate::{error::Error, syscall};

pub(crate) fn fd_read(fd: i32, buf: &mut [u8]) -> Result<usize, crate::error::Error> {
    let res = unsafe { syscall::read(fd, buf.as_mut_ptr(), buf.len()) };
    if res < 0 {
        return Err(Error::Unknown);
    }
    Ok(res as usize)
}

pub(crate) fn fd_write(fd: i32, buf: &[u8]) -> Result<usize, Error> {
    let res = unsafe { syscall::write(fd, buf.as_ptr(), buf.len()) };
    if res < 0 {
        return Err(Error::Unknown);
    }
    Ok(res as usize)
}

pub(crate) fn fd_close(fd: i32) -> Result<(), Error> {
    if syscall::close(fd) < 0 {
        return Err(Error::Unknown);
    }
    Ok(())
}
