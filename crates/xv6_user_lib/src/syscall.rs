use core::arch::naked_asm;

pub use xv6_syscall::{OpenFlags, Stat, StatType, SyscallType};

macro_rules! syscall {
    ($ty:expr => $(#[$attr:meta])* fn $name:ident($($params:tt)*) -> $ret:ty) => {
        $(#[$attr])*
        #[unsafe(no_mangle)]
        #[naked]
        pub extern "C" fn $name($($params)*) -> $ret {
            unsafe {
                naked_asm!(
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
                naked_asm!(
                    "li a7, {ty}",
                    "ecall",
                    "ret",
                    ty = const $ty as usize
                );
            }
        }
    };
}

syscall!(SyscallType::Fork => fn fork() -> i32);
syscall!(SyscallType::Exit => fn exit(status: i32) -> !);
syscall!(
    SyscallType::Wait =>
    /// # Safety
    ///
    /// `wstatus` must be a valid pointer to an `i32`.
    unsafe fn wait(wstatus: *mut i32) -> i32
);
syscall!(
    SyscallType::Pipe =>
    /// # Safety
    ///
    /// `pipefd` must be a valid pointer to an array of 2 `i32`s.
    unsafe fn pipe(pipefd: *mut i32) -> i32);
syscall!(
    SyscallType::Write =>
    /// # Safety
    ///
    /// `buf` must be a valid pointer to an array of `i32`s with a length of `count`.
    unsafe fn write(fd: i32, buf: *const u8, count: usize) -> i32
);
syscall!(
    SyscallType::Read =>
    /// # Safety
    ///
    /// `buf` must be a valid pointer to an array of `i32`s with a length of `count`.
    unsafe fn read(fd: i32, buf: *mut u8, count: usize) -> i32
);
syscall!(SyscallType::Close => fn close(fd: i32) -> i32);
syscall!(SyscallType::Kill => fn kill(pid: i32) -> i32);
syscall!(
    SyscallType::Exec =>
    /// # Safety
    ///
    /// `path` must be a valid pointer to a null-terminated string.
    /// `argv` must be a valid pointer to an array of null-terminated strings.
    /// The last element of `argv` must be a null pointer.
    unsafe fn exec(path: *const u8, argv: *const *const u8) -> i32
);
syscall!(
    SyscallType::Open =>
    /// # Safety
    ///
    /// `path` must be a valid pointer to a null-terminated string.
    unsafe fn open(path: *const u8, flags: OpenFlags) -> i32
);
syscall!(
    SyscallType::Mknod =>
    /// # Safety
    ///
    /// `path` must be a valid pointer to a null-terminated string.
    unsafe fn mknod(path: *const u8, major: i16, minor: i16) -> i32
);
syscall!(
    SyscallType::Unlink =>
    /// # Safety
    ///
    /// `path` must be a valid pointer to a null-terminated string.
    unsafe fn unlink(path: *const u8) -> i32
);
syscall!(
    SyscallType::Fstat =>
    /// # Safety
    ///
    /// `stat` must be a valid pointer to a `Stat` struct.
    unsafe fn fstat(fd: i32, stat: *mut Stat) -> i32
);
syscall!(
    SyscallType::Link =>
    /// # Safety
    ///
    /// `oldpath` and `newpath` must be valid pointers to null-terminated strings.
    unsafe fn link(oldpath: *const u8, newpath: *const u8) -> i32
);
syscall!(
    SyscallType::Mkdir =>
    /// # Safety
    ///
    /// `path` must be a valid pointer to a null-terminated string.
    unsafe fn mkdir(path: *const u8) -> i32
);
syscall!(
    SyscallType::Chdir =>
    /// # Safety
    ///
    /// `path` must be a valid pointer to a null-terminated string.
    unsafe fn chdir(path: *const u8) -> i32
);
syscall!(SyscallType::Dup => fn dup(fd: i32) -> i32);
syscall!(SyscallType::Getpid => fn getpid() -> i32);
syscall!(SyscallType::Sbrk => fn sbrk(incr: i32) -> *mut char);
syscall!(SyscallType::Sleep => fn sleep(n: i32) -> i32);
syscall!(SyscallType::Uptime => fn uptime() -> i32);
