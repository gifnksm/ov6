use core::{
    fmt,
    num::NonZero,
    ptr::{self, NonNull},
};

use ov6_syscall::{UserMutSlice, UserSlice};

use super::{PAGE_SHIFT, PAGE_SIZE, vm_user::UserPageTable};

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
        self.map_addr(page_roundup)
    }

    fn page_rounddown(&self) -> Self {
        self.map_addr(page_rounddown)
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

/// Virtual address
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct VirtAddr(usize);

impl fmt::LowerHex for VirtAddr {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        fmt::LowerHex::fmt(&self.0, f)
    }
}

impl fmt::UpperHex for VirtAddr {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        fmt::UpperHex::fmt(&self.0, f)
    }
}

/// Physical Page Number of a page
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct PhysPageNum(usize);

/// Physical Address
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct PhysAddr(usize);

impl VirtAddr {
    /// One beyond the highest possible virtual address.
    ///
    /// [`VirtAddr::MAX`] is actually one bit less than the max allowed by
    /// Sv39, to avoid having to sign-extend virtual addresses
    /// that have the high bit set.
    pub const MAX: Self = Self(1 << (9 * 3 + PAGE_SHIFT - 1));

    pub const fn new(addr: usize) -> Self {
        Self(addr)
    }

    pub const fn byte_add(self, offset: usize) -> Self {
        // FIXME: need overflow check
        Self(self.0 + offset)
    }

    pub const fn byte_sub(self, offset: usize) -> Self {
        // FIXME: need overflow check
        Self(self.0 - offset)
    }

    pub const fn addr(self) -> usize {
        self.0
    }

    pub fn map_addr(self, f: impl FnOnce(usize) -> usize) -> Self {
        Self(f(self.0))
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

    pub fn byte_add(self, offset: usize) -> Self {
        // FIXME: need overflow check
        Self(self.0 + offset)
    }

    pub fn map_addr(self, f: impl FnOnce(usize) -> usize) -> Self {
        Self(f(self.0))
    }
}

#[derive(Clone, Copy, derive_more::From)]
pub enum GenericSlice<'a, T> {
    User(&'a UserPageTable, UserSlice<T>),
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

impl<'a, T> From<(&'a UserPageTable, &'a UserSlice<T>)> for GenericSlice<'a, T> {
    fn from((pt, s): (&'a UserPageTable, &'a UserSlice<T>)) -> Self {
        Self::User(pt, UserSlice::from_raw_parts(s.addr(), s.len()))
    }
}

#[derive(derive_more::From)]
pub enum GenericMutSlice<'a, T> {
    User(&'a mut UserPageTable, UserMutSlice<T>),
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

impl<'a, T> From<(&'a mut UserPageTable, &'a mut UserMutSlice<T>)> for GenericMutSlice<'a, T> {
    fn from((pt, s): (&'a mut UserPageTable, &'a mut UserMutSlice<T>)) -> Self {
        Self::User(pt, UserMutSlice::from_raw_parts(s.addr(), s.len()))
    }
}
