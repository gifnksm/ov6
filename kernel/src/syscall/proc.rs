use ov6_syscall::{ReturnType, syscall as sys};

use crate::{
    error::KernelError,
    interrupt::trap::{TICKS, TICKS_UPDATED},
    proc::{self, Proc, ProcPrivateDataGuard},
};

pub fn sys_fork(
    p: &'static Proc,
    private: &mut Option<ProcPrivateDataGuard>,
) -> ReturnType<sys::Fork> {
    let private = private.as_mut().unwrap();
    let Ok(()) = super::decode_arg::<sys::Fork>(private.trapframe().unwrap());
    let pid = proc::fork(p, private)?;
    Ok(Some(pid))
}

pub fn sys_exit(
    p: &'static Proc,
    private: &mut Option<ProcPrivateDataGuard>,
) -> ReturnType<sys::Exit> {
    let private = private.take().unwrap();
    let status = match super::decode_arg::<sys::Exit>(private.trapframe().unwrap()) {
        Ok((status,)) => status,
        Err(_e) => -1,
    };
    proc::exit(p, private, status);
}

pub fn sys_wait(
    p: &'static Proc,
    private: &mut Option<ProcPrivateDataGuard>,
) -> ReturnType<sys::Wait> {
    let private = private.as_mut().unwrap();
    let Ok((addr,)) = super::decode_arg::<sys::Wait>(private.trapframe().unwrap());
    let pid = proc::wait(p, private, addr)?;
    Ok(pid)
}

pub fn sys_kill(
    _p: &'static Proc,
    private: &mut Option<ProcPrivateDataGuard>,
) -> ReturnType<sys::Kill> {
    let private = private.as_mut().unwrap();
    let (pid,) =
        super::decode_arg::<sys::Kill>(private.trapframe().unwrap()).map_err(KernelError::from)?;
    proc::kill(pid)?;
    Ok(())
}

pub fn sys_getpid(
    p: &'static Proc,
    private: &mut Option<ProcPrivateDataGuard>,
) -> ReturnType<sys::Getpid> {
    let private = private.as_mut().unwrap();
    let Ok(()) = super::decode_arg::<sys::Getpid>(private.trapframe().unwrap());
    p.shared().lock().pid()
}

pub fn sys_sbrk(
    _p: &'static Proc,
    private: &mut Option<ProcPrivateDataGuard>,
) -> ReturnType<sys::Sbrk> {
    let private = private.as_mut().unwrap();
    let Ok((increment,)) = super::decode_arg::<sys::Sbrk>(private.trapframe().unwrap());
    let addr = private.size();
    proc::grow_proc(private, increment)?;
    Ok(addr)
}

pub fn sys_sleep(
    p: &'static Proc,
    private: &mut Option<ProcPrivateDataGuard>,
) -> ReturnType<sys::Sleep> {
    let private = private.as_mut().unwrap();
    let Ok((dur,)) = super::decode_arg::<sys::Sleep>(private.trapframe().unwrap());
    let mut ticks = TICKS.lock();
    let ticks0 = *ticks;
    while *ticks - ticks0 < dur {
        if p.shared().lock().killed() {
            // process is killed, so return value will never read.
            return;
        }
        ticks = TICKS_UPDATED.wait(ticks);
    }
}

pub fn sys_uptime(
    _p: &'static Proc,
    private: &mut Option<ProcPrivateDataGuard>,
) -> ReturnType<sys::Uptime> {
    let private = private.as_mut().unwrap();
    let Ok(()) = super::decode_arg::<sys::Uptime>(private.trapframe().unwrap());
    *TICKS.lock()
}
