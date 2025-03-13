use core::convert::Infallible;

use ov6_syscall::{
    ArgTypeRepr, Register, RegisterValue, ReturnType, Syscall, SyscallCode, syscall as sys,
};

use crate::{
    println,
    proc::{Proc, ProcPrivateDataGuard, TrapFrame},
};

mod file;
mod proc;

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
        SyscallCode::Fork => sys::Fork::handle(p, private).encode().into(),
        SyscallCode::Exit => {
            let _: Infallible = sys::Exit::handle(p, private_opt);
            unreachable!()
        }
        SyscallCode::Wait => sys::Wait::handle(p, private).encode().into(),
        SyscallCode::Pipe => sys::Pipe::handle(p, private).encode().into(),
        SyscallCode::Read => sys::Read::handle(p, private).encode().into(),
        SyscallCode::Kill => sys::Kill::handle(p, private).encode().into(),
        SyscallCode::Exec => match self::file::sys_exec(p, private) {
            Ok((argc, argv)) => ReturnValue::Ret2(argc, argv),
            Err(e) => ReturnType::<sys::Exec>::Err(e.into()).encode().into(),
        },
        SyscallCode::Fstat => sys::Fstat::handle(p, private).encode().into(),
        SyscallCode::Chdir => sys::Chdir::handle(p, private).encode().into(),
        SyscallCode::Dup => sys::Dup::handle(p, private).encode().into(),
        SyscallCode::Getpid => sys::Getpid::handle(p, private).encode().into(),
        SyscallCode::Sbrk => sys::Sbrk::handle(p, private).encode().into(),
        SyscallCode::Sleep =>
        {
            #[expect(clippy::unit_arg)]
            sys::Sleep::handle(p, private).encode().into()
        }
        SyscallCode::Uptime => sys::Uptime::handle(p, private).encode().into(),
        SyscallCode::Open => sys::Open::handle(p, private).encode().into(),
        SyscallCode::Write => sys::Write::handle(p, private).encode().into(),
        SyscallCode::Mknod => sys::Mknod::handle(p, private).encode().into(),
        SyscallCode::Unlink => sys::Unlink::handle(p, private).encode().into(),
        SyscallCode::Link => sys::Link::handle(p, private).encode().into(),
        SyscallCode::Mkdir => sys::Mkdir::handle(p, private).encode().into(),
        SyscallCode::Close => sys::Close::handle(p, private).encode().into(),
    };

    let private = private_opt.as_mut().unwrap();
    ret.store(private.trapframe_mut());
}
