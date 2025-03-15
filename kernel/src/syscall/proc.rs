use ov6_syscall::syscall as sys;

use super::SyscallExt;
use crate::{
    error::KernelError,
    interrupt::timer::{TICKS, TICKS_UPDATED},
    proc::{self, Proc, ProcPrivateData, ProcPrivateDataGuard},
    sync::WaitError,
};

impl SyscallExt for sys::Fork {
    type Private<'a> = ProcPrivateData;

    fn handle(p: &'static Proc, private: &mut Self::Private<'_>) -> Self::Return {
        let Ok(()) = Self::decode_arg(private.trapframe());
        let pid = proc::ops::fork(p, private)?;
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
        proc::ops::exit(p, private, status);
    }
}

impl SyscallExt for sys::Wait {
    type Private<'a> = ProcPrivateData;

    fn handle(p: &'static Proc, private: &mut Self::Private<'_>) -> Self::Return {
        let Ok((mut user_status,)) = Self::decode_arg(private.trapframe());
        let (pid, status) = proc::ops::wait(p)?;
        // TODO: more reliable check
        if user_status.addr() != 0 {
            private
                .pagetable_mut()
                .copy_out(&mut user_status, &status)?;
        }
        Ok(pid)
    }
}

impl SyscallExt for sys::Kill {
    type Private<'a> = ProcPrivateData;

    fn handle(_p: &'static Proc, private: &mut Self::Private<'_>) -> Self::Return {
        let (pid,) = Self::decode_arg(private.trapframe()).map_err(KernelError::from)?;
        proc::ops::kill(pid)?;
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
        proc::ops::resize_by(private, increment)?;
        Ok(addr)
    }
}

impl SyscallExt for sys::Sleep {
    type Private<'a> = ProcPrivateData;

    fn handle(_p: &'static Proc, private: &mut Self::Private<'_>) -> Self::Return {
        let Ok((dur,)) = Self::decode_arg(private.trapframe());
        let mut ticks = TICKS.lock();
        let ticks0 = *ticks;
        while *ticks - ticks0 < dur {
            ticks = match TICKS_UPDATED.wait(ticks) {
                Ok(ticks) => ticks,
                Err((_ticsk, WaitError::WaitingProcessAlreadyKilled)) => return,
            }
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
