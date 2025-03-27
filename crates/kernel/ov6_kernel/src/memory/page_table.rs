use alloc::boxed::Box;
use core::{
    alloc::AllocError,
    fmt,
    ops::{Range, RangeInclusive},
    ptr, slice,
};

use bitflags::bitflags;
use dataview::Pod;
use riscv::register::satp::{self, Satp};

use super::{PhysAddr, VirtAddr, addr::PhysPageNum, page::PageFrameAllocator};
use crate::{
    error::KernelError,
    memory::{self, PAGE_SHIFT, PAGE_SIZE, PageRound as _, level_page_size},
    println,
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
        let shift = 9 * level + PAGE_SHIFT;
        (va.addr() >> shift) & 0x1ff
    }

    /// Returns the physical address containing this page table
    fn phys_addr(&self) -> PhysAddr {
        PhysAddr::new(ptr::from_ref(self).addr())
    }

    /// Returns the physical page number of the physical page containing this
    /// page table
    fn phys_page_num(&self) -> PhysPageNum {
        self.phys_addr().phys_page_num()
    }

    pub(super) fn satp(&self) -> Satp {
        let mut satp = Satp::from_bits(0);
        satp.set_mode(satp::Mode::Sv39);
        satp.set_ppn(self.phys_page_num().value());
        satp
    }

    fn validate_inclusive(
        &self,
        level: usize,
        va: RangeInclusive<usize>,
        flags: PtEntryFlags,
    ) -> Result<(), KernelError> {
        assert!(level <= 2);
        let shift = 9 * level + PAGE_SHIFT;
        let i_start = va.start() >> shift;
        let i_end = va.end() >> shift;
        assert!(i_start <= i_end && i_end <= self.0.len());

        for i in i_start..=i_end {
            let pte = &self.0[i];
            if !pte.is_valid() {
                return Err(KernelError::VirtualPageNotMapped(
                    VirtAddr::new(*va.start()).unwrap(),
                ));
            }

            if pte.is_leaf() {
                if !pte.flags().contains(flags) {
                    return Err(KernelError::InaccessiblePage(
                        VirtAddr::new(*va.start()).unwrap(),
                    ));
                }
                return Ok(());
            }

            assert!(pte.is_non_leaf());

            let table_start_addr = if i == i_start {
                va.start() & !(0x1ff << shift)
            } else {
                i << shift
            };
            let table_end_addr = if i == i_end {
                va.end() & !(0x1ff << shift)
            } else {
                ((i + 1) << shift) - 1
            };

            pte.get_page_table().unwrap().validate_inclusive(
                level - 1,
                table_start_addr..=table_end_addr,
                flags,
            )?;
        }
        Ok(())
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
    pub(super) fn validate(
        &self,
        va: Range<VirtAddr>,
        flags: PtEntryFlags,
    ) -> Result<(), KernelError> {
        assert!(va.start <= va.end);
        if va.start == va.end {
            return Ok(());
        }

        let start = va.start.addr();
        let end = va.end.addr() - 1;
        self.validate_inclusive(2, start..=end, flags)
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
        pa: PhysAddr,
        perm: PtEntryFlags,
    ) -> Result<(), KernelError> {
        assert!(level <= 2);
        assert!(perm.intersects(PtEntryFlags::RWX), "perm={perm:?}");

        let mut pt = self;
        for level in (level + 1..=2).rev() {
            let index = Self::entry_index(level, va);
            let pte = &mut pt.0[index];
            if !pte.is_valid() {
                let new_pt = Self::try_allocate()?;
                pte.set_page_table(new_pt);
            }
            pt = pte.get_page_table_mut().unwrap();
        }

        let index = Self::entry_index(level, va);
        let pte = &mut pt.0[index];
        assert!(
            !pte.is_valid(),
            "remap on the already mapped address: va={va:?}"
        );
        pte.set_phys_page_num(pa.phys_page_num(), perm | PtEntryFlags::V);
        Ok(())
    }

    fn map_addrs_level(
        &mut self,
        level: usize,
        va: &mut VirtAddr,
        pa: &mut PhysAddr,
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
    pub fn map_addrs(
        &mut self,
        mut va: VirtAddr,
        mut pa: PhysAddr,
        size: usize,
        perm: PtEntryFlags,
    ) -> Result<(), KernelError> {
        assert!(va.is_page_aligned());
        assert!(pa.is_page_aligned());
        assert!(size.is_multiple_of(PAGE_SIZE));
        assert!(perm.intersects(PtEntryFlags::RWX), "perm={perm:?}");

        if va.addr() % level_page_size(1) != pa.addr() % level_page_size(1) {
            return self.map_addrs_level(0, &mut va, &mut pa, size, perm);
        }

        let va_end = va.byte_add(size)?;
        let pa_end = pa.byte_add(size).unwrap();
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
        assert_eq!(pa, pa_end);
        Ok(())
    }

    /// Unmaps the page of memory at virtual page `vpn`.
    ///
    /// Returns the physical address of the page that was unmapped.
    fn unmap_page(&mut self, va: VirtAddr) -> Result<(usize, PhysAddr), KernelError> {
        let (level, pte) = self.find_leaf_entry_mut(va)?;
        let pa = pte.phys_addr();
        pte.clear();
        Ok((level, pa))
    }

    /// Unmaps the pages of memory starting at virtual page `vpn` and
    /// covering `npages` pages.
    pub(super) fn unmap_addrs(
        &mut self,
        va: VirtAddr,
        size: usize,
    ) -> Result<UnmapPages, KernelError> {
        let start = va;
        let end = va.byte_add(size)?;
        Ok(UnmapPages {
            pt: self,
            va_range: start..end,
        })
    }

    /// Returns the leaf PTE in the page tables that corredponds to virtual
    /// page `va`.
    pub(super) fn find_leaf_entry(&self, va: VirtAddr) -> Result<(usize, &PtEntry), KernelError> {
        let mut pt = self;
        for level in (0..=2).rev() {
            let index = Self::entry_index(level, va);
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
    pub(super) fn find_leaf_entry_mut(
        &mut self,
        va: VirtAddr,
    ) -> Result<(usize, &mut PtEntry), KernelError> {
        let mut pt = self;
        for level in (0..=2).rev() {
            let index = Self::entry_index(level, va);
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

        let page_size = memory::level_page_size(level);
        let pa = pte.phys_addr();
        let page = unsafe { slice::from_raw_parts(pa.as_ptr(), page_size) };
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

        let page_size = memory::level_page_size(level);
        let pa = pte.phys_addr();
        let page = unsafe { slice::from_raw_parts_mut(pa.as_mut_ptr().as_ptr(), page_size) };
        let offset = va.addr() % page_size;
        Ok(&mut page[offset..])
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
    va_range: Range<VirtAddr>,
}

impl Iterator for UnmapPages<'_> {
    type Item = Result<(usize, PhysAddr), KernelError>;

    fn next(&mut self) -> Option<Self::Item> {
        let va = self.va_range.start;
        if va >= self.va_range.end {
            return None;
        }
        let (level, pa) = match self.pt.unmap_page(va) {
            Ok(v) => v,
            Err(e) => {
                self.va_range.start = self.va_range.start.byte_add(PAGE_SIZE).unwrap();
                return Some(Err(e));
            }
        };
        let page_size = memory::level_page_size(level);
        self.va_range.start = va.byte_add(page_size).unwrap();
        Some(Ok((level, pa)))
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

    /// Clears the page table entry.
    pub(super) fn clear(&mut self) {
        self.0 = 0;
    }
}

pub(crate) fn dump_pagetable(pt: &PageTable) {
    println!("page table {:p}", pt);

    let level = 2;
    dump_pagetable_level(pt, level, VirtAddr::ZERO);
}

fn pte_va(base_va: VirtAddr, i: usize, level: usize) -> VirtAddr {
    assert!(i <= 512);
    base_va
        .map_addr(|a| a | (i << (9 * level + PAGE_SHIFT)))
        .unwrap()
}

fn dump_pagetable_level(pt: &PageTable, level: usize, base_va: VirtAddr) {
    let mut state = None;

    for (i, pte) in pt.0.iter().enumerate() {
        if !pte.is_valid() {
            if let Some((start_i, start_pte, end_i, end_ptr)) = state.take() {
                print_pte(level, base_va, start_i, start_pte, end_i, end_ptr);
            }
            continue;
        }

        if pte.is_non_leaf() {
            if let Some((start_i, start_pte, end_i, end_pte)) = state.take() {
                print_pte(level, base_va, start_i, start_pte, end_i, end_pte);
            }

            let va = pte_va(base_va, i, level);
            let pa = pte.phys_addr();
            println!(
                "{prefix} [{i:3}] {va:#p} @ {pa:#p}",
                prefix = format_args!("{:.<1$}", "", (2 - level) * 2),
            );
            dump_pagetable_level(pte.get_page_table().unwrap(), level - 1, va);
            continue;
        }

        match &mut state {
            Some((start_i, start_pte, end_i, end_pte)) => {
                if start_pte.flags() == pte.flags()
                    && end_pte.phys_addr().byte_add(level_page_size(level)) == Some(pte.phys_addr())
                {
                    *end_i = i;
                    *end_pte = pte;
                    continue;
                }

                print_pte(level, base_va, *start_i, start_pte, *end_i, end_pte);
                state = Some((i, pte, i, pte));
            }
            None => {
                state = Some((i, pte, i, pte));
            }
        }
    }

    if let Some((start_i, start_pte, end_i, end_pte)) = state.take() {
        print_pte(level, base_va, start_i, start_pte, end_i, end_pte);
    }
}

fn print_pte(
    level: usize,
    base_va: VirtAddr,
    start_i: usize,
    start_pte: &PtEntry,
    end_i: usize,
    end_pte: &PtEntry,
) {
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

    let start_va = pte_va(base_va, start_i, level);
    let start_pa = start_pte.phys_addr();

    let end_va = pte_va(base_va, end_i, level)
        .byte_add(level_page_size(level))
        .unwrap();
    let end_pa = end_pte
        .phys_addr()
        .byte_add(level_page_size(level))
        .unwrap();

    assert_eq!(start_pte.flags(), end_pte.flags());

    println!(
        "{prefix} [{index}] {va} => {pa} {flags:?}",
        prefix = format_args!("{:.<1$}", "", (2 - level) * 2),
        index = format_args!("{:3}..{:3}", start_i, end_i + 1),
        va = format_args!("{start_va:#p}..{end_va:#p}"),
        pa = format_args!("{start_pa:#p}..{end_pa:#p}"),
        flags = Flags(start_pte.flags()),
    );
}
