use crate::{
    error::Error,
    io::{Read, Write},
    os::{
        fd::{AsFd, AsRawFd, BorrowedFd, FromRawFd, IntoRawFd, OwnedFd, RawFd},
        xv6::syscall,
    },
};

pub fn pipe() -> Result<(PipeReader, PipeWriter), Error> {
    let (rx, tx) = syscall::pipe()?;
    Ok((PipeReader(rx), PipeWriter(tx)))
}

pub struct PipeReader(OwnedFd);

pub struct PipeWriter(OwnedFd);

impl PipeReader {
    pub fn try_clone(&self) -> Result<Self, Error> {
        Ok(Self(self.0.try_clone()?))
    }
}

impl PipeWriter {
    pub fn try_clone(&self) -> Result<Self, Error> {
        Ok(Self(self.0.try_clone()?))
    }
}

impl AsFd for PipeReader {
    fn as_fd(&self) -> BorrowedFd<'_> {
        self.0.as_fd()
    }
}

impl AsFd for PipeWriter {
    fn as_fd(&self) -> BorrowedFd<'_> {
        self.0.as_fd()
    }
}

impl AsRawFd for PipeReader {
    fn as_raw_fd(&self) -> RawFd {
        self.0.as_raw_fd()
    }
}

impl AsRawFd for PipeWriter {
    fn as_raw_fd(&self) -> RawFd {
        self.0.as_raw_fd()
    }
}

impl FromRawFd for PipeReader {
    unsafe fn from_raw_fd(fd: RawFd) -> Self {
        unsafe { Self(OwnedFd::from_raw_fd(fd)) }
    }
}

impl FromRawFd for PipeWriter {
    unsafe fn from_raw_fd(fd: RawFd) -> Self {
        unsafe { Self(OwnedFd::from_raw_fd(fd)) }
    }
}

impl IntoRawFd for PipeReader {
    fn into_raw_fd(self) -> RawFd {
        self.0.into_raw_fd()
    }
}

impl IntoRawFd for PipeWriter {
    fn into_raw_fd(self) -> RawFd {
        self.0.into_raw_fd()
    }
}

impl Read for PipeReader {
    fn read(&mut self, buf: &mut [u8]) -> Result<usize, Error> {
        syscall::read(self.0.as_fd(), buf)
    }
}

impl Write for PipeReader {
    fn write(&mut self, buf: &[u8]) -> Result<usize, Error> {
        syscall::write(self.0.as_fd(), buf)
    }
}
