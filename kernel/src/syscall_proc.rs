use crate::{
    proc::{self, Proc, ProcId},
    syscall,
    trap::TICKS,
};

pub fn fork(p: &Proc) -> Result<usize, ()> {
    proc::fork(p).map(|pid| pid.get() as usize).ok_or(())
}

pub fn exit(p: &Proc) -> Result<usize, ()> {
    let n = syscall::arg_int(p, 0);
    proc::exit(p, n as i32);
}

pub fn wait(p: &Proc) -> Result<usize, ()> {
    let addr = syscall::arg_addr(p, 0);
    let pid = proc::wait(p, addr)?;
    Ok(pid.get() as usize)
}

pub fn kill(p: &Proc) -> Result<usize, ()> {
    let pid = syscall::arg_int(p, 0);
    proc::kill(ProcId::new(pid as i32)).map(|()| 0)
}

pub fn getpid(p: &Proc) -> Result<usize, ()> {
    Ok(p.pid().get() as usize)
}

pub fn sbrk(p: &Proc) -> Result<usize, ()> {
    let n = syscall::arg_int(p, 0);
    let addr = p.size();
    proc::grow_proc(p, n as isize)?;
    Ok(addr)
}

pub fn sleep(p: &Proc) -> Result<usize, ()> {
    let n = syscall::arg_int(p, 0) as u64;
    let mut ticks = TICKS.lock();
    let ticks0 = *ticks;
    while *ticks - ticks0 < n {
        let p = Proc::current();
        if p.killed() {
            return Err(());
        }
        proc::sleep((&raw const TICKS).cast(), &mut ticks);
    }
    drop(ticks);
    Ok(0)
}

pub fn uptime(_p: &Proc) -> Result<usize, ()> {
    Ok(*TICKS.lock() as usize)
}
