use crate::{proc::Proc, syscall};

mod ffi {
    unsafe extern "C" {
        pub fn sys_pipe() -> u64;
        pub fn sys_read() -> u64;
        pub fn sys_exec() -> u64;
        pub fn sys_fstat() -> u64;
        pub fn sys_chdir() -> u64;
        pub fn sys_dup() -> u64;
        pub fn sys_open() -> u64;
        pub fn sys_write() -> u64;
        pub fn sys_mknod() -> u64;
        pub fn sys_unlink() -> u64;
        pub fn sys_link() -> u64;
        pub fn sys_mkdir() -> u64;
        pub fn sys_close() -> u64;
    }
}

pub fn pipe(_p: &Proc) -> Result<usize, ()> {
    syscall::wrap_syscall(ffi::sys_pipe)
}

pub fn read(_p: &Proc) -> Result<usize, ()> {
    syscall::wrap_syscall(ffi::sys_read)
}

pub fn exec(_p: &Proc) -> Result<usize, ()> {
    let res = unsafe { ffi::sys_exec() as isize };
    if res < 0 {
        return Err(());
    }
    Ok(res as usize)
}

pub fn fstat(_p: &Proc) -> Result<usize, ()> {
    syscall::wrap_syscall(ffi::sys_fstat)
}

pub fn chdir(_p: &Proc) -> Result<usize, ()> {
    syscall::wrap_syscall(ffi::sys_chdir)
}

pub fn dup(_p: &Proc) -> Result<usize, ()> {
    syscall::wrap_syscall(ffi::sys_dup)
}

pub fn open(_p: &Proc) -> Result<usize, ()> {
    syscall::wrap_syscall(ffi::sys_open)
}

pub fn write(_p: &Proc) -> Result<usize, ()> {
    syscall::wrap_syscall(ffi::sys_write)
}

pub fn mknod(_p: &Proc) -> Result<usize, ()> {
    syscall::wrap_syscall(ffi::sys_mknod)
}

pub fn unlink(_p: &Proc) -> Result<usize, ()> {
    syscall::wrap_syscall(ffi::sys_unlink)
}

pub fn link(_p: &Proc) -> Result<usize, ()> {
    syscall::wrap_syscall(ffi::sys_link)
}

pub fn mkdir(_p: &Proc) -> Result<usize, ()> {
    syscall::wrap_syscall(ffi::sys_mkdir)
}

pub fn close(_p: &Proc) -> Result<usize, ()> {
    syscall::wrap_syscall(ffi::sys_close)
}
