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
    S: Syscall,
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

fn call<T>(
    f: fn(p: &'static Proc, private: &mut Option<ProcPrivateDataGuard>) -> T,
    p: &'static Proc,
    private: &mut Option<ProcPrivateDataGuard>,
) -> ReturnValue
where
    T: RegisterValue,
    T::Repr: Into<ReturnValue>,
{
    f(p, private).encode().into()
}

pub fn syscall(p: &'static Proc, private: &mut Option<ProcPrivateDataGuard>) {
    let private_ref = private.as_mut().unwrap();
    let n = private_ref.trapframe().unwrap().a7;
    let Some(ty) = SyscallCode::from_repr(n) else {
        let shared = p.shared().lock();
        let pid = shared.pid();
        let name = shared.name().display();
        println!("{pid} {name}: unknown sys call {n}");
        private_ref.trapframe_mut().unwrap().a0 = usize::MAX;
        return;
    };
    let _ = private_ref;

    let ret = match ty {
        SyscallCode::Fork => call(self::proc::sys_fork, p, private),
        SyscallCode::Exit => call(self::proc::sys_exit, p, private),
        SyscallCode::Wait => call(self::proc::sys_wait, p, private),
        SyscallCode::Pipe => call(self::file::sys_pipe, p, private),
        SyscallCode::Read => call(self::file::sys_read, p, private),
        SyscallCode::Kill => call(self::proc::sys_kill, p, private),
        SyscallCode::Exec => match self::file::sys_exec(p, private) {
            Ok((argc, argv)) => ReturnValue::Ret2(argc, argv),
            Err(e) => ReturnType::<sys::Exec>::Err(e.into()).encode().into(),
        },
        SyscallCode::Fstat => call(self::file::sys_fstat, p, private),
        SyscallCode::Chdir => call(self::file::sys_chdir, p, private),
        SyscallCode::Dup => call(self::file::sys_dup, p, private),
        SyscallCode::Getpid => call(self::proc::sys_getpid, p, private),
        SyscallCode::Sbrk => call(self::proc::sys_sbrk, p, private),
        SyscallCode::Sleep => call(self::proc::sys_sleep, p, private),
        SyscallCode::Uptime => call(self::proc::sys_uptime, p, private),
        SyscallCode::Open => call(self::file::sys_open, p, private),
        SyscallCode::Write => call(self::file::sys_write, p, private),
        SyscallCode::Mknod => call(self::file::sys_mknod, p, private),
        SyscallCode::Unlink => call(self::file::sys_unlink, p, private),
        SyscallCode::Link => call(self::file::sys_link, p, private),
        SyscallCode::Mkdir => call(self::file::sys_mkdir, p, private),
        SyscallCode::Close => call(self::file::sys_close, p, private),
    };
    let private_ref = private.as_mut().unwrap();
    ret.store(private_ref.trapframe_mut().unwrap());
}
