pub use ov6_syscall::{OpenFlags, Stat, StatType, SyscallCode};
use ov6_syscall::{ReturnTypeRepr, syscall};

#[cfg(target_arch = "riscv64")]
macro_rules! syscall {
    ($ty:expr => $(#[$attr:meta])* fn $name:ident($($params:tt)*) -> $ret:ty) => {
        $(#[$attr])*
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
}

syscall!(SyscallCode::Fork => fn fork() -> ReturnTypeRepr<syscall::Fork>);
syscall!(SyscallCode::Exit => fn exit(a0: usize) -> ReturnTypeRepr<syscall::Exit>);
syscall!(SyscallCode::Wait => fn wait(a0: usize) -> ReturnTypeRepr<syscall::Wait>);
syscall!(SyscallCode::Pipe => fn pipe(a0: usize) -> ReturnTypeRepr<syscall::Pipe>);
syscall!(SyscallCode::Write => fn write(a0: usize, a1: usize, a2: usize) -> ReturnTypeRepr<syscall::Write>);
syscall!(SyscallCode::Read => fn read(a0: usize, a1: usize, a2: usize) -> ReturnTypeRepr<syscall::Read>);
syscall!(SyscallCode::Close => fn close(a0: usize) -> ReturnTypeRepr<syscall::Close>);
syscall!(SyscallCode::Kill => fn kill(a0: usize) -> ReturnTypeRepr<syscall::Kill>);
syscall!(SyscallCode::Exec => fn exec(a0: usize, a1: usize, a2: usize) -> ReturnTypeRepr<syscall::Exec>);
syscall!(SyscallCode::Open => fn open(a0: usize, a1: usize) -> ReturnTypeRepr<syscall::Open>);
syscall!(SyscallCode::Mknod => fn mknod(a0: usize, a1: usize, a2: usize) -> ReturnTypeRepr<syscall::Mknod>);
syscall!(SyscallCode::Unlink => fn unlink(a0: usize) -> ReturnTypeRepr<syscall::Unlink>);
syscall!(SyscallCode::Fstat => fn fstat(a0: usize, a1: usize) -> ReturnTypeRepr<syscall::Fstat>);
syscall!(SyscallCode::Link =>fn link(a0: usize, a1: usize) -> ReturnTypeRepr<syscall::Link>);
syscall!(SyscallCode::Mkdir => fn mkdir(a0: usize) -> ReturnTypeRepr<syscall::Mkdir>);
syscall!(SyscallCode::Chdir => fn chdir(a0: usize) -> ReturnTypeRepr<syscall::Chdir>);
syscall!(SyscallCode::Dup => fn dup(a0: usize) -> ReturnTypeRepr<syscall::Dup>);
syscall!(SyscallCode::Getpid => fn getpid() -> ReturnTypeRepr<syscall::Getpid>);
syscall!(SyscallCode::Sbrk => fn sbrk(a0: usize) -> ReturnTypeRepr<syscall::Sbrk>);
syscall!(SyscallCode::Sleep => fn sleep(a0: usize) -> ReturnTypeRepr<syscall::Sleep>);
syscall!(SyscallCode::Uptime => fn uptime() -> ReturnTypeRepr<syscall::Uptime>);
