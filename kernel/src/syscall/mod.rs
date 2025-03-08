use core::panic;

use ov6_syscall::{Ret0, Ret1, ReturnType, ReturnValueConvert, SyscallCode, syscall as sys};

use crate::{
    error::KernelError,
    memory::{VirtAddr, vm},
    println,
    proc::{Proc, ProcPrivateData, ProcPrivateDataGuard, TrapFrame},
};

mod file;
mod proc;

/// Fetches a `usize` at `addr` from the current process.
fn fetch_addr(private: &ProcPrivateData, addr: VirtAddr) -> Result<VirtAddr, KernelError> {
    private.validate_addr(addr..addr.byte_add(size_of::<usize>()))?;
    let va = vm::copy_in(private.pagetable().unwrap(), addr)?;
    Ok(VirtAddr::new(va))
}

/// Fetches the nul-terminated string at addr from the current process.
///
/// Returns length of the string, not including nul.
fn fetch_str<'a>(
    private: &ProcPrivateData,
    addr: VirtAddr,
    buf: &'a mut [u8],
) -> Result<&'a [u8], KernelError> {
    vm::copy_in_str(private.pagetable().unwrap(), buf, addr)?;
    let len = buf.iter().position(|&c| c == 0).unwrap();
    Ok(&buf[..len])
}

fn arg_raw(private: &ProcPrivateData, n: usize) -> usize {
    let tf = private.trapframe().unwrap();
    (match n {
        0 => tf.a0,
        1 => tf.a1,
        2 => tf.a2,
        3 => tf.a3,
        4 => tf.a4,
        5 => tf.a5,
        _ => panic!(),
    }) as usize
}

/// Fetches the nth 32-bit system call argument.
pub fn arg_int(private: &ProcPrivateData, n: usize) -> usize {
    arg_raw(private, n)
}

/// Retrieves an argument as a pointer.
///
/// Don't check for legality, since
/// `copy_in` / `copy_out` will do that.
pub fn arg_addr(private: &ProcPrivateData, n: usize) -> VirtAddr {
    VirtAddr::new(arg_int(private, n))
}

/// Fetches the nth word-sized system call argument as a nul-terminated string.
///
/// Copies into buf, at most buf's length.
/// Returns string length if Ok, or Err if the string is not nul-terminated.
pub fn arg_str<'a>(
    private: &ProcPrivateData,
    n: usize,
    buf: &'a mut [u8],
) -> Result<&'a [u8], KernelError> {
    let addr = arg_addr(private, n);
    fetch_str(private, addr, buf)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReturnValue {
    Ret0,
    Ret1(usize),
    Ret2(usize, usize),
}

impl<T> From<Ret0<T>> for ReturnValue {
    fn from(_: Ret0<T>) -> Self {
        Self::Ret0
    }
}

impl<T> From<Ret1<T>> for ReturnValue {
    fn from(value: Ret1<T>) -> Self {
        Self::Ret1(value.a0)
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

    fn call<T>(
        f: fn(p: &'static Proc, private: &mut Option<ProcPrivateDataGuard>) -> T,
        p: &'static Proc,
        private: &mut Option<ProcPrivateDataGuard>,
    ) -> ReturnValue
    where
        T: ReturnValueConvert,
        T::Repr: Into<ReturnValue>,
    {
        f(p, private).encode().into()
    }

    let f: &dyn Fn(&'static Proc, &mut Option<ProcPrivateDataGuard>) -> ReturnValue = match ty {
        SyscallCode::Fork => &|p, private| call(self::proc::sys_fork, p, private),
        SyscallCode::Exit => &|p, private| call(self::proc::sys_exit, p, private),
        SyscallCode::Wait => &|p, private| call(self::proc::sys_wait, p, private),
        SyscallCode::Pipe => &|p, private| call(self::file::sys_pipe, p, private),
        SyscallCode::Read => &|p, private| call(self::file::sys_read, p, private),
        SyscallCode::Kill => &|p, private| call(self::proc::sys_kill, p, private),
        SyscallCode::Exec => &|p, private| match self::file::sys_exec(p, private) {
            Ok((argc, argv)) => ReturnValue::Ret2(argc, argv),
            Err(e) => ReturnType::<sys::Exec>::Err(e).encode().into(),
        },
        SyscallCode::Fstat => &|p, private| call(self::file::sys_fstat, p, private),
        SyscallCode::Chdir => &|p, private| call(self::file::sys_chdir, p, private),
        SyscallCode::Dup => &|p, private| call(self::file::sys_dup, p, private),
        SyscallCode::Getpid => &|p, private| call(self::proc::sys_getpid, p, private),
        SyscallCode::Sbrk => &|p, private| call(self::proc::sys_sbrk, p, private),
        SyscallCode::Sleep => &|p, private| call(self::proc::sys_sleep, p, private),
        SyscallCode::Uptime => &|p, private| call(self::proc::sys_uptime, p, private),
        SyscallCode::Open => &|p, private| call(self::file::sys_open, p, private),
        SyscallCode::Write => &|p, private| call(self::file::sys_write, p, private),
        SyscallCode::Mknod => &|p, private| call(self::file::sys_mknod, p, private),
        SyscallCode::Unlink => &|p, private| call(self::file::sys_unlink, p, private),
        SyscallCode::Link => &|p, private| call(self::file::sys_link, p, private),
        SyscallCode::Mkdir => &|p, private| call(self::file::sys_mkdir, p, private),
        SyscallCode::Close => &|p, private| call(self::file::sys_close, p, private),
    };
    let ret = f(p, private);
    let private_ref = private.as_mut().unwrap();
    ret.store(private_ref.trapframe_mut().unwrap());
}
