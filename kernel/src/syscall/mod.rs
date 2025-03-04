use core::panic;

use xv6_syscall::SyscallType;

use crate::{
    error::Error,
    memory::vm::{self, VirtAddr},
    println,
    proc::{Proc, ProcPrivateData, ProcPrivateDataGuard},
};

mod file;
mod proc;

/// Fetches a `usize` at `addr` from the current process.
fn fetch_addr(private: &ProcPrivateData, addr: VirtAddr) -> Result<VirtAddr, Error> {
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
) -> Result<&'a [u8], Error> {
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
/// copy_in/copy_out will do that.
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
) -> Result<&'a [u8], Error> {
    let addr = arg_addr(private, n);
    fetch_str(private, addr, buf)
}

pub fn syscall(p: &Proc, private: &mut Option<ProcPrivateDataGuard>) {
    let private_ref = private.as_mut().unwrap();
    let n = private_ref.trapframe().unwrap().a7;
    let Some(ty) = SyscallType::from_repr(n) else {
        let shared = p.shared().lock();
        let pid = shared.pid();
        let name = shared.name();
        println!("{pid} {name}: unknown sys call {n}");
        private_ref.trapframe_mut().unwrap().a0 = usize::MAX;
        return;
    };
    let f = match ty {
        SyscallType::Fork => self::proc::sys_fork,
        SyscallType::Exit => self::proc::sys_exit,
        SyscallType::Wait => self::proc::sys_wait,
        SyscallType::Pipe => self::file::sys_pipe,
        SyscallType::Read => self::file::sys_read,
        SyscallType::Kill => self::proc::sys_kill,
        SyscallType::Exec => self::file::sys_exec,
        SyscallType::Fstat => self::file::sys_fstat,
        SyscallType::Chdir => self::file::sys_chdir,
        SyscallType::Dup => self::file::sys_dup,
        SyscallType::Getpid => self::proc::sys_getpid,
        SyscallType::Sbrk => self::proc::sys_sbrk,
        SyscallType::Sleep => self::proc::sys_sleep,
        SyscallType::Uptime => self::proc::sys_uptime,
        SyscallType::Open => self::file::sys_open,
        SyscallType::Write => self::file::sys_write,
        SyscallType::Mknod => self::file::sys_mknod,
        SyscallType::Unlink => self::file::sys_unlink,
        SyscallType::Link => self::file::sys_link,
        SyscallType::Mkdir => self::file::sys_mkdir,
        SyscallType::Close => self::file::sys_close,
    };
    let _ = private_ref;
    let ret = match f(p, private) {
        Ok(ret) => ret.cast_signed(),
        Err(e) => {
            let v = xv6_syscall::Error::from(e) as isize;
            assert!(v < 0);
            v
        }
    };
    let private_ref = private.as_mut().unwrap();
    private_ref.trapframe_mut().unwrap().a0 = ret.cast_unsigned();
}
