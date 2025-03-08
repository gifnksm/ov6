use ov6_syscall::{ReturnType, syscall as sys};

use crate::{
    error::KernelError,
    interrupt::trap::{TICKS, TICKS_UPDATED},
    proc::{self, Proc, ProcId, ProcPrivateDataGuard},
    syscall,
};

pub fn sys_fork(
    p: &'static Proc,
    private: &mut Option<ProcPrivateDataGuard>,
) -> ReturnType<sys::Fork> {
    let private = private.as_mut().unwrap();
    let pid = proc::fork(p, private)
        .map(|pid| pid.get().try_into().unwrap())
        .ok_or(KernelError::Unknown)?;
    Ok(pid)
}

pub fn sys_exit(
    p: &'static Proc,
    private: &mut Option<ProcPrivateDataGuard>,
) -> ReturnType<sys::Exit> {
    let private = private.take().unwrap();
    let n = syscall::arg_int(&private, 0);
    proc::exit(p, private, i32::try_from(n).unwrap());
}

pub fn sys_wait(
    p: &'static Proc,
    private: &mut Option<ProcPrivateDataGuard>,
) -> ReturnType<sys::Wait> {
    let private = private.as_mut().unwrap();
    let addr = syscall::arg_addr(private, 0);
    let pid = proc::wait(p, private, addr)?;
    Ok(pid.get().try_into().unwrap())
}

pub fn sys_kill(
    _p: &'static Proc,
    private: &mut Option<ProcPrivateDataGuard>,
) -> ReturnType<sys::Kill> {
    let private = private.as_mut().unwrap();
    let pid = syscall::arg_int(private, 0);
    proc::kill(ProcId::new(i32::try_from(pid).unwrap()))?;
    Ok(0)
}

pub fn sys_getpid(
    p: &'static Proc,
    _private: &mut Option<ProcPrivateDataGuard>,
) -> ReturnType<sys::Getpid> {
    let pid = p.shared().lock().pid();
    Ok(pid.get().try_into().unwrap())
}

pub fn sys_sbrk(
    _p: &'static Proc,
    private: &mut Option<ProcPrivateDataGuard>,
) -> ReturnType<sys::Sbrk> {
    let private = private.as_mut().unwrap();
    let n = syscall::arg_int(private, 0);
    let addr = private.size();
    proc::grow_proc(private, n as isize)?;
    Ok(addr)
}

pub fn sys_sleep(
    p: &'static Proc,
    private: &mut Option<ProcPrivateDataGuard>,
) -> ReturnType<sys::Sleep> {
    let private = private.as_mut().unwrap();
    let n = syscall::arg_int(private, 0) as u64;
    let mut ticks = TICKS.lock();
    let ticks0 = *ticks;
    while *ticks - ticks0 < n {
        if p.shared().lock().killed() {
            return Err(KernelError::Unknown.into());
        }
        ticks = TICKS_UPDATED.wait(ticks);
    }
    drop(ticks);
    Ok(0)
}

pub fn sys_uptime(
    _p: &'static Proc,
    _private: &mut Option<ProcPrivateDataGuard>,
) -> ReturnType<sys::Uptime> {
    Ok(usize::try_from(*TICKS.lock()).unwrap())
}
