use alloc::boxed::Box;
use core::{alloc::AllocError, ops::Range, ptr};

use bitflags::bitflags;
use dataview::Pod;

use super::{PhysAddr, PhysPageNum, VirtAddr, addr::VirtPageNum, page::PageFrameAllocator};
use crate::{error::KernelError, memory::PAGE_SIZE};

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
    fn entry_index(level: usize, vpn: VirtPageNum) -> usize {
        assert!(level <= 2);
        let shift = 9 * level;
        (vpn.value() >> shift) & 0x1ff
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

    /// Creates PTE for virtual page `vpn` that refer to
    /// physical page `ppn`.
    ///
    /// Returns `Ok(())` on success, `Err()` if `walk()` couldn't
    /// allocate a needed page-table page.
    pub fn map_page(
        &mut self,
        vpn: VirtPageNum,
        ppn: PhysPageNum,
        perm: PtEntryFlags,
    ) -> Result<(), KernelError> {
        assert!(perm.intersects(PtEntryFlags::RWX), "perm={perm:?}");

        self.update_level0_entry(vpn, true, |pte| {
            assert!(
                !pte.is_valid(),
                "remap on the already mapped address: va={vpn:?}"
            );
            pte.set_phys_page_num(ppn, perm | PtEntryFlags::V);
        })
    }

    /// Creates PTEs for virtual addresses starting at `vpn` that refer to
    /// physical addresses starting at `ppn`.
    ///
    /// `size` MUST be page-aligned.
    ///
    /// Returns `Ok(())` on success, `Err()` if `walk()` couldn't
    /// allocate a needed page-table page.
    pub fn map_pages(
        &mut self,
        vpn: VirtPageNum,
        npages: usize,
        ppn: PhysPageNum,
        perm: PtEntryFlags,
    ) -> Result<(), KernelError> {
        assert_ne!(npages, 0, "npages={npages:#x}");

        let mut vpn = vpn;
        let mut ppn = ppn;
        let last = vpn.checked_add(npages)?;
        while vpn < last {
            self.map_page(vpn, ppn, perm)?;
            vpn = vpn.checked_add(1).unwrap();
            ppn = ppn.checked_add(1).unwrap();
        }
        Ok(())
    }

    /// Unmaps the page of memory at virtual page `vpn`.
    ///
    /// Returns the physical address of the page that was unmapped.
    pub(super) fn unmap_page(&mut self, vpn: VirtPageNum) -> Result<PhysAddr, KernelError> {
        self.update_level0_entry(vpn, false, |pte| {
            assert!(pte.is_valid());
            assert!(pte.is_leaf(), "{:?}", pte.flags());
            let pa = pte.phys_addr();
            pte.clear();
            pa
        })
    }

    /// Unmaps the pages of memory starting at virtual page `vpn` and
    /// covering `npages` pages.
    pub(super) fn unmap_pages(
        &mut self,
        vpn: VirtPageNum,
        npages: usize,
    ) -> Result<UnmapPages, KernelError> {
        let start = vpn;
        let end = vpn.checked_add(npages)?;
        Ok(UnmapPages {
            pt: self,
            vpns: start..end,
        })
    }

    /// Returns the leaf PTE in the page tables that corredponds to virtual
    /// page `vpn`.
    pub(super) fn find_leaf_entry(&self, vpn: VirtPageNum) -> Result<&PtEntry, KernelError> {
        let mut pt = self;
        for level in (1..=2).rev() {
            let index = Self::entry_index(level, vpn);
            pt = pt.0[index]
                .get_page_table()
                .ok_or(KernelError::VirtualPageNotMapped(vpn))?;
        }

        let index = Self::entry_index(0, vpn);
        let pte = &pt.0[index];
        if !pte.is_leaf() {
            return Err(KernelError::VirtualPageNotMapped(vpn));
        }
        Ok(pte)
    }

    /// Updates the level-0 PTE in the page tables that corredponds to virtual
    /// page `vpn`.
    ///
    /// If `insert_new_table` is `true`, it will allocate new page-table pages
    /// if needed.
    ///
    /// Updated PTE must be leaf PTE or invalid.
    pub(super) fn update_level0_entry<T, F>(
        &mut self,
        vpn: VirtPageNum,
        insert_new_table: bool,
        f: F,
    ) -> Result<T, KernelError>
    where
        F: for<'a> FnOnce(&'a mut PtEntry) -> T,
    {
        let mut pt = self;
        for level in (1..=2).rev() {
            let index = Self::entry_index(level, vpn);
            if !pt.0[index].is_valid() {
                if !insert_new_table {
                    return Err(KernelError::VirtualPageNotMapped(vpn));
                }
                let new_pt = Self::try_allocate()?;
                pt.0[index].set_page_table(new_pt);
            }
            pt = pt.0[index].get_page_table_mut().unwrap();
        }

        let index = Self::entry_index(0, vpn);
        let pte = &mut pt.0[index];
        let res = f(pte);
        // cannot change PTE to non-leaf (level0 PTE must be invalid or leaf)
        assert!(!pte.is_non_leaf());
        Ok(res)
    }

    /// Looks up a virtual address, returns the physical address.
    pub fn resolve_page(
        &self,
        vpn: VirtPageNum,
        flags: PtEntryFlags,
    ) -> Result<PhysPageNum, KernelError> {
        let pte = self.find_leaf_entry(vpn)?;
        assert!(pte.is_valid() && pte.is_leaf());
        if !pte.flags().contains(flags) {
            return Err(KernelError::InaccessiblePage(vpn));
        }

        Ok(pte.phys_page_num())
    }

    /// Looks up a virtual address, returns the physical address.
    pub fn resolve_virt_addr(
        &self,
        va: VirtAddr,
        flags: PtEntryFlags,
    ) -> Result<PhysAddr, KernelError> {
        let ppn = self.resolve_page(va.virt_page_num(), flags)?;
        Ok(ppn.phys_addr().map_addr(|a| a + va.addr() % PAGE_SIZE))
    }

    /// Fetches the page that is mapped at virtual address `va`.
    pub(super) fn fetch_page(
        &self,
        vpn: VirtPageNum,
        flags: PtEntryFlags,
    ) -> Result<&[u8; PAGE_SIZE], KernelError> {
        let ppn = self.resolve_page(vpn, flags)?;
        let page = unsafe { ppn.as_ptr().as_ref() };
        Ok(page)
    }

    /// Fetches the page that is mapped at virtual address `va`.
    #[expect(clippy::needless_pass_by_ref_mut)]
    pub(super) fn fetch_page_mut(
        &mut self,
        vpn: VirtPageNum,
        flags: PtEntryFlags,
    ) -> Result<&mut [u8; PAGE_SIZE], KernelError> {
        let ppn = self.resolve_page(vpn, flags)?;
        let page = unsafe { ppn.as_ptr().as_mut() };
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
    vpns: Range<VirtPageNum>,
}

impl Iterator for UnmapPages<'_> {
    type Item = Result<PhysAddr, KernelError>;

    fn next(&mut self) -> Option<Self::Item> {
        let vpn = self.vpns.start;
        if vpn >= self.vpns.end {
            return None;
        }
        self.vpns.start = vpn.checked_add(1).unwrap();
        Some(self.pt.unmap_page(vpn))
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
