use alloc::boxed::Box;
use core::{alloc::AllocError, ptr};

use bitflags::bitflags;
use dataview::Pod;

use super::{PhysAddr, PhysPageNum, VirtAddr, addr::AddressChunks, page::PageFrameAllocator};
use crate::{
    error::KernelError,
    memory::{PAGE_SHIFT, PAGE_SIZE, PageRound as _},
};

#[repr(transparent)]
#[derive(Pod)]
pub struct PageTable([PtEntry; 512]);

impl PageTable {
    /// Allocates a new empty page table.
    pub(super) fn try_allocate() -> Result<Box<Self, PageFrameAllocator>, KernelError> {
        let pt = Box::try_new_zeroed_in(PageFrameAllocator)
            .map_err(|AllocError| KernelError::NoFreePage)?;
        Ok(unsafe { pt.assume_init() })
    }

    /// Returns the page table index that corresponds to virtual address `va`
    ///
    /// The RISC-V Sv39 schema has three levels of page-table
    /// pages. A page-table page contains 512 64-bit PTEs.
    /// A 64-bit virtual address is split into five fields:
    /// ```text
    ///     39..=63 -- must be zero.
    ///     30..=38 -- 9 bits of level-2 index.
    ///     21..=29 -- 9 bits of level-1 index.
    ///     12..=20 -- 9 bits of level-0 index.
    ///      0..=11 -- 12 bits byte offset with the page.
    /// ```
    fn entry_index(level: usize, va: VirtAddr) -> usize {
        assert!(level <= 2);
        let shift = PAGE_SHIFT + (9 * level);
        (va.addr() >> shift) & 0x1ff
    }

    /// Returns the physical address containing this page table
    pub(super) fn phys_addr(&self) -> PhysAddr {
        PhysAddr::new(ptr::from_ref(self).addr())
    }

    /// Returns the physical page number of the physical page containing this
    /// page table
    pub(super) fn phys_page_num(&self) -> PhysPageNum {
        self.phys_addr().phys_page_num()
    }

    /// Creates PTE for virtual address `va` that refer to
    /// physical addresses `pa`.
    ///
    /// `va` MUST be page-aligned.
    ///
    /// Returns `Ok(())` on success, `Err()` if `walk()` couldn't
    /// allocate a needed page-table page.
    pub fn map_page(
        &mut self,
        va: VirtAddr,
        pa: PhysAddr,
        perm: PtEntryFlags,
    ) -> Result<(), KernelError> {
        assert!(va.is_page_aligned(), "va={va:?}");
        assert!(perm.intersects(PtEntryFlags::RWX), "perm={perm:?}");

        self.update_level0_entry(va, true, |pte| {
            assert!(
                !pte.is_valid(),
                "remap on the already mapped address: va={va:?}"
            );
            pte.set_phys_addr(pa, perm | PtEntryFlags::V);
        })
    }

    /// Creates PTEs for virtual addresses starting at `va` that refer to
    /// physical addresses starting at `pa`.
    ///
    /// `va` and `size` MUST be page-aligned.
    ///
    /// Returns `Ok(())` on success, `Err()` if `walk()` couldn't
    /// allocate a needed page-table page.
    pub fn map_pages(
        &mut self,
        va: VirtAddr,
        size: usize,
        pa: PhysAddr,
        perm: PtEntryFlags,
    ) -> Result<(), KernelError> {
        assert!(va.is_page_aligned(), "va={va:?}");
        assert!(size.is_page_aligned(), "size={size:#x}");
        assert_ne!(size, 0, "size={size:#x}");

        let mut va = va;
        let mut pa = pa;
        let last = va.byte_add(size - PAGE_SIZE)?;
        loop {
            self.map_page(va, pa, perm)?;
            if va == last {
                return Ok(());
            }

            va = va.byte_add(PAGE_SIZE).unwrap();
            pa = pa.byte_add(PAGE_SIZE);
        }
    }

    /// Unmaps the page of memory at virtual address `va`.
    ///
    /// Returns the physical address of the page that was unmapped.
    pub(super) fn unmap_page(&mut self, va: VirtAddr) -> Result<PhysAddr, KernelError> {
        assert!(va.is_page_aligned(), "va={va:?}");

        self.update_level0_entry(va, false, |pte| {
            assert!(pte.is_valid());
            assert!(pte.is_leaf(), "{:?}", pte.flags());
            let pa = pte.phys_addr();
            pte.clear();
            pa
        })
    }

    /// Unmaps the pages of memory starting at virtual address `va` and
    /// covering `npages` pages.
    pub(super) fn unmap_pages(
        &mut self,
        va: VirtAddr,
        npages: usize,
    ) -> Result<UnmapPages, KernelError> {
        let start = va;
        Ok(UnmapPages {
            pt: self,
            chunks: AddressChunks::from_size(start, npages * PAGE_SIZE)?,
        })
    }

    /// Returns the leaf PTE in the page tables that corredponds to virtual
    /// address `va`.
    pub(super) fn find_leaf_entry(&self, va: VirtAddr) -> Result<&PtEntry, KernelError> {
        assert!(va < VirtAddr::MAX);

        let mut pt = self;
        for level in (1..=2).rev() {
            let index = Self::entry_index(level, va);
            pt = pt.0[index]
                .get_page_table()
                .ok_or(KernelError::AddressNotMapped(va))?;
        }

        let index = Self::entry_index(0, va);
        let pte = &pt.0[index];
        if !pte.is_leaf() {
            return Err(KernelError::AddressNotMapped(va));
        }
        Ok(pte)
    }

    /// Updates the level-0 PTE in the page tables that corredponds to virtual
    /// address `va`.
    ///
    /// If `insert_new_table` is `true`, it will allocate new page-table pages
    /// if needed.
    ///
    /// Updated PTE must be leaf PTE or invalid.
    pub(super) fn update_level0_entry<T, F>(
        &mut self,
        va: VirtAddr,
        insert_new_table: bool,
        f: F,
    ) -> Result<T, KernelError>
    where
        F: for<'a> FnOnce(&'a mut PtEntry) -> T,
    {
        assert!(va < VirtAddr::MAX);

        let mut pt = self;
        for level in (1..=2).rev() {
            let index = Self::entry_index(level, va);
            if !pt.0[index].is_valid() {
                if !insert_new_table {
                    return Err(KernelError::AddressNotMapped(va));
                }
                let new_pt = Self::try_allocate()?;
                pt.0[index].set_page_table(new_pt);
            }
            pt = pt.0[index].get_page_table_mut().unwrap();
        }

        let index = Self::entry_index(0, va);
        let pte = &mut pt.0[index];
        let res = f(pte);
        // cannot change PTE to non-leaf (level0 PTE must be invalid or leaf)
        assert!(!pte.is_non_leaf());
        Ok(res)
    }

    /// Looks up a virtual address, returns the physical address.
    pub fn resolve_virtual_address(
        &self,
        va: VirtAddr,
        flags: PtEntryFlags,
    ) -> Result<PhysAddr, KernelError> {
        let pte = self.find_leaf_entry(va)?;
        assert!(pte.is_valid() && pte.is_leaf());
        if !pte.flags().contains(flags) {
            return Err(KernelError::InaccessibleMemory(va));
        }

        Ok(pte.phys_addr())
    }

    /// Fetches the page that is mapped at virtual address `va`.
    pub(super) fn fetch_page(
        &self,
        va: VirtAddr,
        flags: PtEntryFlags,
    ) -> Result<&[u8; PAGE_SIZE], KernelError> {
        let pa = self.resolve_virtual_address(va, flags)?;
        let page = unsafe { pa.as_mut_ptr::<[u8; PAGE_SIZE]>().as_ref() };
        Ok(page)
    }

    /// Fetches the page that is mapped at virtual address `va`.
    #[expect(clippy::needless_pass_by_ref_mut)]
    pub(super) fn fetch_page_mut(
        &mut self,
        va: VirtAddr,
        flags: PtEntryFlags,
    ) -> Result<&mut [u8; PAGE_SIZE], KernelError> {
        let pa = self.resolve_virtual_address(va, flags)?;
        let page = unsafe { pa.as_mut_ptr::<[u8; PAGE_SIZE]>().as_mut() };
        Ok(page)
    }

    /// Recursively frees page-table pages.
    ///
    /// All leaf mappings must already have been removed.
    pub(super) fn free_descendant(&mut self) {
        for pte in &mut self.0 {
            assert!(!pte.is_valid() || pte.is_non_leaf());
            if let Some(mut pt) = pte.take_page_table() {
                pt.free_descendant();
                pte.clear();
            }
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

pub(super) struct UnmapPages<'a> {
    pt: &'a mut PageTable,
    chunks: AddressChunks,
}

impl Iterator for UnmapPages<'_> {
    type Item = Result<PhysAddr, KernelError>;

    fn next(&mut self) -> Option<Self::Item> {
        let chunk = self.chunks.next()?;
        Some(self.pt.unmap_page(chunk.page_range().start))
    }
}

impl Drop for UnmapPages<'_> {
    fn drop(&mut self) {
        for _ in self {}
    }
}

#[repr(transparent)]
#[derive(Pod)]
pub(super) struct PtEntry(usize);

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

    fn get_page_table(&self) -> Option<&PageTable> {
        self.is_non_leaf()
            .then(|| unsafe { self.phys_addr().as_mut_ptr::<PageTable>().as_ref() })
    }

    fn get_page_table_mut(&mut self) -> Option<&mut PageTable> {
        self.is_non_leaf()
            .then(|| unsafe { self.phys_addr().as_mut_ptr::<PageTable>().as_mut() })
    }

    fn set_page_table(&mut self, pt: Box<PageTable, PageFrameAllocator>) {
        assert!(!self.is_valid());
        let ppn = pt.phys_page_num();
        Box::leak(pt);
        *self = Self::new(ppn, PtEntryFlags::V);
    }

    fn take_page_table(&mut self) -> Option<Box<PageTable, PageFrameAllocator>> {
        self.is_non_leaf().then(|| {
            let ptr = self.phys_addr().as_mut_ptr();
            let pt = unsafe { Box::from_raw_in(ptr.as_ptr(), PageFrameAllocator) };
            self.clear();
            pt
        })
    }

    /// Returns physical page number (PPN)
    pub(super) fn phys_page_num(&self) -> PhysPageNum {
        PhysPageNum::new(self.0 >> 10)
    }

    pub(super) fn set_phys_page_num(&mut self, ppn: PhysPageNum, flags: PtEntryFlags) {
        assert!(!self.is_valid());
        assert!(flags.contains(PtEntryFlags::V));
        *self = Self::new(ppn, flags);
    }

    /// Returns physical address (PA)
    pub(super) fn phys_addr(&self) -> PhysAddr {
        self.phys_page_num().phys_addr()
    }

    pub(super) fn set_phys_addr(&mut self, pa: PhysAddr, flags: PtEntryFlags) {
        self.set_phys_page_num(pa.phys_page_num(), flags);
    }

    /// Returns `true` if this page is valid
    pub(super) fn is_valid(&self) -> bool {
        self.flags().contains(PtEntryFlags::V)
    }

    /// Returns `true` if this page is a valid leaf entry.
    pub(super) fn is_leaf(&self) -> bool {
        self.is_valid() && self.flags().intersects(PtEntryFlags::RWX)
    }

    /// Returns `true` if this page is a valid  non-leaf entry.
    pub(super) fn is_non_leaf(&self) -> bool {
        self.is_valid() && !self.is_leaf()
    }

    /// Returns page table entry flags
    pub(super) fn flags(&self) -> PtEntryFlags {
        PtEntryFlags::from_bits_retain(self.0 & Self::FLAGS_MASK)
    }

    /// Sets page table entry flags.
    pub(super) fn set_flags(&mut self, flags: PtEntryFlags) {
        self.0 &= !&Self::FLAGS_MASK;
        self.0 |= flags.bits();
    }

    /// Clears the page table entry.
    pub(super) fn clear(&mut self) {
        self.0 = 0;
    }
}
