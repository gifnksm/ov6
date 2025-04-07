use ov6_syscall::{SystemInfo, syscall};

use super::SyscallExt;
use crate::{
    device::test::{self, Finisher},
    memory::{self, addr::Validate as _, vm_kernel},
    proc::ProcPrivateData,
};

impl SyscallExt for syscall::GetSystemInfo {
    type KernelArg = Self::Arg;
    type KernelReturn = Self::Return;
    type Private<'a> = ProcPrivateData;

    fn call(
        _p: &'static crate::proc::Proc,
        private: &mut Self::Private<'_>,
        (user_sysinfo,): Self::KernelArg,
    ) -> Self::KernelReturn {
        let mut user_sysinfo = user_sysinfo.validate(private.pagetable_mut())?;
        let sysinfo = SystemInfo {
            memory: memory::info(),
        };
        private
            .pagetable_mut()
            .copy_k2u(&mut user_sysinfo, &sysinfo);
        Ok(())
    }
}

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

impl SyscallExt for syscall::DumpKernelPageTable {
    type KernelArg = Self::Arg;
    type KernelReturn = Self::Return;
    type Private<'a> = ProcPrivateData;

    fn call(
        _p: &'static crate::proc::Proc,
        _private: &mut Self::Private<'_>,
        (): Self::KernelArg,
    ) -> Self::KernelReturn {
        vm_kernel::dump();
    }
}
