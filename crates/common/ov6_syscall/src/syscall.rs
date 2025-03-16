use core::{convert::Infallible, time::Duration};

use ov6_types::{fs::RawFd, process::ProcId};

use crate::{
    OpenFlags, Stat, Syscall, SyscallCode, UserMutRef, UserMutSlice, UserSlice, error::SyscallError,
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
syscall!(Exec => fn(UserSlice<u8>, UserSlice<UserSlice<u8>>) -> Result<Infallible, SyscallError>);
syscall!(Fstat => fn(RawFd, UserMutRef<Stat>) -> Result<(), SyscallError>);
syscall!(Chdir => fn(UserSlice<u8>) -> Result<(), SyscallError>);
syscall!(Dup => fn(RawFd) -> Result<RawFd, SyscallError>);
syscall!(Getpid => fn() -> ProcId);
syscall!(Sbrk => fn(isize) -> Result<usize, SyscallError>);
syscall!(Sleep => fn(Duration) -> Result<(), SyscallError>);
syscall!(Open => fn(UserSlice<u8>, OpenFlags) -> Result<RawFd, SyscallError>);
syscall!(Write => fn(RawFd, UserSlice<u8>) -> Result<usize, SyscallError>);
syscall!(Mknod => fn(UserSlice<u8>, u32, i16) -> Result<(), SyscallError>);
syscall!(Unlink => fn(UserSlice<u8>) -> Result<(), SyscallError>);
syscall!(Link => fn(UserSlice<u8>, UserSlice<u8>) -> Result<(), SyscallError>);
syscall!(Mkdir => fn(UserSlice<u8>) -> Result<(), SyscallError>);
syscall!(Close => fn(RawFd) -> Result<(), SyscallError>);
syscall!(Reboot => fn() -> Result<Infallible, SyscallError>);
syscall!(Halt => fn(u16) -> Result<Infallible, SyscallError>);
syscall!(Abort => fn(u16) -> Result<Infallible, SyscallError>);
