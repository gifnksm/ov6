#![no_std]

use core::{convert::Infallible, marker::PhantomData, num::NonZero};

use bitflags::bitflags;
use dataview::Pod;
use ov6_types::{fs::RawFd, process::ProcId};
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
    type Return: ReturnValueConvert;
}

pub type ReturnType<T> = <T as Syscall>::Return;
pub type ReturnTypeRepr<T> = <<T as Syscall>::Return as ReturnValueConvert>::Repr;

pub mod syscall {
    use core::convert::Infallible;

    use ov6_types::{fs::RawFd, process::ProcId};

    use crate::{Syscall, SyscallCode, SyscallError};

    macro_rules! syscall {
        ($name:ident => fn(..) -> $ret:ty) => {
            pub struct $name {}

            impl Syscall for $name {
                type Return = $ret;

                const CODE: SyscallCode = SyscallCode::$name;
            }
        };
    }

    syscall!(Fork => fn(..) -> Result<Option<ProcId>, SyscallError>);
    syscall!(Exit => fn(..) -> Infallible);
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

#[derive(Debug, Clone, Copy, PartialEq, Eq, FromRepr)]
#[repr(isize)]
pub enum SyscallError {
    Unknown = -1,
}

#[must_use]
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Ret0<T> {
    _dummy: usize, // zero-sized-type is not FFI-safe
    _phantom: PhantomData<T>,
}

impl<T> Ret0<T> {
    fn new() -> Self {
        Self {
            _dummy: 0,
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

#[must_use]
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

impl ReturnValueConvert for Result<(), SyscallError> {
    type Repr = Ret1<Self>;

    fn encode(self) -> Self::Repr {
        match self {
            Ok(()) => Ret1::new(0),
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
            assert_eq!(s, 0);
            return Ok(());
        }
        let e = SyscallError::from_repr(s).ok_or(SyscallError::Unknown)?;
        Err(e)
    }
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

impl ReturnValueConvert for Result<Option<ProcId>, SyscallError> {
    type Repr = Ret1<Self>;

    fn encode(self) -> Self::Repr {
        match self {
            Ok(Some(pid)) => Ret1::new(pid.get().get().try_into().unwrap()),
            Ok(None) => Ret1::new(0),
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
            return Ok(NonZero::new(s.try_into().unwrap()).map(ProcId::new));
        }
        let e = SyscallError::from_repr(s).ok_or(SyscallError::Unknown)?;
        Err(e)
    }
}

impl ReturnValueConvert for Result<ProcId, SyscallError> {
    type Repr = Ret1<Self>;

    fn encode(self) -> Self::Repr {
        match self {
            Ok(pid) => Ret1::new(pid.get().get().try_into().unwrap()),
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
            return Ok(ProcId::new(
                NonZero::new(u32::try_from(s).unwrap()).unwrap(),
            ));
        }
        let e = SyscallError::from_repr(s).ok_or(SyscallError::Unknown)?;
        Err(e)
    }
}

impl ReturnValueConvert for Result<Infallible, SyscallError> {
    type Repr = Ret1<Self>;

    fn encode(self) -> Self::Repr {
        match self {
            Err(e) => {
                let e = e as isize;
                assert!(e < 0);
                Ret1::new(e.cast_unsigned())
            }
        }
    }

    fn decode(repr: Self::Repr) -> Self {
        let s = repr.a0.cast_signed();
        assert!(s < 0);
        let e = SyscallError::from_repr(s).ok_or(SyscallError::Unknown)?;
        Err(e)
    }
}

impl ReturnValueConvert for Result<RawFd, SyscallError> {
    type Repr = Ret1<Self>;

    fn encode(self) -> Self::Repr {
        match self {
            Ok(fd) => {
                assert!(fd.get().cast_signed() >= 0);
                Ret1::new(fd.get())
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
            return Ok(RawFd::new(repr.a0));
        }
        let e = SyscallError::from_repr(s).ok_or(SyscallError::Unknown)?;
        Err(e)
    }
}

impl ReturnValueConvert for Infallible {
    type Repr = Ret0<Self>;

    fn encode(self) -> Self::Repr {
        unreachable!()
    }

    fn decode(_repr: Self::Repr) -> Self {
        unreachable!()
    }
}

impl ReturnValueConvert for () {
    type Repr = Ret0<()>;

    fn encode(self) -> Self::Repr {
        Ret0::new()
    }

    fn decode(_: Self::Repr) -> Self {}
}

impl ReturnValueConvert for u64 {
    type Repr = Ret1<Self>;

    fn encode(self) -> Self::Repr {
        Ret1::new(self.try_into().unwrap())
    }

    fn decode(repr: Self::Repr) -> Self {
        repr.a0.try_into().unwrap()
    }
}

impl ReturnValueConvert for ProcId {
    type Repr = Ret1<Self>;

    fn encode(self) -> Self::Repr {
        Ret1::new(self.get().get().try_into().unwrap())
    }

    fn decode(repr: Self::Repr) -> Self {
        Self::new(NonZero::new(u32::try_from(repr.a0).unwrap()).unwrap())
    }
}
