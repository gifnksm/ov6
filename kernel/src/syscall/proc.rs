use crate::{
    error::Error,
    interrupt::trap::TICKS,
    proc::{self, Proc, ProcId, ProcPrivateData},
    syscall,
};

pub fn sys_fork(p: &Proc, private: &mut ProcPrivateData) -> Result<usize, Error> {
    proc::fork(p, private)
        .map(|pid| pid.get() as usize)
        .ok_or(Error::Unknown)
}

pub fn sys_exit(p: &Proc, private: &mut ProcPrivateData) -> Result<usize, Error> {
    let n = syscall::arg_int(private, 0);
    proc::exit(p, private, n as i32);
}

pub fn sys_wait(p: &Proc, private: &mut ProcPrivateData) -> Result<usize, Error> {
    let addr = syscall::arg_addr(private, 0);
    let pid = proc::wait(p, private, addr)?;
    Ok(pid.get() as usize)
}

pub fn sys_kill(_p: &Proc, private: &mut ProcPrivateData) -> Result<usize, Error> {
    let pid = syscall::arg_int(private, 0);
    proc::kill(ProcId::new(pid as i32)).map(|()| 0)
}

pub fn sys_getpid(p: &Proc, _private: &mut ProcPrivateData) -> Result<usize, Error> {
    let pid = p.shared().lock().pid();
    Ok(pid.get() as usize)
}

pub fn sys_sbrk(_p: &Proc, private: &mut ProcPrivateData) -> Result<usize, Error> {
    let n = syscall::arg_int(private, 0);
    let addr = private.size();
    proc::grow_proc(private, n as isize)?;
    Ok(addr)
}

pub fn sys_sleep(p: &Proc, private: &mut ProcPrivateData) -> Result<usize, Error> {
    let n = syscall::arg_int(private, 0) as u64;
    let mut ticks = TICKS.lock();
    let ticks0 = *ticks;
    while *ticks - ticks0 < n {
        if p.shared().lock().killed() {
            return Err(Error::Unknown);
        }
        ticks = proc::sleep((&raw const TICKS).cast(), ticks);
    }
    drop(ticks);
    Ok(0)
}

pub fn sys_uptime(_p: &Proc, _private: &mut ProcPrivateData) -> Result<usize, Error> {
    Ok(*TICKS.lock() as usize)
}
