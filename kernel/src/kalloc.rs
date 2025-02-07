//! Physical memory allocator, for user processes,
//! kernel stacks, page-table pages,
//! and pipe buffers.
//!
//! Allocates whole 4096-byte pages.

use core::{
    ffi::c_void,
    ptr::{self, NonNull},
};

use crate::{
    memlayout::PHYS_TOP,
    spinlock::Mutex,
    vm::{PAGE_SIZE, PageRound as _},
};

mod ffi {
    use super::*;

    #[unsafe(no_mangle)]
    extern "C" fn kalloc() -> *mut c_void {
        super::alloc_page()
            .map(NonNull::as_ptr)
            .unwrap_or_else(ptr::null_mut)
    }

    #[unsafe(no_mangle)]
    extern "C" fn kfree(pa: *mut c_void) {
        super::free_page(NonNull::new(pa).unwrap())
    }
}

/// First address after kernel.
const fn end() -> NonNull<c_void> {
    unsafe extern "C" {
        /// First address after kernel.
        ///
        /// defined by `kernel.ld`
        #[link_name = "end"]
        static mut END: [u8; 0];
    }

    NonNull::new((&raw mut END).cast()).unwrap()
}

const fn top() -> NonNull<c_void> {
    NonNull::new(ptr::without_provenance_mut(PHYS_TOP)).unwrap()
}

struct Run {
    next: Option<NonNull<Run>>,
}

unsafe impl Send for Run {}

static FREE_LIST: Mutex<Run> = Mutex::new(Run { next: None });

pub fn init() {
    let pa_start = end();
    let pa_end = top();

    unsafe {
        let mut p = pa_start.page_roundup();
        while p.byte_add(PAGE_SIZE) <= pa_end {
            free_page(p);
            p = p.byte_add(PAGE_SIZE);
        }
    }
}

/// Frees the page of physical memory pointed at by pa,
/// which normally should have been returned by a
/// call to `kalloc()`.
pub fn free_page(pa: NonNull<c_void>) {
    assert_eq!(pa.addr().get() % PAGE_SIZE, 0, "pa = {pa:#p}");
    assert!(pa >= end(), "pa = {:#p}, end = {:#p}", pa, end());
    assert!(pa < top(), "pa = {:#p}, top = {:#p}", pa, top());

    // Fill with junk to catch dangling refs.
    unsafe {
        pa.write_bytes(1, PAGE_SIZE);
    }

    let mut r = pa.cast::<Run>();
    let mut free_list = FREE_LIST.lock();
    unsafe {
        r.as_mut().next = free_list.next;
        free_list.next = Some(r);
    }
}

/// Allocates one 4096-byte page of physical memory.
///
/// Returns a pointer that the kernel can use.
/// Returns null if the memory cannot be allocated.
pub fn alloc_page() -> Option<NonNull<c_void>> {
    let mut free_list = FREE_LIST.lock();
    let r = free_list.next?;
    unsafe {
        free_list.next = r.as_ref().next;
    }
    drop(free_list);

    let r = r.cast::<c_void>();

    unsafe {
        r.write_bytes(5, PAGE_SIZE);
    }

    Some(r)
}
