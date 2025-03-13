pub use ov6_syscall::{OpenFlags, Stat, StatType, SyscallCode, syscall};
use ov6_syscall::{RegisterValue, ReturnTypeRepr, Syscall};

pub trait SyscallExt: Syscall {
    fn call_raw(arg: Self::Arg) -> ReturnTypeRepr<Self>;

    fn try_call(
        arg: Self::Arg,
    ) -> Result<Self::Return, <Self::Return as RegisterValue>::DecodeError> {
        let ret = Self::call_raw(arg);
        Self::Return::try_decode(ret)
    }

    fn call(arg: Self::Arg) -> Self::Return {
        Self::try_call(arg).unwrap()
    }
}

macro_rules! syscall_fn {
    ($name:ident => $(#[$attr:meta])* fn $fn_name:ident($($params:tt)*) -> $ret:ty) => {
        #[cfg(target_arch = "riscv64")]
        #[naked]
        pub extern "C" fn $fn_name($($params)*) -> $ret {
            unsafe {
                core::arch::naked_asm!(
                    "li a7, {ty}",
                    "ecall",
                    "ret",
                    ty = const SyscallCode::$name as usize
                );
            }
        }

        #[cfg(not(target_arch = "riscv64"))]
        #[allow(clippy::allow_attributes)]
        #[allow(unused_variables)]
        #[allow(clippy::must_use_candidate)]
        pub extern "C" fn $fn_name($($params)*) -> $ret {
            panic!();
        }
    };
}

macro_rules! syscall {
    ($(#[$attr:meta])* $name:ident, $fn_name:ident($($arg:ident),*)) => {
        impl SyscallExt for syscall::$name {
            fn call_raw(arg: Self::Arg) -> ReturnTypeRepr<Self> {
                let [$($arg),*] = Self::Arg::encode(arg).a;
                $fn_name($($arg),*)
            }
        }

        syscall_fn!($name => $(#[$attr])* fn $fn_name($($arg: usize),*) -> ReturnTypeRepr<syscall::$name>);
    }
}

syscall!(Fork, fork());
syscall!(Exit, exit(a0));
syscall!(Wait, wait(a0));
syscall!(Pipe, pipe(a0));
syscall!(Write, write(a0, a1, a2));
syscall!(Read, read(a0, a1, a2));
syscall!(Close, close(a0));
syscall!(Kill, kill(a0));
syscall!(Exec, exec(a0, a1, a2, a3));
syscall!(Open, open(a0, a1, a2));
syscall!(Mknod, mknod(a0, a1, a2, a3));
syscall!(Unlink, unlink(a0, a1));
syscall!(Fstat, fstat(a0, a1));
syscall!(Link, link(a0, a1, a2, a3));
syscall!(Mkdir, mkdir(a0, a1));
syscall!(Chdir, chdir(a0, a1));
syscall!(Dup, dup(a0));
syscall!(Getpid, getpid());
syscall!(Sbrk, sbrk(a0));
syscall!(Sleep, sleep(a0));
syscall!(Uptime, uptime());
