use ov6_syscall::syscall as sys;

use super::SyscallExt;
use crate::{
    error::KernelError,
    interrupt::timer::{TICKS, TICKS_UPDATED},
    proc::{self, Proc, ProcPrivateData, ProcPrivateDataGuard},
};

impl SyscallExt for sys::Fork {
    type Private<'a> = ProcPrivateData;

    fn handle(p: &'static Proc, private: &mut Self::Private<'_>) -> Self::Return {
        let Ok(()) = Self::decode_arg(private.trapframe());
        let pid = proc::fork(p, private)?;
        Ok(Some(pid))
    }
}

impl SyscallExt for sys::Exit {
    type Private<'a> = Option<ProcPrivateDataGuard<'a>>;

    fn handle(p: &'static Proc, private: &mut Self::Private<'_>) -> Self::Return {
        let private = private.take().unwrap();
        let status = match Self::decode_arg(private.trapframe()) {
            Ok((status,)) => status,
            Err(_e) => -1,
        };
        proc::exit(p, private, status);
    }
}

impl SyscallExt for sys::Wait {
    type Private<'a> = ProcPrivateData;

    fn handle(p: &'static Proc, private: &mut Self::Private<'_>) -> Self::Return {
        let Ok((addr,)) = Self::decode_arg(private.trapframe());
        let pid = proc::wait(p, private, addr)?;
        Ok(pid)
    }
}

impl SyscallExt for sys::Kill {
    type Private<'a> = ProcPrivateData;

    fn handle(_p: &'static Proc, private: &mut Self::Private<'_>) -> Self::Return {
        let (pid,) = Self::decode_arg(private.trapframe()).map_err(KernelError::from)?;
        proc::kill(pid)?;
        Ok(())
    }
}

impl SyscallExt for sys::Getpid {
    type Private<'a> = ProcPrivateData;

    fn handle(p: &'static Proc, private: &mut Self::Private<'_>) -> Self::Return {
        let Ok(()) = Self::decode_arg(private.trapframe());
        p.shared().lock().pid()
    }
}

impl SyscallExt for sys::Sbrk {
    type Private<'a> = ProcPrivateData;

    fn handle(_p: &'static Proc, private: &mut Self::Private<'_>) -> Self::Return {
        let Ok((increment,)) = Self::decode_arg(private.trapframe());
        let addr = private.size();
        proc::grow_proc(private, increment)?;
        Ok(addr)
    }
}

impl SyscallExt for sys::Sleep {
    type Private<'a> = ProcPrivateData;

    fn handle(p: &'static Proc, private: &mut Self::Private<'_>) -> Self::Return {
        let Ok((dur,)) = Self::decode_arg(private.trapframe());
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
}

impl SyscallExt for sys::Uptime {
    type Private<'a> = ProcPrivateData;

    fn handle(_p: &'static Proc, private: &mut Self::Private<'_>) -> Self::Return {
        let Ok(()) = Self::decode_arg(private.trapframe());
        *TICKS.lock()
    }
}
