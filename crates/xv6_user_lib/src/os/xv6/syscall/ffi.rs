use core::ffi::c_char;

pub use xv6_syscall::{OpenFlags, Stat, StatType, SyscallType};

#[cfg(target_arch = "riscv64")]
macro_rules! syscall {
    ($ty:expr => $(#[$attr:meta])* fn $name:ident($($params:tt)*) -> $ret:ty) => {
        $(#[$attr])*
        #[unsafe(no_mangle)]
        #[naked]
        pub extern "C" fn $name($($params)*) -> $ret {
            unsafe {
                core::arch::naked_asm!(
                    "li a7, {ty}",
                    "ecall",
                    "ret",
                    ty = const $ty as usize
                );
            }
        }
    };
    ($ty:expr => $(#[$attr:meta])* unsafe fn $name:ident($($params:tt)*) -> $ret:ty) => {
        $(#[$attr])*
        #[unsafe(no_mangle)]
        #[naked]
        pub unsafe extern "C" fn $name($($params)*) -> $ret {
            unsafe {
                core::arch::naked_asm!(
                    "li a7, {ty}",
                    "ecall",
                    "ret",
                    ty = const $ty as usize
                );
            }
        }
    };
}

#[cfg(not(target_arch = "riscv64"))]
macro_rules! syscall {
    ($ty:expr => $(#[$attr:meta])* fn $name:ident($($params:tt)*) -> $ret:ty) => {
        #[allow(unused_variables)]
        $(#[$attr])*
        pub extern "C" fn $name($($params)*) -> $ret {
            panic!();
        }
    };
    ($ty:expr => $(#[$attr:meta])* unsafe fn $name:ident($($params:tt)*) -> $ret:ty) => {
        #[allow(unused_variables)]
        $(#[$attr])*
        pub unsafe extern "C" fn $name($($params)*) -> $ret {
            panic!();
        }
    };
}

syscall!(SyscallType::Fork => fn fork() -> isize);
syscall!(SyscallType::Exit => fn exit(status: i32) -> !);
syscall!(
    SyscallType::Wait =>
    /// # Safety
    ///
    /// `wstatus` must be a valid pointer to an `i32`.
    unsafe fn wait(wstatus: *mut i32) -> isize
);
syscall!(
    SyscallType::Pipe =>
    /// # Safety
    ///
    /// `pipefd` must be a valid pointer to an array of 2 `i32`s.
    unsafe fn pipe(pipefd: *mut i32) -> isize
);
syscall!(
    SyscallType::Write =>
    /// # Safety
    ///
    /// `buf` must be a valid pointer to an array of `i32`s with a length of `count`.
    unsafe fn write(fd: i32, buf: *const u8, count: usize) -> isize
);
syscall!(
    SyscallType::Read =>
    /// # Safety
    ///
    /// `buf` must be a valid pointer to an array of `i32`s with a length of `count`.
    unsafe fn read(fd: i32, buf: *mut u8, count: usize) -> isize
);
syscall!(SyscallType::Close => fn close(fd: i32) -> isize);
syscall!(SyscallType::Kill => fn kill(pid: u32) -> isize);
syscall!(
    SyscallType::Exec =>
    /// # Safety
    ///
    /// `path` must be a valid pointer to a null-terminated string.
    /// `argv` must be a valid pointer to an array of null-terminated strings.
    /// The last element of `argv` must be a null pointer.
    unsafe fn exec(path: *const c_char, argv: *const *const c_char) -> isize
);
syscall!(
    SyscallType::Open =>
    /// # Safety
    ///
    /// `path` must be a valid pointer to a null-terminated string.
    unsafe fn open(path: *const c_char, flags: OpenFlags) -> isize
);
syscall!(
    SyscallType::Mknod =>
    /// # Safety
    ///
    /// `path` must be a valid pointer to a null-terminated string.
    unsafe fn mknod(path: *const c_char, major: i16, minor: i16) -> isize
);
syscall!(
    SyscallType::Unlink =>
    /// # Safety
    ///
    /// `path` must be a valid pointer to a null-terminated string.
    unsafe fn unlink(path: *const c_char) -> isize
);
syscall!(
    SyscallType::Fstat =>
    /// # Safety
    ///
    /// `stat` must be a valid pointer to a `Stat` struct.
    unsafe fn fstat(fd: i32, stat: *mut Stat) -> isize
);
syscall!(
    SyscallType::Link =>
    /// # Safety
    ///
    /// `oldpath` and `newpath` must be valid pointers to null-terminated strings.
    unsafe fn link(oldpath: *const c_char, newpath: *const c_char) -> isize
);
syscall!(
    SyscallType::Mkdir =>
    /// # Safety
    ///
    /// `path` must be a valid pointer to a null-terminated string.
    unsafe fn mkdir(path: *const c_char) -> isize
);
syscall!(
    SyscallType::Chdir =>
    /// # Safety
    ///
    /// `path` must be a valid pointer to a null-terminated string.
    unsafe fn chdir(path: *const c_char) -> isize
);
syscall!(SyscallType::Dup => fn dup(fd: i32) -> isize);
syscall!(SyscallType::Getpid => fn getpid() -> isize);
syscall!(SyscallType::Sbrk => fn sbrk(incr: isize) -> isize);
syscall!(SyscallType::Sleep => fn sleep(n: i32) -> isize);
syscall!(SyscallType::Uptime => fn uptime() -> isize);
