//! Physical memory allocator, for user processes,
//! kernel stacks, page-table pages,
//! and pipe buffers.
//!
//! Allocates whole 4096-byte pages.

use core::{ffi::c_void, ptr};

use crate::{
    memlayout::PHYS_TOP,
    spinlock::Mutex,
    vm::{self, PAGE_SIZE},
};

mod ffi {
    use super::*;

    #[unsafe(no_mangle)]
    extern "C" fn kalloc() -> *mut c_void {
        super::kalloc()
    }

    #[unsafe(no_mangle)]
    extern "C" fn kfree(pa: *mut c_void) {
        super::kfree(pa)
    }
}

/// First address after kernel.
const fn end() -> *mut c_void {
    unsafe extern "C" {
        /// First address after kernel.
        ///
        /// defined by `kernel.ld`
        #[link_name = "end"]
        static mut END: [u8; 0];
    }

    (&raw mut END).cast()
}

const fn top() -> *mut c_void {
    ptr::without_provenance_mut(PHYS_TOP)
}

struct Run {
    next: *mut Run,
}

unsafe impl Send for Run {}

static FREE_LIST: Mutex<Run> = Mutex::new(Run {
    next: ptr::null_mut(),
});

pub fn init() {
    unsafe { free_range(end(), top()) }
}

unsafe fn free_range(pa_start: *mut c_void, pa_end: *mut c_void) {
    unsafe {
        let mut p = ptr::without_provenance_mut::<c_void>(vm::page_roundup(pa_start.addr()));
        while p.byte_add(PAGE_SIZE) <= pa_end {
            kfree(p);
            p = p.byte_add(PAGE_SIZE);
        }
    }
}

/// Frees the page of physical memory pointed at by pa,
/// which normally should have been returned by a
/// call to `kalloc()`.
pub fn kfree(pa: *mut c_void) {
    assert_eq!(pa.addr() % PAGE_SIZE, 0, "pa = {pa:#p}");
    assert!(pa >= end(), "pa = {:#p}, end = {:#p}", pa, end());
    assert!(pa < top(), "pa = {:#p}, top = {:#p}", pa, top());

    // Fill with junk to catch dangling refs.
    unsafe {
        ptr::write_bytes(pa, 1, PAGE_SIZE);
    }

    let r = pa.cast::<Run>();
    let mut free_list = FREE_LIST.lock();
    unsafe {
        (*r).next = free_list.next;
        free_list.next = r;
    }
}

/// Allocates one 4096-byte page of physical memory.
///
/// Returns a pointer that the kernel can use.
/// Returns null if the memory cannot be allocated.
pub fn kalloc() -> *mut c_void {
    let mut free_list = FREE_LIST.lock();
    let r = free_list.next;
    unsafe {
        if !r.is_null() {
            free_list.next = (*r).next;
        }
    }
    drop(free_list);

    let r = r.cast::<c_void>();

    if !r.is_null() {
        unsafe {
            ptr::write_bytes(r, 5, PAGE_SIZE);
        }
    }

    r
}
