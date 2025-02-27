#![cfg_attr(not(test), no_std)]

pub mod heap_allocator;
pub mod page_frame_allocator;

pub use self::page_frame_allocator::{PageFrameAllocator, RetrievePageFrameAllocator};
