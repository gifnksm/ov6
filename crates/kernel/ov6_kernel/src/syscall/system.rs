use ov6_syscall::syscall;

use super::SyscallExt;
use crate::{
    device::test::{self, Finisher},
    proc::ProcPrivateData,
};

impl SyscallExt for syscall::Reboot {
    type KernelArg = Self::Arg;
    type KernelReturn = Self::Return;
    type Private<'a> = ProcPrivateData;

    fn call(
        _p: &'static crate::proc::Proc,
        _private: &mut Self::Private<'_>,
        (): Self::Arg,
    ) -> Self::Return {
        crate::println!("ov6 - reboot requested");
        test::finish(Finisher::Reset);
    }
}

impl SyscallExt for syscall::Halt {
    type KernelArg = Self::Arg;
    type KernelReturn = Self::Return;
    type Private<'a> = ProcPrivateData;

    fn call(
        _p: &'static crate::proc::Proc,
        _private: &mut Self::Private<'_>,
        (code,): Self::Arg,
    ) -> Self::Return {
        crate::println!("ov6 - halt requested");
        test::finish(Finisher::Pass(code));
    }
}

impl SyscallExt for syscall::Abort {
    type KernelArg = Self::Arg;
    type KernelReturn = Self::Return;
    type Private<'a> = ProcPrivateData;

    fn call(
        _p: &'static crate::proc::Proc,
        _private: &mut Self::Private<'_>,
        (code,): Self::Arg,
    ) -> Self::Return {
        crate::println!("ov6 - abort requested");
        test::finish(Finisher::Fail(code));
    }
}
