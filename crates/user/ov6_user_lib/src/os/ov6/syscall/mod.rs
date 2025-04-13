use core::{
    convert::Infallible,
    net::{Ipv4Addr, SocketAddrV4},
    ptr,
    time::Duration,
};

use dataview::PodMethods as _;
pub use ov6_syscall::{MemoryInfo, OpenFlags, Stat, StatType, SyscallCode, SystemInfo};
use ov6_syscall::{
    USYSCALL_ADDR, USyscallData, UserMutRef, UserMutSlice, UserRef, UserSlice, WaitTarget, syscall,
};
use ov6_types::{fs::RawFd, path::Path, process::ProcId};

use self::ffi::SyscallExt as _;
use crate::{
    error::Ov6Error,
    os::fd::{FromRawFd as _, OwnedFd},
    process::ExitStatus,
};

pub mod ffi;

pub fn fork() -> Result<Option<ProcId>, Ov6Error> {
    let pid = syscall::Fork::call(())?;
    Ok(pid)
}

pub fn exit(status: i32) -> ! {
    syscall::Exit::call((status,));
    unreachable!()
}

pub fn wait(target: WaitTarget) -> Result<(ProcId, ExitStatus), Ov6Error> {
    let mut status = 0;
    let pid = syscall::Wait::call((target, UserMutRef::new(&mut status)))?;
    Ok((pid, ExitStatus::new(status)))
}

pub fn pipe() -> Result<(OwnedFd, OwnedFd), Ov6Error> {
    let mut pipefd = [const { RawFd::new(0) }; 2];
    syscall::Pipe::call((UserMutRef::new(&mut pipefd),))?;
    Ok((unsafe { OwnedFd::from_raw_fd(pipefd[0]) }, unsafe {
        OwnedFd::from_raw_fd(pipefd[1])
    }))
}

pub fn write(fd: RawFd, buf: &[u8]) -> Result<usize, Ov6Error> {
    let nwritten = syscall::Write::call((fd, UserSlice::new(buf)))?;
    Ok(nwritten)
}

pub fn read(fd: RawFd, buf: &mut [u8]) -> Result<usize, Ov6Error> {
    let nread = syscall::Read::call((fd, UserMutSlice::new(buf)))?;
    Ok(nread)
}

/// # Safety
///
/// This invalidates `OwnedFd` and `BorrowedFd` instances that refer to the
/// closed file descriptor.
pub unsafe fn close(fd: RawFd) -> Result<(), Ov6Error> {
    syscall::Close::call((fd,))?;
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

pub fn mknod(path: &Path, major: u32, minor: u16) -> Result<(), Ov6Error> {
    syscall::Mknod::call((UserSlice::new(path.as_os_str().as_bytes()), major, minor))?;
    Ok(())
}

pub fn unlink(path: &Path) -> Result<(), Ov6Error> {
    syscall::Unlink::call((UserSlice::new(path.as_os_str().as_bytes()),))?;
    Ok(())
}

pub fn fstat(fd: RawFd) -> Result<Stat, Ov6Error> {
    let mut stat = Stat::zeroed();
    syscall::Fstat::call((fd, UserMutRef::new(&mut stat)))?;
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

pub fn dup(fd: RawFd) -> Result<OwnedFd, Ov6Error> {
    let fd = syscall::Dup::call((fd,))?;
    Ok(unsafe { OwnedFd::from_raw_fd(fd) })
}

#[must_use]
pub fn ugetpid() -> ProcId {
    let usyscall_data = ptr::with_exposed_provenance::<USyscallData>(USYSCALL_ADDR);
    unsafe { (*usyscall_data).pid }
}

/// # Safety
///
/// This function is unsafe because it may invalidate the region of memory that
/// was previously allocated by the kernel.
pub unsafe fn sbrk(increment: isize) -> Result<*mut u8, Ov6Error> {
    let addr = syscall::Sbrk::call((increment,))?;
    Ok(ptr::with_exposed_provenance_mut(addr))
}

pub fn sleep(dur: Duration) -> Result<(), Ov6Error> {
    syscall::Sleep::call((dur,))?;
    Ok(())
}

pub fn alarm_set(dur: Duration, handler: extern "C" fn()) -> Result<(), Ov6Error> {
    syscall::AlarmSet::call((dur, UserRef::from_fn(handler)))?;
    Ok(())
}

pub fn alarm_clear() -> Result<(), Ov6Error> {
    syscall::AlarmClear::call(())?;
    Ok(())
}

pub fn signal_return() -> Result<Infallible, Ov6Error> {
    let _: Infallible = syscall::SignalReturn::call(())?;
    unreachable!()
}

pub fn bind(port: u16) -> Result<(), Ov6Error> {
    syscall::Bind::call((port,))?;
    Ok(())
}

pub fn unbind(port: u16) -> Result<(), Ov6Error> {
    syscall::Unbind::call((port,))?;
    Ok(())
}

pub fn recv(port: u16, bytes: &mut [u8]) -> Result<(usize, SocketAddrV4), Ov6Error> {
    let mut src = SocketAddrV4::new(Ipv4Addr::UNSPECIFIED, 0).into();
    let len = syscall::Recv::call((port, UserMutRef::new(&mut src), UserMutSlice::new(bytes)))?;
    Ok((len, src.into()))
}

pub fn send(src_port: u16, dst: SocketAddrV4, bytes: &[u8]) -> Result<usize, Ov6Error> {
    let len = syscall::Send::call((src_port, dst, UserSlice::new(bytes)))?;
    Ok(len)
}

pub fn get_system_info() -> Result<SystemInfo, Ov6Error> {
    let mut info = SystemInfo::zeroed();
    syscall::GetSystemInfo::call((UserMutRef::new(&mut info),))?;
    Ok(info)
}

pub fn reboot() -> Result<Infallible, Ov6Error> {
    let _: Infallible = syscall::Reboot::call(())?;
    unreachable!()
}

pub fn halt(code: u16) -> Result<Infallible, Ov6Error> {
    let _: Infallible = syscall::Halt::call((code,))?;
    unreachable!()
}

pub fn abort(code: u16) -> Result<Infallible, Ov6Error> {
    let _: Infallible = syscall::Abort::call((code,))?;
    unreachable!()
}

#[must_use]
#[cfg(target_arch = "riscv64")]
pub fn uptime() -> u64 {
    let time: u64;
    unsafe {
        core::arch::asm!("csrr {}, time", out(reg) time);
    }
    time * 100
}

#[must_use]
#[cfg(not(target_arch = "riscv64"))]
pub fn uptime() -> u64 {
    unimplemented!()
}

pub fn trace(mask: u64) {
    syscall::Trace::call((mask,));
}

pub fn dump_kernel_page_table() {
    syscall::DumpKernelPageTable::call(());
}

pub fn dump_user_page_table() {
    syscall::DumpUserPageTable::call(());
}
