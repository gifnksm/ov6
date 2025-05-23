use core::convert::Infallible;

use ov6_syscall::{Register, RegisterValue, Syscall, error::SyscallError, syscall};

use super::SyscallExt;
use crate::{
    error::KernelError,
    interrupt::timer::{NANOS_PER_TICKS, TICKS, TICKS_UPDATED},
    memory::{VirtAddr, addr::Validate as _},
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

impl SyscallExt for syscall::Sbrk {
    type KernelArg = Self::Arg;
    type KernelReturn = Self::Return;
    type Private<'a> = ProcPrivateData;

    fn call(
        _p: &'static Proc,
        private: &mut Self::Private<'_>,
        (increment,): Self::Arg,
    ) -> Self::Return {
        let pb = private.program_break();
        proc::ops::resize_by(private, increment)?;
        Ok(pb.addr())
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

impl SyscallExt for syscall::AlarmSet {
    type KernelArg = Self::Arg;
    type KernelReturn = Self::Return;
    type Private<'a> = ProcPrivateData;

    fn call(
        p: &'static Proc,
        _private: &mut Self::Private<'_>,
        (dur, handler): Self::KernelArg,
    ) -> Self::KernelReturn {
        let handler = VirtAddr::new(handler.addr())?;
        let mut shared = p.shared().lock();
        shared.set_alarm(dur, handler);
        Ok(())
    }
}

impl SyscallExt for syscall::AlarmClear {
    type KernelArg = Self::Arg;
    type KernelReturn = Self::Return;
    type Private<'a> = ProcPrivateData;

    fn call(
        p: &'static Proc,
        _private: &mut Self::Private<'_>,
        (): Self::KernelArg,
    ) -> Self::KernelReturn {
        let mut shared = p.shared().lock();
        shared.clear_alarm();
        Ok(())
    }
}

#[derive(Debug)]
pub(super) enum SignalReturn {
    Ok(usize, usize),
    Err(SyscallError),
}

impl RegisterValue for SignalReturn {
    type DecodeError = Infallible;
    type Repr = Register<Self, 2>;

    fn encode(self) -> Self::Repr {
        match self {
            Self::Ok(a0, a1) => Register::new([a0, a1]),
            Self::Err(e) => {
                let [a0, a1] = <syscall::Exec as Syscall>::Return::Err(e).encode().a;
                Register::new([a0, a1])
            }
        }
    }

    fn try_decode(_repr: Self::Repr) -> Result<Self, Self::DecodeError> {
        unreachable!()
    }
}

impl SyscallExt for syscall::SignalReturn {
    type KernelArg = Self::Arg;
    type KernelReturn = SignalReturn;
    type Private<'a> = ProcPrivateData;

    fn call(
        _p: &'static Proc,
        private: &mut Self::Private<'_>,
        (): Self::KernelArg,
    ) -> Self::KernelReturn {
        match private.exit_from_signal_handler() {
            Ok(()) => {
                let ur = private.trapframe().user_registers;
                SignalReturn::Ok(ur.a0, ur.a1)
            }
            Err(e) => SignalReturn::Err(e.into()),
        }
    }
}

impl SyscallExt for syscall::Trace {
    type KernelArg = Self::Arg;
    type KernelReturn = Self::Return;
    type Private<'a> = ProcPrivateData;

    fn call(
        _p: &'static Proc,
        private: &mut Self::Private<'_>,
        (trace_mask,): Self::KernelArg,
    ) -> Self::KernelReturn {
        private.set_trace_mask(trace_mask);
    }
}

impl SyscallExt for syscall::DumpUserPageTable {
    type KernelArg = Self::Arg;
    type KernelReturn = Self::Return;
    type Private<'a> = ProcPrivateData;

    fn call(
        _p: &'static Proc,
        private: &mut Self::Private<'_>,
        (): Self::KernelArg,
    ) -> Self::KernelReturn {
        private.pagetable().dump();
    }
}
