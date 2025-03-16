use alloc::boxed::Box;
use core::{ops::Range, ptr, slice};

use dataview::{Pod, PodMethods as _};
use ov6_syscall::{UserMutRef, UserMutSlice, UserRef, UserSlice};

use super::{
    PAGE_SIZE, PageRound as _, PhysAddr, PhysPageNum, VirtAddr,
    addr::{AddressChunks, GenericMutSlice, GenericSlice, Validated},
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
            pt.unmap_pages(TRAMPOLINE, 1).unwrap();
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
            VirtAddr::MIN,
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
        let map_start = VirtAddr::new(self.size.page_roundup()).unwrap();
        let map_end = VirtAddr::new(new_size)?;
        for range in AddressChunks::from_range(map_start..map_end) {
            let va0 = range.page_range().start;
            self.size = va0.addr();

            let mem = match page::alloc_zeroed_page() {
                Ok(mem) => mem,
                Err(e) => {
                    self.shrink_to(old_size);
                    return Err(e);
                }
            };

            if let Err(e) = self.pt.map_page(
                va0,
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
            let start_va = VirtAddr::new(new_size.page_roundup()).unwrap();
            for pa in self.pt.unmap_pages(start_va, npages).unwrap() {
                let pa = pa.unwrap();
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
            for chunk in AddressChunks::from_size(VirtAddr::MIN, self.size).unwrap() {
                let va = chunk.page_range().start;
                target.size = va.addr();
                let pte = self.pt.find_leaf_entry(va)?;
                assert!(pte.is_valid() && pte.is_leaf());

                let src_pa = pte.phys_addr();
                let flags = pte.flags();

                let dst = page::alloc_page()?;
                unsafe {
                    dst.as_ptr().copy_from(src_pa.as_ptr(), PAGE_SIZE);
                }

                target
                    .pt
                    .map_page(va, PhysAddr::new(dst.addr().get()), flags)?;
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

    pub fn validate_read(&self, va: Range<VirtAddr>) -> Result<(), KernelError> {
        for chunk in AddressChunks::try_new(va)? {
            let va = chunk.page_range().start;
            let _pa = self.pt.resolve_virtual_address(va, PtEntryFlags::UR)?;
        }
        Ok(())
    }

    pub fn validate_write(&self, va: Range<VirtAddr>) -> Result<(), KernelError> {
        for chunk in AddressChunks::try_new(va)? {
            let va = chunk.page_range().start;
            let _pa = self.pt.resolve_virtual_address(va, PtEntryFlags::UW)?;
        }
        Ok(())
    }

    /// Copies from user to kernel.
    pub fn copy_k2u<T>(&mut self, dst: &mut Validated<UserMutRef<T>>, src: &T)
    where
        T: Pod,
    {
        self.copy_k2u_bytes(&mut dst.as_bytes_mut(), src.as_bytes());
    }

    /// Copies from kernel to user.
    pub fn copy_k2u_bytes(&mut self, dst: &mut Validated<UserMutSlice<u8>>, mut src: &[u8]) {
        assert_eq!(dst.len(), src.len());
        for chunk in AddressChunks::new(dst) {
            let va0 = chunk.page_range().start;
            let offset = chunk.offset_in_page().start;
            let n = chunk.size();

            let dst_page = self.pt.fetch_page_mut(va0, PtEntryFlags::UW).unwrap();
            let dst = &mut dst_page[offset..][..n];
            dst.copy_from_slice(&src[..n]);
            src = &src[n..];
        }
    }

    /// Copies to either a user address, or kernel address.
    #[track_caller]
    pub fn copy_k2x_bytes(dst: &mut GenericMutSlice<u8>, src: &[u8]) {
        assert_eq!(dst.len(), src.len());
        match dst {
            GenericMutSlice::User(pt, dst) => pt.copy_k2u_bytes(dst, src),
            GenericMutSlice::Kernel(dst) => dst.copy_from_slice(src),
        }
    }

    /// Copies from user to kernel.
    pub fn copy_u2k<T>(&self, src: &Validated<UserRef<T>>) -> T
    where
        T: Pod,
    {
        let mut dst = T::zeroed();
        self.copy_u2k_bytes(dst.as_bytes_mut(), &src.as_bytes());
        dst
    }

    /// Copies from user to kernel.
    pub fn copy_u2k_bytes(&self, mut dst: &mut [u8], src: &Validated<UserSlice<u8>>) {
        assert_eq!(src.len(), dst.len());
        for chunk in AddressChunks::new(src) {
            let va0 = chunk.page_range().start;
            let offset = chunk.offset_in_page().start;
            let n = chunk.size();

            let src_page = self.pt.fetch_page(va0, PtEntryFlags::UR).unwrap();
            let src = &src_page[offset..][..n];
            dst[..n].copy_from_slice(src);
            dst = &mut dst[n..];
        }
    }

    /// Copies from either a user address, or kernel address.
    pub fn copy_x2k_bytes(dst: &mut [u8], src: &GenericSlice<u8>) {
        assert_eq!(dst.len(), src.len());
        match src {
            GenericSlice::User(pt, src) => pt.copy_u2k_bytes(dst, src),
            GenericSlice::Kernel(src) => dst.copy_from_slice(src),
        }
    }

    /// Copies from user to user.
    pub fn copy_u2u_bytes(
        dst_pt: &mut Self,
        dst: &mut Validated<UserMutSlice<u8>>,
        src_pt: &Self,
        src: &Validated<UserSlice<u8>>,
    ) {
        assert_eq!(src.len(), dst.len());
        let mut dst_chunks = AddressChunks::new(dst);
        let mut src_chunks = AddressChunks::new(src);
        let mut dst_bytes = &mut [][..];
        let mut src_bytes = &[][..];

        let mut total_copied = 0;
        while total_copied < src.len() {
            if dst_bytes.is_empty() {
                let dst_chunk = dst_chunks.next().unwrap();
                let page = dst_pt
                    .pt
                    .fetch_page_mut(dst_chunk.page_range().start, PtEntryFlags::UW)
                    .unwrap();
                dst_bytes = &mut page[dst_chunk.offset_in_page()];
            }

            if src_bytes.is_empty() {
                let src_chunk = src_chunks.next().unwrap();
                let page = src_pt
                    .pt
                    .fetch_page(src_chunk.page_range().start, PtEntryFlags::UR)
                    .unwrap();
                src_bytes = &page[src_chunk.offset_in_page()];
            }

            let n = usize::min(dst_bytes.len(), src_bytes.len());
            dst_bytes[..n].copy_from_slice(&src_bytes[..n]);
            dst_bytes = &mut dst_bytes[n..];
            src_bytes = &src_bytes[n..];
            total_copied += n;
        }
    }
}

impl Drop for UserPageTable {
    fn drop(&mut self) {
        let _ = self.pt.unmap_page(TRAMPOLINE).unwrap();
        let _ = self.pt.unmap_page(TRAPFRAME).unwrap();

        if self.size > 0 {
            let npages = self.size.page_roundup() / PAGE_SIZE;
            let unmapped_pages = self.pt.unmap_pages(VirtAddr::MIN, npages).unwrap();
            for pa in unmapped_pages {
                let pa = pa.unwrap();
                unsafe {
                    page::free_page(pa.as_mut_ptr());
                }
            }
        }
        self.pt.free_descendant();
    }
}
