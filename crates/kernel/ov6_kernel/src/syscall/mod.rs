use core::convert::Infallible;

use ov6_syscall::{
    ArgTypeRepr, Register, RegisterValue, ReturnType, Syscall, SyscallCode, syscall,
};

use crate::{
    interrupt::trap::TrapFrame,
    println,
    proc::{Proc, ProcPrivateDataGuard},
};

mod file;
mod proc;
mod system;

fn decode_arg<S>(
    tf: &TrapFrame,
) -> Result<<ArgTypeRepr<S> as Arg>::Target, <ArgTypeRepr<S> as Arg>::DecodeError>
where
    S: Syscall + ?Sized,
    ArgTypeRepr<S>: Arg,
{
    ArgTypeRepr::<S>::decode_arg(tf)
}

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

trait SyscallExt: Syscall {
    type Private<'a>;

    fn decode_arg(
        tf: &TrapFrame,
    ) -> Result<<ArgTypeRepr<Self> as Arg>::Target, <ArgTypeRepr<Self> as Arg>::DecodeError>
    where
        ArgTypeRepr<Self>: Arg,
    {
        ArgTypeRepr::<Self>::decode_arg(tf)
    }

    fn handle(p: &'static Proc, private: &mut Self::Private<'_>) -> Self::Return;
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
        SyscallCode::Fork => syscall::Fork::handle(p, private).encode().into(),
        SyscallCode::Exit => {
            let _: Infallible = syscall::Exit::handle(p, private_opt);
            unreachable!()
        }
        SyscallCode::Wait => syscall::Wait::handle(p, private).encode().into(),
        SyscallCode::Pipe => syscall::Pipe::handle(p, private).encode().into(),
        SyscallCode::Read => syscall::Read::handle(p, private).encode().into(),
        SyscallCode::Kill => syscall::Kill::handle(p, private).encode().into(),
        SyscallCode::Exec => match self::file::sys_exec(p, private) {
            Ok((argc, argv)) => ReturnValue::Ret2(argc, argv),
            Err(e) => ReturnType::<syscall::Exec>::Err(e.into()).encode().into(),
        },
        SyscallCode::Fstat => syscall::Fstat::handle(p, private).encode().into(),
        SyscallCode::Chdir => syscall::Chdir::handle(p, private).encode().into(),
        SyscallCode::Dup => syscall::Dup::handle(p, private).encode().into(),
        SyscallCode::Getpid => syscall::Getpid::handle(p, private).encode().into(),
        SyscallCode::Sbrk => syscall::Sbrk::handle(p, private).encode().into(),
        SyscallCode::Sleep => syscall::Sleep::handle(p, private).encode().into(),
        SyscallCode::Open => syscall::Open::handle(p, private).encode().into(),
        SyscallCode::Write => syscall::Write::handle(p, private).encode().into(),
        SyscallCode::Mknod => syscall::Mknod::handle(p, private).encode().into(),
        SyscallCode::Unlink => syscall::Unlink::handle(p, private).encode().into(),
        SyscallCode::Link => syscall::Link::handle(p, private).encode().into(),
        SyscallCode::Mkdir => syscall::Mkdir::handle(p, private).encode().into(),
        SyscallCode::Close => syscall::Close::handle(p, private).encode().into(),
        SyscallCode::Reboot => syscall::Reboot::handle(p, private).encode().into(),
        SyscallCode::Halt => syscall::Halt::handle(p, private).encode().into(),
        SyscallCode::Abort => syscall::Abort::handle(p, private).encode().into(),
    };

    let private = private_opt.as_mut().unwrap();
    ret.store(private.trapframe_mut());
}
