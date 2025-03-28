use alloc::boxed::Box;
use core::{ops::Range, ptr, slice};

use dataview::{Pod, PodMethods as _};
use ov6_syscall::{USyscallData, UserMutRef, UserMutSlice, UserRef, UserSlice};
use ov6_types::process::ProcId;
use riscv::register::satp::Satp;

use super::{
    PAGE_SIZE, PageRound as _, PhysAddr, VirtAddr,
    addr::{GenericMutSlice, GenericSlice, Validated},
    layout::{TRAMPOLINE, TRAMPOLINE_SIZE, TRAPFRAME, TRAPFRAME_SIZE, USYSCALL, USYSCALL_SIZE},
    page::{self, PageFrameAllocator},
    page_table::{self, PageTable, PtEntryFlags},
};
use crate::{
    error::KernelError,
    interrupt::{trampoline, trap::TrapFrame},
    memory::{self, addr::AsVirtAddrRange as _},
};

pub struct UserPageTable {
    pt: Box<PageTable, PageFrameAllocator>,
    size: usize,
}

impl UserPageTable {
    /// Creates a user page table with a given trapframe, with no user memory,
    /// but with trampoline and trapframe pages.
    pub fn new(pid: ProcId, tf: &TrapFrame) -> Result<Self, KernelError> {
        // An empty page table.
        let mut pt = Self {
            pt: PageTable::try_allocate()?,
            size: 0,
        };

        let usyscall = page::alloc_zeroed_page()?;
        unsafe {
            usyscall.cast::<USyscallData>().write(USyscallData { pid });
        }
        if let Err(e) = pt.pt.map_addrs(
            USYSCALL,
            PhysAddr::new(usyscall.addr().get()),
            USYSCALL_SIZE,
            PtEntryFlags::UR,
        ) {
            unsafe {
                page::free_page(usyscall);
            }
            return Err(e);
        }

        pt.pt.map_addrs(
            TRAMPOLINE,
            PhysAddr::new(trampoline::trampoline as usize),
            TRAMPOLINE_SIZE,
            PtEntryFlags::RX,
        )?;

        pt.pt.map_addrs(
            TRAPFRAME,
            PhysAddr::new(ptr::from_ref(tf).addr()),
            TRAPFRAME_SIZE,
            PtEntryFlags::RW,
        )?;

        Ok(pt)
    }

    pub fn satp(&self) -> Satp {
        self.pt.satp()
    }

    pub fn program_break(&self) -> VirtAddr {
        VirtAddr::MIN_AVA.byte_add(self.size).unwrap()
    }

    /// Loads the user initcode into address 0 of pagetable.
    ///
    /// For the very first process.
    /// `src.len()` must be less than a page.
    pub fn map_first(&mut self, src: &[u8]) -> Result<(), KernelError> {
        assert!(src.len() < PAGE_SIZE, "src.len()={:#x}", src.len());

        let mem = page::alloc_zeroed_page().unwrap();
        self.pt.map_addrs(
            VirtAddr::MIN_AVA,
            PhysAddr::new(mem.addr().get()),
            PAGE_SIZE,
            PtEntryFlags::URWX,
        )?;
        unsafe { slice::from_raw_parts_mut(mem.as_ptr(), src.len()) }.copy_from_slice(src);
        self.size += PAGE_SIZE;

        Ok(())
    }

    pub fn grow_by(&mut self, increment: usize, xperm: PtEntryFlags) -> Result<(), KernelError> {
        let new_size = self.size.saturating_add(increment);
        self.grow_to_size(new_size, xperm)
    }

    pub fn grow_to_addr(
        &mut self,
        end_addr: VirtAddr,
        xperm: PtEntryFlags,
    ) -> Result<(), KernelError> {
        let Some(new_size) = end_addr.checked_sub(VirtAddr::MIN_AVA) else {
            return Ok(());
        };
        self.grow_to_size(new_size, xperm)
    }

    /// Allocates PTEs and physical memory to grow process to `new_size`,
    /// which need not be page aligned.
    fn grow_to_size(&mut self, new_size: usize, xperm: PtEntryFlags) -> Result<(), KernelError> {
        if new_size < self.size {
            return Ok(());
        }

        let old_size = self.size;
        let mut map_start = VirtAddr::MIN_AVA
            .byte_add(self.size)
            .unwrap()
            .page_roundup();
        let map_end = VirtAddr::MIN_AVA.byte_add(new_size)?;
        while map_start < map_end {
            self.size = map_start.addr() - VirtAddr::MIN_AVA.addr();

            let mem = match page::alloc_zeroed_page() {
                Ok(mem) => mem,
                Err(e) => {
                    self.shrink_to_size(old_size);
                    return Err(e);
                }
            };

            if let Err(e) =
                self.pt
                    .map_addrs(map_start, PhysAddr::new(mem.addr().get()), PAGE_SIZE, xperm)
            {
                unsafe {
                    page::free_page(mem);
                }
                self.shrink_to_size(old_size);
                return Err(e);
            }
            map_start = map_start.byte_add(PAGE_SIZE).unwrap();
        }

        self.size = new_size;

        Ok(())
    }

    pub fn shrink_by(&mut self, decrement: usize) {
        let new_size = self.size.saturating_sub(decrement);
        self.shrink_to_size(new_size);
    }

    /// Deallocates user pages to bring the process size to `new_size`.
    ///
    /// `new_size` need not be page-aligned.
    /// `new_size` need not to be less than current size.
    fn shrink_to_size(&mut self, new_size: usize) {
        if new_size >= self.size {
            return;
        }

        if new_size.page_roundup() < self.size.page_roundup() {
            let size = self.size.page_roundup() - new_size.page_roundup();
            let start_va = VirtAddr::MIN_AVA.byte_add(new_size).unwrap().page_roundup();
            for pa in self.pt.unmap_addrs(start_va, size).unwrap() {
                let (level, pa) = pa.unwrap();
                assert_eq!(level, 0, "super page is not supported yet");
                unsafe {
                    page::free_page(pa.as_mut_ptr());
                }
            }
        }

        self.size = new_size;
    }

    pub fn try_clone_into(&self, target: &mut Self) -> Result<(), KernelError> {
        target.shrink_to_size(0);

        (|| {
            let mut map_start = VirtAddr::MIN_AVA;
            let map_end = map_start.byte_add(self.size)?;
            while map_start < map_end {
                let va = map_start;
                target.size = va.addr();

                let (level, pte) = self.pt.find_leaf_entry(va)?;
                assert_eq!(level, 0, "super page is not supported yet");
                assert!(pte.is_valid() && pte.is_leaf());

                let page_size = memory::level_page_size(level);
                let src_pa = pte.phys_addr();
                let flags = pte.flags();

                let dst = page::alloc_page()?;
                unsafe {
                    dst.as_ptr().copy_from(src_pa.as_ptr(), page_size);
                }

                target
                    .pt
                    .map_addrs(va, PhysAddr::new(dst.addr().get()), page_size, flags)?;

                map_start = map_start.byte_add(page_size).unwrap();
            }
            target.size = self.size;
            Ok(())
        })()
        .inspect_err(|_| {
            target.shrink_to_size(0);
        })
    }

    pub fn fetch_chunk_mut(
        &mut self,
        va: VirtAddr,
        flags: PtEntryFlags,
    ) -> Result<&mut [u8], KernelError> {
        self.pt.fetch_chunk_mut(va, flags)
    }

    pub fn validate(&self, va: Range<VirtAddr>, perm: PtEntryFlags) -> Result<(), KernelError> {
        self.pt.validate(va, perm)
    }

    pub fn validate_user_read(&self, va: Range<VirtAddr>) -> Result<(), KernelError> {
        self.validate(va, PtEntryFlags::UR)
    }

    pub fn validate_user_write(&self, va: Range<VirtAddr>) -> Result<(), KernelError> {
        self.validate(va, PtEntryFlags::UW)
    }

    /// Copies from user to kernel.
    pub fn copy_k2u<T>(&mut self, dst: &mut Validated<UserMutRef<T>>, src: &T)
    where
        T: Pod,
    {
        self.copy_k2u_bytes(&mut dst.as_bytes_mut(), src.as_bytes());
    }

    /// Copies from kernel to user.
    #[expect(clippy::needless_pass_by_ref_mut)]
    pub fn copy_k2u_bytes(&mut self, dst: &mut Validated<UserMutSlice<u8>>, mut src: &[u8]) {
        assert_eq!(dst.len(), src.len());
        let dst_range = dst.as_va_range();
        let mut dst_start = dst_range.start;
        let dst_end = dst_range.end;
        while dst_start < dst_end {
            let dst_chunk = self
                .pt
                .fetch_chunk_mut(dst_start, PtEntryFlags::UW)
                .unwrap();
            let n = usize::min(src.len(), dst_chunk.len());
            dst_chunk[..n].copy_from_slice(&src[..n]);
            dst_start = dst_start.byte_add(n).unwrap();
            src = &src[n..];
        }
        assert_eq!(dst_start, dst_end);
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
        let src_range = src.as_va_range();
        let mut src_start = src_range.start;
        let src_end = src_range.end;
        while src_start < src_end {
            let src_chunk = self.pt.fetch_chunk(src_start, PtEntryFlags::UR).unwrap();
            let n = usize::min(dst.len(), src_chunk.len());
            dst[..n].copy_from_slice(&src_chunk[..n]);
            dst = &mut dst[n..];
            src_start = src_start.byte_add(n).unwrap();
        }
        assert_eq!(src_start, src_end);
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
    #[expect(clippy::needless_pass_by_ref_mut)]
    pub fn copy_u2u_bytes(
        dst_pt: &mut Self,
        dst: &mut Validated<UserMutSlice<u8>>,
        src_pt: &Self,
        src: &Validated<UserSlice<u8>>,
    ) {
        assert_eq!(src.len(), dst.len());
        let copy_size = src.len();

        let dst_range = dst.as_va_range();
        let mut dst_start = dst_range.start;
        let dst_end = dst_range.end;
        let mut dst_bytes = &mut [][..];

        let src_range = src.as_va_range();
        let mut src_start = src_range.start;
        let src_end = src_range.end;
        let mut src_bytes = &[][..];

        let mut total_copied = 0;
        while total_copied < copy_size {
            if dst_bytes.is_empty() {
                assert!(dst_start < dst_end);
                let rest_len = dst_end.addr() - dst_start.addr();
                dst_bytes = dst_pt
                    .pt
                    .fetch_chunk_mut(dst_start, PtEntryFlags::UW)
                    .unwrap();
                if dst_bytes.len() > rest_len {
                    dst_bytes = &mut dst_bytes[..rest_len];
                }
                dst_start = dst_start.byte_add(dst_bytes.len()).unwrap();
            }

            if src_bytes.is_empty() {
                assert!(src_start < src_end);
                let rest_len = src_end.addr() - src_start.addr();
                src_bytes = src_pt.pt.fetch_chunk(src_start, PtEntryFlags::UR).unwrap();
                if src_bytes.len() > rest_len {
                    src_bytes = &src_bytes[..rest_len];
                }
                src_start = src_start.byte_add(src_bytes.len()).unwrap();
            }

            let n = usize::min(dst_bytes.len(), src_bytes.len());
            dst_bytes[..n].copy_from_slice(&src_bytes[..n]);
            dst_bytes = &mut dst_bytes[n..];
            src_bytes = &src_bytes[n..];
            total_copied += n;
        }
        assert_eq!(dst_start, dst_end);
        assert_eq!(src_start, src_end);
    }

    pub fn dump(&self) {
        page_table::dump_pagetable(&self.pt);
    }
}

impl Drop for UserPageTable {
    fn drop(&mut self) {
        let unmapped_pages = self.pt.unmap_addrs(USYSCALL, USYSCALL_SIZE).unwrap();
        for pa in unmapped_pages {
            let Ok((level, pa)) = pa else {
                continue;
            };
            assert_eq!(level, 0, "super page is not supported yet");
            unsafe {
                page::free_page(pa.as_mut_ptr());
            }
        }

        let _ = self.pt.unmap_addrs(TRAMPOLINE, TRAMPOLINE_SIZE).unwrap();
        let _ = self.pt.unmap_addrs(TRAPFRAME, TRAPFRAME_SIZE).unwrap();

        if self.size > 0 {
            let size = self.size.page_roundup();
            let unmapped_pages = self.pt.unmap_addrs(VirtAddr::MIN_AVA, size).unwrap();
            for pa in unmapped_pages {
                let (level, pa) = pa.unwrap();
                assert_eq!(level, 0, "super page is not supported yet");
                unsafe {
                    page::free_page(pa.as_mut_ptr());
                }
            }
        }
        self.pt.free_descendant();
    }
}
