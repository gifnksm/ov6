use ov6_syscall::MemoryInfo;

pub use self::addr::{PageRound, PhysAddr, VirtAddr};

/// Bytes per page
pub const PAGE_SIZE: usize = 4096;

pub const fn level_page_size(level: usize) -> usize {
    assert!(level <= 2);
    PAGE_SIZE << (level * 9)
}

/// Bits of offset within a page
pub const PAGE_SHIFT: usize = 12;

pub mod addr;
pub mod heap;
pub mod layout;
pub mod page;
mod page_allocator;
mod page_manager;
pub mod page_table;
pub mod vm_kernel;
pub mod vm_user;

pub(crate) fn info() -> MemoryInfo {
    page::info()
}
