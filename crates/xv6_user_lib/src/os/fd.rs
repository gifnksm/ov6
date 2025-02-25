use core::{fmt, marker::PhantomData};

use super::xv6::syscall;

pub type RawFd = i32;

pub struct OwnedFd {
    fd: RawFd,
}

impl Drop for OwnedFd {
    fn drop(&mut self) {
        let _ = unsafe { syscall::close(self.fd) };
    }
}

impl fmt::Debug for OwnedFd {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("OwnedFd").field("fd", &self.fd).finish()
    }
}

pub struct BorrowedFd<'fd> {
    fd: RawFd,
    _phantom: PhantomData<&'fd OwnedFd>,
}

impl fmt::Debug for BorrowedFd<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("OwnedFd").field("fd", &self.fd).finish()
    }
}

pub trait AsFd {
    fn as_fd(&self) -> BorrowedFd<'_>;
}

impl AsFd for OwnedFd {
    fn as_fd(&self) -> BorrowedFd<'_> {
        BorrowedFd {
            fd: self.fd,
            _phantom: PhantomData,
        }
    }
}

impl AsFd for BorrowedFd<'_> {
    fn as_fd(&self) -> BorrowedFd<'_> {
        BorrowedFd {
            fd: self.fd,
            _phantom: PhantomData,
        }
    }
}

pub trait AsRawFd {
    fn as_raw_fd(&self) -> RawFd;
}

impl AsRawFd for OwnedFd {
    fn as_raw_fd(&self) -> RawFd {
        self.fd
    }
}

impl AsRawFd for BorrowedFd<'_> {
    fn as_raw_fd(&self) -> RawFd {
        self.fd
    }
}

impl AsRawFd for RawFd {
    fn as_raw_fd(&self) -> RawFd {
        *self
    }
}

pub trait FromRawFd {
    unsafe fn from_raw_fd(fd: RawFd) -> Self;
}

impl FromRawFd for OwnedFd {
    unsafe fn from_raw_fd(fd: RawFd) -> Self {
        Self { fd }
    }
}

impl FromRawFd for RawFd {
    unsafe fn from_raw_fd(fd: RawFd) -> Self {
        fd
    }
}

pub trait IntoRawFd {
    fn into_raw_fd(self) -> RawFd;
}

impl IntoRawFd for OwnedFd {
    fn into_raw_fd(self) -> RawFd {
        let fd = self.fd;
        core::mem::forget(self);
        fd
    }
}

impl IntoRawFd for RawFd {
    fn into_raw_fd(self) -> RawFd {
        self
    }
}
