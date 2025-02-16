use core::panic;

use crate::{
    memory::vm::{self, VirtAddr},
    println,
    proc::Proc,
};

mod fcntl;
mod file;
mod proc;

// System call numbers

pub const SYS_FORK: usize = 1;
pub const SYS_EXIT: usize = 2;
pub const SYS_WAIT: usize = 3;
pub const SYS_PIPE: usize = 4;
pub const SYS_READ: usize = 5;
pub const SYS_KILL: usize = 6;
pub const SYS_EXEC: usize = 7;
pub const SYS_FSTAT: usize = 8;
pub const SYS_CHDIR: usize = 9;
pub const SYS_DUP: usize = 10;
pub const SYS_GETPID: usize = 11;
pub const SYS_SBRK: usize = 12;
pub const SYS_SLEEP: usize = 13;
pub const SYS_UPTIME: usize = 14;
pub const SYS_OPEN: usize = 15;
pub const SYS_WRITE: usize = 16;
pub const SYS_MKNOD: usize = 17;
pub const SYS_UNLINK: usize = 18;
pub const SYS_LINK: usize = 19;
pub const SYS_MKDIR: usize = 20;
pub const SYS_CLOSE: usize = 21;

/// Fetches a `usize` at `addr` from the current process.
fn fetch_addr(p: &Proc, addr: VirtAddr) -> Result<VirtAddr, ()> {
    if !p.is_valid_addr(addr..addr.byte_add(size_of::<usize>())) {
        return Err(());
    }
    let va = vm::copy_in(p.pagetable().unwrap(), addr)?;
    Ok(VirtAddr::new(va))
}

/// Fetches the nul-terminated string at addr from the current process.
///
/// Returns length of the string, not including nul.
fn fetch_str<'a>(p: &Proc, addr: VirtAddr, buf: &'a mut [u8]) -> Result<&'a [u8], ()> {
    vm::copy_in_str(p.pagetable().unwrap(), buf, addr)?;
    let len = buf.iter().position(|&c| c == 0).unwrap();
    Ok(&buf[..len])
}

fn arg_raw(p: &Proc, n: usize) -> usize {
    let tf = p.trapframe().unwrap();
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
pub fn arg_int(p: &Proc, n: usize) -> usize {
    arg_raw(p, n)
}

/// Retrieves an argument as a pointer.
///
/// Don't check for legality, since
/// copy_in/copy_out will do that.
pub fn arg_addr(p: &Proc, n: usize) -> VirtAddr {
    VirtAddr::new(arg_int(p, n))
}

/// Fetches the nth word-sized system call argument as a nul-terminated string.
///
/// Copies into buf, at most buf's length.
/// Returns string length if Ok, or Err if the string is not nul-terminated.
pub fn arg_str<'a>(p: &Proc, n: usize, buf: &'a mut [u8]) -> Result<&'a [u8], ()> {
    let addr = arg_addr(p, n);
    fetch_str(p, addr, buf)
}

pub fn syscall(p: &Proc) {
    let n = p.trapframe().unwrap().a7 as usize;
    let f = match n {
        SYS_FORK => self::proc::sys_fork,
        SYS_EXIT => self::proc::sys_exit,
        SYS_WAIT => self::proc::sys_wait,
        SYS_PIPE => self::file::sys_pipe,
        SYS_READ => self::file::sys_read,
        SYS_KILL => self::proc::sys_kill,
        SYS_EXEC => self::file::sys_exec,
        SYS_FSTAT => self::file::sys_fstat,
        SYS_CHDIR => self::file::sys_chdir,
        SYS_DUP => self::file::sys_dup,
        SYS_GETPID => self::proc::sys_getpid,
        SYS_SBRK => self::proc::sys_sbrk,
        SYS_SLEEP => self::proc::sys_sleep,
        SYS_UPTIME => self::proc::sys_uptime,
        SYS_OPEN => self::file::sys_open,
        SYS_WRITE => self::file::sys_write,
        SYS_MKNOD => self::file::sys_mknod,
        SYS_UNLINK => self::file::sys_unlink,
        SYS_LINK => self::file::sys_link,
        SYS_MKDIR => self::file::sys_mkdir,
        SYS_CLOSE => self::file::sys_close,
        _ => {
            println!("{} {}: unknown sys call {}\n", p.pid(), p.name(), n);
            p.trapframe_mut().unwrap().a0 = u64::MAX;
            return;
        }
    };
    let res = f(p).map(|f| f as u64).unwrap_or(u64::MAX);
    p.trapframe_mut().unwrap().a0 = res as u64;
}
