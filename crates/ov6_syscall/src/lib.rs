#![no_std]

use core::{convert::Infallible, marker::PhantomData};

use bitflags::bitflags;
use dataview::Pod;
use strum::FromRepr;

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
    pub _pad: [u8; 4],
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
    type Return: ReturnValueConvert;
}

macro_rules! syscall {
    ($name:ident => fn(..) -> $ret:ty) => {
        pub struct $name {}

        impl Syscall for $name {
            type Return = $ret;

            const CODE: SyscallCode = SyscallCode::$name;
        }
    };
}

pub type ReturnType<T> = <T as Syscall>::Return;
pub type ReturnTypeRepr<T> = <<T as Syscall>::Return as ReturnValueConvert>::Repr;

pub mod syscall {
    use super::*;

    syscall!(Fork => fn(..) -> Result<usize, SyscallError>);
    syscall!(Exit => fn(..) -> Infallible);
    syscall!(Wait => fn(..) -> Result<usize, SyscallError>);
    syscall!(Pipe => fn(..) -> Result<usize, SyscallError>);
    syscall!(Read => fn(..) -> Result<usize, SyscallError>);
    syscall!(Kill => fn(..) -> Result<usize, SyscallError>);
    syscall!(Exec => fn(..) -> Result<usize, SyscallError>);
    syscall!(Fstat => fn(..) -> Result<usize, SyscallError>);
    syscall!(Chdir => fn(..) -> Result<usize, SyscallError>);
    syscall!(Dup => fn(..) -> Result<usize, SyscallError>);
    syscall!(Getpid => fn(..) -> Result<usize, SyscallError>);
    syscall!(Sbrk => fn(..) -> Result<usize, SyscallError>);
    syscall!(Sleep => fn(..) -> Result<usize, SyscallError>);
    syscall!(Uptime => fn(..) -> Result<usize, SyscallError>);
    syscall!(Open => fn(..) -> Result<usize, SyscallError>);
    syscall!(Write => fn(..) -> Result<usize, SyscallError>);
    syscall!(Mknod => fn(..) -> Result<usize, SyscallError>);
    syscall!(Unlink => fn(..) -> Result<usize, SyscallError>);
    syscall!(Link => fn(..) -> Result<usize, SyscallError>);
    syscall!(Mkdir => fn(..) -> Result<usize, SyscallError>);
    syscall!(Close => fn(..) -> Result<usize, SyscallError>);
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, FromRepr)]
#[repr(isize)]
pub enum SyscallError {
    Unknown = -1,
}

#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RetInfailible {
    _dummy: usize, // zero-sized-type is not FFI-safe
}

#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Ret1<T> {
    pub a0: usize,
    _phantom: PhantomData<T>,
}

impl<T> Ret1<T> {
    fn new(a0: usize) -> Self {
        Self {
            a0,
            _phantom: PhantomData,
        }
    }

    #[must_use]
    pub fn decode(self) -> T
    where
        T: ReturnValueConvert<Repr = Self>,
    {
        T::decode(self)
    }
}

pub trait ReturnValueConvert {
    type Repr;

    fn encode(self) -> Self::Repr;
    fn decode(repr: Self::Repr) -> Self;
}

impl ReturnValueConvert for Result<usize, SyscallError> {
    type Repr = Ret1<Self>;

    fn encode(self) -> Self::Repr {
        match self {
            Ok(a) => {
                assert!(a.cast_signed() >= 0);
                Ret1::new(a)
            }
            Err(e) => {
                let e = e as isize;
                assert!(e < 0);
                Ret1::new(e.cast_unsigned())
            }
        }
    }

    fn decode(repr: Self::Repr) -> Self {
        let s = repr.a0.cast_signed();
        if s >= 0 {
            return Ok(repr.a0);
        }
        let e = SyscallError::from_repr(s).ok_or(SyscallError::Unknown)?;
        Err(e)
    }
}

impl ReturnValueConvert for Infallible {
    type Repr = RetInfailible;

    fn encode(self) -> Self::Repr {
        unreachable!()
    }

    fn decode(_repr: Self::Repr) -> Self {
        unreachable!()
    }
}
