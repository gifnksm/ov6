use core::{
    ffi::{c_int, c_void},
    num::NonZero,
    ops::Range,
    ptr::{self, NonNull},
    sync::atomic::{AtomicUsize, Ordering},
};

use bitflags::bitflags;
use riscv::{asm, register::satp};

use crate::{
    kalloc,
    memlayout::{KERN_BASE, PHYS_TOP, PLIC, TRAMPOLINE, UART0, VIRTIO0},
    proc, trampoline,
};

mod ffi {
    use core::{ffi::c_char, slice};

    use super::*;

    #[unsafe(no_mangle)]
    extern "C" fn copyout(
        pagetable: *mut PageTable,
        dst_va: u64,
        src: *const c_char,
        len: u64,
    ) -> c_int {
        let pagetable = unsafe { pagetable.as_ref().unwrap() };
        let src = unsafe { slice::from_raw_parts(src, len as usize) };
        match super::copy_out(pagetable, VirtAddr(dst_va as usize), src) {
            Ok(()) => 0,
            Err(()) => -1,
        }
    }
}

/// Bytes per page
pub const PAGE_SIZE: usize = 4096;
/// Bits of offset within a page
pub const PAGE_SHIFT: usize = 12;

/// The kernel's page table address.
static KERNEL_PAGETABLE: AtomicUsize = AtomicUsize::new(0);

/// Address of the end of kernel code.
const ETEXT: NonNull<c_void> = {
    unsafe extern "C" {
        #[link_name = "etext"]
        static mut ETEXT: [u8; 0];
    }
    NonNull::new((&raw mut ETEXT).cast()).unwrap()
};

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
        Self(self.0.page_roundup())
    }

    fn page_rounddown(&self) -> Self {
        Self(self.0.page_rounddown())
    }

    fn is_page_aligned(&self) -> bool {
        is_page_aligned(self.addr())
    }
}

impl PageRound for PhysAddr {
    fn page_roundup(&self) -> Self {
        Self(self.0.page_roundup())
    }

    fn page_rounddown(&self) -> Self {
        Self(self.0.page_rounddown())
    }

    fn is_page_aligned(&self) -> bool {
        is_page_aligned(self.addr())
    }
}

/// Makes a direct-map page table for the kernel.
fn make_kernel_pt() -> &'static mut PageTable {
    use PtEntryFlags as F;

    let etext = ETEXT.addr().into();
    let phys_trampoline = PhysAddr(trampoline::trampoline as usize);

    unsafe fn ident_map(
        kpgtbl: &mut PageTable,
        addr: usize,
        size: usize,
        perm: PtEntryFlags,
    ) -> Result<(), ()> {
        kpgtbl.map_pages(VirtAddr(addr), size, PhysAddr(addr), perm)
    }

    let rw = F::RW;
    let rx = F::RX;

    let mut kpgtbl = PageTable::allocate().unwrap();
    let kpgtbl = unsafe { kpgtbl.as_mut() };

    unsafe {
        // uart registers
        ident_map(kpgtbl, UART0, PAGE_SIZE, rw).unwrap();

        // virtio mmio disk interface
        ident_map(kpgtbl, VIRTIO0, PAGE_SIZE, rw).unwrap();

        // PLIC
        ident_map(kpgtbl, PLIC, 0x400_0000, rw).unwrap();

        // map kernel text executable and red-only.
        ident_map(kpgtbl, KERN_BASE, etext - KERN_BASE, rx).unwrap();

        // map kernel data and the physical RAM we'll make use of.
        ident_map(kpgtbl, etext, PHYS_TOP - etext, rw).unwrap();

        // map the trampoline for trap entry/exit to
        // the highest virtual address in the kernel.
        kpgtbl
            .map_pages(TRAMPOLINE, PAGE_SIZE, phys_trampoline, rx)
            .unwrap();

        // allocate and map a kernel stack for each process.
        proc::map_stacks(kpgtbl);
    }

    kpgtbl
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
}

impl PhysPageNum {
    pub const fn phys_addr(&self) -> PhysAddr {
        PhysAddr(self.0 << PAGE_SHIFT)
    }
}

impl PhysAddr {
    pub const fn new(addr: usize) -> Self {
        Self(addr)
    }

    pub fn addr(&self) -> usize {
        self.0
    }

    fn as_ptr<T>(&self) -> *const T {
        ptr::without_provenance(self.0)
    }

    fn as_mut_ptr<T>(&self) -> NonNull<T> {
        NonNull::new(ptr::without_provenance_mut(self.0)).unwrap()
    }

    fn phys_page_num(&self) -> PhysPageNum {
        PhysPageNum(self.0 >> PAGE_SHIFT)
    }

    fn byte_add(&self, offset: usize) -> Self {
        Self(self.0 + offset)
    }
}

#[repr(transparent)]
pub struct PageTable([PtEntry; 512]);

impl PageTable {
    /// Allocates a new empty page table.
    fn allocate() -> Result<NonNull<Self>, ()> {
        let pt = kalloc::alloc_page().ok_or(())?.cast::<Self>();
        unsafe {
            pt.write_bytes(0, 1);
        }
        Ok(pt)
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
        (va.0 >> shift) & 0x1ff
    }

    /// Returns the physical address containing this page table
    fn phys_addr(&self) -> PhysAddr {
        PhysAddr(ptr::from_ref(self).addr())
    }

    /// Returns the physical page number of the physical page containing this page table
    fn phys_page_num(&self) -> PhysPageNum {
        self.phys_addr().phys_page_num()
    }

    /// Creates PTE for virtual address `va` that refer to
    /// physical addresses `pa`.
    ///
    /// `va` MUST be page-aligned.
    ///
    /// Returns `Ok(())` on success, `Err(())` if `walk()` couldn't
    /// allocate a needed page-table page.
    pub fn map_page(&mut self, va: VirtAddr, pa: PhysAddr, perm: PtEntryFlags) -> Result<(), ()> {
        assert!(va.is_page_aligned(), "va={va:?}");
        assert!(perm.intersects(PtEntryFlags::RWX), "perm={perm:?}");

        self.update_level0_entry(va, true, |pte| {
            assert!(
                !pte.is_valid(),
                "remap on the already mapped address: va={va:?}"
            );
            *pte = PtEntry::new(pa.phys_page_num(), perm | PtEntryFlags::V);
        })
    }

    /// Creates PTEs for virtual addresses starting at `va` that refer to
    /// physical addresses starting at `pa`.
    ///
    /// `va` and `size` MUST be page-aligned.
    ///
    /// Returns `Ok(())` on success, `Err(())` if `walk()` couldn't
    /// allocate a needed page-table page.
    pub fn map_pages(
        &mut self,
        va: VirtAddr,
        size: usize,
        pa: PhysAddr,
        perm: PtEntryFlags,
    ) -> Result<(), ()> {
        assert!(va.is_page_aligned(), "va={va:?}");
        assert!(size.is_page_aligned(), "size={size:#x}");
        assert_ne!(size, 0, "size={size:#x}");

        let mut va = va;
        let mut pa = pa;
        let last = va.byte_add(size - PAGE_SIZE);
        loop {
            self.map_page(va, pa, perm)?;
            if va == last {
                return Ok(());
            }

            va = va.byte_add(PAGE_SIZE);
            pa = pa.byte_add(PAGE_SIZE);
        }
    }

    /// Unmaps the page of memory at virtual address `va`.
    ///
    /// Returns the physical address of the page that was unmapped.
    fn upmap_page(&mut self, va: VirtAddr) -> PhysAddr {
        assert!(va.is_page_aligned(), "va={va:?}");

        self.update_level0_entry(va, false, |pte| {
            assert!(pte.is_valid());
            assert!(pte.is_leaf(), "{:?}", pte.flags());
            let pa = pte.phys_addr();
            pte.clear();
            pa
        })
        .unwrap()
    }

    /// Unmaps the pages of memory starting at virtual address `va` and
    /// covering `npages` pages.
    fn unmap_pages(&mut self, va: VirtAddr, npages: usize) -> UnmapPages {
        UnmapPages {
            pt: self,
            va,
            offsets: 0..npages,
        }
    }

    /// Returns the leaf PTE in the page tables that corredponds to virtual address `va`.
    fn find_leaf_entry(&self, va: VirtAddr) -> Option<&PtEntry> {
        assert!(va < VirtAddr::MAX);

        let mut pt = self;
        for level in (1..=2).rev() {
            let index = Self::entry_index(level, va);
            let pte = &pt.0[index];
            if pte.is_valid() {
                assert!(pte.is_non_leaf());
                pt = unsafe { pte.phys_addr().as_ptr::<PageTable>().as_ref().unwrap() };
                continue;
            }
            return None;
        }

        let index = Self::entry_index(0, va);
        let pte = &pt.0[index];
        if !pte.is_leaf() {
            return None;
        }
        Some(pte)
    }

    /// Updates the level-0 PTE in the page tables that corredponds to virtual address `va`.
    ///
    /// If `insert_new_table` is `true`, it will allocate new page-table pages if needed.
    ///
    /// Updated PTE must be leaf PTE or invalid.
    fn update_level0_entry<T, F>(
        &mut self,
        va: VirtAddr,
        insert_new_table: bool,
        f: F,
    ) -> Result<T, ()>
    where
        F: for<'a> FnOnce(&'a mut PtEntry) -> T,
    {
        assert!(va < VirtAddr::MAX);

        unsafe {
            let mut pt = NonNull::new(&raw mut *self).unwrap();
            for level in (1..=2).rev() {
                let index = Self::entry_index(level, va);
                let pte = &mut pt.as_mut().0[index];
                if pte.is_valid() {
                    assert!(pte.is_non_leaf());
                    pt = (*pte).phys_addr().as_mut_ptr();
                    continue;
                }

                if !insert_new_table {
                    return Err(());
                }

                pt = Self::allocate()?;
                *pte = PtEntry::new(pt.as_ref().phys_page_num(), PtEntryFlags::V);
            }

            let index = Self::entry_index(0, va);
            let pte = &mut pt.as_mut().0[index];
            let res = f(pte);
            // cannot change PTE to non-leaf (level0 PTE must be invalid or leaf)
            assert!(!pte.is_non_leaf());
            Ok(res)
        }
    }

    /// Looks up a virtual address, returns the physical address,
    /// or `None` if not mapped.
    pub fn resolve_virtual_address(&self, va: VirtAddr, flags: PtEntryFlags) -> Option<PhysAddr> {
        if va >= VirtAddr::MAX {
            return None;
        }

        let pte = self.find_leaf_entry(va)?;
        assert!(pte.is_valid() && pte.is_leaf());
        if !pte.flags().contains(flags) {
            return None;
        }

        Some(pte.phys_addr())
    }

    /// Fetches the page that is mapped at virtual address `va`.
    fn fetch_page(&self, va: VirtAddr, flags: PtEntryFlags) -> Option<&mut [u8; PAGE_SIZE]> {
        let pa = self.resolve_virtual_address(va, flags)?;
        let page = unsafe { pa.as_mut_ptr::<[u8; PAGE_SIZE]>().as_mut() };
        Some(page)
    }

    /// Recursively frees page-table pages.
    ///
    /// All leaf mappings must already have been removed.
    fn free_descendant(&mut self) {
        for pte in &mut self.0 {
            if !pte.is_valid() {
                continue;
            }
            assert!(pte.is_non_leaf());
            let mut child_ptr = pte.phys_addr().as_mut_ptr::<PageTable>();
            {
                let child = unsafe { child_ptr.as_mut() };
                child.free_descendant();
            }
            kalloc::free_page(child_ptr.cast());
            pte.clear();
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

struct UnmapPages<'a> {
    pt: &'a mut PageTable,
    va: VirtAddr,
    offsets: Range<usize>,
}

impl Iterator for UnmapPages<'_> {
    type Item = PhysAddr;

    fn next(&mut self) -> Option<Self::Item> {
        let i = self.offsets.next()?;
        let va = self.va.byte_add(i * PAGE_SIZE);
        Some(self.pt.upmap_page(va))
    }
}

impl Drop for UnmapPages<'_> {
    fn drop(&mut self) {
        for _ in self {}
    }
}

#[repr(transparent)]
struct PtEntry(usize);

impl PtEntry {
    const FLAGS_MASK: usize = 0x3FF;

    fn new(ppn: PhysPageNum, flags: PtEntryFlags) -> Self {
        assert_eq!(
            flags.bits() & Self::FLAGS_MASK,
            flags.bits(),
            "flags: {flags:#x}={flags:?}"
        );
        let bits = (ppn.0 << 10) | (flags.bits() & 0x3FF);
        Self(bits)
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

    /// Returns physical page number (PPN)
    fn phys_page_num(&self) -> PhysPageNum {
        PhysPageNum(self.0 >> 10)
    }

    /// Returns physical address (PA)
    fn phys_addr(&self) -> PhysAddr {
        self.phys_page_num().phys_addr()
    }

    /// Returns page table entry flags
    fn flags(&self) -> PtEntryFlags {
        PtEntryFlags::from_bits_retain(self.0 & Self::FLAGS_MASK)
    }

    /// Sets page table entry flags.
    fn set_flags(&mut self, flags: PtEntryFlags) {
        self.0 &= !&Self::FLAGS_MASK;
        self.0 |= flags.bits();
    }

    /// Clears the page table entry.
    fn clear(&mut self) {
        self.0 = 0;
    }
}

pub mod kernel {
    use super::*;

    /// Initialize the one kernel_pagetable
    pub fn init() {
        let kpgtbl = make_kernel_pt();
        KERNEL_PAGETABLE.store(ptr::from_mut(kpgtbl).addr(), Ordering::Release);
    }

    /// Switch h/w page table register to the kernel's page table,
    /// and enable paging.
    pub fn init_hart() {
        // wait for any previous writes to the page table memory to finish.
        asm::sfence_vma_all();

        let addr = PhysAddr(KERNEL_PAGETABLE.load(Ordering::Acquire));
        unsafe {
            satp::set(satp::Mode::Sv39, 0, addr.phys_page_num().0);
        }

        // flush state entries from the TLB.
        asm::sfence_vma_all();
    }
}

pub mod user {
    use core::slice;

    use super::*;

    /// Removes npages of mappings starting from `va``.
    ///
    /// `va`` must be page-aligned.
    /// The mappings must exist.
    ///
    /// Optionally free the physical memory.
    pub fn unmap(pagetable: &mut PageTable, va: VirtAddr, npages: usize, do_free: bool) {
        for pa in pagetable.unmap_pages(va, npages) {
            if do_free {
                kalloc::free_page(pa.as_mut_ptr());
            }
        }
    }

    /// Creates an empty user page table.
    ///
    /// Returns `Err(())` if out of memory.
    pub fn create() -> Result<NonNull<PageTable>, ()> {
        PageTable::allocate()
    }

    /// Loads the user initcode into address 0 of pagetable.
    ///
    /// For the very first process.
    /// `sz` must be less than a page.
    pub fn map_first(pagetable: &mut PageTable, src: &[u8]) {
        assert!(src.len() < PAGE_SIZE, "src.len()={:#x}", src.len());

        unsafe {
            let mem = kalloc::alloc_page().unwrap().cast::<u8>();
            pagetable
                .map_page(VirtAddr(0), PhysAddr(mem.addr().get()), PtEntryFlags::URWX)
                .unwrap();
            mem.write_bytes(0, PAGE_SIZE);
            slice::from_raw_parts_mut(mem.as_ptr(), src.len()).copy_from_slice(src);
        }
    }

    /// Allocates PTEs and physical memory to grow process from `oldsz` to `newsz`,
    /// which need not be page aligned.
    ///
    /// Returns new size.
    pub fn alloc(
        pagetable: &mut PageTable,
        oldsz: usize,
        newsz: usize,
        xperm: PtEntryFlags,
    ) -> Result<usize, ()> {
        if newsz < oldsz {
            return Ok(oldsz);
        }

        let oldsz = page_roundup(oldsz);
        for va in (oldsz..newsz).step_by(PAGE_SIZE) {
            let Some(mem) = kalloc::alloc_page().map(|p| p.cast::<u8>()) else {
                dealloc(pagetable, va, oldsz);
                return Err(());
            };
            unsafe {
                mem.write_bytes(0, PAGE_SIZE);
            }
            if pagetable
                .map_page(
                    VirtAddr(va),
                    PhysAddr(mem.addr().get()),
                    xperm | PtEntryFlags::UR,
                )
                .is_err()
            {
                kalloc::free_page(mem.cast());
                dealloc(pagetable, va, oldsz);
                return Err(());
            }
        }

        Ok(newsz)
    }

    /// Deallocates user pages to bring the process size from `oldsz` to `newsz`.
    ///
    /// `oldsz` and `newsz` need not be page-aligned, nor does `newsz`
    /// need to be less than `oldsz`.
    /// `oldsz` can be larger than the acrual process size.
    ///
    /// Returns the new process size.
    pub fn dealloc(pagetable: &mut PageTable, oldsz: usize, newsz: usize) -> usize {
        if newsz >= oldsz {
            return oldsz;
        }

        if page_roundup(newsz) < page_roundup(oldsz) {
            let npages = (page_roundup(oldsz) - page_roundup(newsz)) / PAGE_SIZE;
            unmap(pagetable, VirtAddr(page_roundup(newsz)), npages, true);
        }

        newsz
    }

    /// Recursively free page-table pages.
    ///
    /// All leaf mappings must already have been removed.
    pub(super) unsafe fn free_walk(pagetable_addr: usize) {
        unsafe {
            let mut pagetable_ptr =
                NonNull::new(ptr::without_provenance_mut::<PageTable>(pagetable_addr)).unwrap();
            pagetable_ptr.as_mut().free_descendant();
            kalloc::free_page(pagetable_ptr.cast());
        }
    }

    /// Frees user memory pages, then free page-table pages.
    pub unsafe fn free(pagetable_addr: usize, sz: usize) {
        {
            let pagetable = unsafe {
                ptr::without_provenance_mut::<PageTable>(pagetable_addr)
                    .as_mut()
                    .unwrap()
            };
            if sz > 0 {
                unmap(pagetable, VirtAddr(0), page_roundup(sz) / PAGE_SIZE, true);
            }
            // drop pagetable pointer here
            let _ = pagetable;
        }
        unsafe {
            free_walk(pagetable_addr);
        }
    }

    /// Given a parent process's page table, copies
    /// its memory into a child's page table.
    ///
    /// Copies both the page table and the
    /// physical memory.
    pub fn copy(old: &PageTable, new: &mut PageTable, sz: usize) -> Result<(), ()> {
        let res = (|| {
            for va in (0..sz).step_by(PAGE_SIZE) {
                let pte = old.find_leaf_entry(VirtAddr(va)).ok_or(va)?;
                assert!(pte.is_valid() && pte.is_leaf());
                let src_pa = pte.phys_addr();
                let flags = pte.flags();
                let Some(dst) = kalloc::alloc_page() else {
                    return Err(va);
                };
                unsafe {
                    dst.cast::<u8>()
                        .as_ptr()
                        .copy_from(src_pa.as_ptr(), PAGE_SIZE);
                }
                if new
                    .map_page(VirtAddr(va), PhysAddr(dst.addr().get()), flags)
                    .is_err()
                {
                    return Err(va);
                }
            }
            Ok(())
        })();

        if let Err(va) = res {
            unmap(new, VirtAddr(0), va / PAGE_SIZE, true);
        }

        res.map_err(|_| ())
    }

    /// Marks a PTE invalid for user access.
    ///
    /// Used by exec for the user stackguard page.
    pub fn forbide_user_access(pagetable: &mut PageTable, va: VirtAddr) {
        pagetable
            .update_level0_entry(va, false, |pte| {
                let mut flags = pte.flags();
                flags.remove(PtEntryFlags::U);
                pte.set_flags(flags);
            })
            .unwrap();
    }
}

/// Copies from kernel to user.
///
/// Copies `len`` bytes from `src`` to virtual address `dst_va` in a given page table.
pub fn copy_out(pagetable: &PageTable, mut dst_va: VirtAddr, mut src: &[u8]) -> Result<(), ()> {
    while !src.is_empty() {
        let va0 = dst_va.page_rounddown();
        if va0 >= VirtAddr::MAX {
            return Err(());
        }
        let offset = dst_va.0 - va0.0;
        let mut n = PAGE_SIZE - offset;
        if n > src.len() {
            n = src.len();
        }

        let dst_page = pagetable.fetch_page(va0, PtEntryFlags::UW).ok_or(())?;
        let dst = &mut dst_page[offset..][..n];
        dst.copy_from_slice(&src[..n]);
        src = &src[n..];
        dst_va = va0.byte_add(PAGE_SIZE);
    }

    Ok(())
}

/// Copies from user to kernel.
///
/// Copies `len` bytes to `dst` from virtual address `src_va` in a given page table.
pub fn copy_in(pagetable: &PageTable, mut dst: &mut [u8], mut src_va: VirtAddr) -> Result<(), ()> {
    while !dst.is_empty() {
        let va0 = src_va.page_rounddown();
        let offset = src_va.0 - va0.0;
        let mut n = PAGE_SIZE - offset;
        if n > dst.len() {
            n = dst.len();
        }
        let src_page = pagetable.fetch_page(va0, PtEntryFlags::UR).ok_or(())?;
        let src = &src_page[offset..][..n];
        dst[..n].copy_from_slice(src);
        dst = &mut dst[n..];
        src_va = va0.byte_add(PAGE_SIZE);
    }

    Ok(())
}

/// Copies a null-terminated string from user to kernel.
///
/// Copies bytes to `dst` from virtual address `src_va` in a given page table,
/// until a '\0', or max.
pub fn copy_in_str(
    pagetable: &PageTable,
    mut dst: &mut [u8],
    mut src_va: VirtAddr,
) -> Result<(), ()> {
    while !dst.is_empty() {
        let va0 = src_va.page_rounddown();
        let src_page = pagetable.fetch_page(va0, PtEntryFlags::UR).ok_or(())?;

        let offset = src_va.0 - va0.0;
        let mut n = PAGE_SIZE - offset;
        if n > dst.len() {
            n = dst.len();
        }

        let mut p = &src_page[offset..];
        while n > 0 {
            if p[0] == b'\0' {
                dst[0] = b'\0';
                return Ok(());
            }
            dst[0] = p[0];
            n -= 1;
            p = &p[1..];
            dst = &mut dst[1..];
        }

        src_va = va0.byte_add(PAGE_SIZE);
    }
    Err(())
}
