use alloc::boxed::Box;
use core::slice;

use bitflags::bitflags;
use dataview::Pod;

use super::PageTableEntries;
use crate::{
    error::KernelError,
    memory::{
        self, PhysAddr, VirtAddr,
        addr::PhysPageNum,
        level_page_size,
        page::{self, PageFrameAllocator},
        page_manager::Page,
    },
};

bitflags! {
    /// Flags for page table entries.
    ///
    /// These flags define the properties and permissions of a page table entry.
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
        /// If set, the CPU can execute instructions on this virtual address.
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
        /// If set, this virtual address has been accessed.
        const A = 1 << 6;

        /// Dirty Bit of page table entry.
        ///
        /// If set, this virtual address has been written to.
        const D = 1 << 7;

        /// Copy-On-Write Bit of page table entry.
        ///
        /// If set, this virtual address is a copy-on-write mapping.
        const C = 1 << 8;

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

/// Represents a single page table entry.
///
/// This structure encapsulates the physical address and flags associated with
/// a page table entry.
#[repr(transparent)]
#[derive(Pod)]
pub(super) struct PtEntry(usize);

impl PtEntry {
    const FLAGS_MASK: usize = 0x3FF;

    /// Creates a new page table entry with the given physical page number and
    /// flags.
    ///
    /// # Panics
    ///
    /// Panics if the provided flags contain bits outside the valid range.
    ///
    /// # Safety
    ///
    /// The caller must ensure that the physical address points to a valid page
    /// table.
    unsafe fn new(ppn: PhysPageNum, flags: PtEntryFlags) -> Self {
        assert_eq!(
            flags.bits() & Self::FLAGS_MASK,
            flags.bits(),
            "flags: {flags:#x}={flags:?}"
        );
        let bits = (ppn.value() << 10) | (flags.bits() & 0x3FF);
        Self(bits)
    }

    /// Returns a reference to the page table entries if this entry is a
    /// non-leaf entry.
    pub(super) fn get_page_table(&self) -> Option<&PageTableEntries> {
        self.is_non_leaf()
            .then(|| unsafe { self.phys_addr().as_non_null::<PageTableEntries>().as_ref() })
    }

    /// Returns a mutable reference to the page table entries if this entry is a
    /// non-leaf entry.
    pub(super) fn get_page_table_mut(&mut self) -> Option<&mut PageTableEntries> {
        self.is_non_leaf()
            .then(|| unsafe { self.phys_addr().as_non_null::<PageTableEntries>().as_mut() })
    }

    /// Sets this entry to point to a new page table.
    ///
    /// The provided page table is leaked into the entry.
    ///
    /// # Panics
    ///
    /// Panics if the entry is already valid.
    pub(super) fn set_page_table(&mut self, pt: Box<PageTableEntries, PageFrameAllocator>) {
        assert!(!self.is_valid());
        let ppn = pt.phys_page_num();
        Box::leak(pt);
        *self = unsafe { Self::new(ppn, PtEntryFlags::V) };
    }

    /// Returns a reference to the memory bytes for this entry if it is a leaf
    /// entry.
    pub(super) fn get_page_bytes(&self, level: usize) -> Option<&[u8]> {
        self.is_leaf().then(|| {
            let pa = self.phys_addr();
            let page_size = memory::level_page_size(level);
            unsafe { slice::from_raw_parts(pa.as_ptr(), page_size) }
        })
    }

    /// Returns a mutable reference to the memory bytes for this entry if it is
    /// a leaf entry.
    #[expect(clippy::needless_pass_by_ref_mut)]
    pub(super) fn get_page_bytes_mut(&mut self, level: usize) -> Option<&mut [u8]> {
        self.is_leaf().then(|| {
            let pa = self.phys_addr();
            let page_size = memory::level_page_size(level);
            unsafe { slice::from_raw_parts_mut(pa.as_mut_ptr(), page_size) }
        })
    }

    /// Frees the memory associated with this entry.
    ///
    /// If the entry is a non-leaf entry, it recursively frees all child
    /// entries.
    pub(super) fn free(&mut self, level: usize) {
        if !self.is_valid() {
            return;
        }

        let pa = self.phys_addr();
        let is_non_leaf = self.is_non_leaf();
        self.0 = 0;

        if is_non_leaf {
            assert!(level > 0);
            let ptr = pa.as_mut_ptr::<PageTableEntries>();
            let mut pt = unsafe { Box::from_raw_in(ptr, PageFrameAllocator) };
            pt.free(level - 1);
            drop(pt);
        } else {
            assert_eq!(level, 0, "super page is not supported yet");
            if page::is_heap_addr(pa) {
                drop(Page::from_raw(pa));
            }
        }
    }

    /// Returns the physical page number (PPN) associated with this entry.
    fn phys_page_num(&self) -> PhysPageNum {
        PhysPageNum::new(self.0 >> 10)
    }

    /// Sets the physical page number and flags for this entry.
    ///
    /// # Panics
    ///
    /// Panics if the entry is already valid or if the flags do not include the
    /// valid bit.
    ///
    /// # Safety
    ///
    /// The caller must ensure that the physical address points to a valid page
    /// table.
    pub(super) unsafe fn set_phys_page_num(&mut self, ppn: PhysPageNum, flags: PtEntryFlags) {
        assert!(!self.is_valid());
        assert!(flags.contains(PtEntryFlags::V));
        *self = unsafe { Self::new(ppn, flags) };
    }

    /// Returns the physical address (PA) associated with this entry.
    pub(super) fn phys_addr(&self) -> PhysAddr {
        self.phys_page_num().phys_addr()
    }

    /// Returns `true` if this entry is valid.
    pub(super) fn is_valid(&self) -> bool {
        self.flags().contains(PtEntryFlags::V)
    }

    /// Returns `true` if this entry is a valid leaf entry.
    pub(super) fn is_leaf(&self) -> bool {
        self.is_valid() && self.flags().intersects(PtEntryFlags::RWX)
    }

    /// Returns `true` if this entry is a valid non-leaf entry.
    pub(super) fn is_non_leaf(&self) -> bool {
        self.is_valid() && !self.is_leaf()
    }

    /// Returns the flags associated with this entry.
    pub(super) fn flags(&self) -> PtEntryFlags {
        PtEntryFlags::from_bits_retain(self.0 & Self::FLAGS_MASK)
    }

    pub(crate) fn make_copy_on_write(&mut self) {
        let mut flags = self.flags();
        if flags.contains(PtEntryFlags::W) {
            flags.remove(PtEntryFlags::W);
            flags.insert(PtEntryFlags::C);
            *self = unsafe { Self::new(self.phys_page_num(), flags) };
        }
    }

    pub(crate) fn request_user_write(
        &mut self,
        level: usize,
        va: VirtAddr,
    ) -> Result<(), KernelError> {
        let mut flags = self.flags();
        if flags.contains(PtEntryFlags::UW) {
            return Ok(());
        }

        if !flags.contains(PtEntryFlags::U | PtEntryFlags::C) {
            return Err(KernelError::InaccessiblePage(va));
        }

        flags.remove(PtEntryFlags::C);
        flags.insert(PtEntryFlags::W);

        assert_eq!(level, 0, "super page is not supported yet");

        let page = Page::from_raw(self.phys_addr());
        if page.ref_count() == 1 {
            let pa = page.into_raw();
            *self = unsafe { Self::new(pa.phys_page_num(), flags) };
            return Ok(());
        }

        let old_pa = self.phys_addr();
        let new_page = Page::alloc()?;
        let new_pa = new_page.into_raw();

        unsafe {
            new_pa
                .as_mut_ptr::<u8>()
                .copy_from(old_pa.as_ptr::<u8>(), level_page_size(level));
        }
        *self = unsafe { Self::new(new_pa.phys_page_num(), flags) };

        drop(page);

        Ok(())
    }
}
