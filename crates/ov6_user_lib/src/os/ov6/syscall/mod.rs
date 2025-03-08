use core::{
    convert::Infallible,
    ffi::{CStr, c_char},
    mem::MaybeUninit,
    ptr,
};

pub use ov6_syscall::{OpenFlags, Stat, StatType, SyscallCode};
use ov6_syscall::{Ret1, SyscallError};

use crate::{
    error::Ov6Error,
    os::fd::{AsRawFd, FromRawFd as _, OwnedFd},
    process::{ExitStatus, ForkResult},
};

pub mod ffi;

fn to_result<T>(res: Ret1<Result<usize, SyscallError>>) -> Result<T, Ov6Error>
where
    T: TryFrom<usize>,
{
    let res = res.decode()?;
    res.try_into().or(Err(Ov6Error::Unknown))
}

fn to_result_zero(res: Ret1<Result<usize, SyscallError>>) -> Result<(), Ov6Error> {
    if to_result::<usize>(res)? != 0 {
        Err(Ov6Error::Unknown)
    } else {
        Ok(())
    }
}

pub fn fork() -> Result<ForkResult, Ov6Error> {
    let pid = to_result(ffi::fork())?;
    if pid == 0 {
        Ok(ForkResult::Child)
    } else {
        Ok(ForkResult::Parent { child: pid })
    }
}

pub fn exit(status: i32) -> ! {
    ffi::exit(status);
    unreachable!()
}

pub fn wait() -> Result<(u32, ExitStatus), Ov6Error> {
    let mut status = 0;
    let pid = to_result(unsafe { ffi::wait(&mut status) })?;
    Ok((pid, ExitStatus::new(status)))
}

pub fn pipe() -> Result<(OwnedFd, OwnedFd), Ov6Error> {
    unsafe {
        let mut pipefd = [0; 2];
        to_result_zero(ffi::pipe(pipefd.as_mut_ptr()))?;
        Ok((
            OwnedFd::from_raw_fd(pipefd[0]),
            OwnedFd::from_raw_fd(pipefd[1]),
        ))
    }
}

pub fn write(fd: impl AsRawFd, buf: &[u8]) -> Result<usize, Ov6Error> {
    let count = buf.len();
    let nwritten = to_result(unsafe { ffi::write(fd.as_raw_fd(), buf.as_ptr(), count) })?;
    Ok(nwritten)
}

pub fn read(fd: impl AsRawFd, buf: &mut [u8]) -> Result<usize, Ov6Error> {
    let count = buf.len();
    let nread = to_result(unsafe { ffi::read(fd.as_raw_fd(), buf.as_mut_ptr(), count) })?;
    Ok(nread)
}

/// # Safety
///
/// This invalidates `OwnedFd` and `BorrowedFd` instances that refer to the
/// closed file descriptor.
pub unsafe fn close(fd: impl AsRawFd) -> Result<(), Ov6Error> {
    to_result_zero(ffi::close(fd.as_raw_fd()))
}

pub fn kill(pid: u32) -> Result<(), Ov6Error> {
    to_result_zero(ffi::kill(pid))
}

pub fn exec(path: &CStr, argv: &[*const c_char]) -> Result<Infallible, Ov6Error> {
    assert!(
        argv.last().unwrap().is_null(),
        "last element of argv must be null"
    );
    to_result::<isize>(unsafe { ffi::exec(path.as_ptr(), argv.as_ptr()) })?;
    unreachable!()
}

pub fn open(path: &CStr, flags: OpenFlags) -> Result<OwnedFd, Ov6Error> {
    unsafe {
        let fd = to_result(ffi::open(path.as_ptr(), flags))?;
        Ok(OwnedFd::from_raw_fd(fd))
    }
}

pub fn mknod(path: &CStr, major: i16, minor: i16) -> Result<(), Ov6Error> {
    to_result_zero(unsafe { ffi::mknod(path.as_ptr(), major, minor) })
}

pub fn unlink(path: &CStr) -> Result<(), Ov6Error> {
    to_result_zero(unsafe { ffi::unlink(path.as_ptr()) })
}

pub fn fstat(fd: impl AsRawFd) -> Result<Stat, Ov6Error> {
    unsafe {
        let mut stat = MaybeUninit::uninit();
        to_result_zero(ffi::fstat(fd.as_raw_fd(), stat.as_mut_ptr()))?;
        Ok(stat.assume_init())
    }
}

pub fn link(old: &CStr, new: &CStr) -> Result<(), Ov6Error> {
    to_result_zero(unsafe { ffi::link(old.as_ptr(), new.as_ptr()) })
}

pub fn mkdir(path: &CStr) -> Result<(), Ov6Error> {
    to_result_zero(unsafe { ffi::mkdir(path.as_ptr()) })
}

pub fn chdir(path: &CStr) -> Result<(), Ov6Error> {
    to_result_zero(unsafe { ffi::chdir(path.as_ptr()) })
}

pub fn dup(fd: impl AsRawFd) -> Result<OwnedFd, Ov6Error> {
    let fd = to_result(ffi::dup(fd.as_raw_fd()))?;
    Ok(unsafe { OwnedFd::from_raw_fd(fd) })
}

pub fn getpid() -> Result<u32, Ov6Error> {
    to_result(ffi::getpid())
}

/// # Safety
///
/// This function is unsafe because it may invalidate the region of memory that
/// was previously allocated by the kernel.
pub unsafe fn sbrk(n: isize) -> Result<*mut u8, Ov6Error> {
    let addr: usize = to_result(ffi::sbrk(n))?;
    Ok(ptr::with_exposed_provenance_mut(addr))
}

pub fn sleep(n: i32) -> Result<(), Ov6Error> {
    to_result_zero(ffi::sleep(n))
}

pub fn uptime() -> Result<usize, Ov6Error> {
    to_result(ffi::uptime())
}
