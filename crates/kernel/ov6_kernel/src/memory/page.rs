//! Physical memory allocator, for user processes,
//! kernel stacks, page-table pages,
//! and pipe buffers.
//!
//! Allocates whole 4096-byte pages.

use core::{
    alloc::{AllocError, Allocator, Layout},
    ptr::{self, NonNull},
};

use once_init::OnceInit;

use super::{PAGE_SIZE, PageRound as _, layout::KERNEL_END};
use crate::{error::KernelError, memory::layout::PHYS_TOP, sync::SpinLock};

/// First address after kernel.
fn end() -> NonNull<u8> {
    let end = unsafe { KERNEL_END };
    NonNull::new(ptr::with_exposed_provenance_mut::<u8>(end)).unwrap()
}

fn top() -> NonNull<u8> {
    let top = unsafe { PHYS_TOP };
    NonNull::new(ptr::with_exposed_provenance_mut(top)).unwrap()
}

static PAGE_FRAME_ALLOCATOR: OnceInit<SpinLock<page_alloc::PageFrameAllocator<PAGE_SIZE>>> =
    OnceInit::new();

pub fn init() {
    let pa_start = end().map_addr(|a| a.get().page_roundup().try_into().unwrap());
    let pa_end = top().map_addr(|a| a.get().page_rounddown().try_into().unwrap());

    unsafe {
        PAGE_FRAME_ALLOCATOR.init(SpinLock::new(page_alloc::PageFrameAllocator::new(
            pa_start.as_ptr()..pa_end.as_ptr(),
        )));
    }
}

/// Frees the page of physical memory pointed at by pa,
/// which normally should have been returned by a
/// call to `kalloc()`.
pub unsafe fn free_page(pa: NonNull<u8>) {
    // Fill with junk to catch dangling refs.
    unsafe {
        pa.write_bytes(1, PAGE_SIZE);
    }
    unsafe { PAGE_FRAME_ALLOCATOR.get().lock().free(pa) }
}

/// Allocates one 4096-byte page of physical memory.
///
/// Returns a pointer that the kernel can use.
/// Returns `None` if the memory cannot be allocated.
pub fn alloc_page() -> Result<NonNull<u8>, KernelError> {
    let p = PAGE_FRAME_ALLOCATOR
        .get()
        .lock()
        .alloc()
        .ok_or(KernelError::NoFreePage)?;
    unsafe {
        p.write_bytes(5, PAGE_SIZE);
    }
    Ok(p)
}

/// Allocates one 4096-byte zeroed page of physical memory.
///
/// Returns a pointer that the kernel can use.
/// Returns `None` if the memory cannot be allocated.
pub fn alloc_zeroed_page() -> Result<NonNull<u8>, KernelError> {
    PAGE_FRAME_ALLOCATOR
        .get()
        .lock()
        .alloc_zeroed()
        .ok_or(KernelError::NoFreePage)
}

#[derive(Clone)]
pub struct PageFrameAllocator;

unsafe impl Allocator for PageFrameAllocator {
    fn allocate(&self, layout: Layout) -> Result<NonNull<[u8]>, AllocError> {
        assert!(layout.size() <= PAGE_SIZE);
        assert_eq!(PAGE_SIZE % layout.align(), 0);

        #[expect(clippy::map_err_ignore)]
        let page = alloc_page().map_err(|_| AllocError)?;
        Ok(NonNull::slice_from_raw_parts(page.cast(), PAGE_SIZE))
    }

    unsafe fn deallocate(&self, ptr: NonNull<u8>, layout: Layout) {
        assert!(layout.size() <= PAGE_SIZE);
        assert_eq!(PAGE_SIZE % layout.align(), 0);
        assert_eq!(ptr.addr().get() % PAGE_SIZE, 0);

        unsafe { free_page(ptr.cast()) }
    }
}
