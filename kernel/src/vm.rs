use core::{
    ffi::{c_int, c_void},
    ops::Range,
    ptr::{self, NonNull},
    sync::atomic::{AtomicUsize, Ordering},
};

use bitflags::bitflags;
use riscv::{asm, register::satp};

use crate::{
    kalloc,
    memlayout::{KERN_BASE, PHYS_TOP, PLIC, TRAMPOLINE, UART0, VIRTIO0},
    proc,
};

mod ffi {
    use core::ffi::c_char;

    use super::*;

    #[unsafe(no_mangle)]
    extern "C" fn mappages(
        pagetable: *mut PageTable,
        va: u64,
        size: u64,
        pa: u64,
        perm: c_int,
    ) -> c_int {
        let res = unsafe {
            (*pagetable).map_pages(
                VirtAddr(va as usize),
                size as usize,
                PhysAddr(pa as usize),
                PtEntryFlags::from_bits_retain(perm as usize),
            )
        };
        match res {
            Ok(()) => 0,
            Err(()) => -1,
        }
    }

    #[unsafe(no_mangle)]
    extern "C" fn walkaddr(pagetable: *mut PageTable, va: u64) -> u64 {
        let pa = unsafe { (*pagetable).walk_addr(VirtAddr(va as usize)) };
        pa.map(|pa| pa.0 as u64).unwrap_or(0)
    }

    #[unsafe(no_mangle)]
    extern "C" fn kvmmap(kpgtbl: *mut PageTable, va: u64, pa: u64, sz: u64, perm: c_int) {
        let kpgtbl = unsafe { kpgtbl.as_mut().unwrap() };
        kpgtbl
            .map_pages(
                VirtAddr(va as usize),
                sz as usize,
                PhysAddr(pa as usize),
                PtEntryFlags::from_bits_retain(perm as usize),
            )
            .unwrap();
    }

    #[unsafe(no_mangle)]
    extern "C" fn uvmunmap(pagetable: *mut PageTable, va: u64, npages: u64, do_free: c_int) {
        let pagetable = unsafe { &mut (*pagetable) };
        user::unmap(
            pagetable,
            VirtAddr(va as usize),
            npages as usize,
            do_free != 0,
        );
    }

    #[unsafe(no_mangle)]
    extern "C" fn uvmcreate() -> *mut PageTable {
        user::create()
            .map(NonNull::as_ptr)
            .unwrap_or(ptr::null_mut())
    }

    #[unsafe(no_mangle)]
    extern "C" fn uvmfirst(pagetable: *mut PageTable, src: *const u8, sz: u64) {
        let pagetable = unsafe { pagetable.as_mut().unwrap() };
        user::first(pagetable, src, sz as usize);
    }

    #[unsafe(no_mangle)]
    extern "C" fn uvmalloc(pagetable: *mut PageTable, oldsz: u64, newsz: u64, xperm: c_int) -> u64 {
        let pagetable = unsafe { pagetable.as_mut().unwrap() };
        user::alloc(
            pagetable,
            oldsz as usize,
            newsz as usize,
            PtEntryFlags::from_bits_retain(xperm as usize),
        )
        .unwrap_or(0) as u64
    }

    #[unsafe(no_mangle)]
    extern "C" fn uvmdealloc(pagetable: *mut PageTable, oldsz: u64, newsz: u64) -> u64 {
        let pagetable = unsafe { pagetable.as_mut().unwrap() };
        user::dealloc(pagetable, oldsz as usize, newsz as usize) as u64
    }

    #[unsafe(no_mangle)]
    extern "C" fn uvmfree(pagetable: *mut PageTable, sz: u64) {
        unsafe { user::free(pagetable.addr(), sz as usize) }
    }

    #[unsafe(no_mangle)]
    extern "C" fn uvmcopy(old: *mut PageTable, new: *mut PageTable, sz: u64) -> c_int {
        let old = unsafe { old.as_ref().unwrap() };
        let new = unsafe { new.as_mut().unwrap() };
        match user::copy(old, new, sz as usize) {
            Ok(()) => 0,
            Err(()) => -1,
        }
    }

    #[unsafe(no_mangle)]
    extern "C" fn uvmclear(pagetable: *mut PageTable, va: u64) {
        let pagetable = unsafe { pagetable.as_mut().unwrap() };
        user::clear(pagetable, VirtAddr(va as usize));
    }

    #[unsafe(no_mangle)]
    extern "C" fn copyout(
        pagetable: *mut PageTable,
        dst_va: u64,
        src: *const c_char,
        len: u64,
    ) -> c_int {
        let pagetable = unsafe { pagetable.as_ref().unwrap() };
        match super::copy_out(
            pagetable,
            VirtAddr(dst_va as usize),
            src.cast(),
            len as usize,
        ) {
            Ok(()) => 0,
            Err(()) => -1,
        }
    }

    #[unsafe(no_mangle)]
    extern "C" fn copyin(
        pagetable: *mut PageTable,
        dst: *mut c_char,
        src_va: u64,
        len: u64,
    ) -> c_int {
        let pagetable = unsafe { pagetable.as_ref().unwrap() };
        match super::copy_in(
            pagetable,
            dst.cast(),
            VirtAddr(src_va as usize),
            len as usize,
        ) {
            Ok(()) => 0,
            Err(()) => -1,
        }
    }

    #[unsafe(no_mangle)]
    extern "C" fn copyinstr(
        pagetable: *mut PageTable,
        dst: *mut c_char,
        src_va: u64,
        max: u64,
    ) -> c_int {
        let pagetable = unsafe { pagetable.as_ref().unwrap() };
        match super::copy_instr(
            pagetable,
            dst.cast(),
            VirtAddr(src_va as usize),
            max as usize,
        ) {
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

const PHYS_TRAMPOLINE: NonNull<c_void> = {
    unsafe extern "C" {
        #[link_name = "trampoline"]
        static mut PHYS_TRAMPOLINE: [u8; 0];
    }
    NonNull::new((&raw mut PHYS_TRAMPOLINE).cast()).unwrap()
};

pub const fn page_roundup(addr: usize) -> usize {
    (addr + PAGE_SIZE - 1) & !(PAGE_SIZE - 1)
}

pub const fn page_rounddown(addr: usize) -> usize {
    addr & !(PAGE_SIZE - 1)
}

/// Makes a direct-map page table for the kernel.
fn make_kernel_pt() -> &'static mut PageTable {
    use PtEntryFlags as F;

    let etext = ETEXT.addr().into();
    let phys_trampoline = PhysAddr(PHYS_TRAMPOLINE.addr().into());

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

    pub const fn byte_add(&self, offset: usize) -> Self {
        Self(self.0 + offset)
    }

    pub const fn byte_sub(&self, offset: usize) -> Self {
        Self(self.0 - offset)
    }

    // pub const fn page_roundup(&self) -> Self {
    //     Self(page_roundup(self.0))
    // }

    pub const fn page_rounddown(&self) -> Self {
        Self(page_rounddown(self.0))
    }
}

impl PhysPageNum {
    pub const fn phys_addr(&self) -> PhysAddr {
        PhysAddr(self.0 << PAGE_SHIFT)
    }
}

impl PhysAddr {
    fn as_ptr<T>(&self) -> *const T {
        ptr::without_provenance(self.0)
    }

    fn as_mut_ptr<T>(&self) -> *mut T {
        ptr::without_provenance_mut(self.0)
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
        let pt = kalloc::kalloc().cast::<Self>();
        if pt.is_null() {
            return Err(());
        }
        unsafe {
            pt.write_bytes(0, 1);
        }
        Ok(NonNull::new(pt).unwrap())
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
    fn index(level: usize, va: VirtAddr) -> usize {
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

    /// Returns the reference to PTE for a given virtual address.
    fn entry(&self, level: usize, va: VirtAddr) -> &PtEntry {
        let index = Self::index(level, va);
        &self.0[index]
    }

    /// Returns the mutable reference to PTE for a given virtual address.
    fn entry_mut(&mut self, level: usize, va: VirtAddr) -> &mut PtEntry {
        let index = Self::index(level, va);
        &mut self.0[index]
    }

    /// Creates PTE for virtual address `va` that refer to
    /// physical addresses `pa`.
    ///
    /// `va` MUST be page-aligned.
    ///
    /// Returns `Ok(())` on success, `Err(())` if `walk()` couldn't
    /// allocate a needed page-table page.
    fn map_page(&mut self, va: VirtAddr, pa: PhysAddr, perm: PtEntryFlags) -> Result<(), ()> {
        assert_eq!(va.0 % PAGE_SIZE, 0, "va={va:?}");

        let Some(pte) = self.walk_and_insert(va) else {
            return Err(());
        };

        assert!(
            !pte.is_valid(),
            "remap on the already mapped address: va={va:?}"
        );

        *pte = PtEntry::new(pa.phys_page_num(), 0, perm | PtEntryFlags::V);
        Ok(())
    }

    /// Creates PTEs for virtual addresses starting at `va` that refer to
    /// physical addresses starting at `pa`.
    ///
    /// `va` and `size` MUST be page-aligned.
    ///
    /// Returns `Ok(())` on success, `Err(())` if `walk()` couldn't
    /// allocate a needed page-table page.
    fn map_pages(
        &mut self,
        va: VirtAddr,
        size: usize,
        pa: PhysAddr,
        perm: PtEntryFlags,
    ) -> Result<(), ()> {
        assert_eq!(va.0 % PAGE_SIZE, 0, "va={va:?}");
        assert_eq!(size % PAGE_SIZE, 0, "size={size:#x}");
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
        assert_eq!(va.0 % PAGE_SIZE, 0, "va={va:?}");

        let pte = self.walk_mut(va).unwrap();
        assert!(pte.is_valid());
        assert!(pte.is_leaf(), "{:?}", pte.flags());
        let pa = pte.phys_addr();
        pte.clear();
        pa
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

    /// Returns the address of the PTE in the page table that corredponds to virtual address `va`.
    fn walk(&self, va: VirtAddr) -> Option<&PtEntry> {
        assert!(va < VirtAddr::MAX);

        let mut pt = self;
        for level in (1..=2).rev() {
            let pte = pt.entry(level, va);
            if pte.is_valid() {
                pt = unsafe { pte.phys_addr().as_ptr::<PageTable>().as_ref().unwrap() };
                continue;
            }
            return None;
        }

        Some(pt.entry(0, va))
    }

    /// Returns the address of the PTE in the page table that corredponds to virtual address `va`.
    fn walk_mut(&mut self, va: VirtAddr) -> Option<&mut PtEntry> {
        assert!(va < VirtAddr::MAX);

        let mut pt = self;
        for level in (1..=2).rev() {
            let pte = pt.entry_mut(level, va);
            if pte.is_valid() {
                pt = unsafe { pte.phys_addr().as_mut_ptr::<PageTable>().as_mut().unwrap() };
                continue;
            }
            return None;
        }

        Some(pt.entry_mut(0, va))
    }

    /// Returns the address of the PTE in the page table that corredponds to virtual address `va`.
    ///
    /// If PTE does not exists, create any required page-table pages.
    fn walk_and_insert(&mut self, va: VirtAddr) -> Option<&mut PtEntry> {
        assert!(va < VirtAddr::MAX);

        unsafe {
            let mut pt = &raw mut *self;
            for level in (1..=2).rev() {
                let pte = (*pt).entry_mut(level, va);
                if (*pte).is_valid() {
                    pt = (*pte).phys_addr().as_mut_ptr();
                    continue;
                }

                pt = Self::allocate().ok()?.as_ptr();
                *pte = PtEntry::new((*pt).phys_page_num(), 0, PtEntryFlags::V);
            }

            Some((*pt).entry_mut(0, va))
        }
    }

    /// Looks up a virtual address, returns the physical address,
    /// or `None` if not mapped.
    ///
    /// Can only be used to look up user pages.
    fn walk_addr(&self, va: VirtAddr) -> Option<PhysAddr> {
        if va >= VirtAddr::MAX {
            return None;
        }

        let pte = self.walk(va)?;
        if !pte.is_valid() || !pte.is_user() {
            return None;
        }

        Some(pte.phys_addr())
    }

    /// Recursively frees page-table pages.
    ///
    /// All leaf mappings must already have been removed.
    fn free_descendant(&mut self) {
        for pte in &mut self.0 {
            if !pte.is_valid() {
                continue;
            }
            assert!(!pte.is_leaf());
            let child_ptr = pte.phys_addr().as_mut_ptr::<PageTable>();
            {
                let child = unsafe { child_ptr.as_mut().unwrap() };
                child.free_descendant();
            }
            kalloc::kfree(child_ptr.cast());
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
    fn new(ppn: PhysPageNum, rsv: usize, flags: PtEntryFlags) -> Self {
        assert_eq!(rsv & 0b11, rsv, "rsv: {rsv:#x}");
        let bits = (ppn.0 << 10) | ((rsv & 0b11) << 8) | (flags.bits() & 0xFF);
        Self(bits)
    }

    /// Returns `true` if this page is valid
    fn is_valid(&self) -> bool {
        self.flags().contains(PtEntryFlags::V)
    }

    /// Returns `true` is this page is writable
    fn is_writable(&self) -> bool {
        self.flags().contains(PtEntryFlags::W)
    }

    /// Returns `true` if this page is available for userspace.
    fn is_user(&self) -> bool {
        self.flags().contains(PtEntryFlags::U)
    }

    /// Returns `true` if this page is a leaf node.
    fn is_leaf(&self) -> bool {
        self.flags()
            .intersects(PtEntryFlags::R | PtEntryFlags::W | PtEntryFlags::X)
    }

    /// Returns physical page number (PPN)
    fn phys_page_num(&self) -> PhysPageNum {
        PhysPageNum(self.0 >> 10)
    }

    /// Returns physical address (PA)
    fn phys_addr(&self) -> PhysAddr {
        self.phys_page_num().phys_addr()
    }

    // /// Returns software reserved bits (RSV)
    // fn rsv(&self) -> usize {
    //     (self.0 >> 8) & 0b11
    // }

    /// Returns page table entry flags
    fn flags(&self) -> PtEntryFlags {
        PtEntryFlags::from_bits_retain(self.0 & 0xFF)
    }

    /// Sets page table entry flags.
    fn set_flags(&mut self, flags: PtEntryFlags) {
        self.0 &= !0xFF;
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

mod user {
    use super::*;

    /// Removes npages of mappings starting from `va``.
    ///
    /// `va`` must be page-aligned.
    /// The mappings must exist.
    ///
    /// Optionally free the physical memory.
    pub(super) fn unmap(pagetable: &mut PageTable, va: VirtAddr, npages: usize, do_free: bool) {
        for pa in pagetable.unmap_pages(va, npages) {
            if do_free {
                kalloc::kfree(pa.as_mut_ptr());
            }
        }
    }

    /// Creates an empty user page table.
    ///
    /// Returns `Err(())` if out of memory.
    pub(super) fn create() -> Result<NonNull<PageTable>, ()> {
        PageTable::allocate()
    }

    /// Loads the user initcode into address 0 of pagetable.
    ///
    /// For the very first process.
    /// `sz` must be less than a page.
    pub(super) fn first(pagetable: &mut PageTable, src: *const u8, sz: usize) {
        assert!(sz < PAGE_SIZE, "sz={sz:#x}");

        unsafe {
            let mem = kalloc::kalloc().cast::<u8>();
            ptr::write_bytes(mem, 0, PAGE_SIZE);
            pagetable
                .map_page(VirtAddr(0), PhysAddr(mem.addr()), PtEntryFlags::URWX)
                .unwrap();
            ptr::copy(src, mem, sz);
        }
    }

    /// Allocates PTEs and physical memory to grow process from `oldsz` to `newsz`,
    /// which need not be page aligned.
    ///
    /// Returns new size.
    pub(super) fn alloc(
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
            let mem = kalloc::kalloc().cast::<u8>();
            if mem.is_null() {
                dealloc(pagetable, va, oldsz);
                return Err(());
            }
            unsafe {
                mem.write_bytes(0, PAGE_SIZE);
            }
            if pagetable
                .map_page(VirtAddr(va), PhysAddr(mem.addr()), xperm | PtEntryFlags::UR)
                .is_err()
            {
                kalloc::kfree(mem.cast());
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
    pub(super) fn dealloc(pagetable: &mut PageTable, oldsz: usize, newsz: usize) -> usize {
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
            let pagetable_ptr = ptr::without_provenance_mut::<PageTable>(pagetable_addr);
            pagetable_ptr.as_mut().unwrap().free_descendant();
            kalloc::kfree(pagetable_ptr.cast());
        }
    }

    /// Frees user memory pages, then free page-table pages.
    ///
    ///
    pub(super) unsafe fn free(pagetable_addr: usize, sz: usize) {
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
    pub(super) fn copy(old: &PageTable, new: &mut PageTable, sz: usize) -> Result<(), ()> {
        let res = (|| {
            for va in (0..sz).step_by(PAGE_SIZE) {
                let pte = old.walk(VirtAddr(va)).unwrap();
                assert!(pte.is_valid());
                let src_pa = pte.phys_addr();
                let flags = pte.flags();
                let dst = kalloc::kalloc();
                if dst.is_null() {
                    return Err(va);
                }
                unsafe {
                    ptr::copy::<u8>(src_pa.as_ptr(), dst.cast(), PAGE_SIZE);
                }
                if new
                    .map_page(VirtAddr(va), PhysAddr(dst.addr()), flags)
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
    pub(super) fn clear(pagetable: &mut PageTable, va: VirtAddr) {
        let pte = pagetable.walk_mut(va).unwrap();
        let mut flags = pte.flags();
        flags.remove(PtEntryFlags::U);
        pte.set_flags(flags);
    }
}

/// Copies from kernel to user.
///
/// Copies `len`` bytes from `src`` to virtual address `dst_va` in a given page table.
fn copy_out(
    pagetable: &PageTable,
    mut dst_va: VirtAddr,
    mut src: *const u8,
    mut len: usize,
) -> Result<(), ()> {
    while len > 0 {
        let va0 = dst_va.page_rounddown();
        if va0 >= VirtAddr::MAX {
            return Err(());
        }

        let pte = pagetable.walk(va0).ok_or(())?;
        if !pte.is_valid() || !pte.is_user() || !pte.is_writable() {
            return Err(());
        }

        let pa0 = pte.phys_addr();
        let offset = dst_va.0 - va0.0;
        let mut n = PAGE_SIZE - offset;
        if n > len {
            n = len;
        }
        unsafe {
            ptr::copy(src, pa0.as_mut_ptr::<u8>().byte_add(offset), n);

            len -= n;
            src = src.byte_add(n);
            dst_va = va0.byte_add(PAGE_SIZE);
        }
    }

    Ok(())
}

/// Copies from user to kernel.
///
/// Copies `len` bytes to `dst` from virtual address `src_va` in a given page table.
fn copy_in(
    pagetable: &PageTable,
    mut dst: *mut u8,
    mut src_va: VirtAddr,
    mut len: usize,
) -> Result<(), ()> {
    while len > 0 {
        let va0 = src_va.page_rounddown();
        let pa0 = pagetable.walk_addr(va0).ok_or(())?;
        let offset = src_va.0 - va0.0;
        let mut n = PAGE_SIZE - offset;
        if n > len {
            n = len;
        }
        unsafe {
            ptr::copy(pa0.as_mut_ptr::<u8>().byte_add(offset), dst, n);

            len -= n;
            dst = dst.byte_add(n);
            src_va = va0.byte_add(PAGE_SIZE);
        }
    }

    Ok(())
}

/// Copies a null-terminated string from user to kernel.
///
/// Copies bytes to `dst` from virtual address `src_va` in a given page table,
/// until a '\0', or max.
fn copy_instr(
    pagetable: &PageTable,
    mut dst: *mut u8,
    mut src_va: VirtAddr,
    mut max: usize,
) -> Result<(), ()> {
    let mut got_null = false;

    while !got_null && max > 0 {
        let va0 = src_va.page_rounddown();
        let pa0 = pagetable.walk_addr(va0).ok_or(())?;

        let offset = src_va.0 - va0.0;
        let mut n = PAGE_SIZE - offset;
        if n > max {
            n = max;
        }

        unsafe {
            let mut p = pa0.as_mut_ptr::<u8>().byte_add(offset);
            while n > 0 {
                if *p == 0 {
                    *dst = 0;
                    got_null = true;
                    break;
                }
                *dst = *p;
                n -= 1;
                max -= 1;
                p = p.add(1);
                dst = dst.add(1);
            }

            src_va = va0.byte_add(PAGE_SIZE);
        }
    }

    if got_null { Ok(()) } else { Err(()) }
}
