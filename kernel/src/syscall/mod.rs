use core::panic;

use ov6_syscall::{Ret1, RetInfailible, ReturnValueConvert as _, SyscallCode};

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
enum ReturnValue {
    Ret0,
    Ret1(usize),
}

impl From<RetInfailible> for ReturnValue {
    fn from(_: RetInfailible) -> Self {
        Self::Ret0
    }
}

impl<T> From<Ret1<T>> for ReturnValue {
    fn from(value: Ret1<T>) -> Self {
        Self::Ret1(value.a0)
    }
}

impl ReturnValue {
    fn store(self, tf: &mut TrapFrame) {
        let a0 = match self {
            Self::Ret0 => 0,
            Self::Ret1(a) => a,
        };
        tf.a0 = a0;
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

    let f: &dyn Fn(&'static Proc, &mut Option<ProcPrivateDataGuard>) -> ReturnValue = match ty {
        SyscallCode::Fork => &|p, private| self::proc::sys_fork(p, private).encode().into(),
        SyscallCode::Exit => &|p, private| self::proc::sys_exit(p, private).encode().into(),
        SyscallCode::Wait => &|p, private| self::proc::sys_wait(p, private).encode().into(),
        SyscallCode::Pipe => &|p, private| self::file::sys_pipe(p, private).encode().into(),
        SyscallCode::Read => &|p, private| self::file::sys_read(p, private).encode().into(),
        SyscallCode::Kill => &|p, private| self::proc::sys_kill(p, private).encode().into(),
        SyscallCode::Exec => &|p, private| self::file::sys_exec(p, private).encode().into(),
        SyscallCode::Fstat => &|p, private| self::file::sys_fstat(p, private).encode().into(),
        SyscallCode::Chdir => &|p, private| self::file::sys_chdir(p, private).encode().into(),
        SyscallCode::Dup => &|p, private| self::file::sys_dup(p, private).encode().into(),
        SyscallCode::Getpid => &|p, private| self::proc::sys_getpid(p, private).encode().into(),
        SyscallCode::Sbrk => &|p, private| self::proc::sys_sbrk(p, private).encode().into(),
        SyscallCode::Sleep => &|p, private| self::proc::sys_sleep(p, private).encode().into(),
        SyscallCode::Uptime => &|p, private| self::proc::sys_uptime(p, private).encode().into(),
        SyscallCode::Open => &|p, private| self::file::sys_open(p, private).encode().into(),
        SyscallCode::Write => &|p, private| self::file::sys_write(p, private).encode().into(),
        SyscallCode::Mknod => &|p, private| self::file::sys_mknod(p, private).encode().into(),
        SyscallCode::Unlink => &|p, private| self::file::sys_unlink(p, private).encode().into(),
        SyscallCode::Link => &|p, private| self::file::sys_link(p, private).encode().into(),
        SyscallCode::Mkdir => &|p, private| self::file::sys_mkdir(p, private).encode().into(),
        SyscallCode::Close => &|p, private| self::file::sys_close(p, private).encode().into(),
    };
    let ret = f(p, private);
    let private_ref = private.as_mut().unwrap();
    ret.store(private_ref.trapframe_mut().unwrap());
}
