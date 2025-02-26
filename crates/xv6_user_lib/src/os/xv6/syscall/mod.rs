use core::{
    convert::Infallible,
    ffi::{CStr, c_char},
    mem::MaybeUninit,
};

pub use xv6_syscall::{OpenFlags, Stat, StatType, SyscallType};

use crate::{
    error::Error,
    os::fd::{AsRawFd, FromRawFd as _, OwnedFd},
    process::{ExitStatus, ForkResult},
};

pub mod ffi;

fn to_result<T>(res: isize) -> Result<T, Error>
where
    T: TryFrom<isize>,
{
    if res < 0 {
        return Err(xv6_syscall::Error::from_repr(res).map_or(Error::Unknown, Error::from));
    }
    res.try_into().or(Err(Error::Unknown))
}

fn to_result_zero(res: isize) -> Result<(), Error> {
    if to_result::<isize>(res)? != 0 {
        Err(Error::Unknown)
    } else {
        Ok(())
    }
}

pub fn fork() -> Result<ForkResult, Error> {
    let pid = to_result(ffi::fork())?;
    if pid == 0 {
        Ok(ForkResult::Child)
    } else {
        Ok(ForkResult::Parent { child: pid })
    }
}

pub fn exit(status: i32) -> ! {
    ffi::exit(status)
}

pub fn wait() -> Result<(u32, ExitStatus), Error> {
    let mut status = 0;
    let pid = to_result(unsafe { ffi::wait(&mut status) })?;
    Ok((pid, ExitStatus::new(status)))
}

pub fn pipe() -> Result<(OwnedFd, OwnedFd), Error> {
    unsafe {
        let mut pipefd = [0; 2];
        to_result_zero(ffi::pipe(pipefd.as_mut_ptr()))?;
        Ok((
            OwnedFd::from_raw_fd(pipefd[0]),
            OwnedFd::from_raw_fd(pipefd[1]),
        ))
    }
}

pub fn write(fd: impl AsRawFd, buf: &[u8]) -> Result<usize, Error> {
    let count = buf.len();
    let nwritten = to_result(unsafe { ffi::write(fd.as_raw_fd(), buf.as_ptr(), count) })?;
    Ok(nwritten)
}

pub fn read(fd: impl AsRawFd, buf: &mut [u8]) -> Result<usize, Error> {
    let count = buf.len();
    let nread = to_result(unsafe { ffi::read(fd.as_raw_fd(), buf.as_mut_ptr(), count) })?;
    Ok(nread)
}

pub unsafe fn close(fd: impl AsRawFd) -> Result<(), Error> {
    to_result_zero(ffi::close(fd.as_raw_fd()))
}

pub fn kill(pid: u32) -> Result<(), Error> {
    to_result_zero(ffi::kill(pid))
}

pub fn exec(path: &CStr, argv: &[*const c_char]) -> Result<Infallible, Error> {
    assert!(
        argv.last().unwrap().is_null(),
        "last element of argv must be null"
    );
    to_result::<isize>(unsafe { ffi::exec(path.as_ptr(), argv.as_ptr()) })?;
    unreachable!()
}

pub fn open(path: &CStr, flags: OpenFlags) -> Result<OwnedFd, Error> {
    unsafe {
        let fd = to_result(ffi::open(path.as_ptr(), flags))?;
        Ok(OwnedFd::from_raw_fd(fd))
    }
}

pub fn mknod(path: &CStr, major: i16, minor: i16) -> Result<(), Error> {
    to_result_zero(unsafe { ffi::mknod(path.as_ptr(), major, minor) })
}

pub fn unlink(path: &CStr) -> Result<(), Error> {
    to_result_zero(unsafe { ffi::unlink(path.as_ptr()) })
}

pub fn fstat(fd: impl AsRawFd) -> Result<Stat, Error> {
    unsafe {
        let mut stat = MaybeUninit::uninit();
        to_result_zero(ffi::fstat(fd.as_raw_fd(), stat.as_mut_ptr()))?;
        Ok(stat.assume_init())
    }
}

pub fn link(old: &CStr, new: &CStr) -> Result<(), Error> {
    to_result_zero(unsafe { ffi::link(old.as_ptr(), new.as_ptr()) })
}

pub fn mkdir(path: &CStr) -> Result<(), Error> {
    to_result_zero(unsafe { ffi::mkdir(path.as_ptr()) })
}

pub fn chdir(path: &CStr) -> Result<(), Error> {
    to_result_zero(unsafe { ffi::chdir(path.as_ptr()) })
}

pub fn dup(fd: impl AsRawFd) -> Result<OwnedFd, Error> {
    let fd = to_result(ffi::dup(fd.as_raw_fd()))?;
    Ok(unsafe { OwnedFd::from_raw_fd(fd) })
}

pub fn getpid() -> Result<u32, Error> {
    to_result(ffi::getpid())
}

pub unsafe fn sbrk(n: isize) -> Result<*mut u8, Error> {
    let addr: usize = to_result(ffi::sbrk(n))?;
    Ok(addr as _) // FIXME: ptr::without_provenance causes null pointer dereference in malloc
}

pub fn sleep(n: i32) -> Result<(), Error> {
    to_result_zero(ffi::sleep(n))
}

pub fn uptime() -> Result<usize, Error> {
    to_result(ffi::uptime())
}
