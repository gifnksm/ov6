#![cfg_attr(not(test), no_std)]

pub mod boxed;
pub mod heap_allocator;
pub mod page_frame_allocator;

pub use self::{
    boxed::PageBox,
    page_frame_allocator::{PageFrameAllocator, RetrievePageFrameAllocator},
};
