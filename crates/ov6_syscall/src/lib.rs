#![no_std]

use core::{convert::Infallible, fmt, marker::PhantomData, num::TryFromIntError};

use bitflags::bitflags;
use dataview::Pod;
use strum::FromRepr;

mod register;

bitflags! {
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    #[repr(transparent)]
    pub struct OpenFlags: usize {
        const READ_ONLY = 0x000;
        const WRITE_ONLY = 0x001;
        const READ_WRITE = 0x002;
        const CREATE = 0x200;
        const TRUNC = 0x400;
    }
}

#[repr(C)]
#[derive(Pod)]
pub struct Stat {
    /// File system's disk device
    pub dev: i32,
    /// Inode number
    pub ino: u32,
    /// Type of file
    pub ty: i16,
    /// Number of links to file
    pub nlink: i16,
    pub padding: [u8; 4],
    /// Size of file in bytes
    pub size: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, FromRepr)]
#[repr(i16)]
pub enum StatType {
    Dir = 1,
    File = 2,
    Dev = 3,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, FromRepr)]
#[repr(usize)]
pub enum SyscallCode {
    Fork = 1,
    Exit = 2,
    Wait = 3,
    Pipe = 4,
    Read = 5,
    Kill = 6,
    Exec = 7,
    Fstat = 8,
    Chdir = 9,
    Dup = 10,
    Getpid = 11,
    Sbrk = 12,
    Sleep = 13,
    Uptime = 14,
    Open = 15,
    Write = 16,
    Mknod = 17,
    Unlink = 18,
    Link = 19,
    Mkdir = 20,
    Close = 21,
}

pub trait Syscall {
    const CODE: SyscallCode;
    type Arg: RegisterValue;
    type Return: RegisterValue;
}

pub type ArgType<T> = <T as Syscall>::Arg;
pub type ArgTypeRepr<T> = <<T as Syscall>::Arg as RegisterValue>::Repr;
pub type ReturnType<T> = <T as Syscall>::Return;
pub type ReturnTypeRepr<T> = <<T as Syscall>::Return as RegisterValue>::Repr;

#[must_use]
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Register<T, const N: usize> {
    pub a: [usize; N],
    _phantom: PhantomData<T>,
}

#[derive(Debug, thiserror::Error)]
pub enum RegisterDecodeError {
    #[error("int conversion: {0}")]
    IntConversion(#[from] TryFromIntError),
    #[error("invalid syscall error number: {0}")]
    InvalidSyscallErrorNo(isize),
    #[error("unexpected zero")]
    UnexpectedZero,
}

impl From<Infallible> for RegisterDecodeError {
    fn from(_: Infallible) -> Self {
        unreachable!()
    }
}

pub trait RegisterValue
where
    Self: Sized,
{
    type DecodeError: fmt::Debug;
    type Repr;

    fn encode(self) -> Self::Repr;
    fn try_decode(repr: Self::Repr) -> Result<Self, Self::DecodeError>;
}

pub mod syscall {
    use core::convert::Infallible;

    use ov6_types::{fs::RawFd, process::ProcId};

    use crate::{Syscall, SyscallCode, SyscallError};

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
        ($name:ident => fn(..) -> $ret:ty) => {
            pub struct $name {}

            impl Syscall for $name {
                type Arg = ();
                type Return = $ret;

                const CODE: SyscallCode = SyscallCode::$name;
            }
        };
    }

    syscall!(Fork => fn() -> Result<Option<ProcId>, SyscallError>);
    syscall!(Exit => fn(i32) -> Infallible);
    syscall!(Wait => fn(..) -> Result<ProcId, SyscallError>);
    syscall!(Pipe => fn(..) -> Result<(), SyscallError>);
    syscall!(Read => fn(..) -> Result<usize, SyscallError>);
    syscall!(Kill => fn(..) -> Result<(), SyscallError>);
    syscall!(Exec => fn(..) -> Result<Infallible, SyscallError>);
    syscall!(Fstat => fn(..) -> Result<(), SyscallError>);
    syscall!(Chdir => fn(..) -> Result<(), SyscallError>);
    syscall!(Dup => fn(..) -> Result<RawFd, SyscallError>);
    syscall!(Getpid => fn(..) -> ProcId);
    syscall!(Sbrk => fn(..) -> Result<usize, SyscallError>);
    syscall!(Sleep => fn(..) -> ());
    syscall!(Uptime => fn(..) -> u64);
    syscall!(Open => fn(..) -> Result<RawFd, SyscallError>);
    syscall!(Write => fn(..) -> Result<usize, SyscallError>);
    syscall!(Mknod => fn(..) -> Result<(), SyscallError>);
    syscall!(Unlink => fn(..) -> Result<(), SyscallError>);
    syscall!(Link => fn(..) -> Result<(), SyscallError>);
    syscall!(Mkdir => fn(..) -> Result<(), SyscallError>);
    syscall!(Close => fn(..) -> Result<(), SyscallError>);
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, FromRepr, thiserror::Error)]
#[repr(isize)]
pub enum SyscallError {
    #[error("unknown error")]
    Unknown = -1,
}
