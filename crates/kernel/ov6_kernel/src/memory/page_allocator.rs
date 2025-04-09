use core::{ops::Range, ptr::NonNull};

use ov6_syscall::MemoryInfo;
use page_alloc::PageFrameAllocator;

use super::{PAGE_SIZE, PhysAddr};
use crate::{error::KernelError, sync::SpinLock};

pub(super) struct PageAllocator {
    allocator: SpinLock<PageFrameAllocator<PAGE_SIZE>>,
}

impl PageAllocator {
    pub(super) unsafe fn new(heap_range: Range<PhysAddr>) -> Self {
        let Range {
            start: heap_start,
            end: heap_end,
        } = heap_range;

        let allocator =
            unsafe { PageFrameAllocator::new(heap_start.as_non_null()..heap_end.as_non_null()) };

        Self {
            allocator: SpinLock::new(allocator),
        }
    }

    /// Checks if the given pointer is within the allocated address range.
    ///
    /// Returns `true` if the pointer is within the range, otherwise `false`.
    pub(super) fn is_heap_addr(&self, ptr: NonNull<u8>) -> bool {
        self.allocator.lock().is_allocated_pointer(ptr)
    }

    /// Frees the page of physical memory pointed at by `pa`.
    ///
    /// # Safety
    ///
    /// The caller must ensure that:
    ///
    /// - The page was previously allocated by `alloc_page` or
    ///   `alloc_zeroed_page`.
    /// - The page is not accessed after being freed.
    /// - The page is not freed more than once.
    ///
    /// This function fills the page with junk data to catch dangling
    /// references.
    pub(super) unsafe fn free_page(&self, pa: NonNull<u8>) {
        // Fill with junk to catch dangling refs.
        unsafe {
            pa.write_bytes(1, PAGE_SIZE);
        }

        unsafe { self.allocator.lock().free(pa) }
    }

    /// Allocates one 4096-byte page of physical memory.
    ///
    /// Returns a pointer to the allocated page, or an error if no memory is
    /// available.
    ///
    /// The allocated page is filled with a specific pattern to help detect
    /// uninitialized usage.
    pub(super) fn alloc_page(&self) -> Result<NonNull<u8>, KernelError> {
        let p = self
            .allocator
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
    pub(super) fn alloc_zeroed_page(&self) -> Result<NonNull<u8>, KernelError> {
        self.allocator
            .lock()
            .alloc_zeroed()
            .ok_or(KernelError::NoFreePage)
    }

    /// Retrieves memory information, including the number of free and total
    /// pages.
    pub(super) fn info(&self) -> MemoryInfo {
        let allocator = self.allocator.lock();
        MemoryInfo {
            free_pages: allocator.free_pages(),
            total_pages: allocator.total_pages(),
            page_size: PAGE_SIZE,
        }
    }
}
