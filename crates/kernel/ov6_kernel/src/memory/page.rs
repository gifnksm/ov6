//! Physical memory allocator, for user processes,
//! kernel stacks, page-table pages,
//! and pipe buffers.
//!
//! Allocates whole 4096-byte pages.

use ov6_syscall::MemoryInfo;

pub use self::page_manager::PageFrameAllocator;
use super::{PhysAddr, layout::KERNEL_END, page_manager};
use crate::memory::layout::PHYS_TOP;

/// Initializes the physical memory allocator.
///
/// This function sets up the range of physical addresses available for
/// allocation and initializes the page frame allocator.
pub fn init() {
    let pa_start = PhysAddr::new(unsafe { KERNEL_END });
    let pa_end = PhysAddr::new(unsafe { PHYS_TOP });

    unsafe { page_manager::init(pa_start..pa_end) }
}

/// Checks if the given address is within the allocated address range.
///
/// Returns `true` if the pointer is within the range, otherwise `false`.
pub(super) fn is_heap_addr(pa: PhysAddr) -> bool {
    page_manager::get().is_heap_addr(pa)
}

/// Retrieves memory information, including the number of free and total pages.
pub(crate) fn info() -> MemoryInfo {
    page_manager::get().info()
}
