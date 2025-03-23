use core::convert::Infallible;

use ov6_syscall::{
    Register, RegisterDecodeError, RegisterValue, Syscall, SyscallCode, error::SyscallError,
    syscall,
};

use crate::{
    error::KernelError,
    interrupt::trap::TrapFrame,
    println,
    proc::{Proc, ProcPrivateData, ProcPrivateDataGuard},
};

mod file;
mod proc;
mod system;

trait Arg: Sized {
    type Target;
    type DecodeError;
    fn decode_arg(tf: &TrapFrame) -> Result<Self::Target, Self::DecodeError>;
}

impl<T> Arg for Register<T, 0>
where
    T: RegisterValue<Repr = Self>,
{
    type DecodeError = T::DecodeError;
    type Target = T;

    fn decode_arg(_tf: &TrapFrame) -> Result<Self::Target, Self::DecodeError> {
        Self::new([]).try_decode()
    }
}

impl<T> Arg for Register<T, 1>
where
    T: RegisterValue<Repr = Self>,
{
    type DecodeError = T::DecodeError;
    type Target = T;

    fn decode_arg(tf: &TrapFrame) -> Result<Self::Target, Self::DecodeError> {
        Self::new([tf.a0]).try_decode()
    }
}

impl<T> Arg for Register<T, 2>
where
    T: RegisterValue<Repr = Self>,
{
    type DecodeError = T::DecodeError;
    type Target = T;

    fn decode_arg(tf: &TrapFrame) -> Result<Self::Target, Self::DecodeError> {
        Self::new([tf.a0, tf.a1]).try_decode()
    }
}

impl<T> Arg for Register<T, 3>
where
    T: RegisterValue<Repr = Self>,
{
    type DecodeError = T::DecodeError;
    type Target = T;

    fn decode_arg(tf: &TrapFrame) -> Result<Self::Target, Self::DecodeError> {
        Self::new([tf.a0, tf.a1, tf.a2]).try_decode()
    }
}

impl<T> Arg for Register<T, 4>
where
    T: RegisterValue<Repr = Self>,
{
    type DecodeError = T::DecodeError;
    type Target = T;

    fn decode_arg(tf: &TrapFrame) -> Result<Self::Target, Self::DecodeError> {
        Self::new([tf.a0, tf.a1, tf.a2, tf.a3]).try_decode()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReturnValue {
    Ret0,
    Ret1(usize),
    Ret2(usize, usize),
}

impl<T> From<Register<T, 0>> for ReturnValue {
    fn from(_: Register<T, 0>) -> Self {
        Self::Ret0
    }
}

impl<T> From<Register<T, 1>> for ReturnValue {
    fn from(value: Register<T, 1>) -> Self {
        let [a0] = value.a;
        Self::Ret1(a0)
    }
}

impl<T> From<Register<T, 2>> for ReturnValue {
    fn from(value: Register<T, 2>) -> Self {
        let [a0, a1] = value.a;
        Self::Ret2(a0, a1)
    }
}

impl ReturnValue {
    pub fn store(self, tf: &mut TrapFrame) {
        let (a0, a1) = match self {
            Self::Ret0 => (0, None),
            Self::Ret1(a0) => (a0, None),
            Self::Ret2(a0, a1) => (a0, Some(a1)),
        };
        tf.a0 = a0;
        if let Some(a1) = a1 {
            tf.a1 = a1;
        }
    }
}

trait GetTrapFrame {
    fn get_trapframe(&self) -> &TrapFrame;
}

impl GetTrapFrame for Option<ProcPrivateDataGuard<'_>> {
    fn get_trapframe(&self) -> &TrapFrame {
        self.as_ref().unwrap().trapframe()
    }
}

impl GetTrapFrame for ProcPrivateData {
    fn get_trapframe(&self) -> &TrapFrame {
        self.trapframe()
    }
}

trait IntoReturn<T> {
    fn into_return(self) -> T;
}

impl<T> IntoReturn<T> for Infallible {
    fn into_return(self) -> T {
        match self {}
    }
}

impl<T> IntoReturn<Result<T, SyscallError>> for RegisterDecodeError {
    fn into_return(self) -> Result<T, SyscallError> {
        Err(KernelError::from(self).into())
    }
}

trait SyscallExt: Syscall {
    type Private<'a>: GetTrapFrame;
    type KernelArg: RegisterValue;
    type KernelReturn: RegisterValue;

    fn handle(p: &'static Proc, private: &mut Self::Private<'_>) -> ReturnValue
    where
        <Self::KernelArg as RegisterValue>::Repr: Arg<Target = Self::KernelArg>,
        <<Self::KernelArg as RegisterValue>::Repr as Arg>::DecodeError:
            IntoReturn<Self::KernelReturn>,
        <Self::KernelReturn as RegisterValue>::Repr: Into<ReturnValue>,
    {
        let arg = <Self::KernelArg as RegisterValue>::Repr::decode_arg(private.get_trapframe());
        let ret = match arg {
            Ok(arg) => Self::call(p, private, arg),
            Err(e) => e.into_return(),
        };
        ret.encode().into()
    }

    fn call(
        p: &'static Proc,
        private: &mut Self::Private<'_>,
        arg: Self::KernelArg,
    ) -> Self::KernelReturn;
}

pub fn syscall(p: &'static Proc, private_opt: &mut Option<ProcPrivateDataGuard>) {
    let private = private_opt.as_mut().unwrap();
    let n = private.trapframe().a7;
    let Some(ty) = SyscallCode::from_repr(n) else {
        let shared = p.shared().lock();
        let pid = shared.pid();
        let name = shared.name().display();
        println!("{pid} {name}: unknown sys call {n}");
        private.trapframe_mut().a0 = usize::MAX;
        return;
    };

    let ret = match ty {
        SyscallCode::Fork => syscall::Fork::handle(p, private),
        SyscallCode::Exit => syscall::Exit::handle(p, private_opt),
        SyscallCode::Wait => syscall::Wait::handle(p, private),
        SyscallCode::Pipe => syscall::Pipe::handle(p, private),
        SyscallCode::Read => syscall::Read::handle(p, private),
        SyscallCode::Kill => syscall::Kill::handle(p, private),
        SyscallCode::Exec => syscall::Exec::handle(p, private),
        SyscallCode::Fstat => syscall::Fstat::handle(p, private),
        SyscallCode::Chdir => syscall::Chdir::handle(p, private),
        SyscallCode::Dup => syscall::Dup::handle(p, private),
        SyscallCode::Getpid => syscall::Getpid::handle(p, private),
        SyscallCode::Sbrk => syscall::Sbrk::handle(p, private),
        SyscallCode::Sleep => syscall::Sleep::handle(p, private),
        SyscallCode::Open => syscall::Open::handle(p, private),
        SyscallCode::Write => syscall::Write::handle(p, private),
        SyscallCode::Mknod => syscall::Mknod::handle(p, private),
        SyscallCode::Unlink => syscall::Unlink::handle(p, private),
        SyscallCode::Link => syscall::Link::handle(p, private),
        SyscallCode::Mkdir => syscall::Mkdir::handle(p, private),
        SyscallCode::Close => syscall::Close::handle(p, private),
        SyscallCode::Reboot => syscall::Reboot::handle(p, private),
        SyscallCode::Halt => syscall::Halt::handle(p, private),
        SyscallCode::Abort => syscall::Abort::handle(p, private),
    };

    let private = private_opt.as_mut().unwrap();
    ret.store(private.trapframe_mut());
}
