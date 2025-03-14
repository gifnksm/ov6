use ov6_syscall::syscall;

use super::SyscallExt;
use crate::{
    error::KernelError,
    interrupt::timer::{NANOS_PER_TICKS, TICKS, TICKS_UPDATED},
    proc::{self, Proc, ProcPrivateData, ProcPrivateDataGuard},
    sync::WaitError,
};

impl SyscallExt for syscall::Fork {
    type Private<'a> = ProcPrivateData;

    fn handle(p: &'static Proc, private: &mut Self::Private<'_>) -> Self::Return {
        let Ok(()) = Self::decode_arg(private.trapframe());
        let pid = proc::ops::fork(p, private)?;
        Ok(Some(pid))
    }
}

impl SyscallExt for syscall::Exit {
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

impl SyscallExt for syscall::Wait {
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

impl SyscallExt for syscall::Kill {
    type Private<'a> = ProcPrivateData;

    fn handle(_p: &'static Proc, private: &mut Self::Private<'_>) -> Self::Return {
        let (pid,) = Self::decode_arg(private.trapframe()).map_err(KernelError::from)?;
        proc::ops::kill(pid)?;
        Ok(())
    }
}

impl SyscallExt for syscall::Getpid {
    type Private<'a> = ProcPrivateData;

    fn handle(p: &'static Proc, private: &mut Self::Private<'_>) -> Self::Return {
        let Ok(()) = Self::decode_arg(private.trapframe());
        p.shared().lock().pid()
    }
}

impl SyscallExt for syscall::Sbrk {
    type Private<'a> = ProcPrivateData;

    fn handle(_p: &'static Proc, private: &mut Self::Private<'_>) -> Self::Return {
        let Ok((increment,)) = Self::decode_arg(private.trapframe());
        let addr = private.size();
        proc::ops::resize_by(private, increment)?;
        Ok(addr)
    }
}

impl SyscallExt for syscall::Sleep {
    type Private<'a> = ProcPrivateData;

    fn handle(_p: &'static Proc, private: &mut Self::Private<'_>) -> Self::Return {
        let Ok((dur,)) = Self::decode_arg(private.trapframe()) else {
            return;
        };

        let sleep_ticks = (dur.as_nanos().div_ceil(u128::from(NANOS_PER_TICKS)))
            .try_into()
            .unwrap_or(u64::MAX);

        let mut ticks = TICKS.lock();
        let ticks0 = *ticks;
        let end_ticks = ticks0.saturating_add(sleep_ticks);
        while *ticks < end_ticks {
            ticks = match TICKS_UPDATED.wait(ticks) {
                Ok(ticks) => ticks,
                Err((_ticsk, WaitError::WaitingProcessAlreadyKilled)) => return,
            }
        }
    }
}
