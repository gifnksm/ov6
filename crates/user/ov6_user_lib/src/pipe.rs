use ov6_types::fs::RawFd;

use crate::{
    error::Ov6Error,
    io::{Read, Write},
    os::{
        fd::{AsFd, AsRawFd, BorrowedFd, FromRawFd, IntoRawFd, OwnedFd},
        ov6::syscall,
    },
};

pub fn pipe() -> Result<(PipeReader, PipeWriter), Ov6Error> {
    let (rx, tx) = syscall::pipe()?;
    Ok((PipeReader(rx), PipeWriter(tx)))
}

pub struct PipeReader(OwnedFd);

pub struct PipeWriter(OwnedFd);

impl PipeReader {
    pub fn try_clone(&self) -> Result<Self, Ov6Error> {
        Ok(Self(self.0.try_clone()?))
    }
}

impl PipeWriter {
    pub fn try_clone(&self) -> Result<Self, Ov6Error> {
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
    fn read(&mut self, buf: &mut [u8]) -> Result<usize, Ov6Error> {
        syscall::read(self.0.as_raw_fd(), buf)
    }
}

impl Write for PipeWriter {
    fn write(&mut self, buf: &[u8]) -> Result<usize, Ov6Error> {
        syscall::write(self.0.as_raw_fd(), buf)
    }
}
