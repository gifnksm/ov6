use core::{convert::Infallible, ptr};

use dataview::PodMethods as _;
pub use ov6_syscall::{OpenFlags, Stat, StatType, SyscallCode};
use ov6_syscall::{UserMutRef, UserMutSlice, UserSlice, syscall};
use ov6_types::{fs::RawFd, path::Path, process::ProcId};

use self::ffi::SyscallExt as _;
use crate::{
    error::Ov6Error,
    os::fd::{AsRawFd, FromRawFd as _, OwnedFd},
    process::{ExitStatus, ForkResult},
};

pub mod ffi;

pub fn fork() -> Result<ForkResult, Ov6Error> {
    let pid = syscall::Fork::call(())?;
    Ok(pid.map_or(ForkResult::Child, |pid| ForkResult::Parent { child: pid }))
}

pub fn exit(status: i32) -> ! {
    syscall::Exit::call((status,));
    unreachable!()
}

pub fn wait() -> Result<(ProcId, ExitStatus), Ov6Error> {
    let mut status = 0;
    let pid = syscall::Wait::call((UserMutRef::new(&mut status),))?;
    Ok((pid, ExitStatus::new(status)))
}

pub fn pipe() -> Result<(OwnedFd, OwnedFd), Ov6Error> {
    let mut pipefd = [const { RawFd::new(0) }; 2];
    syscall::Pipe::call((UserMutRef::new(&mut pipefd),))?;
    Ok((unsafe { OwnedFd::from_raw_fd(pipefd[0]) }, unsafe {
        OwnedFd::from_raw_fd(pipefd[1])
    }))
}

pub fn write(fd: impl AsRawFd, buf: &[u8]) -> Result<usize, Ov6Error> {
    let nwritten = syscall::Write::call((fd.as_raw_fd(), UserSlice::new(buf)))?;
    Ok(nwritten)
}

pub fn read(fd: impl AsRawFd, buf: &mut [u8]) -> Result<usize, Ov6Error> {
    let nread = syscall::Read::call((fd.as_raw_fd(), UserMutSlice::new(buf)))?;
    Ok(nread)
}

/// # Safety
///
/// This invalidates `OwnedFd` and `BorrowedFd` instances that refer to the
/// closed file descriptor.
pub unsafe fn close(fd: impl AsRawFd) -> Result<(), Ov6Error> {
    syscall::Close::call((fd.as_raw_fd(),))?;
    Ok(())
}

pub fn kill(pid: ProcId) -> Result<(), Ov6Error> {
    syscall::Kill::call((pid,))?;
    Ok(())
}

pub fn exec(path: &Path, argv: &[UserSlice<u8>]) -> Result<Infallible, Ov6Error> {
    syscall::Exec::call((
        UserSlice::new(path.as_os_str().as_bytes()),
        UserSlice::new(argv),
    ))?;
    unreachable!()
}

pub fn open(path: &Path, flags: OpenFlags) -> Result<OwnedFd, Ov6Error> {
    let fd = syscall::Open::call((UserSlice::new(path.as_os_str().as_bytes()), flags))?;
    unsafe { Ok(OwnedFd::from_raw_fd(fd)) }
}

pub fn mknod(path: &Path, major: u32, minor: i16) -> Result<(), Ov6Error> {
    syscall::Mknod::call((UserSlice::new(path.as_os_str().as_bytes()), major, minor))?;
    Ok(())
}

pub fn unlink(path: &Path) -> Result<(), Ov6Error> {
    syscall::Unlink::call((UserSlice::new(path.as_os_str().as_bytes()),))?;
    Ok(())
}

pub fn fstat(fd: impl AsRawFd) -> Result<Stat, Ov6Error> {
    let mut stat = Stat::zeroed();
    syscall::Fstat::call((fd.as_raw_fd(), UserMutRef::new(&mut stat)))?;
    Ok(stat)
}

pub fn link(old: &Path, new: &Path) -> Result<(), Ov6Error> {
    syscall::Link::call((
        UserSlice::new(old.as_os_str().as_bytes()),
        UserSlice::new(new.as_os_str().as_bytes()),
    ))?;
    Ok(())
}

pub fn mkdir(path: &Path) -> Result<(), Ov6Error> {
    syscall::Mkdir::call((UserSlice::new(path.as_os_str().as_bytes()),))?;
    Ok(())
}

pub fn chdir(path: &Path) -> Result<(), Ov6Error> {
    syscall::Chdir::call((UserSlice::new(path.as_os_str().as_bytes()),))?;
    Ok(())
}

pub fn dup(fd: impl AsRawFd) -> Result<OwnedFd, Ov6Error> {
    let fd = syscall::Dup::call((fd.as_raw_fd(),))?;
    Ok(unsafe { OwnedFd::from_raw_fd(fd) })
}

#[must_use]
pub fn getpid() -> ProcId {
    syscall::Getpid::call(())
}

/// # Safety
///
/// This function is unsafe because it may invalidate the region of memory that
/// was previously allocated by the kernel.
pub unsafe fn sbrk(increment: isize) -> Result<*mut u8, Ov6Error> {
    let addr = syscall::Sbrk::call((increment,))?;
    Ok(ptr::with_exposed_provenance_mut(addr))
}

pub fn sleep(n: u64) {
    syscall::Sleep::call((n,));
}

#[must_use]
pub fn uptime() -> u64 {
    syscall::Uptime::call(())
}
