use core::{
    num::NonZero,
    ptr::{self, NonNull},
};

use super::{PAGE_SHIFT, PAGE_SIZE};

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
        NonZero::new(page_roundup(self.get())).unwrap()
    }

    fn page_rounddown(&self) -> Self {
        NonZero::new(page_rounddown(self.get())).unwrap()
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

/// Physical Page Number of a page
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct PhysPageNum(usize);

/// Physical Address
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct PhysAddr(usize);

impl VirtAddr {
    /// One beyond the highest possible virtual address.
    ///
    /// VirtAddr::MAX is actually one bit less than the max allowed by
    /// Sv39, to avoid having to sign-extend virtual addresses
    /// that have the high bit set.
    pub const MAX: Self = Self(1 << (9 * 3 + PAGE_SHIFT - 1));

    pub const fn new(addr: usize) -> Self {
        Self(addr)
    }

    pub const fn byte_add(&self, offset: usize) -> Self {
        Self(self.0 + offset)
    }

    pub const fn byte_sub(&self, offset: usize) -> Self {
        Self(self.0 - offset)
    }

    pub const fn addr(&self) -> usize {
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

    pub const fn phys_addr(&self) -> PhysAddr {
        PhysAddr(self.0 << PAGE_SHIFT)
    }

    pub const fn value(&self) -> usize {
        self.0
    }
}

impl PhysAddr {
    pub const fn new(addr: usize) -> Self {
        Self(addr)
    }

    pub fn addr(&self) -> usize {
        self.0
    }

    pub fn as_ptr<T>(&self) -> *const T {
        ptr::with_exposed_provenance(self.0)
    }

    pub fn as_mut_ptr<T>(&self) -> NonNull<T> {
        NonNull::new(ptr::with_exposed_provenance_mut(self.0)).unwrap()
    }

    pub fn phys_page_num(&self) -> PhysPageNum {
        PhysPageNum(self.0 >> PAGE_SHIFT)
    }

    pub fn byte_add(&self, offset: usize) -> Self {
        Self(self.0 + offset)
    }

    pub fn map_addr(self, f: impl FnOnce(usize) -> usize) -> Self {
        Self(f(self.0))
    }
}
