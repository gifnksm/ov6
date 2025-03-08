use core::ffi::c_char;

pub use ov6_syscall::{OpenFlags, Stat, StatType, SyscallCode};
use ov6_syscall::{ReturnTypeRepr, syscall};

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
        #[allow(clippy::allow_attributes)]
        #[allow(unused_variables)]
        #[allow(clippy::must_use_candidate)]
        $(#[$attr])*
        pub extern "C" fn $name($($params)*) -> $ret {
            panic!();
        }
    };
    ($ty:expr => $(#[$attr:meta])* unsafe fn $name:ident($($params:tt)*) -> $ret:ty) => {
        #[allow(clippy::allow_attributes)]
        #[allow(unused_variables)]
        #[allow(clippy::must_use_candidate)]
        $(#[$attr])*
        pub unsafe extern "C" fn $name($($params)*) -> $ret {
            panic!();
        }
    };
}

syscall!(SyscallCode::Fork => fn fork() -> ReturnTypeRepr<syscall::Fork>);
syscall!(SyscallCode::Exit => fn exit(status: i32) -> ReturnTypeRepr<syscall::Exit>);
syscall!(
    SyscallCode::Wait =>
    /// # Safety
    ///
    /// `wstatus` must be a valid pointer to an `i32`.
    unsafe fn wait(wstatus: *mut i32) -> ReturnTypeRepr<syscall::Wait>
);
syscall!(
    SyscallCode::Pipe =>
    /// # Safety
    ///
    /// `pipefd` must be a valid pointer to an array of 2 `i32`s.
    unsafe fn pipe(pipefd: *mut i32) -> ReturnTypeRepr<syscall::Pipe>
);
syscall!(
    SyscallCode::Write =>
    /// # Safety
    ///
    /// `buf` must be a valid pointer to an array of `i32`s with a length of `count`.
    unsafe fn write(fd: i32, buf: *const u8, count: usize) -> ReturnTypeRepr<syscall::Write>
);
syscall!(
    SyscallCode::Read =>
    /// # Safety
    ///
    /// `buf` must be a valid pointer to an array of `i32`s with a length of `count`.
    unsafe fn read(fd: i32, buf: *mut u8, count: usize) -> ReturnTypeRepr<syscall::Read>
);
syscall!(SyscallCode::Close => fn close(fd: i32) -> ReturnTypeRepr<syscall::Close>);
syscall!(SyscallCode::Kill => fn kill(pid: u32) -> ReturnTypeRepr<syscall::Kill>);
syscall!(
    SyscallCode::Exec =>
    /// # Safety
    ///
    /// `path` must be a valid pointer to a null-terminated string.
    /// `argv` must be a valid pointer to an array of null-terminated strings.
    /// The last element of `argv` must be a null pointer.
    unsafe fn exec(path: *const c_char, argv: *const *const c_char) -> ReturnTypeRepr<syscall::Exec>
);
syscall!(
    SyscallCode::Open =>
    /// # Safety
    ///
    /// `path` must be a valid pointer to a null-terminated string.
    unsafe fn open(path: *const c_char, flags: OpenFlags) -> ReturnTypeRepr<syscall::Open>
);
syscall!(
    SyscallCode::Mknod =>
    /// # Safety
    ///
    /// `path` must be a valid pointer to a null-terminated string.
    unsafe fn mknod(path: *const c_char, major: i16, minor: i16) -> ReturnTypeRepr<syscall::Mknod>
);
syscall!(
    SyscallCode::Unlink =>
    /// # Safety
    ///
    /// `path` must be a valid pointer to a null-terminated string.
    unsafe fn unlink(path: *const c_char) -> ReturnTypeRepr<syscall::Unlink>
);
syscall!(
    SyscallCode::Fstat =>
    /// # Safety
    ///
    /// `stat` must be a valid pointer to a `Stat` struct.
    unsafe fn fstat(fd: i32, stat: *mut Stat) -> ReturnTypeRepr<syscall::Fstat>
);
syscall!(
    SyscallCode::Link =>
    /// # Safety
    ///
    /// `oldpath` and `newpath` must be valid pointers to null-terminated strings.
    unsafe fn link(oldpath: *const c_char, newpath: *const c_char) -> ReturnTypeRepr<syscall::Link>
);
syscall!(
    SyscallCode::Mkdir =>
    /// # Safety
    ///
    /// `path` must be a valid pointer to a null-terminated string.
    unsafe fn mkdir(path: *const c_char) -> ReturnTypeRepr<syscall::Mkdir>
);
syscall!(
    SyscallCode::Chdir =>
    /// # Safety
    ///
    /// `path` must be a valid pointer to a null-terminated string.
    unsafe fn chdir(path: *const c_char) -> ReturnTypeRepr<syscall::Chdir>
);
syscall!(SyscallCode::Dup => fn dup(fd: i32) -> ReturnTypeRepr<syscall::Dup>);
syscall!(SyscallCode::Getpid => fn getpid() -> ReturnTypeRepr<syscall::Getpid>);
syscall!(SyscallCode::Sbrk => fn sbrk(incr: isize) -> ReturnTypeRepr<syscall::Sbrk>);
syscall!(SyscallCode::Sleep => fn sleep(n: i32) -> ReturnTypeRepr<syscall::Sleep>);
syscall!(SyscallCode::Uptime => fn uptime() -> ReturnTypeRepr<syscall::Uptime>);
