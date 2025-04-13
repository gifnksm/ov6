use ov6_syscall::syscall;

use super::SyscallExt;
use crate::{
    memory::addr::Validate as _,
    net::{self, udp},
    proc::ProcPrivateData,
};

impl SyscallExt for syscall::Bind {
    type KernelArg = Self::Arg;
    type KernelReturn = Self::Return;
    type Private<'a> = ProcPrivateData;

    fn call(
        _p: &'static crate::proc::Proc,
        _private: &mut Self::Private<'_>,
        (port,): Self::KernelArg,
    ) -> Self::KernelReturn {
        udp::bind(port)?;
        Ok(())
    }
}

impl SyscallExt for syscall::Unbind {
    type KernelArg = Self::Arg;
    type KernelReturn = Self::Return;
    type Private<'a> = ProcPrivateData;

    fn call(
        _p: &'static crate::proc::Proc,
        _private: &mut Self::Private<'_>,
        (port,): Self::KernelArg,
    ) -> Self::KernelReturn {
        udp::unbind(port)?;
        Ok(())
    }
}

impl SyscallExt for syscall::Recv {
    type KernelArg = Self::Arg;
    type KernelReturn = Self::Return;
    type Private<'a> = ProcPrivateData;

    fn call(
        _p: &'static crate::proc::Proc,
        private: &mut Self::Private<'_>,
        (port, user_src, user_bytes): Self::KernelArg,
    ) -> Self::KernelReturn {
        let pt = private.pagetable_mut();
        let mut user_src = user_src.validate(pt)?;
        let user_bytes = user_bytes.validate(pt)?;

        let (len, src) = udp::recv_from(port, &mut (&mut *pt, user_bytes).into())?;
        pt.copy_k2u(&mut user_src, &src.into());
        Ok(len)
    }
}

impl SyscallExt for syscall::Send {
    type KernelArg = Self::Arg;
    type KernelReturn = Self::Return;
    type Private<'a> = ProcPrivateData;

    fn call(
        _p: &'static crate::proc::Proc,
        private: &mut Self::Private<'_>,
        (src_port, dst, bytes): Self::KernelArg,
    ) -> Self::KernelReturn {
        let pt = private.pagetable();
        let bytes = bytes.validate(pt)?;
        let sent = net::udp::send(src_port, dst, &(pt, bytes).into())?;
        Ok(sent)
    }
}
