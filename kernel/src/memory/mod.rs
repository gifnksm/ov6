pub use self::addr::{PageRound, PhysAddr, PhysPageNum, VirtAddr};

/// Bytes per page
pub const PAGE_SIZE: usize = 4096;

/// Bits of offset within a page
pub const PAGE_SHIFT: usize = 12;

mod addr;
pub mod heap;
pub mod layout;
pub mod page;
pub mod vm;
