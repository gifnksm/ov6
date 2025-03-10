use core::{
    convert::Infallible,
    ffi::{CStr, c_char},
};

use ov6_types::{fs::RawFd, process::ProcId};

use crate::{
    OpenFlags, Stat, Syscall, SyscallCode, UserMutRef, UserMutSlice, UserRef, UserSlice,
    error::SyscallError,
};

macro_rules! syscall {
        ($name:ident => fn($arg:ty $(,)?) -> $ret:ty) => {
            pub struct $name {}

            impl Syscall for $name {
                type Arg = ( $arg ,);
                type Return = $ret;

                const CODE: SyscallCode = SyscallCode::$name;
            }
        };
        ($name:ident => fn($($arg:ty),* $(,)?) -> $ret:ty) => {
            pub struct $name {}

            impl Syscall for $name {
                type Arg = ( $($arg),* );
                type Return = $ret;

                const CODE: SyscallCode = SyscallCode::$name;
            }
        };
    }

syscall!(Fork => fn() -> Result<Option<ProcId>, SyscallError>);
syscall!(Exit => fn(i32) -> Infallible);
syscall!(Wait => fn(UserMutRef<i32>) -> Result<ProcId, SyscallError>);
syscall!(Pipe => fn(UserMutRef<[RawFd; 2]>) -> Result<(), SyscallError>);
syscall!(Read => fn(RawFd, UserMutSlice<u8>) -> Result<usize, SyscallError>);
syscall!(Kill => fn(ProcId) -> Result<(), SyscallError>);
syscall!(Exec => fn(UserRef<CStr>, UserSlice<*const c_char>) -> Result<Infallible, SyscallError>);
syscall!(Fstat => fn(RawFd, UserMutRef<Stat>) -> Result<(), SyscallError>);
syscall!(Chdir => fn(UserRef<CStr>) -> Result<(), SyscallError>);
syscall!(Dup => fn(RawFd) -> Result<RawFd, SyscallError>);
syscall!(Getpid => fn() -> ProcId);
syscall!(Sbrk => fn(isize) -> Result<usize, SyscallError>);
syscall!(Sleep => fn(u64) -> ());
syscall!(Uptime => fn() -> u64);
syscall!(Open => fn(UserRef<CStr>, OpenFlags) -> Result<RawFd, SyscallError>);
syscall!(Write => fn(RawFd, UserSlice<u8>) -> Result<usize, SyscallError>);
syscall!(Mknod => fn(UserRef<CStr>, u32, i16) -> Result<(), SyscallError>);
syscall!(Unlink => fn(UserRef<CStr>) -> Result<(), SyscallError>);
syscall!(Link => fn(UserRef<CStr>, UserRef<CStr>) -> Result<(), SyscallError>);
syscall!(Mkdir => fn(UserRef<CStr>) -> Result<(), SyscallError>);
syscall!(Close => fn(RawFd) -> Result<(), SyscallError>);
