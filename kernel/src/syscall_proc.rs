mod ffi {
    unsafe extern "C" {
        pub fn sys_fork() -> u64;
        pub fn sys_exit() -> u64;
        pub fn sys_wait() -> u64;
        pub fn sys_pipe() -> u64;
        pub fn sys_read() -> u64;
        pub fn sys_kill() -> u64;
        pub fn sys_exec() -> u64;
        pub fn sys_fstat() -> u64;
        pub fn sys_chdir() -> u64;
        pub fn sys_dup() -> u64;
        pub fn sys_getpid() -> u64;
        pub fn sys_sbrk() -> u64;
        pub fn sys_sleep() -> u64;
        pub fn sys_uptime() -> u64;
        pub fn sys_open() -> u64;
        pub fn sys_write() -> u64;
        pub fn sys_mknod() -> u64;
        pub fn sys_unlink() -> u64;
        pub fn sys_link() -> u64;
        pub fn sys_mkdir() -> u64;
        pub fn sys_close() -> u64;
    }
}

pub fn fork() -> usize {
    unsafe { ffi::sys_fork() as usize }
}
pub fn exit() -> usize {
    unsafe { ffi::sys_exit() as usize }
}
pub fn wait() -> usize {
    unsafe { ffi::sys_wait() as usize }
}
pub fn pipe() -> usize {
    unsafe { ffi::sys_pipe() as usize }
}
pub fn read() -> usize {
    unsafe { ffi::sys_read() as usize }
}
pub fn kill() -> usize {
    unsafe { ffi::sys_kill() as usize }
}
pub fn exec() -> usize {
    unsafe { ffi::sys_exec() as usize }
}
pub fn fstat() -> usize {
    unsafe { ffi::sys_fstat() as usize }
}
pub fn chdir() -> usize {
    unsafe { ffi::sys_chdir() as usize }
}
pub fn dup() -> usize {
    unsafe { ffi::sys_dup() as usize }
}
pub fn getpid() -> usize {
    unsafe { ffi::sys_getpid() as usize }
}
pub fn sbrk() -> usize {
    unsafe { ffi::sys_sbrk() as usize }
}
pub fn sleep() -> usize {
    unsafe { ffi::sys_sleep() as usize }
}
pub fn uptime() -> usize {
    unsafe { ffi::sys_uptime() as usize }
}
pub fn open() -> usize {
    unsafe { ffi::sys_open() as usize }
}
pub fn write() -> usize {
    unsafe { ffi::sys_write() as usize }
}
pub fn mknod() -> usize {
    unsafe { ffi::sys_mknod() as usize }
}
pub fn unlink() -> usize {
    unsafe { ffi::sys_unlink() as usize }
}
pub fn link() -> usize {
    unsafe { ffi::sys_link() as usize }
}
pub fn mkdir() -> usize {
    unsafe { ffi::sys_mkdir() as usize }
}
pub fn close() -> usize {
    unsafe { ffi::sys_close() as usize }
}
