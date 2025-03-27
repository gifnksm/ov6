use core::{convert::Infallible, time::Duration};

use ov6_types::{fs::RawFd, process::ProcId};

use crate::{
    OpenFlags, Stat, Syscall, SyscallCode, UserMutRef, UserMutSlice, UserSlice, WaitTarget,
    error::SyscallError,
};

macro_rules! syscall {
    ($( struct $name:ident (fn($($arg:ty),* $(,)?) -> $ret:ty ) ;) *) => {
        $(
            pub struct $name {}

            impl Syscall for $name {
                type Arg = ( $($arg ,)* );
                type Return = $ret;

                const CODE: SyscallCode = SyscallCode::$name;
            }
        )*
    };
}

syscall! {
    struct Fork(fn() -> Result<Option<ProcId>, SyscallError>);
    struct Exit(fn(i32) -> Infallible);
    struct Wait(fn(WaitTarget, UserMutRef<i32>) -> Result<ProcId, SyscallError>);
    struct Pipe(fn(UserMutRef<[RawFd; 2]>) -> Result<(), SyscallError>);
    struct Read(fn(RawFd, UserMutSlice<u8>) -> Result<usize, SyscallError>);
    struct Kill(fn(ProcId) -> Result<(), SyscallError>);
    struct Exec(fn(UserSlice<u8>, UserSlice<UserSlice<u8>>) -> Result<Infallible, SyscallError>);
    struct Fstat(fn(RawFd, UserMutRef<Stat>) -> Result<(), SyscallError>);
    struct Chdir(fn(UserSlice<u8>) -> Result<(), SyscallError>);
    struct Dup(fn(RawFd) -> Result<RawFd, SyscallError>);
    struct Sbrk(fn(isize) -> Result<usize, SyscallError>);
    struct Sleep(fn(Duration) -> Result<(), SyscallError>);
    struct Open(fn(UserSlice<u8>, OpenFlags) -> Result<RawFd, SyscallError>);
    struct Write(fn(RawFd, UserSlice<u8>) -> Result<usize, SyscallError>);
    struct Mknod(fn(UserSlice<u8>, u32, u16) -> Result<(), SyscallError>);
    struct Unlink(fn(UserSlice<u8>) -> Result<(), SyscallError>);
    struct Link(fn(UserSlice<u8>, UserSlice<u8>) -> Result<(), SyscallError>);
    struct Mkdir(fn(UserSlice<u8>) -> Result<(), SyscallError>);
    struct Close(fn(RawFd) -> Result<(), SyscallError>);
    struct Reboot(fn() -> Result<Infallible, SyscallError>);
    struct Halt(fn(u16) -> Result<Infallible, SyscallError>);
    struct Abort(fn(u16) -> Result<Infallible, SyscallError>);
    struct Trace(fn(u64) -> ());
    struct DumpKernelPageTable(fn() -> ());
    struct DumpUserPageTable(fn() -> ());
}
