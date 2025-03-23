#![no_std]

use core::{any, convert::Infallible, fmt, marker::PhantomData, num::TryFromIntError, ptr};

use bitflags::bitflags;
use dataview::Pod;
use ov6_types::process::ProcId;
use strum::{Display, EnumString, FromRepr};

pub mod error;
mod register;
pub mod syscall;

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
#[derive(Debug, Pod)]
pub struct Stat {
    /// File system's disk device
    pub dev: u32,
    /// Inode number
    pub ino: u32,
    /// Type of file
    pub ty: u16,
    /// Number of links to file
    pub nlink: u16,
    pub padding: [u8; 4],
    /// Size of file in bytes
    pub size: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, FromRepr)]
#[repr(u16)]
pub enum StatType {
    Dir = 1,
    File,
    Dev,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WaitTarget {
    AnyProcess,
    Process(ProcId),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, FromRepr, EnumString, Display)]
#[repr(usize)]
#[strum(serialize_all = "snake_case")]
#[strum(ascii_case_insensitive)]
pub enum SyscallCode {
    Fork = 1,
    Exit,
    Wait,
    Pipe,
    Read,
    Kill,
    Exec,
    Fstat,
    Chdir,
    Dup,
    Getpid,
    Sbrk,
    Sleep,
    Open,
    Write,
    Mknod,
    Unlink,
    Link,
    Mkdir,
    Close,
    Reboot,
    Halt,
    Abort,
    Trace,
}

pub trait Syscall {
    const CODE: SyscallCode;
    type Arg: RegisterValue;
    type Return: RegisterValue;
}

pub struct UserRef<T>
where
    T: ?Sized + 'static,
{
    addr: usize,
    _phantom: PhantomData<&'static T>,
}

impl<T> fmt::Debug for UserRef<T>
where
    T: ?Sized,
{
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:#x} as &{}", self.addr, any::type_name::<T>())
    }
}

impl<T> UserRef<T>
where
    T: ?Sized,
{
    pub fn new(r: &T) -> Self {
        Self {
            addr: ptr::from_ref(r).addr(),
            _phantom: PhantomData,
        }
    }

    #[must_use]
    pub fn addr(&self) -> usize {
        self.addr
    }

    #[must_use]
    pub const fn size(&self) -> usize
    where
        T: Sized,
    {
        size_of::<T>()
    }

    #[must_use]
    pub fn as_bytes(&self) -> UserSlice<u8>
    where
        T: Pod + Sized,
    {
        UserSlice {
            addr: self.addr,
            len: size_of::<T>(),
            _phantom: PhantomData,
        }
    }
}

pub struct UserMutRef<T>
where
    T: ?Sized + 'static,
{
    addr: usize,
    _phantom: PhantomData<&'static mut T>,
}

impl<T> fmt::Debug for UserMutRef<T>
where
    T: ?Sized,
{
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:#x} as &mut {}", self.addr, any::type_name::<T>())
    }
}

impl<T> UserMutRef<T>
where
    T: ?Sized,
{
    pub fn new(r: &mut T) -> Self {
        Self {
            addr: ptr::from_mut(r).addr(),
            _phantom: PhantomData,
        }
    }

    #[must_use]
    pub fn addr(&self) -> usize {
        self.addr
    }

    #[must_use]
    pub const fn size(&self) -> usize
    where
        T: Sized,
    {
        size_of::<T>()
    }

    #[must_use]
    pub fn as_bytes_mut(&mut self) -> UserMutSlice<u8>
    where
        T: Pod + Sized,
    {
        UserMutSlice {
            addr: self.addr,
            len: size_of::<T>(),
            _phantom: PhantomData,
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
#[repr(C)]
pub struct UserSlice<T> {
    addr: usize,
    len: usize,
    _phantom: PhantomData<T>,
}

unsafe impl<T> Pod for UserSlice<T> where T: Pod {}

impl<T> fmt::Debug for UserSlice<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{:#x} as &[{}; {}]",
            self.addr,
            any::type_name::<T>(),
            self.len
        )
    }
}

impl<T> UserSlice<T> {
    #[must_use]
    pub fn new(s: &[T]) -> Self {
        Self {
            addr: s.as_ptr().addr(),
            len: s.len(),
            _phantom: PhantomData,
        }
    }

    #[must_use]
    pub const fn from_raw_parts(addr: usize, len: usize) -> Self {
        Self {
            addr,
            len,
            _phantom: PhantomData,
        }
    }

    #[must_use]
    pub const fn addr(&self) -> usize {
        self.addr
    }

    #[expect(clippy::len_without_is_empty)]
    #[must_use]
    pub const fn len(&self) -> usize {
        self.len
    }

    #[must_use]
    pub const fn size(&self) -> Option<usize>
    where
        T: Sized,
    {
        size_of::<T>().checked_mul(self.len)
    }

    #[must_use]
    #[track_caller]
    pub const fn cast<U>(&self) -> UserSlice<U> {
        assert!(self.addr() % align_of::<U>() == 0);
        assert!(self.size().unwrap() % size_of::<U>() == 0);

        UserSlice {
            addr: self.addr,
            len: self.size().unwrap() / size_of::<U>(),
            _phantom: PhantomData,
        }
    }

    #[must_use]
    #[track_caller]
    pub const fn nth(&self, n: usize) -> UserRef<T> {
        assert!(n < self.len());
        UserRef {
            addr: self.addr + n * size_of::<T>(),
            _phantom: PhantomData,
        }
    }

    #[must_use]
    #[track_caller]
    pub const fn skip(&self, amt: usize) -> Self {
        assert!(amt <= self.len);
        Self {
            addr: self.addr + amt,
            len: self.len - amt,
            _phantom: PhantomData,
        }
    }

    #[must_use]
    #[track_caller]
    pub const fn take(&self, amt: usize) -> Self {
        assert!(amt <= self.len);
        Self {
            addr: self.addr,
            len: amt,
            _phantom: PhantomData,
        }
    }
}

#[derive(PartialEq, Eq)]
#[repr(C)]
pub struct UserMutSlice<T> {
    addr: usize,
    len: usize,
    _phantom: PhantomData<T>,
}

unsafe impl<T> Pod for UserMutSlice<T> where T: Pod {}

impl<T> fmt::Debug for UserMutSlice<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{:#x} as &mut [{}; {}]",
            self.addr,
            any::type_name::<T>(),
            self.len
        )
    }
}

impl<T> UserMutSlice<T> {
    #[must_use]
    pub fn new(s: &mut [T]) -> Self {
        Self {
            addr: s.as_mut_ptr().addr(),
            len: s.len(),
            _phantom: PhantomData,
        }
    }

    #[must_use]
    pub const fn from_raw_parts(addr: usize, len: usize) -> Self {
        Self {
            addr,
            len,
            _phantom: PhantomData,
        }
    }

    #[must_use]
    pub const fn addr(&self) -> usize {
        self.addr
    }

    #[expect(clippy::len_without_is_empty)]
    #[must_use]
    pub const fn len(&self) -> usize {
        self.len
    }

    #[must_use]
    pub const fn size(&self) -> Option<usize>
    where
        T: Sized,
    {
        size_of::<T>().checked_mul(self.len)
    }

    #[must_use]
    #[track_caller]
    pub const fn cast_mut<U>(&mut self) -> UserMutSlice<U> {
        assert!(self.addr() % align_of::<U>() == 0);
        assert!(self.size().unwrap() % size_of::<U>() == 0);

        UserMutSlice {
            addr: self.addr,
            len: self.size().unwrap() / size_of::<U>(),
            _phantom: PhantomData,
        }
    }

    #[must_use]
    #[track_caller]
    pub const fn nth_mut(&mut self, n: usize) -> UserMutRef<T> {
        assert!(n < self.len());
        UserMutRef {
            addr: self.addr + n * size_of::<T>(),
            _phantom: PhantomData,
        }
    }

    #[must_use]
    #[track_caller]
    pub const fn skip_mut(&mut self, amt: usize) -> Self {
        assert!(amt <= self.len);
        Self {
            addr: self.addr + amt,
            len: self.len - amt,
            _phantom: PhantomData,
        }
    }

    #[must_use]
    #[track_caller]
    pub const fn take_mut(&mut self, amt: usize) -> Self {
        assert!(amt <= self.len);
        Self {
            addr: self.addr,
            len: amt,
            _phantom: PhantomData,
        }
    }
}

pub type ArgType<T> = <T as Syscall>::Arg;
pub type ArgTypeRepr<T> = <<T as Syscall>::Arg as RegisterValue>::Repr;
pub type ReturnType<T> = <T as Syscall>::Return;
pub type ReturnTypeRepr<T> = <<T as Syscall>::Return as RegisterValue>::Repr;

#[must_use]
#[repr(C)]
#[derive(Debug, PartialEq, Eq)]
pub struct Register<T, const N: usize> {
    pub a: [usize; N],
    _phantom: PhantomData<T>,
}

impl<T, const N: usize> Copy for Register<T, N> {}
impl<T, const N: usize> Clone for Register<T, N> {
    fn clone(&self) -> Self {
        *self
    }
}

#[derive(Debug, thiserror::Error)]
pub enum RegisterDecodeError {
    #[error("int conversion: {0}")]
    IntConversion(#[from] TryFromIntError),
    #[error("invalid syscall error number: {0}")]
    InvalidSyscallErrorNo(isize),
    #[error("invalid open flags: {0:#x}")]
    InvalidOpenFlags(usize),
    #[error("invalid result designator: {0:#x}")]
    InvalidDesignator(usize),
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
