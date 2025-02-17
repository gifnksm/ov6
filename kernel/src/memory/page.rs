//! Physical memory allocator, for user processes,
//! kernel stacks, page-table pages,
//! and pipe buffers.
//!
//! Allocates whole 4096-byte pages.

use core::ptr::{self, NonNull};

use page_alloc::{PageFrameAllocator, RetrievePageFrameAllocator};

use crate::{
    memory::{layout::PHYS_TOP, vm::PAGE_SIZE},
    sync::{Once, SpinLock, SpinLockGuard},
};

use super::vm::PageRound as _;

/// First address after kernel.
const fn end() -> NonNull<u8> {
    unsafe extern "C" {
        /// First address after kernel.
        ///
        /// defined by `kernel.ld`
        #[link_name = "end"]
        static mut END: [u8; 0];
    }

    NonNull::new(&raw mut END).unwrap().cast()
}

const fn top() -> NonNull<u8> {
    NonNull::new(ptr::without_provenance_mut(PHYS_TOP)).unwrap()
}

static PAGE_FRAME_ALLOCATOR: Once<SpinLock<PageFrameAllocator<PAGE_SIZE>>> = Once::new();

pub struct PageFrameAllocatorRetriever;
impl RetrievePageFrameAllocator<PAGE_SIZE> for PageFrameAllocatorRetriever {
    type AllocatorRef = SpinLockGuard<'static, PageFrameAllocator<PAGE_SIZE>>;

    fn retrieve_allocator() -> Self::AllocatorRef {
        PAGE_FRAME_ALLOCATOR.get().lock()
    }
}

pub fn init() {
    let pa_start = end().page_roundup();
    let pa_end = top().page_rounddown();

    unsafe {
        PAGE_FRAME_ALLOCATOR.init(SpinLock::new(PageFrameAllocator::new(
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
pub fn alloc_page() -> Option<NonNull<u8>> {
    let p = PAGE_FRAME_ALLOCATOR.get().lock().alloc()?;
    unsafe {
        p.write_bytes(5, PAGE_SIZE);
    }
    Some(p)
}

/// Allocates one 4096-byte zeroed page of physical memory.
///
/// Returns a pointer that the kernel can use.
/// Returns `None` if the memory cannot be allocated.
pub fn alloc_zeroed_page() -> Option<NonNull<u8>> {
    PAGE_FRAME_ALLOCATOR.get().lock().alloc_zeroed()
}
