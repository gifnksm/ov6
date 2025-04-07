//! Physical memory allocator, for user processes,
//! kernel stacks, page-table pages,
//! and pipe buffers.
//!
//! Allocates whole 4096-byte pages.

use core::{
    alloc::{AllocError, Allocator, Layout},
    ops::Range,
    ptr::NonNull,
};

use once_init::OnceInit;
use ov6_syscall::MemoryInfo;

use super::{PAGE_SIZE, PageRound as _, PhysAddr, layout::KERNEL_END};
use crate::{error::KernelError, memory::layout::PHYS_TOP, sync::SpinLock};

/// Returns the first physical address after the kernel.
///
/// This is used to determine the starting point for physical memory allocation.
fn end() -> PhysAddr {
    let end = unsafe { KERNEL_END };
    PhysAddr::new(end)
}

/// Returns the top physical address of the system.
///
/// This is used to determine the upper limit of physical memory allocation.
fn top() -> PhysAddr {
    let top = unsafe { PHYS_TOP };
    PhysAddr::new(top)
}

static PAGE_FRAME_ALLOCATOR: OnceInit<SpinLock<page_alloc::PageFrameAllocator<PAGE_SIZE>>> =
    OnceInit::new();

static PAGE_ADDR_RANGE: OnceInit<Range<PhysAddr>> = OnceInit::new();

/// Initializes the physical memory allocator.
///
/// This function sets up the range of physical addresses available for
/// allocation and initializes the page frame allocator.
pub fn init() {
    let pa_start = end().page_roundup();
    let pa_end = top().page_rounddown();

    PAGE_ADDR_RANGE.init(pa_start..pa_end);

    unsafe {
        PAGE_FRAME_ALLOCATOR.init(SpinLock::new(page_alloc::PageFrameAllocator::new(
            pa_start.as_mut_ptr()..pa_end.as_mut_ptr(),
        )));
    }
}

/// Checks if the given pointer is within the allocated address range.
///
/// Returns `true` if the pointer is within the range, otherwise `false`.
pub fn is_allocated_addr(ptr: NonNull<u8>) -> bool {
    let range = PAGE_ADDR_RANGE.get();
    range.contains(&ptr.into())
}

/// Frees the page of physical memory pointed at by `pa`.
///
/// # Safety
///
/// The caller must ensure that:
///
/// - The page was previously allocated by `alloc_page` or `alloc_zeroed_page`.
/// - The page is not accessed after being freed.
/// - The page is not freed more than once.
///
/// This function fills the page with junk data to catch dangling references.
pub unsafe fn free_page(pa: NonNull<u8>) {
    // Fill with junk to catch dangling refs.
    unsafe {
        pa.write_bytes(1, PAGE_SIZE);
    }
    unsafe { PAGE_FRAME_ALLOCATOR.get().lock().free(pa) }
}

/// Allocates one 4096-byte page of physical memory.
///
/// Returns a pointer to the allocated page, or an error if no memory is
/// available.
///
/// The allocated page is filled with a specific pattern to help detect
/// uninitialized usage.
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
/// Returns a pointer to the allocated page, or an error if no memory is
/// available.
///
/// The allocated page is initialized to zero.
pub fn alloc_zeroed_page() -> Result<NonNull<u8>, KernelError> {
    PAGE_FRAME_ALLOCATOR
        .get()
        .lock()
        .alloc_zeroed()
        .ok_or(KernelError::NoFreePage)
}

/// A wrapper for the page frame allocator that implements the `Allocator`
/// trait.
///
/// This allows the page frame allocator to be used with Rust's allocator APIs.
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

pub(crate) fn info() -> MemoryInfo {
    let allocator = PAGE_FRAME_ALLOCATOR.get().lock();
    MemoryInfo {
        free_pages: allocator.free_pages(),
        total_pages: allocator.total_pages(),
    }
}
