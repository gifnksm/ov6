use core::{
    convert::Infallible,
    ffi::{CStr, c_char},
    ptr,
};

use dataview::PodMethods as _;
use ov6_syscall::{
    ArgType, RegisterValue as _, UserMutRef, UserMutSlice, UserRef, UserSlice, syscall,
};
pub use ov6_syscall::{OpenFlags, Stat, StatType, SyscallCode};
use ov6_types::{fs::RawFd, process::ProcId};

use crate::{
    error::Ov6Error,
    os::fd::{AsRawFd, FromRawFd as _, OwnedFd},
    process::{ExitStatus, ForkResult},
};

pub mod ffi;

pub fn fork() -> Result<ForkResult, Ov6Error> {
    let [] = ArgType::<syscall::Fork>::encode(()).a;
    Ok((ffi::fork().decode()?).map_or(ForkResult::Child, |pid| ForkResult::Parent { child: pid }))
}

pub fn exit(status: i32) -> ! {
    let [a0] = ArgType::<syscall::Exit>::encode((status,)).a;
    let _x: Infallible = ffi::exit(a0).decode();
    unreachable!()
}

pub fn wait() -> Result<(ProcId, ExitStatus), Ov6Error> {
    let mut status = 0;
    let [a0] = ArgType::<syscall::Wait>::encode((UserMutRef::new(&mut status),)).a;
    let pid = ffi::wait(a0).decode()?;
    Ok((pid, ExitStatus::new(status)))
}

pub fn pipe() -> Result<(OwnedFd, OwnedFd), Ov6Error> {
    let mut pipefd = [const { RawFd::new(0) }; 2];
    let [a0] = ArgType::<syscall::Pipe>::encode((UserMutRef::new(&mut pipefd),)).a;
    ffi::pipe(a0).decode()?;
    Ok((unsafe { OwnedFd::from_raw_fd(pipefd[0]) }, unsafe {
        OwnedFd::from_raw_fd(pipefd[1])
    }))
}

pub fn write(fd: impl AsRawFd, buf: &[u8]) -> Result<usize, Ov6Error> {
    let [a0, a1, a2] = ArgType::<syscall::Write>::encode((fd.as_raw_fd(), UserSlice::new(buf))).a;
    let nwritten = ffi::write(a0, a1, a2).decode()?;
    Ok(nwritten)
}

pub fn read(fd: impl AsRawFd, buf: &mut [u8]) -> Result<usize, Ov6Error> {
    let [a0, a1, a2] = ArgType::<syscall::Read>::encode((fd.as_raw_fd(), UserMutSlice::new(buf))).a;
    let nread = ffi::read(a0, a1, a2).decode()?;
    Ok(nread)
}

/// # Safety
///
/// This invalidates `OwnedFd` and `BorrowedFd` instances that refer to the
/// closed file descriptor.
pub unsafe fn close(fd: impl AsRawFd) -> Result<(), Ov6Error> {
    let [a0] = ArgType::<syscall::Close>::encode((fd.as_raw_fd(),)).a;
    ffi::close(a0).decode()?;
    Ok(())
}

pub fn kill(pid: ProcId) -> Result<(), Ov6Error> {
    let [a0] = ArgType::<syscall::Kill>::encode((pid,)).a;
    ffi::kill(a0).decode()?;
    Ok(())
}

pub fn exec(path: &CStr, argv: &[*const c_char]) -> Result<Infallible, Ov6Error> {
    assert!(
        argv.last().unwrap().is_null(),
        "last element of argv must be null"
    );
    let [a0, a1, a2] =
        ArgType::<syscall::Exec>::encode((UserRef::new(path), UserSlice::new(argv))).a;
    ffi::exec(a0, a1, a2).decode()?;
    unreachable!()
}

pub fn open(path: &CStr, flags: OpenFlags) -> Result<OwnedFd, Ov6Error> {
    let [a0, a1] = ArgType::<syscall::Open>::encode((UserRef::new(path), flags)).a;
    let fd = ffi::open(a0, a1).decode()?;
    unsafe { Ok(OwnedFd::from_raw_fd(fd)) }
}

pub fn mknod(path: &CStr, major: u32, minor: i16) -> Result<(), Ov6Error> {
    let [a0, a1, a2] = ArgType::<syscall::Mknod>::encode((UserRef::new(path), major, minor)).a;
    ffi::mknod(a0, a1, a2).decode()?;
    Ok(())
}

pub fn unlink(path: &CStr) -> Result<(), Ov6Error> {
    let [a0] = ArgType::<syscall::Unlink>::encode((UserRef::new(path),)).a;
    ffi::unlink(a0).decode()?;
    Ok(())
}

pub fn fstat(fd: impl AsRawFd) -> Result<Stat, Ov6Error> {
    let mut stat = Stat::zeroed();
    let [a0, a1] =
        ArgType::<syscall::Fstat>::encode((fd.as_raw_fd(), UserMutRef::new(&mut stat))).a;
    ffi::fstat(a0, a1).decode()?;
    Ok(stat)
}

pub fn link(old: &CStr, new: &CStr) -> Result<(), Ov6Error> {
    let [a0, a1] = ArgType::<syscall::Link>::encode((UserRef::new(old), UserRef::new(new))).a;
    ffi::link(a0, a1).decode()?;
    Ok(())
}

pub fn mkdir(path: &CStr) -> Result<(), Ov6Error> {
    let [a0] = ArgType::<syscall::Mkdir>::encode((UserRef::new(path),)).a;
    ffi::mkdir(a0).decode()?;
    Ok(())
}

pub fn chdir(path: &CStr) -> Result<(), Ov6Error> {
    let [a0] = ArgType::<syscall::Chdir>::encode((UserRef::new(path),)).a;
    ffi::chdir(a0).decode()?;
    Ok(())
}

pub fn dup(fd: impl AsRawFd) -> Result<OwnedFd, Ov6Error> {
    let [a0] = ArgType::<syscall::Dup>::encode((fd.as_raw_fd(),)).a;
    let fd = ffi::dup(a0).decode()?;
    Ok(unsafe { OwnedFd::from_raw_fd(fd) })
}

#[must_use]
pub fn getpid() -> ProcId {
    let [] = ArgType::<syscall::Getpid>::encode(()).a;
    ffi::getpid().decode()
}

/// # Safety
///
/// This function is unsafe because it may invalidate the region of memory that
/// was previously allocated by the kernel.
pub unsafe fn sbrk(increment: isize) -> Result<*mut u8, Ov6Error> {
    let [a0] = ArgType::<syscall::Sbrk>::encode((increment,)).a;
    let addr = ffi::sbrk(a0).decode()?;
    Ok(ptr::with_exposed_provenance_mut(addr))
}

pub fn sleep(n: u64) {
    let [a0] = ArgType::<syscall::Sleep>::encode((n,)).a;
    ffi::sleep(a0).decode()
}

#[must_use]
pub fn uptime() -> u64 {
    let [] = ArgType::<syscall::Uptime>::encode(()).a;
    ffi::uptime().decode()
}
