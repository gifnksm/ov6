use ov6_syscall::syscall;

use super::SyscallExt;
use crate::{
    device::test::{self, Finisher},
    error::KernelError,
    interrupt::timer::TICKS,
    proc::{Proc, ProcPrivateData},
};

impl SyscallExt for syscall::Uptime {
    type Private<'a> = ProcPrivateData;

    fn handle(_p: &'static Proc, private: &mut Self::Private<'_>) -> Self::Return {
        let Ok(()) = Self::decode_arg(private.trapframe());
        *TICKS.lock()
    }
}

impl SyscallExt for syscall::Reboot {
    type Private<'a> = ProcPrivateData;

    fn handle(_p: &'static crate::proc::Proc, private: &mut Self::Private<'_>) -> Self::Return {
        let Ok(()) = Self::decode_arg(private.trapframe());
        crate::println!("ov6 - reboot requested");
        test::finish(Finisher::Reset);
    }
}

impl SyscallExt for syscall::Halt {
    type Private<'a> = ProcPrivateData;

    fn handle(_p: &'static crate::proc::Proc, private: &mut Self::Private<'_>) -> Self::Return {
        let (code,) = Self::decode_arg(private.trapframe()).map_err(KernelError::from)?;
        crate::println!("ov6 - halt requested");
        test::finish(Finisher::Pass(code));
    }
}

impl SyscallExt for syscall::Abort {
    type Private<'a> = ProcPrivateData;

    fn handle(_p: &'static crate::proc::Proc, private: &mut Self::Private<'_>) -> Self::Return {
        let (code,) = Self::decode_arg(private.trapframe()).map_err(KernelError::from)?;
        crate::println!("ov6 - abort requested");
        test::finish(Finisher::Fail(code));
    }
}
