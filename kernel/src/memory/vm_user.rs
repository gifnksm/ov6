use alloc::boxed::Box;
use core::{ptr, slice};

use dataview::{Pod, PodMethods as _};
use ov6_syscall::{UserMutRef, UserMutSlice, UserRef, UserSlice};

use super::{
    PAGE_SIZE, PageRound as _, PhysAddr, PhysPageNum, VirtAddr,
    addr::{GenericMutSlice, GenericSlice},
    layout::{TRAMPOLINE, TRAPFRAME},
    page::{self, PageFrameAllocator},
    page_table::{PageTable, PtEntryFlags},
};
use crate::{
    error::KernelError,
    interrupt::{trampoline, trap::TrapFrame},
};

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

    pub fn try_clone_into(&self, target: &mut Self) -> Result<(), KernelError> {
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

    /// Copies from user to kernel.
    pub fn copy_out<T>(&mut self, mut dst: UserMutRef<T>, src: &T) -> Result<(), KernelError>
    where
        T: Pod,
    {
        self.copy_out_bytes(dst.as_bytes_mut(), src.as_bytes())
    }

    /// Copies from kernel to user.
    pub fn copy_out_bytes(
        &mut self,
        dst: UserMutSlice<u8>,
        mut src: &[u8],
    ) -> Result<(), KernelError> {
        assert_eq!(dst.len(), src.len());
        let mut dst_va = VirtAddr::new(dst.addr());

        while !src.is_empty() {
            let va0 = dst_va.page_rounddown();
            if va0 >= VirtAddr::MAX {
                return Err(KernelError::TooLargeVirtualAddress(dst_va));
            }

            let offset = dst_va.addr() - va0.addr();
            let mut n = PAGE_SIZE - offset;
            if n > src.len() {
                n = src.len();
            }

            let dst_page = self.fetch_page_mut(va0, PtEntryFlags::UW)?;
            let dst = &mut dst_page[offset..][..n];
            dst.copy_from_slice(&src[..n]);
            src = &src[n..];
            dst_va = va0.byte_add(PAGE_SIZE);
        }

        Ok(())
    }

    /// Copies to either a user address, or kernel address.
    pub fn either_copy_out_bytes(
        &mut self,
        dst: GenericMutSlice<u8>,
        src: &[u8],
    ) -> Result<(), KernelError> {
        assert_eq!(dst.len(), src.len());
        match dst {
            GenericMutSlice::User(dst) => self.copy_out_bytes(dst, src)?,
            GenericMutSlice::Kernel(dst) => dst.copy_from_slice(src),
        }
        Ok(())
    }

    /// Copies from user to kernel.
    pub fn copy_in<T>(&self, src: UserRef<T>) -> Result<T, KernelError>
    where
        T: Pod,
    {
        let mut dst = T::zeroed();
        self.copy_in_bytes(dst.as_bytes_mut(), src.as_bytes())?;
        Ok(dst)
    }

    /// Copies from user to kernel.
    pub fn copy_in_bytes(&self, mut dst: &mut [u8], src: UserSlice<u8>) -> Result<(), KernelError> {
        assert_eq!(src.len(), dst.len());
        let mut src_va = VirtAddr::new(src.addr());
        while !dst.is_empty() {
            let va0 = src_va.page_rounddown();
            let offset = src_va.addr() - va0.addr();
            let mut n = PAGE_SIZE - offset;
            if n > dst.len() {
                n = dst.len();
            }
            let src_page = self.fetch_page(va0, PtEntryFlags::UR)?;
            let src = &src_page[offset..][..n];
            dst[..n].copy_from_slice(src);
            dst = &mut dst[n..];
            src_va = va0.byte_add(PAGE_SIZE);
        }

        Ok(())
    }

    /// Copies from either a user address, or kernel address.
    pub fn either_copy_in_bytes(
        &self,
        dst: &mut [u8],
        src: GenericSlice<u8>,
    ) -> Result<(), KernelError> {
        assert_eq!(dst.len(), src.len());
        match src {
            GenericSlice::User(src) => self.copy_in_bytes(dst, src)?,
            GenericSlice::Kernel(src) => dst.copy_from_slice(src),
        }
        Ok(())
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
