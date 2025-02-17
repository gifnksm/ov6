use core::{
    ops::{DerefMut, Range},
    ptr::NonNull,
};

struct Run {
    next: Option<NonNull<Run>>,
}

/// A simple page allocator that allocates pages of physical memory.
#[derive(Debug)]
pub struct PageFrameAllocator<const PAGE_SIZE: usize> {
    heap: Range<*mut u8>,
    free_list: Option<NonNull<Run>>,
}

impl<const PAGE_SIZE: usize> PageFrameAllocator<PAGE_SIZE> {
    /// Creates a new `PageAllocator` that manages the given range of physical memory.
    ///
    /// The given range of physical memory must be page-aligned.
    ///
    /// # Safety
    ///
    /// The given range of physical memory must be valid and not overlap with other memory regions.
    pub unsafe fn new(heap: Range<*mut u8>) -> Self {
        assert!(size_of::<Run>() <= PAGE_SIZE);

        assert!(heap.start.addr() > 0);
        assert_eq!(heap.start.addr() % PAGE_SIZE, 0);
        assert_eq!(heap.end.addr() % PAGE_SIZE, 0);

        let mut free_list = None;
        let mut p = heap.start;

        while p < heap.end {
            let mut run = NonNull::new(p).unwrap().cast::<Run>();
            unsafe {
                run.as_mut().next = free_list;
            }
            free_list = Some(run);
            unsafe {
                p = p.byte_add(PAGE_SIZE);
            }
        }

        Self { heap, free_list }
    }

    /// Allocates a page of physical memory.
    pub fn alloc(&mut self) -> Option<NonNull<u8>> {
        let page = self.free_list.take()?;
        self.free_list = unsafe { page.as_ref().next };
        Some(page.cast())
    }

    /// Allocates a page of physical memory and zeroes it.
    pub fn alloc_zeroed(&mut self) -> Option<NonNull<u8>> {
        let page = self.alloc()?;
        unsafe {
            page.cast::<u8>().write_bytes(0, PAGE_SIZE);
        }
        Some(page)
    }

    /// Frees a page of physical memory.
    ///
    /// # Safety
    ///
    /// The given page must have been previously allocated by this `PageAllocator`.
    /// The page must not be accessed after it has been freed.
    /// The page must not be freed more than once.
    pub unsafe fn free(&mut self, page: NonNull<u8>) {
        assert!(self.heap.contains(&page.as_ptr()));
        assert_eq!(page.addr().get() % PAGE_SIZE, 0);

        unsafe {
            let mut run = page.cast::<Run>();
            run.as_mut().next = self.free_list;
            self.free_list = Some(run);
        }
    }
}

unsafe impl<const PAGE_SIZE: usize> Send for PageFrameAllocator<PAGE_SIZE> {}

/// A trait for types that can retrieve a [`PageFrameAllocator`].
pub trait RetrievePageFrameAllocator<const PAGE_SIZE: usize> {
    type AllocatorRef: DerefMut<Target = PageFrameAllocator<PAGE_SIZE>>;

    /// Returns a mutable reference to a [`PageFrameAllocator`].
    fn retrieve_allocator() -> Self::AllocatorRef;
}

#[cfg(test)]
mod tests {
    use core::cell::UnsafeCell;
    use std::collections::HashSet;

    use super::*;

    const PAGE_SIZE: usize = 64;

    #[repr(align(64))]
    struct Heap(UnsafeCell<[u8; PAGE_SIZE * 100]>);
    unsafe impl Sync for Heap {}

    #[test]
    fn test_page_allocator() {
        let heap = Heap(UnsafeCell::new([0; PAGE_SIZE * 100]));
        let heap_range = unsafe { (*heap.0.get()).as_mut_ptr_range() };

        let mut allocator = unsafe { PageFrameAllocator::<PAGE_SIZE>::new(heap_range) };

        let page0 = allocator.alloc().unwrap();
        assert_eq!(page0.addr().get() % PAGE_SIZE, 0);
        let page1 = allocator.alloc().unwrap();
        assert_eq!(page1.addr().get() % PAGE_SIZE, 0);
        assert_ne!(page0, page1);
        unsafe {
            allocator.free(page0);
            allocator.free(page1);
        }
    }

    #[test]
    fn test_all_pages() {
        let heap = Heap(UnsafeCell::new([0; PAGE_SIZE * 100]));
        let heap_range = unsafe { (*heap.0.get()).as_mut_ptr_range() };

        let mut allocator = unsafe { PageFrameAllocator::<PAGE_SIZE>::new(heap_range) };

        let mut pages = vec![];
        let mut addrs = HashSet::new();

        // allocate all pages
        for _ in 0..100 {
            let page = allocator.alloc().unwrap();
            assert_eq!(page.addr().get() % PAGE_SIZE, 0, "page is not aligned");
            assert!(addrs.insert(page.addr()), "page is duplicated");
            pages.push(page);
        }

        // fail to allocate one more page
        assert!(allocator.alloc().is_none());

        // free one page and allocate one page
        let page = pages.pop().unwrap();
        unsafe {
            allocator.free(page);
        }
        let page = allocator.alloc().unwrap();
        assert_eq!(page.addr().get() % PAGE_SIZE, 0);
        pages.push(page);

        // free all pages
        for page in pages {
            unsafe {
                allocator.free(page);
            }
        }
    }
}
