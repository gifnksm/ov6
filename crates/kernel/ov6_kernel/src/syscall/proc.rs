use core::convert::Infallible;

use ov6_syscall::{Register, RegisterValue, syscall};

use super::SyscallExt;
use crate::{
    error::KernelError,
    interrupt::timer::{NANOS_PER_TICKS, TICKS, TICKS_UPDATED},
    memory::addr::Validate as _,
    proc::{self, Proc, ProcPrivateData, ProcPrivateDataGuard},
    sync::WaitError,
};

impl SyscallExt for syscall::Fork {
    type KernelArg = Self::Arg;
    type KernelReturn = Self::Return;
    type Private<'a> = ProcPrivateData;

    fn call(p: &'static Proc, private: &mut Self::Private<'_>, (): Self::Arg) -> Self::Return {
        let pid = proc::ops::fork(p, private)?;
        Ok(Some(pid))
    }
}

#[derive(Debug)]
pub(super) struct ExitArg(i32);

impl RegisterValue for ExitArg {
    type DecodeError = Infallible;
    type Repr = Register<Self, 1>;

    fn encode(self) -> Self::Repr {
        unreachable!()
    }

    fn try_decode(repr: Self::Repr) -> Result<Self, Self::DecodeError> {
        Ok(i32::try_decode(Register::new(repr.a)).map_or(Self(-1), Self))
    }
}

impl SyscallExt for syscall::Exit {
    type KernelArg = ExitArg;
    type KernelReturn = Self::Return;
    type Private<'a> = Option<ProcPrivateDataGuard<'a>>;

    fn call(
        p: &'static Proc,
        private: &mut Self::Private<'_>,
        ExitArg(status): Self::KernelArg,
    ) -> Self::Return {
        let private = private.take().unwrap();
        proc::ops::exit(p, private, status);
    }
}

impl SyscallExt for syscall::Wait {
    type KernelArg = Self::Arg;
    type KernelReturn = Self::Return;
    type Private<'a> = ProcPrivateData;

    fn call(
        p: &'static Proc,
        private: &mut Self::Private<'_>,
        (target, user_status): Self::Arg,
    ) -> Self::Return {
        let mut user_status = user_status.validate(private.pagetable())?;

        let (pid, status) = proc::ops::wait(p, target)?;
        private.pagetable_mut().copy_k2u(&mut user_status, &status);
        Ok(pid)
    }
}

impl SyscallExt for syscall::Kill {
    type KernelArg = Self::Arg;
    type KernelReturn = Self::Return;
    type Private<'a> = ProcPrivateData;

    fn call(
        _p: &'static Proc,
        _private: &mut Self::Private<'_>,
        (pid,): Self::Arg,
    ) -> Self::Return {
        proc::ops::kill(pid)?;
        Ok(())
    }
}

impl SyscallExt for syscall::Getpid {
    type KernelArg = Self::Arg;
    type KernelReturn = Self::Return;
    type Private<'a> = ProcPrivateData;

    fn call(p: &'static Proc, _private: &mut Self::Private<'_>, (): Self::Arg) -> Self::Return {
        p.shared().lock().pid()
    }
}

impl SyscallExt for syscall::Sbrk {
    type KernelArg = Self::Arg;
    type KernelReturn = Self::Return;
    type Private<'a> = ProcPrivateData;

    fn call(
        _p: &'static Proc,
        private: &mut Self::Private<'_>,
        (increment,): Self::Arg,
    ) -> Self::Return {
        let addr = private.size();
        proc::ops::resize_by(private, increment)?;
        Ok(addr)
    }
}

impl SyscallExt for syscall::Sleep {
    type KernelArg = Self::Arg;
    type KernelReturn = Self::Return;
    type Private<'a> = ProcPrivateData;

    fn call(
        _p: &'static Proc,
        _private: &mut Self::Private<'_>,
        (dur,): Self::Arg,
    ) -> Self::Return {
        let sleep_ticks = (dur.as_nanos().div_ceil(u128::from(NANOS_PER_TICKS)))
            .try_into()
            .unwrap_or(u64::MAX);

        let mut ticks = TICKS.lock();
        let ticks0 = *ticks;
        let end_ticks = ticks0.saturating_add(sleep_ticks);
        while *ticks < end_ticks {
            ticks = TICKS_UPDATED.wait(ticks).map_err(
                |(_ticks, WaitError::WaitingProcessAlreadyKilled)| {
                    KernelError::CallerProcessAlreadyKilled
                },
            )?;
        }

        Ok(())
    }
}
