//! Physical memory allocator, for user processes,
//! kernel stacks, page-table pages,
//! and pipe buffers.
//!
//! Allocates whole 4096-byte pages.

use core::{
    ffi::c_void,
    ops::{Deref, DerefMut},
    ptr::{self, NonNull},
};

use crate::{
    memory::{
        layout::PHYS_TOP,
        vm::{PAGE_SIZE, PageRound as _},
    },
    sync::SpinLock,
};

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

static FREE_LIST: SpinLock<Run> = SpinLock::new(Run { next: None });

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
/// Returns `None` if the memory cannot be allocated.
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

/// Allocates one 4096-byte zeroed page of physical memory.
///
/// Returns a pointer that the kernel can use.
/// Returns `None` if the memory cannot be allocated.
pub fn alloc_zeroed_page() -> Option<NonNull<c_void>> {
    let page = alloc_page()?;
    unsafe { page.cast::<u8>().write_bytes(0, PAGE_SIZE) }
    Some(page)
}

/// A pointer type that uniquely owns a page of type `T`.
#[derive(Debug)]
pub struct PageBox<T> {
    ptr: NonNull<T>,
}

impl<T> Deref for PageBox<T> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        unsafe { self.ptr.as_ref() }
    }
}

impl<T> DerefMut for PageBox<T> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        unsafe { self.ptr.as_mut() }
    }
}

impl<T> Drop for PageBox<T> {
    fn drop(&mut self) {
        unsafe {
            ptr::drop_in_place(self.ptr.as_ptr());
        }
        free_page(self.ptr.cast())
    }
}

impl<T> PageBox<T> {
    /// Allocates a page and then places `value` into it.
    pub fn new(value: T) -> PageBox<T> {
        Self::try_new(value).unwrap()
    }

    /// Allocates a page and then places `value` into it, returning an error if the allocation fails.
    pub fn try_new(value: T) -> Option<PageBox<T>> {
        assert!(size_of::<T>() < PAGE_SIZE);
        assert_eq!(PAGE_SIZE % align_of::<T>(), 0);

        let ptr = alloc_page()?.cast();
        unsafe {
            ptr.write(value);
        }

        Some(Self { ptr })
    }

    /// Returns a raw pointer to the `PageBox`'s contents.
    pub fn as_ptr(this: &Self) -> *const T {
        this.ptr.as_ptr()
    }
}
