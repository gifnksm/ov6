use alloc::boxed::Box;
use core::{
    alloc::AllocError,
    fmt,
    iter::{Peekable, Zip},
    mem,
    ops::{RangeBounds, RangeInclusive},
    slice,
};

use arrayvec::ArrayVec;
use bitflags::bitflags;
use dataview::Pod;
use riscv::register::satp::{self, Satp};

use super::{
    PhysAddr, VirtAddr,
    addr::PhysPageNum,
    page::{self, PageFrameAllocator},
};
use crate::{
    error::KernelError,
    memory::{self, PAGE_SIZE, PageRound as _, level_page_size},
    println,
};

pub struct PageTable(Box<PageTableBody, PageFrameAllocator>);

impl PageTable {
    pub fn try_allocate() -> Result<Self, KernelError> {
        Ok(Self(PageTableBody::try_allocate()?))
    }

    pub(super) fn satp(&self) -> Satp {
        let mut satp = Satp::from_bits(0);
        satp.set_mode(satp::Mode::Sv39);
        satp.set_ppn(self.0.phys_page_num().value());
        satp
    }

    /// Validates that the virtual address range `va` at the specified `level`
    /// is mapped with the required `flags`.
    ///
    /// This function ensures that all pages within the given range are mapped
    /// and accessible with the specified permissions. If any page in the range
    /// is not mapped or does not meet the required permissions, an error is
    /// returned.
    ///
    /// # Returns
    ///
    /// - `Ok(())` if all pages in the range are valid and meet the required
    ///   permissions.
    /// - `Err(KernelError)` if any page in the range is invalid or does not
    ///   meet the required permissions.
    ///
    /// # Panics
    ///
    /// - Panics if `va.start > va.end`.
    ///
    /// # Notes
    ///
    /// - If the range is empty (`va.start == va.end`), the function returns
    ///   `Ok(())` immediately.
    pub(super) fn validate<R>(&self, va_range: R, flags: PtEntryFlags) -> Result<(), KernelError>
    where
        R: RangeBounds<VirtAddr>,
    {
        self.entries(va_range).try_for_each(|(_level, va, pte)| {
            if !pte.is_valid() {
                return Err(KernelError::VirtualPageNotMapped(va));
            }
            if pte.is_leaf() && !pte.flags().contains(flags) {
                return Err(KernelError::InaccessiblePage(va));
            }
            Ok(())
        })
    }

    /// Creates PTEs for virtual addresses starting at `va` that refer to
    /// physical addresses starting at `pa`.
    ///
    /// `size` MUST be page-aligned.
    ///
    /// Returns `Ok(())` on success, `Err()` if `walk()` couldn't
    /// allocate a needed page-table page.
    pub(super) fn map_addrs(
        &mut self,
        va: VirtAddr,
        pa: MapTarget,
        size: usize,
        perm: PtEntryFlags,
    ) -> Result<(), KernelError> {
        self.0.map_addrs(va, pa, size, perm)
    }

    pub(super) fn clone_pages_from<R>(
        &mut self,
        other: &Self,
        va_range: R,
        flags: PtEntryFlags,
    ) -> Result<(), KernelError>
    where
        R: RangeBounds<VirtAddr>,
    {
        for (level, va, src_pte) in other.entries(va_range) {
            if !src_pte.is_leaf() {
                continue;
            }
            if !src_pte.flags().contains(flags) {
                return Err(KernelError::InaccessiblePage(va));
            }

            let page_size = memory::level_page_size(level);
            let flags = src_pte.flags();

            let mut dst_va = va;
            let mut dst_pa = MapTarget::allocated_addr(false, true);
            self.0
                .map_addrs_level(level, &mut dst_va, &mut dst_pa, page_size, flags)?;

            let src = other.fetch_chunk(va, PtEntryFlags::U)?;
            let dst = self.fetch_chunk_mut(va, PtEntryFlags::U)?;
            dst.copy_from_slice(src);
        }

        Ok(())
    }

    /// Unmaps the pages of memory starting at virtual page `vpn` and
    /// covering `npages` pages.
    pub(super) fn unmap_addrs(&mut self, va: VirtAddr, size: usize) -> Result<(), KernelError> {
        let start = va;
        let end = va.byte_add(size)?;
        self.unmap_range(start..end);
        Ok(())
    }

    fn unmap_range<R>(&mut self, va_range: R)
    where
        R: RangeBounds<VirtAddr>,
    {
        for (level, _va, pte) in self.leaves_mut(va_range) {
            pte.free(level);
        }
    }

    fn entries<R>(&self, va_range: R) -> Entries<'_>
    where
        R: RangeBounds<VirtAddr>,
    {
        Entries::new(&self.0, va_range)
    }

    fn leaves_mut<R>(&mut self, va_range: R) -> LeavesMut<'_>
    where
        R: RangeBounds<VirtAddr>,
    {
        LeavesMut::new(&mut self.0, va_range)
    }

    /// Returns the leaf PTE in the page tables that corredponds to virtual
    /// page `va`.
    fn find_leaf_entry(&self, va: VirtAddr) -> Result<(usize, &PtEntry), KernelError> {
        let mut pt = &*self.0;
        for level in (0..=2).rev() {
            let index = va.level_idx(level);
            let pte = &pt.0[index];
            if !pte.is_valid() {
                return Err(KernelError::VirtualPageNotMapped(va));
            }
            if pte.is_leaf() {
                return Ok((level, pte));
            }
            assert!(pte.is_non_leaf());
            pt = pte.get_page_table().unwrap();
        }
        panic!("invalid page table");
    }

    /// Returns the leaf PTE in the page tables that corredponds to virtual
    /// page `va`.
    fn find_leaf_entry_mut(&mut self, va: VirtAddr) -> Result<(usize, &mut PtEntry), KernelError> {
        let mut pt = &mut *self.0;
        for level in (0..=2).rev() {
            let index = va.level_idx(level);
            let pte = &mut pt.0[index];
            if !pte.is_valid() {
                return Err(KernelError::VirtualPageNotMapped(va));
            }
            if pte.is_leaf() {
                return Ok((level, pte));
            }
            assert!(pte.is_non_leaf());
            pt = pte.get_page_table_mut().unwrap();
        }
        panic!("invalid page table");
    }

    pub(super) fn fetch_chunk(
        &self,
        va: VirtAddr,
        flags: PtEntryFlags,
    ) -> Result<&[u8], KernelError> {
        let (level, pte) = self.find_leaf_entry(va)?;
        assert!(pte.is_valid() && pte.is_leaf());
        if !pte.flags().contains(flags) {
            return Err(KernelError::InaccessiblePage(va));
        }

        let page = pte.get_page_bytes(level).unwrap();
        let page_size = page.len();
        let offset = va.addr() % page_size;
        Ok(&page[offset..])
    }

    pub(super) fn fetch_chunk_mut(
        &mut self,
        va: VirtAddr,
        flags: PtEntryFlags,
    ) -> Result<&mut [u8], KernelError> {
        let (level, pte) = self.find_leaf_entry_mut(va)?;
        assert!(pte.is_valid() && pte.is_leaf());
        if !pte.flags().contains(flags) {
            return Err(KernelError::InaccessiblePage(va));
        }

        let page = pte.get_page_bytes_mut(level).unwrap();
        let page_size = page.len();
        let offset = va.addr() % page_size;
        Ok(&mut page[offset..])
    }
}

impl Drop for PageTable {
    fn drop(&mut self) {
        self.0.free(2);
    }
}

#[derive(Debug, Clone, Copy)]
pub enum MapTarget {
    AllocatedAddr { zeroed: bool, allocate_new: bool },
    FixedAddr { addr: PhysAddr },
}

impl MapTarget {
    pub fn allocate_new_zeroed() -> Self {
        Self::AllocatedAddr {
            zeroed: true,
            allocate_new: true,
        }
    }

    pub fn allocated_addr(zeroed: bool, allocate_new: bool) -> Self {
        Self::AllocatedAddr {
            zeroed,
            allocate_new,
        }
    }

    pub fn fixed_addr(addr: PhysAddr) -> Self {
        Self::FixedAddr { addr }
    }

    fn is_page_aligned(&self) -> bool {
        match self {
            Self::AllocatedAddr { .. } => true,
            Self::FixedAddr { addr } => addr.is_page_aligned(),
        }
    }

    fn is_level_page_aligned(&self, level: usize) -> bool {
        match self {
            Self::AllocatedAddr { .. } => true,
            Self::FixedAddr { addr } => addr.is_level_page_aligned(level),
        }
    }

    fn level_offset(&self, level: usize) -> usize {
        match self {
            Self::AllocatedAddr { .. } => 0,
            Self::FixedAddr { addr } => addr.addr() % level_page_size(level),
        }
    }

    fn byte_add(&self, bytes: usize) -> Option<Self> {
        let new = match self {
            Self::AllocatedAddr { .. } => *self,
            Self::FixedAddr { addr } => Self::FixedAddr {
                addr: addr.byte_add(bytes)?,
            },
        };
        Some(new)
    }

    fn allocate_new(&self) -> bool {
        match self {
            Self::AllocatedAddr {
                allocate_new: create_new,
                ..
            } => *create_new,
            Self::FixedAddr { .. } => true,
        }
    }

    fn get_or_allocate(&self, level: usize) -> Result<PhysAddr, KernelError> {
        match self {
            Self::AllocatedAddr { zeroed, .. } => {
                assert_eq!(level, 0, "super page is not supported yet");
                let page = if *zeroed {
                    page::alloc_zeroed_page()?
                } else {
                    page::alloc_page()?
                };
                Ok(PhysAddr::from(page))
            }
            Self::FixedAddr { addr } => Ok(*addr),
        }
    }
}

#[repr(transparent)]
#[derive(Pod)]
struct PageTableBody([PtEntry; 512]);

impl PageTableBody {
    /// Allocates a new empty page table.
    fn try_allocate() -> Result<Box<Self, PageFrameAllocator>, KernelError> {
        let pt = Box::try_new_zeroed_in(PageFrameAllocator)
            .map_err(|AllocError| KernelError::NoFreePage)?;
        Ok(unsafe { pt.assume_init() })
    }

    /// Returns the physical address containing this page table
    fn phys_addr(&self) -> PhysAddr {
        PhysAddr::from(self)
    }

    /// Returns the physical page number of the physical page containing this
    /// page table
    fn phys_page_num(&self) -> PhysPageNum {
        self.phys_addr().phys_page_num()
    }

    fn find_or_create_leaf(
        &mut self,
        level: usize,
        va: VirtAddr,
    ) -> Result<&mut PtEntry, KernelError> {
        assert!(level <= 2);
        let mut pt = self;
        for level in (level + 1..=2).rev() {
            let index = va.level_idx(level);
            let pte = &mut pt.0[index];
            if !pte.is_valid() {
                let new_pt = Self::try_allocate()?;
                pte.set_page_table(new_pt);
            }
            pt = pte.get_page_table_mut().unwrap();
        }

        let index = va.level_idx(level);
        let pte = &mut pt.0[index];
        Ok(pte)
    }

    /// Creates PTE for virtual page `vpn` that refer to
    /// physical page `ppn`.
    ///
    /// Returns `Ok(())` on success, `Err()` if `walk()` couldn't
    /// allocate a needed page-table page.
    fn map_page(
        &mut self,
        level: usize,
        va: VirtAddr,
        pa: MapTarget,
        perm: PtEntryFlags,
    ) -> Result<(), KernelError> {
        assert!(perm.intersects(PtEntryFlags::RWX), "perm={perm:?}");
        let pte = self.find_or_create_leaf(level, va)?;

        if pte.is_valid() {
            assert!(
                !pa.allocate_new(),
                "remap on the already mapped address: va={va:?}"
            );
            let pte_perm = pte.flags() & PtEntryFlags::URWX;
            if perm != pte_perm {
                return Err(KernelError::VirtualAddressWithUnexpectedPerm(
                    va, perm, pte_perm,
                ));
            }
            return Ok(());
        }

        let pa = pa.get_or_allocate(level)?;
        pte.set_phys_page_num(pa.phys_page_num(), perm | PtEntryFlags::V);
        Ok(())
    }

    fn map_addrs_level(
        &mut self,
        level: usize,
        va: &mut VirtAddr,
        pa: &mut MapTarget,
        size: usize,
        perm: PtEntryFlags,
    ) -> Result<(), KernelError> {
        let page_size = memory::level_page_size(level);
        assert!(va.is_level_page_aligned(level));
        assert!(pa.is_level_page_aligned(level));
        assert!(size.is_multiple_of(page_size));

        assert!(perm.intersects(PtEntryFlags::RWX), "perm={perm:?}");

        let va_end = va.byte_add(size)?;
        while *va < va_end {
            self.map_page(level, *va, *pa, perm)?;
            *va = va.byte_add(page_size).unwrap();
            *pa = pa.byte_add(page_size).unwrap();
        }
        Ok(())
    }

    /// Creates PTEs for virtual addresses starting at `va` that refer to
    /// physical addresses starting at `pa`.
    ///
    /// `size` MUST be page-aligned.
    ///
    /// Returns `Ok(())` on success, `Err()` if `walk()` couldn't
    /// allocate a needed page-table page.
    fn map_addrs(
        &mut self,
        mut va: VirtAddr,
        mut pa: MapTarget,
        size: usize,
        perm: PtEntryFlags,
    ) -> Result<(), KernelError> {
        assert!(va.is_page_aligned());
        assert!(pa.is_page_aligned());
        assert!(size.is_multiple_of(PAGE_SIZE));
        assert!(perm.intersects(PtEntryFlags::RWX), "perm={perm:?}");

        if let MapTarget::AllocatedAddr { .. } = pa {
            // currently, supr page allocation is not supported
            return self.map_addrs_level(0, &mut va, &mut pa, size, perm);
        }

        if va.addr() % level_page_size(1) != pa.level_offset(1) {
            return self.map_addrs_level(0, &mut va, &mut pa, size, perm);
        }

        let va_end = va.byte_add(size)?;
        let lv1_start_va = va.level_page_roundup(1);
        let lv1_end_va = va_end.level_page_rounddown(1);
        if lv1_start_va >= lv1_end_va {
            return self.map_addrs_level(0, &mut va, &mut pa, size, perm);
        }

        if va < lv1_start_va {
            let size = lv1_start_va.addr() - va.addr();
            self.map_addrs_level(0, &mut va, &mut pa, size, perm)?;
        }
        if lv1_start_va < lv1_end_va {
            let size = lv1_end_va.addr() - va.addr();
            self.map_addrs_level(1, &mut va, &mut pa, size, perm)?;
        }
        if va < va_end {
            let size = va_end.addr() - va.addr();
            self.map_addrs_level(0, &mut va, &mut pa, size, perm)?;
        }
        assert_eq!(va, va_end);
        Ok(())
    }

    fn free(&mut self, level: usize) {
        for pte in &mut self.0 {
            pte.free(level);
        }
    }
}

bitflags! {
    /// Page table entry flags
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub struct PtEntryFlags: usize {
        /// Valid Bit of page table entry.
        ///
        /// If set, an entry for this virtual address exists.
        const V = 1 << 0;

        /// Read Bit of page table entry.
        ///
        /// If set, the CPU can read to this virtual address.
        const R = 1 << 1;

        /// Write Bit of page table entry.
        ///
        /// If set, the CPU can write to this virtual address.
        const W = 1 << 2;

        /// Executable Bit of page table entry.
        ///
        /// If set, the CPU can executes the instructions on this virtual address.
        const X = 1 << 3;

        /// UserMode Bit of page table entry.
        ///
        /// If set, userspace can access this virtual address.
        const U = 1 << 4;

        /// Global Mapping Bit of page table entry.
        ///
        /// If set, this virtual address exists in all address spaces.
        const G = 1 << 5;

        /// Access Bit of page table entry.
        ///
        /// If set, this virtual address have been accesses.
        const A = 1 << 6;

        /// Dirty Bit of page table entry.
        ///
        /// If set, this virtual address have been written.
        const D = 1 << 7;

        const RW = Self::R.bits() | Self::W.bits();
        const RX = Self::R.bits() | Self::X.bits();
        const RWX = Self::R.bits() | Self::W.bits() | Self::X.bits();
        const UR = Self::U.bits() | Self::R.bits();
        const UW = Self::U.bits() | Self::W.bits();
        const URW = Self::U.bits() | Self::RW.bits();
        const URX = Self::U.bits() | Self::RX.bits();
        const URWX = Self::U.bits() | Self::RWX.bits();
    }
}

type EntriesIter<'a> = Peekable<Zip<RangeInclusive<usize>, slice::Iter<'a, PtEntry>>>;
type EntriesStack<'a> = ArrayVec<(usize, VirtAddr, EntriesIter<'a>), 3>;
struct Entries<'a> {
    state: Option<(RangeInclusive<VirtAddr>, EntriesStack<'a>)>,
    last_item_is_non_leaf: bool,
}

impl<'a> Entries<'a> {
    fn new<R>(pt: &'a PageTableBody, va_range: R) -> Self
    where
        R: RangeBounds<VirtAddr>,
    {
        let Some(va_range) = VirtAddr::range_inclusive(va_range) else {
            return Self {
                state: None,
                last_item_is_non_leaf: false,
            };
        };

        let min_va = *va_range.start();
        let max_va = *va_range.end();
        let level_min_idx = min_va.level_idx(2);
        let level_max_idx = max_va.level_idx(2);
        let mut stack = ArrayVec::<_, 3>::new();
        let it = (level_min_idx..=level_max_idx)
            .zip(&pt.0[level_min_idx..=level_max_idx])
            .peekable();
        stack.push((2, VirtAddr::ZERO, it));
        Self {
            state: Some((va_range, stack)),
            last_item_is_non_leaf: false,
        }
    }
}

impl<'a> Iterator for Entries<'a> {
    type Item = (usize, VirtAddr, &'a PtEntry);

    fn next(&mut self) -> Option<Self::Item> {
        let (va_range, stack) = self.state.as_mut()?;
        let min_va = *va_range.start();
        let max_va = *va_range.end();

        if mem::take(&mut self.last_item_is_non_leaf) {
            let (level, base_va, ptes) = stack.last_mut().unwrap();
            let (idx, pte) = ptes.next().unwrap();
            let level_va = base_va.with_level_idx(*level, idx);

            assert_eq!(level_va.level_idx(*level - 1), 0);
            let level_min_va = level_va.with_level_idx(*level - 1, 0);
            let leval_max_va = level_va.with_level_idx(*level - 1, 511);
            let level_min_idx = VirtAddr::max(min_va, level_min_va).level_idx(*level - 1);
            let level_max_idx = VirtAddr::min(max_va, leval_max_va).level_idx(*level - 1);

            let pt = pte.get_page_table().unwrap();
            let it = (level_min_idx..=level_max_idx)
                .zip(&pt.0[level_min_idx..=level_max_idx])
                .peekable();
            let elem = (*level - 1, level_min_va, it);
            stack.push(elem);
        }

        while let Some((level, base_va, ptes)) = stack.last_mut() {
            if let Some((idx, pte)) = ptes.next_if(|(_idx, pte)| !pte.is_valid() || pte.is_leaf()) {
                let level_min_va = base_va.with_level_idx(*level, idx);
                return Some((*level, level_min_va, pte));
            }

            let Some((idx, pte)) = ptes.peek() else {
                stack.pop();
                continue;
            };

            self.last_item_is_non_leaf = true;
            let level_min_va = base_va.with_level_idx(*level, *idx);
            return Some((*level, level_min_va, *pte));
        }
        None
    }
}

type LeavesMutIter<'a> = Zip<RangeInclusive<usize>, slice::IterMut<'a, PtEntry>>;
type LeavesMutStack<'a> = ArrayVec<(usize, VirtAddr, LeavesMutIter<'a>), 3>;

struct LeavesMut<'a>(Option<(RangeInclusive<VirtAddr>, LeavesMutStack<'a>)>);

impl<'a> LeavesMut<'a> {
    fn new<R>(pt: &'a mut PageTableBody, va_range: R) -> Self
    where
        R: RangeBounds<VirtAddr>,
    {
        let Some(va_range) = VirtAddr::range_inclusive(va_range) else {
            return Self(None);
        };

        let min_va = *va_range.start();
        let max_va = *va_range.end();
        let level_min_idx = min_va.level_idx(2);
        let level_max_idx = max_va.level_idx(2);
        let mut stack = ArrayVec::<_, 3>::new();
        let it = (level_min_idx..=level_max_idx).zip(&mut pt.0[level_min_idx..=level_max_idx]);
        stack.push((2, VirtAddr::ZERO, it));
        Self(Some((va_range, stack)))
    }
}

impl<'a> Iterator for LeavesMut<'a> {
    type Item = (usize, VirtAddr, &'a mut PtEntry);

    fn next(&mut self) -> Option<Self::Item> {
        let (va_range, stack) = self.0.as_mut()?;
        let min_va = *va_range.start();
        let max_va = *va_range.end();
        while let Some((level, base_va, ptes)) = stack.last_mut() {
            let Some((idx, pte)) = ptes.next() else {
                stack.pop();
                continue;
            };

            let level_min_va = base_va.with_level_idx(*level, idx);
            assert!(!pte.is_leaf() || va_range.contains(&level_min_va));

            if !pte.is_valid() {
                continue;
            }

            if pte.is_leaf() {
                return Some((*level, level_min_va, pte));
            }

            let leval_max_va = level_min_va.with_level_idx(*level - 1, 511);
            let level_min_idx = VirtAddr::max(min_va, level_min_va).level_idx(*level - 1);
            let level_max_idx = VirtAddr::min(max_va, leval_max_va).level_idx(*level - 1);

            let pt = pte.get_page_table_mut().unwrap();
            let it = (level_min_idx..=level_max_idx).zip(&mut pt.0[level_min_idx..=level_max_idx]);
            let elem = (*level - 1, level_min_va, it);
            stack.push(elem);
        }
        None
    }
}

#[repr(transparent)]
#[derive(Pod)]
struct PtEntry(usize);

impl PtEntry {
    const FLAGS_MASK: usize = 0x3FF;

    fn new(ppn: PhysPageNum, flags: PtEntryFlags) -> Self {
        assert_eq!(
            flags.bits() & Self::FLAGS_MASK,
            flags.bits(),
            "flags: {flags:#x}={flags:?}"
        );
        let bits = (ppn.value() << 10) | (flags.bits() & 0x3FF);
        Self(bits)
    }

    fn get_page_table(&self) -> Option<&PageTableBody> {
        self.is_non_leaf()
            .then(|| unsafe { self.phys_addr().as_non_null::<PageTableBody>().as_ref() })
    }

    fn get_page_table_mut(&mut self) -> Option<&mut PageTableBody> {
        self.is_non_leaf()
            .then(|| unsafe { self.phys_addr().as_non_null::<PageTableBody>().as_mut() })
    }

    fn set_page_table(&mut self, pt: Box<PageTableBody, PageFrameAllocator>) {
        assert!(!self.is_valid());
        let ppn = pt.phys_page_num();
        Box::leak(pt);
        *self = Self::new(ppn, PtEntryFlags::V);
    }

    fn get_page_bytes(&self, level: usize) -> Option<&[u8]> {
        self.is_leaf().then(|| {
            let pa = self.phys_addr();
            let page_size = memory::level_page_size(level);
            unsafe { slice::from_raw_parts(pa.as_ptr(), page_size) }
        })
    }

    #[expect(clippy::needless_pass_by_ref_mut)]
    fn get_page_bytes_mut(&mut self, level: usize) -> Option<&mut [u8]> {
        self.is_leaf().then(|| {
            let pa = self.phys_addr();
            let page_size = memory::level_page_size(level);
            unsafe { slice::from_raw_parts_mut(pa.as_mut_ptr(), page_size) }
        })
    }

    fn free(&mut self, level: usize) {
        if !self.is_valid() {
            return;
        }

        let pa = self.phys_addr();
        let is_non_leaf = self.is_non_leaf();
        self.0 = 0;

        if is_non_leaf {
            assert!(level > 0);
            let ptr = pa.as_mut_ptr::<PageTableBody>();
            let mut pt = unsafe { Box::from_raw_in(ptr, PageFrameAllocator) };
            pt.free(level - 1);
            drop(pt);
        } else {
            assert_eq!(level, 0, "super page is not supported yet");
            if page::is_allocated_addr(pa.as_non_null()) {
                unsafe { page::free_page(pa.as_non_null()) }
            }
        }
    }

    /// Returns physical page number (PPN)
    fn phys_page_num(&self) -> PhysPageNum {
        PhysPageNum::new(self.0 >> 10)
    }

    fn set_phys_page_num(&mut self, ppn: PhysPageNum, flags: PtEntryFlags) {
        assert!(!self.is_valid());
        assert!(flags.contains(PtEntryFlags::V));
        *self = Self::new(ppn, flags);
    }

    /// Returns physical address (PA)
    fn phys_addr(&self) -> PhysAddr {
        self.phys_page_num().phys_addr()
    }

    /// Returns `true` if this page is valid
    fn is_valid(&self) -> bool {
        self.flags().contains(PtEntryFlags::V)
    }

    /// Returns `true` if this page is a valid leaf entry.
    fn is_leaf(&self) -> bool {
        self.is_valid() && self.flags().intersects(PtEntryFlags::RWX)
    }

    /// Returns `true` if this page is a valid  non-leaf entry.
    fn is_non_leaf(&self) -> bool {
        self.is_valid() && !self.is_leaf()
    }

    /// Returns page table entry flags
    fn flags(&self) -> PtEntryFlags {
        PtEntryFlags::from_bits_retain(self.0 & Self::FLAGS_MASK)
    }
}

pub(crate) fn dump_pagetable(pt: &PageTable) {
    println!("page table {:p}", pt);

    let mut state = DumpState(None);
    for (level, va, pte) in pt.entries(VirtAddr::ZERO..VirtAddr::MAX) {
        if !pte.is_valid() {
            state.dump();
            continue;
        }

        if pte.is_non_leaf() {
            state.dump();
            print_non_leaf(level, va, pte);
            continue;
        }

        state.append_or_dump(level, va, pte);
    }
    state.dump();
}

struct DumpLeaves {
    level: usize,
    flags: PtEntryFlags,
    start_va: VirtAddr,
    start_pa: PhysAddr,
    end_va: VirtAddr,
    end_pa: PhysAddr,
}

struct DumpState(Option<DumpLeaves>);

impl DumpState {
    fn dump(&mut self) {
        if let Some(leaves) = self.0.take() {
            print_leaves(&leaves);
        }
    }

    fn append_or_dump(&mut self, level: usize, va: VirtAddr, pte: &PtEntry) {
        if let Some(leaves) = &mut self.0 {
            if leaves.level == level
                && leaves.flags == pte.flags()
                && leaves
                    .end_va
                    .byte_add(level_page_size(level))
                    .is_ok_and(|end_va| end_va == va)
                && leaves
                    .end_pa
                    .byte_add(level_page_size(level))
                    .is_some_and(|end_pa| end_pa == pte.phys_addr())
            {
                leaves.end_va = va;
                leaves.end_pa = pte.phys_addr();
                return;
            }
        }
        self.dump();
        self.0 = Some(DumpLeaves {
            level,
            flags: pte.flags(),
            start_va: va,
            start_pa: pte.phys_addr(),
            end_va: va,
            end_pa: pte.phys_addr(),
        });
    }
}

fn print_non_leaf(level: usize, va: VirtAddr, pte: &PtEntry) {
    let i = va.level_idx(level);
    let pa = pte.phys_addr();
    println!(
        "{prefix} [{i:3}] {va:#p} @ {pa:#p}",
        prefix = format_args!("{:.<1$}", "", (2 - level) * 2),
    );
}

fn print_leaves(leaves: &DumpLeaves) {
    struct Flags(PtEntryFlags);
    impl fmt::Debug for Flags {
        fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
            let mut all_flags = PtEntryFlags::all();
            all_flags.remove(PtEntryFlags::V);
            for (name, flag) in all_flags.iter_names() {
                if self.0.contains(flag) {
                    for ch in name.chars() {
                        write!(f, "{}", ch.to_ascii_lowercase())?;
                    }
                } else {
                    write!(f, "-")?;
                }
            }
            Ok(())
        }
    }

    let &DumpLeaves {
        level,
        flags,
        start_va,
        start_pa,
        end_va,
        end_pa,
    } = leaves;

    let start_i = start_va.level_idx(level);

    let end_i = end_va.level_idx(level) + 1;
    let end_va = end_va.byte_add(level_page_size(level)).unwrap();
    let end_pa = end_pa.byte_add(level_page_size(level)).unwrap();

    println!(
        "{prefix} [{index}] {va} => {pa} {flags:?}",
        prefix = format_args!("{:.<1$}", "", (2 - level) * 2),
        index = format_args!("{:3}..{:3}", start_i, end_i),
        va = format_args!("{start_va:#p}..{end_va:#p}"),
        pa = format_args!("{start_pa:#p}..{end_pa:#p}"),
        flags = Flags(flags),
    );
}
