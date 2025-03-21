use core::{
    cmp, fmt,
    num::NonZero,
    ops::Range,
    ptr::{self, NonNull},
};

use dataview::Pod;
use ov6_syscall::{UserMutRef, UserMutSlice, UserRef, UserSlice};

use super::{PAGE_SHIFT, PAGE_SIZE, vm_user::UserPageTable};
use crate::error::KernelError;

pub const fn page_roundup(addr: usize) -> usize {
    (addr + PAGE_SIZE - 1) & !(PAGE_SIZE - 1)
}

pub const fn page_rounddown(addr: usize) -> usize {
    addr & !(PAGE_SIZE - 1)
}

pub const fn is_page_aligned(addr: usize) -> bool {
    addr % PAGE_SIZE == 0
}

pub trait PageRound {
    fn page_roundup(&self) -> Self;
    fn page_rounddown(&self) -> Self;
    fn is_page_aligned(&self) -> bool;
}

impl PageRound for usize {
    fn page_roundup(&self) -> Self {
        page_roundup(*self)
    }

    fn page_rounddown(&self) -> Self {
        page_rounddown(*self)
    }

    fn is_page_aligned(&self) -> bool {
        is_page_aligned(*self)
    }
}

impl PageRound for NonZero<usize> {
    fn page_roundup(&self) -> Self {
        Self::new(page_roundup(self.get())).unwrap()
    }

    fn page_rounddown(&self) -> Self {
        Self::new(page_rounddown(self.get())).unwrap()
    }

    fn is_page_aligned(&self) -> bool {
        is_page_aligned(self.get())
    }
}

impl<T> PageRound for NonNull<T> {
    fn page_roundup(&self) -> Self {
        self.map_addr(|a| a.page_roundup())
    }

    fn page_rounddown(&self) -> Self {
        self.map_addr(|a| a.page_rounddown())
    }

    fn is_page_aligned(&self) -> bool {
        is_page_aligned(self.as_ptr().addr())
    }
}

impl PageRound for VirtAddr {
    fn page_roundup(&self) -> Self {
        self.map_addr(page_roundup).unwrap()
    }

    fn page_rounddown(&self) -> Self {
        self.map_addr(page_rounddown).unwrap()
    }

    fn is_page_aligned(&self) -> bool {
        is_page_aligned(self.addr())
    }
}

impl PageRound for PhysAddr {
    fn page_roundup(&self) -> Self {
        self.map_addr(page_roundup)
    }

    fn page_rounddown(&self) -> Self {
        self.map_addr(page_rounddown)
    }

    fn is_page_aligned(&self) -> bool {
        is_page_aligned(self.addr())
    }
}

struct Hex(usize);
impl fmt::Debug for Hex {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::LowerHex::fmt(&self.0, f)
    }
}

macro_rules! impl_fmt {
    ($ty:ident) => {
        impl fmt::Debug for $ty {
            fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
                f.debug_tuple(stringify!($ty)).field(&Hex(self.0)).finish()
            }
        }
        impl fmt::LowerHex for $ty {
            fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
                fmt::LowerHex::fmt(&self.0, f)
            }
        }
        impl fmt::UpperHex for $ty {
            fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
                fmt::UpperHex::fmt(&self.0, f)
            }
        }
    };
}

/// Physical Page Number of a page
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct VirtPageNum(usize);
impl_fmt!(VirtPageNum);

/// Virtual address
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct VirtAddr(usize);
impl_fmt!(VirtAddr);

/// Physical Page Number of a page
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct PhysPageNum(usize);
impl_fmt!(PhysPageNum);

/// Physical Address
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct PhysAddr(usize);
impl_fmt!(PhysAddr);

impl VirtPageNum {
    pub const MAX: Self = Self(1 << (9 * 3 - 1));
    pub const MIN: Self = Self(0);
}

impl VirtAddr {
    /// One beyond the highest possible virtual address.
    ///
    /// [`VirtAddr::MAX`] is actually one bit less than the max allowed by
    /// Sv39, to avoid having to sign-extend virtual addresses
    /// that have the high bit set.
    pub const MAX: Self = Self(1 << (9 * 3 + PAGE_SHIFT - 1));
    pub const MIN: Self = Self(0);
}

const _: () = {
    assert!(VirtAddr::MAX.0 == VirtPageNum::MAX.0 << PAGE_SHIFT);
    assert!(VirtAddr::MIN.0 == VirtPageNum::MIN.0 << PAGE_SHIFT);
};

impl VirtPageNum {
    pub const fn new(vpn: usize) -> Result<Self, KernelError> {
        if vpn > Self::MAX.0 {
            return Err(KernelError::TooLargeVirtualPageNumber(vpn));
        }
        Ok(Self(vpn))
    }

    pub const fn virt_addr(self) -> VirtAddr {
        VirtAddr(self.0 << PAGE_SHIFT)
    }

    pub const fn value(self) -> usize {
        self.0
    }

    pub const fn checked_add(self, n: usize) -> Result<Self, KernelError> {
        let Some(vpn) = self.0.checked_add(n) else {
            return Err(KernelError::TooLargeVirtualPageNumber(usize::MAX));
        };
        Self::new(vpn)
    }
}

impl VirtAddr {
    pub const fn new(addr: usize) -> Result<Self, KernelError> {
        if addr > Self::MAX.0 {
            return Err(KernelError::TooLargeVirtualAddress(addr));
        }
        Ok(Self(addr))
    }

    pub const fn addr(self) -> usize {
        self.0
    }

    pub fn virt_page_num(self) -> VirtPageNum {
        VirtPageNum(self.0 >> PAGE_SHIFT)
    }

    pub const fn byte_add(self, offset: usize) -> Result<Self, KernelError> {
        let Some(addr) = self.0.checked_add(offset) else {
            return Err(KernelError::TooLargeVirtualAddress(usize::MAX));
        };
        Self::new(addr)
    }

    pub const fn byte_sub(self, offset: usize) -> Result<Self, KernelError> {
        let Some(addr) = self.0.checked_sub(offset) else {
            return Err(KernelError::VirtualAddressUnderflow);
        };
        Self::new(addr)
    }

    pub fn map_addr(self, f: impl FnOnce(usize) -> usize) -> Result<Self, KernelError> {
        Self::new(f(self.0))
    }
}

impl PhysPageNum {
    pub const fn new(value: usize) -> Self {
        Self(value)
    }

    pub const fn phys_addr(self) -> PhysAddr {
        PhysAddr(self.0 << PAGE_SHIFT)
    }

    pub const fn value(self) -> usize {
        self.0
    }

    pub fn checked_add(self, n: usize) -> Option<Self> {
        self.0.checked_add(n).map(Self)
    }

    pub fn as_ptr(self) -> NonNull<[u8; PAGE_SIZE]> {
        let addr = self.phys_addr().addr();
        NonNull::new(ptr::with_exposed_provenance_mut(addr)).unwrap()
    }
}

impl PhysAddr {
    pub const fn new(addr: usize) -> Self {
        Self(addr)
    }

    pub fn addr(self) -> usize {
        self.0
    }

    pub fn as_ptr<T>(self) -> *const T {
        ptr::with_exposed_provenance(self.0)
    }

    pub fn as_mut_ptr<T>(self) -> NonNull<T> {
        NonNull::new(ptr::with_exposed_provenance_mut(self.0)).unwrap()
    }

    pub fn phys_page_num(self) -> PhysPageNum {
        PhysPageNum(self.0 >> PAGE_SHIFT)
    }

    pub fn map_addr(self, f: impl FnOnce(usize) -> usize) -> Self {
        Self(f(self.0))
    }
}

pub trait TryAsVirtAddrRange {
    fn try_as_va_range(&self) -> Result<Range<VirtAddr>, KernelError>;
}

impl TryAsVirtAddrRange for Range<VirtAddr> {
    fn try_as_va_range(&self) -> Result<Range<VirtAddr>, KernelError> {
        Ok(self.clone())
    }
}

impl<T> TryAsVirtAddrRange for UserRef<T> {
    fn try_as_va_range(&self) -> Result<Range<VirtAddr>, KernelError> {
        let start = VirtAddr::new(self.addr())?;
        let end = start.byte_add(self.size())?;
        Ok(start..end)
    }
}

impl<T> TryAsVirtAddrRange for UserMutRef<T> {
    fn try_as_va_range(&self) -> Result<Range<VirtAddr>, KernelError> {
        let start = VirtAddr::new(self.addr())?;
        let end = start.byte_add(self.size())?;
        Ok(start..end)
    }
}

impl<T> TryAsVirtAddrRange for UserSlice<T> {
    fn try_as_va_range(&self) -> Result<Range<VirtAddr>, KernelError> {
        let size = self
            .size()
            .ok_or(KernelError::TooLargeVirtualAddress(usize::MAX))?;
        let start = VirtAddr::new(self.addr())?;
        let end = start.byte_add(size)?;
        Ok(start..end)
    }
}

impl<T> TryAsVirtAddrRange for UserMutSlice<T> {
    fn try_as_va_range(&self) -> Result<Range<VirtAddr>, KernelError> {
        let size = self
            .size()
            .ok_or(KernelError::TooLargeVirtualAddress(usize::MAX))?;
        let start = VirtAddr::new(self.addr())?;
        let end = start.byte_add(size)?;
        Ok(start..end)
    }
}

impl<T> TryAsVirtAddrRange for &'_ T
where
    T: TryAsVirtAddrRange,
{
    fn try_as_va_range(&self) -> Result<Range<VirtAddr>, KernelError> {
        <T as TryAsVirtAddrRange>::try_as_va_range(*self)
    }
}

impl<T> TryAsVirtAddrRange for &'_ mut T
where
    T: TryAsVirtAddrRange,
{
    fn try_as_va_range(&self) -> Result<Range<VirtAddr>, KernelError> {
        <T as TryAsVirtAddrRange>::try_as_va_range(*self)
    }
}

pub trait AsVirtAddrRange {
    fn as_va_range(&self) -> Range<VirtAddr>;
}

impl<R> AsVirtAddrRange for Validated<R>
where
    R: TryAsVirtAddrRange,
{
    fn as_va_range(&self) -> Range<VirtAddr> {
        self.0.try_as_va_range().unwrap()
    }
}

impl<R> AsVirtAddrRange for &'_ Validated<R>
where
    R: TryAsVirtAddrRange,
{
    fn as_va_range(&self) -> Range<VirtAddr> {
        self.0.try_as_va_range().unwrap()
    }
}

impl<R> AsVirtAddrRange for &'_ mut Validated<R>
where
    R: TryAsVirtAddrRange,
{
    fn as_va_range(&self) -> Range<VirtAddr> {
        self.0.try_as_va_range().unwrap()
    }
}

#[derive(Debug)]
pub struct AddressChunks {
    range: Range<VirtAddr>,
}

impl AddressChunks {
    #[expect(clippy::needless_pass_by_value)]
    pub fn new<R>(range: R) -> Self
    where
        R: AsVirtAddrRange,
    {
        let range = range.as_va_range();
        Self { range }
    }

    pub fn try_new<R>(range: R) -> Result<Self, KernelError>
    where
        R: TryAsVirtAddrRange,
    {
        let range = range.try_as_va_range()?;
        Ok(Self { range })
    }

    pub fn from_size(start: VirtAddr, size: usize) -> Result<Self, KernelError> {
        let end = start.byte_add(size)?;
        Ok(Self { range: start..end })
    }

    pub fn from_range(range: Range<VirtAddr>) -> Self {
        Self { range }
    }
}

#[derive(Debug, Clone)]
pub struct AddressChunk {
    range: Range<VirtAddr>,
}

impl AddressChunk {
    pub fn page_num(&self) -> VirtPageNum {
        self.range.start.virt_page_num()
    }

    pub fn offset_in_page(&self) -> Range<usize> {
        (self.range.start.addr() % PAGE_SIZE)..(self.range.end.addr() % PAGE_SIZE)
    }

    pub fn size(&self) -> usize {
        self.range.end.addr() - self.range.start.addr()
    }
}

impl Iterator for AddressChunks {
    type Item = AddressChunk;

    fn next(&mut self) -> Option<Self::Item> {
        if self.range.start >= self.range.end {
            return None;
        }

        let start = self.range.start;
        let end = start
            .byte_add(PAGE_SIZE)
            .map(|a| cmp::min(a.page_rounddown(), self.range.end))
            .unwrap_or(self.range.end);
        self.range.start = end;
        Some(AddressChunk { range: start..end })
    }
}

#[derive(Debug, Clone, Copy)]
#[repr(transparent)]
pub struct Validated<T>(T);

unsafe impl<T> Pod for Validated<T> where T: Pod {}

pub trait Validate: Sized {
    fn validate(self, pt: &UserPageTable) -> Result<Validated<Self>, KernelError>;
}

impl<T> Validate for UserRef<T> {
    fn validate(self, pt: &UserPageTable) -> Result<Validated<Self>, KernelError> {
        pt.validate_read(self.try_as_va_range()?)?;
        Ok(Validated(self))
    }
}

impl<T> Validate for UserMutRef<T> {
    fn validate(self, pt: &UserPageTable) -> Result<Validated<Self>, KernelError> {
        pt.validate_write(self.try_as_va_range()?)?;
        Ok(Validated(self))
    }
}

impl<T> Validate for UserSlice<T> {
    fn validate(self, pt: &UserPageTable) -> Result<Validated<Self>, KernelError> {
        pt.validate_read(self.try_as_va_range()?)?;
        Ok(Validated(self))
    }
}

impl<T> Validate for UserMutSlice<T> {
    fn validate(self, pt: &UserPageTable) -> Result<Validated<Self>, KernelError> {
        pt.validate_write(self.try_as_va_range()?)?;
        Ok(Validated(self))
    }
}

impl<T> Validated<UserRef<T>> {
    #[expect(unused)]
    pub fn addr(&self) -> usize {
        self.0.addr()
    }

    #[expect(unused)]
    pub fn size(&self) -> usize {
        self.0.size()
    }

    pub fn as_bytes(&self) -> Validated<UserSlice<u8>>
    where
        T: Pod + Sized,
    {
        Validated(self.0.as_bytes())
    }
}

impl<T> Validated<UserMutRef<T>> {
    #[expect(unused)]
    pub fn addr(&self) -> usize {
        self.0.addr()
    }

    #[expect(unused)]
    pub fn size(&self) -> usize {
        self.0.size()
    }

    pub fn as_bytes_mut(&mut self) -> Validated<UserMutSlice<u8>>
    where
        T: Pod + Sized,
    {
        Validated(self.0.as_bytes_mut())
    }
}

impl<T> Validated<UserSlice<T>> {
    pub fn addr(&self) -> usize {
        self.0.addr()
    }

    pub fn len(&self) -> usize {
        self.0.len()
    }

    #[expect(unused)]
    pub fn size(&self) -> usize {
        self.0.size().unwrap()
    }

    #[expect(unused)]
    #[track_caller]
    pub fn cast<U>(&self) -> Validated<UserSlice<U>> {
        Validated(self.0.cast())
    }

    #[track_caller]
    pub fn nth(&self, n: usize) -> Validated<UserRef<T>> {
        Validated(self.0.nth(n))
    }

    #[track_caller]
    pub fn skip(&self, amt: usize) -> Self {
        Self(self.0.skip(amt))
    }

    #[track_caller]
    pub fn take(&self, amt: usize) -> Self {
        Self(self.0.take(amt))
    }
}

impl<T> Validated<UserMutSlice<T>> {
    pub fn addr(&self) -> usize {
        self.0.addr()
    }

    pub fn len(&self) -> usize {
        self.0.len()
    }

    #[expect(unused)]
    pub fn size(&self) -> usize {
        self.0.size().unwrap()
    }

    #[track_caller]
    pub fn cast_mut<U>(&mut self) -> Validated<UserMutSlice<U>> {
        Validated(self.0.cast_mut())
    }

    #[track_caller]
    pub fn nth_mut(&mut self, n: usize) -> Validated<UserMutRef<T>> {
        Validated(self.0.nth_mut(n))
    }

    #[track_caller]
    pub fn skip_mut(&mut self, amt: usize) -> Self {
        Self(self.0.skip_mut(amt))
    }

    #[track_caller]
    pub fn take_mut(&mut self, amt: usize) -> Self {
        Self(self.0.take_mut(amt))
    }
}

#[derive(Clone, Copy, derive_more::From)]
pub enum GenericSlice<'a, T> {
    User(&'a UserPageTable, Validated<UserSlice<T>>),
    Kernel(&'a [T]),
}

impl<T> GenericSlice<'_, T> {
    pub fn len(&self) -> usize {
        match self {
            Self::User(_pt, s) => s.len(),
            Self::Kernel(s) => s.len(),
        }
    }

    pub fn skip(&self, amt: usize) -> GenericSlice<'_, T> {
        assert!(amt <= self.len());
        match self {
            Self::User(pt, s) => GenericSlice::User(pt, s.skip(amt)),
            Self::Kernel(s) => GenericSlice::Kernel(&s[amt..]),
        }
    }

    pub fn take(&self, amt: usize) -> GenericSlice<'_, T> {
        assert!(amt <= self.len());
        match self {
            Self::User(pt, s) => GenericSlice::User(pt, s.take(amt)),
            Self::Kernel(s) => GenericSlice::Kernel(&s[..amt]),
        }
    }
}

impl<'a, T> From<(&'a UserPageTable, &'a Validated<UserSlice<T>>)> for GenericSlice<'a, T> {
    fn from((pt, s): (&'a UserPageTable, &'a Validated<UserSlice<T>>)) -> Self {
        Self::User(pt, Validated(UserSlice::from_raw_parts(s.addr(), s.len())))
    }
}

#[derive(derive_more::From)]
pub enum GenericMutSlice<'a, T> {
    User(&'a mut UserPageTable, Validated<UserMutSlice<T>>),
    Kernel(&'a mut [T]),
}

impl<T> GenericMutSlice<'_, T> {
    pub fn len(&self) -> usize {
        match self {
            Self::User(_, s) => s.len(),
            Self::Kernel(s) => s.len(),
        }
    }

    pub fn skip_mut(&mut self, amt: usize) -> GenericMutSlice<'_, T> {
        assert!(amt <= self.len());
        match self {
            Self::User(pt, s) => GenericMutSlice::User(pt, s.skip_mut(amt)),
            Self::Kernel(s) => GenericMutSlice::Kernel(&mut s[amt..]),
        }
    }

    pub fn take_mut(&mut self, amt: usize) -> GenericMutSlice<'_, T> {
        assert!(amt <= self.len());
        match self {
            Self::User(pt, s) => GenericMutSlice::User(pt, s.take_mut(amt)),
            Self::Kernel(s) => GenericMutSlice::Kernel(&mut s[..amt]),
        }
    }
}

impl<'a, T> From<(&'a mut UserPageTable, &'a mut Validated<UserMutSlice<T>>)>
    for GenericMutSlice<'a, T>
{
    fn from((pt, s): (&'a mut UserPageTable, &'a mut Validated<UserMutSlice<T>>)) -> Self {
        Self::User(
            pt,
            Validated(UserMutSlice::from_raw_parts(s.addr(), s.len())),
        )
    }
}
