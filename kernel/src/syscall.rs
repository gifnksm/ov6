use core::panic;

use crate::{
    println,
    proc::Proc,
    syscall_proc,
    vm::{self, VirtAddr},
};

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

mod ffi {
    use core::ffi::{c_char, c_int};

    use super::*;

    #[unsafe(no_mangle)]
    unsafe extern "C" fn fetchaddr(addr: u64, ip: *mut usize) -> c_int {
        let p = Proc::myproc().unwrap();
        super::fetch_addr(p, VirtAddr::new(addr as usize))
            .map(|val| {
                unsafe {
                    *ip = val;
                }
                0
            })
            .unwrap_or(-1)
    }

    #[unsafe(no_mangle)]
    unsafe extern "C" fn fetchstr(addr: u64, buf: *mut c_char, max: c_int) -> c_int {
        let p = Proc::myproc().unwrap();
        let buf = unsafe { core::slice::from_raw_parts_mut(buf.cast(), max as usize) };
        super::fetch_str(p, VirtAddr::new(addr as usize), buf)
            .map(|len| len as c_int)
            .unwrap_or(-1)
    }

    #[unsafe(no_mangle)]
    unsafe extern "C" fn argint(n: c_int, ip: *mut c_int) {
        let p = Proc::myproc().unwrap();
        unsafe {
            *ip = super::arg_int(p, n as usize) as c_int;
        }
    }

    #[unsafe(no_mangle)]
    unsafe extern "C" fn argaddr(n: c_int, ip: *mut u64) {
        let p = Proc::myproc().unwrap();
        unsafe {
            *ip = super::arg_addr(p, n as usize).addr() as u64;
        }
    }

    #[unsafe(no_mangle)]
    unsafe extern "C" fn argstr(n: c_int, buf: *mut c_char, max: c_int) -> c_int {
        let p = Proc::myproc().unwrap();
        let buf = unsafe { core::slice::from_raw_parts_mut(buf.cast(), max as usize) };
        super::arg_str(p, n as usize, buf)
            .map(|len| len as c_int)
            .unwrap_or(-1)
    }

    #[unsafe(no_mangle)]
    unsafe extern "C" fn syscall() {
        let p = Proc::myproc().unwrap();
        super::syscall(p)
    }
}

/// Fetches a `usize` at `addr` from the current process.
fn fetch_addr(p: &Proc, addr: VirtAddr) -> Result<usize, ()> {
    if !p.is_valid_addr(addr..addr.byte_add(size_of::<usize>())) {
        return Err(());
    }
    let mut bytes = [0u8; size_of::<usize>()];
    vm::copy_in(p.pagetable().unwrap(), &mut bytes, addr).map_err(|_| ())?;
    Ok(usize::from_ne_bytes(bytes))
}

/// Fetches the nul-terminated string at addr from the current process.
///
/// Returns length of the string, not including nul.
fn fetch_str(p: &Proc, addr: VirtAddr, buf: &mut [u8]) -> Result<usize, ()> {
    vm::copy_in_str(p.pagetable().unwrap(), buf, addr)?;
    Ok(buf.iter().position(|&c| c == 0).unwrap())
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
fn arg_int(p: &Proc, n: usize) -> usize {
    arg_raw(p, n)
}

/// Retrieves an argument as a pointer.
///
/// Don't check for legality, since
/// copy_in/copy_out will do that.
fn arg_addr(p: &Proc, n: usize) -> VirtAddr {
    VirtAddr::new(arg_int(p, n))
}

/// Fetches the nth word-sized system call argument as a nul-terminated string.
///
/// Copies into buf, at most buf's length.
/// Returns string length if Ok, or Err if the string is not nul-terminated.
fn arg_str(p: &Proc, n: usize, buf: &mut [u8]) -> Result<usize, ()> {
    let addr = arg_addr(p, n);
    fetch_str(p, addr, buf)
}

pub fn syscall(p: &mut Proc) {
    let n = p.trapframe().unwrap().a7 as usize;
    let f = match n {
        SYS_FORK => syscall_proc::fork,
        SYS_EXIT => syscall_proc::exit,
        SYS_WAIT => syscall_proc::wait,
        SYS_PIPE => syscall_proc::pipe,
        SYS_READ => syscall_proc::read,
        SYS_KILL => syscall_proc::kill,
        SYS_EXEC => syscall_proc::exec,
        SYS_FSTAT => syscall_proc::fstat,
        SYS_CHDIR => syscall_proc::chdir,
        SYS_DUP => syscall_proc::dup,
        SYS_GETPID => syscall_proc::getpid,
        SYS_SBRK => syscall_proc::sbrk,
        SYS_SLEEP => syscall_proc::sleep,
        SYS_UPTIME => syscall_proc::uptime,
        SYS_OPEN => syscall_proc::open,
        SYS_WRITE => syscall_proc::write,
        SYS_MKNOD => syscall_proc::mknod,
        SYS_UNLINK => syscall_proc::unlink,
        SYS_LINK => syscall_proc::link,
        SYS_MKDIR => syscall_proc::mkdir,
        SYS_CLOSE => syscall_proc::close,
        _ => {
            println!("{} {}: unknown sys call {}\n", p.pid(), p.name(), n);
            p.trapframe_mut().unwrap().a0 = u64::MAX;
            return;
        }
    };
    p.trapframe_mut().unwrap().a0 = f() as u64;
}
