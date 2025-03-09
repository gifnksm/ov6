use core::ffi::c_char;

pub use ov6_syscall::{OpenFlags, Stat, StatType, SyscallCode};
use ov6_syscall::{ReturnTypeRepr, syscall};
use ov6_types::fs::RawFd;

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
syscall!(SyscallCode::Exit => fn exit(a0: usize) -> ReturnTypeRepr<syscall::Exit>);
syscall!(SyscallCode::Wait => fn wait(a0: usize) -> ReturnTypeRepr<syscall::Wait>);
syscall!(SyscallCode::Pipe => fn pipe(a0: usize) -> ReturnTypeRepr<syscall::Pipe>);
syscall!(SyscallCode::Write => fn write(a0: usize, a1: usize, a2: usize) -> ReturnTypeRepr<syscall::Write>);
syscall!(SyscallCode::Read => fn read(a0: usize, a1: usize, a2: usize) -> ReturnTypeRepr<syscall::Read>);
syscall!(SyscallCode::Close => fn close(fd: RawFd) -> ReturnTypeRepr<syscall::Close>);
syscall!(SyscallCode::Kill => fn kill(a0: usize) -> ReturnTypeRepr<syscall::Kill>);
syscall!(SyscallCode::Exec => fn exec(a0: usize, a1: usize, a2: usize) -> ReturnTypeRepr<syscall::Exec>);
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
    unsafe fn fstat(fd: RawFd, stat: *mut Stat) -> ReturnTypeRepr<syscall::Fstat>
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
syscall!(SyscallCode::Dup => fn dup(fd: RawFd) -> ReturnTypeRepr<syscall::Dup>);
syscall!(SyscallCode::Getpid => fn getpid() -> ReturnTypeRepr<syscall::Getpid>);
syscall!(SyscallCode::Sbrk => fn sbrk(incr: isize) -> ReturnTypeRepr<syscall::Sbrk>);
syscall!(SyscallCode::Sleep => fn sleep(n: i32) -> ReturnTypeRepr<syscall::Sleep>);
syscall!(SyscallCode::Uptime => fn uptime() -> ReturnTypeRepr<syscall::Uptime>);
