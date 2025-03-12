use alloc::boxed::Box;
use core::{ptr, slice};

use super::{
    PAGE_SIZE, PageRound as _, PhysAddr, PhysPageNum, VirtAddr,
    layout::{TRAMPOLINE, TRAPFRAME},
    page::{self, PageFrameAllocator},
    page_table::{PageTable, PtEntryFlags},
};
use crate::{error::KernelError, interrupt::trampoline, proc::TrapFrame};

pub struct UserPageTable {
    pt: Box<PageTable, PageFrameAllocator>,
    size: usize,
}

impl UserPageTable {
    /// Creates a user page table with a given trapframe, with no user memory,
    /// but with trampoline and trapframe pages.
    pub fn new(tf: &TrapFrame) -> Result<Self, KernelError> {
        // An empty page table.
        let mut pt = PageTable::try_allocate()?;
        if let Err(e) = pt.map_page(
            TRAMPOLINE,
            PhysAddr::new(trampoline::trampoline as usize),
            PtEntryFlags::RX,
        ) {
            pt.free_descendant();
            drop(pt);
            return Err(e);
        }

        if let Err(e) = pt.map_page(
            TRAPFRAME,
            PhysAddr::new(ptr::from_ref(tf).addr()),
            PtEntryFlags::RW,
        ) {
            pt.unmap_pages(TRAMPOLINE, 1);
            pt.free_descendant();
            drop(pt);
            return Err(e);
        }

        Ok(Self { pt, size: 0 })
    }

    pub fn phys_page_num(&self) -> PhysPageNum {
        self.pt.phys_page_num()
    }

    /// Returns process size.
    pub fn size(&self) -> usize {
        self.size
    }

    /// Loads the user initcode into address 0 of pagetable.
    ///
    /// For the very first process.
    /// `src.len()` must be less than a page.
    pub fn map_first(&mut self, src: &[u8]) -> Result<(), KernelError> {
        assert!(src.len() < PAGE_SIZE, "src.len()={:#x}", src.len());

        let mem = page::alloc_zeroed_page().unwrap();
        self.pt.map_page(
            VirtAddr::new(0),
            PhysAddr::new(mem.addr().get()),
            PtEntryFlags::URWX,
        )?;
        unsafe { slice::from_raw_parts_mut(mem.as_ptr(), src.len()) }.copy_from_slice(src);
        self.size += PAGE_SIZE;

        Ok(())
    }

    /// Allocates PTEs and physical memory to grow process to `new_size`,
    /// which need not be page aligned.
    pub fn grow_to(&mut self, new_size: usize, xperm: PtEntryFlags) -> Result<(), KernelError> {
        if new_size < self.size {
            return Ok(());
        }

        let old_size = self.size;
        for va in (self.size.page_roundup()..new_size).step_by(PAGE_SIZE) {
            self.size = va;
            let mem = match page::alloc_zeroed_page() {
                Ok(mem) => mem,
                Err(e) => {
                    self.shrink_to(old_size);
                    return Err(e);
                }
            };

            if let Err(e) = self.pt.map_page(
                VirtAddr::new(va),
                PhysAddr::new(mem.addr().get()),
                xperm | PtEntryFlags::UR,
            ) {
                unsafe {
                    page::free_page(mem);
                }
                self.shrink_to(old_size);
                return Err(e);
            }
        }
        self.size = new_size;

        Ok(())
    }

    /// Deallocates user pages to bring the process size to `new_size`.
    ///
    /// `new_size` need not be page-aligned.
    /// `new_size` need not to be less than current size.
    pub fn shrink_to(&mut self, new_size: usize) {
        if new_size >= self.size {
            return;
        }

        if new_size.page_roundup() < self.size.page_roundup() {
            let npages = (self.size.page_roundup() - new_size.page_roundup()) / PAGE_SIZE;
            for pa in self
                .pt
                .unmap_pages(VirtAddr::new(new_size.page_roundup()), npages)
                .flatten()
            {
                unsafe {
                    page::free_page(pa.as_mut_ptr());
                }
            }
        }

        self.size = new_size;
    }

    pub fn try_clone(&self, target: &mut Self) -> Result<(), KernelError> {
        target.shrink_to(0);

        (|| {
            for va in (0..self.size).step_by(PAGE_SIZE) {
                target.size = va;
                let pte = self.pt.find_leaf_entry(VirtAddr::new(va))?;
                assert!(pte.is_valid() && pte.is_leaf());

                let src_pa = pte.phys_addr();
                let flags = pte.flags();

                let dst = page::alloc_page()?;
                unsafe {
                    dst.as_ptr().copy_from(src_pa.as_ptr(), PAGE_SIZE);
                }

                target
                    .pt
                    .map_page(VirtAddr::new(va), PhysAddr::new(dst.addr().get()), flags)?;
            }
            target.size = self.size;
            Ok(())
        })()
        .inspect_err(|_| {
            target.shrink_to(0);
        })
    }

    /// Marks a PTE invalid for user access.
    ///
    /// Used by exec for the user stackguard page.
    pub fn forbide_user_access(&mut self, va: VirtAddr) -> Result<(), KernelError> {
        self.pt.update_level0_entry(va, false, |pte| {
            let mut flags = pte.flags();
            flags.remove(PtEntryFlags::U);
            pte.set_flags(flags);
        })
    }

    pub fn resolve_virtual_address(
        &self,
        va: VirtAddr,
        flags: PtEntryFlags,
    ) -> Result<PhysAddr, KernelError> {
        self.pt.resolve_virtual_address(va, flags)
    }

    pub fn fetch_page(
        &self,
        va: VirtAddr,
        flags: PtEntryFlags,
    ) -> Result<&[u8; PAGE_SIZE], KernelError> {
        self.pt.fetch_page(va, flags)
    }

    pub fn fetch_page_mut(
        &mut self,
        va: VirtAddr,
        flags: PtEntryFlags,
    ) -> Result<&mut [u8; PAGE_SIZE], KernelError> {
        self.pt.fetch_page_mut(va, flags)
    }
}

impl Drop for UserPageTable {
    fn drop(&mut self) {
        let _ = self.pt.unmap_pages(TRAMPOLINE, 1);
        let _ = self.pt.unmap_pages(TRAPFRAME, 1);

        if self.size > 0 {
            let npages = self.size.page_roundup() / PAGE_SIZE;
            for pa in self.pt.unmap_pages(VirtAddr::new(0), npages).flatten() {
                unsafe {
                    page::free_page(pa.as_mut_ptr());
                }
            }
        }
        self.pt.free_descendant();
    }
}
