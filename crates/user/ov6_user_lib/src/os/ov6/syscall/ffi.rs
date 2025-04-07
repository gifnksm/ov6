pub use ov6_syscall::{OpenFlags, Stat, StatType, SyscallCode, syscall};
use ov6_syscall::{Register, RegisterValue, ReturnTypeRepr, Syscall};

trait CallWithArg {
    fn call_with_arg(self, code: SyscallCode) -> [usize; 2];
}

impl<T> CallWithArg for Register<T, 0> {
    #[cfg(not(target_arch = "riscv64"))]
    fn call_with_arg(self, _code: SyscallCode) -> [usize; 2] {
        unimplemented!()
    }

    #[cfg(target_arch = "riscv64")]
    fn call_with_arg(self, code: SyscallCode) -> [usize; 2] {
        let [] = self.a;
        let mut out = [0, 0];
        unsafe {
            core::arch::asm!(
                "ecall",
                in("a7") code as usize,
                lateout("a0") out[0],
                lateout("a1") out[1],
            );
        }
        out
    }
}

impl<T> CallWithArg for Register<T, 1> {
    #[cfg(not(target_arch = "riscv64"))]
    fn call_with_arg(self, _code: SyscallCode) -> [usize; 2] {
        unimplemented!()
    }

    #[cfg(target_arch = "riscv64")]
    fn call_with_arg(self, code: SyscallCode) -> [usize; 2] {
        let [a0] = self.a;
        let mut out = [0, 0];
        unsafe {
            core::arch::asm!(
                "ecall",
                in("a0") a0,
                in("a7") code as usize,
                lateout("a0") out[0],
                lateout("a1") out[1],
            );
        }
        out
    }
}

impl<T> CallWithArg for Register<T, 2> {
    #[cfg(not(target_arch = "riscv64"))]
    fn call_with_arg(self, _code: SyscallCode) -> [usize; 2] {
        unimplemented!()
    }

    #[cfg(target_arch = "riscv64")]
    fn call_with_arg(self, code: SyscallCode) -> [usize; 2] {
        let [a0, a1] = self.a;
        let mut out = [0, 0];
        unsafe {
            core::arch::asm!(
                "ecall",
                in("a0") a0,
                in("a1") a1,
                in("a7") code as usize,
                lateout("a0") out[0],
                lateout("a1") out[1],
            );
        }
        out
    }
}

impl<T> CallWithArg for Register<T, 3> {
    #[cfg(not(target_arch = "riscv64"))]
    fn call_with_arg(self, _code: SyscallCode) -> [usize; 2] {
        unimplemented!()
    }

    #[cfg(target_arch = "riscv64")]
    fn call_with_arg(self, code: SyscallCode) -> [usize; 2] {
        let [a0, a1, a2] = self.a;
        let mut out = [0, 0];
        unsafe {
            core::arch::asm!(
                "ecall",
                in("a0") a0,
                in("a1") a1,
                in("a2") a2,
                in("a7") code as usize,
                lateout("a0") out[0],
                lateout("a1") out[1],
            );
        }
        out
    }
}

impl<T> CallWithArg for Register<T, 4> {
    #[cfg(not(target_arch = "riscv64"))]
    fn call_with_arg(self, _code: SyscallCode) -> [usize; 2] {
        unimplemented!()
    }

    #[cfg(target_arch = "riscv64")]
    fn call_with_arg(self, code: SyscallCode) -> [usize; 2] {
        let [a0, a1, a2, a3] = self.a;
        let mut out = [0, 0];
        unsafe {
            core::arch::asm!(
                "ecall",
                in("a0") a0,
                in("a1") a1,
                in("a2") a2,
                in("a3") a3,
                in("a7") code as usize,
                lateout("a0") out[0],
                lateout("a1") out[1],
            );
        }
        out
    }
}

trait FromArray {
    fn from_array(a: [usize; 2]) -> Self;
}

impl<T> FromArray for Register<T, 0> {
    fn from_array([_a0, _a1]: [usize; 2]) -> Self {
        Self::new([])
    }
}

impl<T> FromArray for Register<T, 1> {
    fn from_array([a0, _a1]: [usize; 2]) -> Self {
        Self::new([a0])
    }
}

impl<T> FromArray for Register<T, 2> {
    fn from_array([a0, a1]: [usize; 2]) -> Self {
        Self::new([a0, a1])
    }
}

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

macro_rules! syscall {
    ($name:ident) => {
        impl SyscallExt for syscall::$name {
            fn call_raw(arg: Self::Arg) -> ReturnTypeRepr<Self> {
                FromArray::from_array(Self::Arg::encode(arg).call_with_arg(Self::CODE))
            }
        }
    };
}

syscall!(Fork);
syscall!(Exit);
syscall!(Wait);
syscall!(Pipe);
syscall!(Write);
syscall!(Read);
syscall!(Close);
syscall!(Kill);
syscall!(Exec);
syscall!(Open);
syscall!(Mknod);
syscall!(Unlink);
syscall!(Fstat);
syscall!(Link);
syscall!(Mkdir);
syscall!(Chdir);
syscall!(Dup);
syscall!(Sbrk);
syscall!(Sleep);
syscall!(AlarmSet);
syscall!(AlarmClear);
syscall!(SignalReturn);
syscall!(GetSystemInfo);
syscall!(Reboot);
syscall!(Halt);
syscall!(Abort);
syscall!(Trace);
syscall!(DumpKernelPageTable);
syscall!(DumpUserPageTable);
