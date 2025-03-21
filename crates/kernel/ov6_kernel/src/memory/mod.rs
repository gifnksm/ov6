pub use self::addr::{PageRound, PhysAddr, PhysPageNum, VirtAddr};

/// Bytes per page
pub const PAGE_SIZE: usize = 4096;

/// Bits of offset within a page
pub const PAGE_SHIFT: usize = 12;

pub mod addr;
pub mod heap;
pub mod layout;
pub mod page;
pub mod page_table;
pub mod vm_kernel;
pub mod vm_user;
