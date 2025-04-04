use alloc::boxed::Box;
use core::{ops::Range, ptr};

use dataview::{DataView, Pod, PodMethods as _};
use ov6_syscall::{USyscallData, UserMutRef, UserMutSlice, UserRef, UserSlice};
use ov6_types::process::ProcId;
use riscv::register::satp::Satp;

use super::{
    PageRound as _, PhysAddr, VirtAddr,
    addr::{GenericMutSlice, GenericSlice, Validated},
    layout::{
        TRAMPOLINE, TRAMPOLINE_SIZE, TRAPFRAME, TRAPFRAME_SIZE, USER_STACK_BOTTOM, USER_STACK_SIZE,
        USYSCALL, USYSCALL_SIZE,
    },
    page::{self, PageFrameAllocator},
    page_table::{self, MapTarget, PageTable, PtEntryFlags},
};
use crate::{
    error::KernelError,
    interrupt::{trampoline, trap::TrapFrame},
    memory::addr::AsVirtAddrRange as _,
};

pub struct UserPageTable {
    pt: Box<PageTable, PageFrameAllocator>,
    heap_start: VirtAddr,
    heap_size: usize,
    stack_start: VirtAddr,
    stack_size: usize,
}

impl UserPageTable {
    /// Creates a user page table with a given trapframe, with no user memory,
    /// but with trampoline and trapframe pages.
    pub fn new(pid: ProcId, tf: &TrapFrame) -> Result<Self, KernelError> {
        // An empty page table.
        let mut pt = Self {
            pt: PageTable::try_allocate()?,
            heap_start: VirtAddr::MIN_AVA,
            heap_size: 0,
            stack_start: USER_STACK_BOTTOM,
            stack_size: USER_STACK_SIZE,
        };

        pt.alloc_usyscall(pid)?;

        pt.pt.map_addrs(
            TRAMPOLINE,
            MapTarget::fixed_addr(PhysAddr::new(trampoline::trampoline as usize)),
            TRAMPOLINE_SIZE,
            PtEntryFlags::RX,
        )?;

        pt.pt.map_addrs(
            TRAPFRAME,
            MapTarget::fixed_addr(PhysAddr::new(ptr::from_ref(tf).addr())),
            TRAPFRAME_SIZE,
            PtEntryFlags::RW,
        )?;

        Ok(pt)
    }

    pub fn satp(&self) -> Satp {
        self.pt.satp()
    }

    pub fn heap_start(&self) -> VirtAddr {
        self.heap_start
    }

    pub fn set_heap_start(&mut self, heap_start: VirtAddr) {
        self.heap_start = heap_start;
    }

    pub fn program_break(&self) -> VirtAddr {
        self.heap_start.byte_add(self.heap_size).unwrap()
    }

    pub fn stack_top(&self) -> VirtAddr {
        self.stack_start.byte_add(self.stack_size).unwrap()
    }

    pub fn alloc_stack(&mut self) -> Result<(), KernelError> {
        self.pt.map_addrs(
            self.stack_start,
            MapTarget::allocate_new_zeroed(),
            self.stack_size,
            PtEntryFlags::URW,
        )
    }

    pub fn alloc_usyscall(&mut self, pid: ProcId) -> Result<(), KernelError> {
        let start_va = USYSCALL;
        let size = USYSCALL_SIZE;
        self.pt.map_addrs(
            start_va,
            MapTarget::allocate_new_zeroed(),
            size,
            PtEntryFlags::UR,
        )?;

        let bytes = self.fetch_chunk_mut(start_va, PtEntryFlags::U)?;
        assert!(bytes.len() >= size_of::<USyscallData>());
        *DataView::from_mut(bytes).get_mut::<USyscallData>(0) = USyscallData { pid };

        Ok(())
    }

    pub fn map_addrs(
        &mut self,
        va: VirtAddr,
        pa: MapTarget,
        size: usize,
        flags: PtEntryFlags,
    ) -> Result<(), KernelError> {
        self.pt.map_addrs(va, pa, size, flags)
    }

    pub fn grow_heap_by(
        &mut self,
        increment: usize,
        xperm: PtEntryFlags,
    ) -> Result<(), KernelError> {
        let new_size = self
            .heap_size
            .checked_add(increment)
            .ok_or(KernelError::HeapSizeOverflow)?;
        self.grow_heap_to_size(new_size, xperm)
    }

    /// Allocates PTEs and physical memory to grow process to `new_size`,
    /// which need not be page aligned.
    fn grow_heap_to_size(
        &mut self,
        new_size: usize,
        xperm: PtEntryFlags,
    ) -> Result<(), KernelError> {
        if new_size < self.heap_size {
            return Ok(());
        }

        let old_size = self.heap_size;
        self.heap_size = new_size;

        let map_start = self.heap_start.byte_add(old_size).unwrap().page_roundup();
        let map_end = self.heap_start.byte_add(new_size)?;
        if map_start < map_end {
            let map_size = map_end.page_roundup().checked_sub(map_start).unwrap();
            if let Err(e) =
                self.pt
                    .map_addrs(map_start, MapTarget::allocate_new_zeroed(), map_size, xperm)
            {
                self.shrink_heap_to_size(old_size);
                return Err(e);
            }
        }

        Ok(())
    }

    pub fn shrink_heap_by(&mut self, decrement: usize) -> Result<(), KernelError> {
        let new_size = self
            .heap_size
            .checked_sub(decrement)
            .ok_or(KernelError::HeapSizeUnderflow)?;
        self.shrink_heap_to_size(new_size);

        Ok(())
    }

    /// Deallocates user pages to bring the process size to `new_size`.
    ///
    /// `new_size` need not be page-aligned.
    /// `new_size` need not to be less than current size.
    fn shrink_heap_to_size(&mut self, new_size: usize) {
        if new_size >= self.heap_size {
            return;
        }

        if new_size.page_roundup() < self.heap_size.page_roundup() {
            let size = self.heap_size.page_roundup() - new_size.page_roundup();
            let start_va = self.heap_start.byte_add(new_size).unwrap().page_roundup();
            for (level, va, pa) in self.pt.unmap_addrs(start_va, size).unwrap() {
                assert_eq!(
                    level, 0,
                    "super page is not supported yet, level={level}, va={va:#x}, pa={pa:#x}"
                );
                unsafe {
                    page::free_page(pa.as_mut_ptr());
                }
            }
        }

        self.heap_size = new_size;
    }

    pub fn clone_from(&mut self, other: &Self) -> Result<(), KernelError> {
        self.pt.clone_pages_from(
            &other.pt,
            VirtAddr::MIN_AVA..other.heap_start,
            PtEntryFlags::U,
        )?;

        self.pt.clone_pages_from(
            &other.pt,
            other.stack_start..other.stack_top(),
            PtEntryFlags::U,
        )?;
        self.stack_start = other.stack_start;
        self.stack_size = other.stack_size;

        let heap_start = other.heap_start;
        let heap_end = heap_start.byte_add(other.heap_size)?;
        self.pt
            .clone_pages_from(&other.pt, heap_start..heap_end, PtEntryFlags::U)?;
        self.heap_start = other.heap_start;
        self.heap_size = other.heap_size;

        Ok(())
    }

    pub fn fetch_chunk(&self, va: VirtAddr, flags: PtEntryFlags) -> Result<&[u8], KernelError> {
        self.pt.fetch_chunk(va, flags)
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

    /// Copies from either a user address, or kernel address.
    pub fn copy_x2u_bytes(
        &mut self,
        dst: &mut Validated<UserMutSlice<u8>>,
        src: &GenericSlice<u8>,
    ) {
        assert_eq!(dst.len(), src.len());
        match src {
            GenericSlice::User(src_pt, src) => Self::copy_u2u_bytes(self, dst, src_pt, src),
            GenericSlice::Kernel(src) => self.copy_k2u_bytes(dst, src),
        }
    }

    pub fn dump(&self) {
        page_table::dump_pagetable(&self.pt);
    }
}

impl Drop for UserPageTable {
    fn drop(&mut self) {
        let _ = self.pt.unmap_addrs(TRAMPOLINE, TRAMPOLINE_SIZE).unwrap();
        let _ = self.pt.unmap_addrs(TRAPFRAME, TRAPFRAME_SIZE).unwrap();

        for (level, va, pa) in self.pt.unmap_range(VirtAddr::MIN_AVA..VirtAddr::MAX) {
            assert_eq!(
                level, 0,
                "super page is not supported yet, level={level}, va={va:#x}, pa={pa:#x}"
            );
            unsafe {
                page::free_page(pa.as_mut_ptr());
            }
        }

        self.pt.free_descendant();
    }
}
