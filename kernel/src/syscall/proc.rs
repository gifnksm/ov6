use crate::{
    error::Error,
    interrupt::trap::TICKS,
    proc::{self, Proc, ProcId},
    syscall,
};

pub fn sys_fork(p: &Proc) -> Result<usize, Error> {
    proc::fork(p)
        .map(|pid| pid.get() as usize)
        .ok_or(Error::Unknown)
}

pub fn sys_exit(p: &Proc) -> Result<usize, Error> {
    let n = syscall::arg_int(p, 0);
    proc::exit(p, n as i32);
}

pub fn sys_wait(p: &Proc) -> Result<usize, Error> {
    let addr = syscall::arg_addr(p, 0);
    let pid = proc::wait(p, addr)?;
    Ok(pid.get() as usize)
}

pub fn sys_kill(p: &Proc) -> Result<usize, Error> {
    let pid = syscall::arg_int(p, 0);
    proc::kill(ProcId::new(pid as i32)).map(|()| 0)
}

pub fn sys_getpid(p: &Proc) -> Result<usize, Error> {
    Ok(p.pid().get() as usize)
}

pub fn sys_sbrk(p: &Proc) -> Result<usize, Error> {
    let n = syscall::arg_int(p, 0);
    let addr = p.size();
    proc::grow_proc(p, n as isize)?;
    Ok(addr)
}

pub fn sys_sleep(p: &Proc) -> Result<usize, Error> {
    let n = syscall::arg_int(p, 0) as u64;
    let mut ticks = TICKS.lock();
    let ticks0 = *ticks;
    while *ticks - ticks0 < n {
        let p = Proc::current();
        if p.killed() {
            return Err(Error::Unknown);
        }
        proc::sleep((&raw const TICKS).cast(), &mut ticks);
    }
    drop(ticks);
    Ok(0)
}

pub fn sys_uptime(_p: &Proc) -> Result<usize, Error> {
    Ok(*TICKS.lock() as usize)
}
