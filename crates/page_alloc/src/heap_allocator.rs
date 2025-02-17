use core::{
    alloc::{GlobalAlloc, Layout},
    marker::PhantomData,
    ops::DerefMut,
    ptr::{self, NonNull},
};

use crate::{PageFrameAllocator, RetrievePageFrameAllocator};

const MIN_ALLOC_SIZE: usize = 16;

const _: () = {
    assert!(MIN_ALLOC_SIZE.is_power_of_two());
    assert!(MIN_ALLOC_SIZE >= size_of::<Run>());
};

/// A simple heap allocator that allocates memory in power of two sizes.
#[derive(Default)]
pub struct HeapAllocator<const PAGE_SIZE: usize> {
    free_list_heads:
        [Option<NonNull<Run>>; (usize::BITS / MIN_ALLOC_SIZE.trailing_zeros()) as usize],
}

struct Run {
    next: Option<NonNull<Run>>,
}

unsafe impl<const PAGE_SIZE: usize> Send for HeapAllocator<PAGE_SIZE> {}

impl<const PAGE_SIZE: usize> HeapAllocator<PAGE_SIZE> {
    /// Creates a new `HeapAllocator`.
    pub const fn new() -> Self {
        Self {
            free_list_heads: [None; (usize::BITS / MIN_ALLOC_SIZE.trailing_zeros()) as usize],
        }
    }

    /// Allocates memory with the given layout.
    ///
    /// # Safety
    ///
    /// The caller must ensure that the given layout is valid.
    pub unsafe fn alloc(
        &mut self,
        page_alloc: &mut PageFrameAllocator<PAGE_SIZE>,
        layout: Layout,
    ) -> *mut u8 {
        assert!(layout.size() <= PAGE_SIZE);
        assert!(layout.align() <= PAGE_SIZE);
        assert_eq!(PAGE_SIZE % layout.align(), 0);
        assert!(PAGE_SIZE.is_power_of_two());

        unsafe {
            let (bin_size, bin_idx) = bin(layout.size());
            let mut free_list = &mut self.free_list_heads[bin_idx];

            loop {
                // If free_list is empty, extend the list.
                let mut head = match *free_list {
                    None => {
                        let Some(head) = allocate_list(page_alloc, bin_size) else {
                            return ptr::null_mut();
                        };
                        *free_list = Some(head);
                        head
                    }
                    Some(head) => head,
                };

                // If list head satisfies alignment requirement, remove and return it.
                if head.addr().get() % layout.align() == 0 {
                    let new_next = head.as_ref().next;
                    *free_list = new_next;
                    return head.as_ptr().cast();
                }

                // Go to next element.
                free_list = &mut head.as_mut().next;
            }
        }
    }

    /// Deallocates the memory at the given `ptr` with the given `layout`.
    ///
    /// # Safety
    ///
    /// The caller must ensure:
    ///
    /// * `ptr` is a block of memory currently allocated via this allocator, and,
    /// * `layout` is the same layout that was used to allocate that block of memory.
    unsafe fn dealloc(&mut self, ptr: *mut u8, layout: Layout) {
        assert!(layout.size() <= PAGE_SIZE);
        assert!(layout.align() <= PAGE_SIZE);
        assert_eq!(PAGE_SIZE % layout.align(), 0);
        assert!(PAGE_SIZE.is_power_of_two());

        let (_bin_size, bin_idx) = bin(layout.size());
        let free_list = &mut self.free_list_heads[bin_idx];
        let mut run = NonNull::new(ptr.cast::<Run>()).unwrap();
        unsafe {
            run.as_mut().next = *free_list;
            *free_list = Some(run);
        }
    }
}

fn bin(size: usize) -> (usize, usize) {
    assert!(size > 0);
    let size = usize::max(size, MIN_ALLOC_SIZE);
    let size = size.next_power_of_two();
    (
        size,
        (size.trailing_zeros() - MIN_ALLOC_SIZE.trailing_zeros()) as usize,
    )
}

fn allocate_list<const PAGE_SIZE: usize>(
    page_alloc: &mut PageFrameAllocator<PAGE_SIZE>,
    size: usize,
) -> Option<NonNull<Run>> {
    let page = page_alloc.alloc()?;
    let mut free_list = None;
    unsafe {
        let mut p = page;
        let end = page.byte_add(PAGE_SIZE);
        while p < end {
            let mut run = p.cast::<Run>();
            run.as_mut().next = free_list;
            free_list = Some(run);
            p = p.byte_add(size);
        }
    }
    free_list
}

/// A trait for types that can retrieve a [`HeapAllocator`].
pub trait RetrieveHeapAllocator<const PAGE_SIZE: usize> {
    type AllocatorRef: DerefMut<Target = HeapAllocator<PAGE_SIZE>>;

    /// Returns a mutable reference to a [`HeapAllocator`].
    fn retrieve_allocator() -> Self::AllocatorRef;
}

#[derive(Default)]
pub struct GlobalHeapAllocator<P, H, const PAGE_SIZE: usize> {
    _p: PhantomData<P>,
    _h: PhantomData<H>,
}

impl<P, H, const PAGE_SIZE: usize> GlobalHeapAllocator<P, H, PAGE_SIZE> {
    pub const fn new() -> Self {
        Self {
            _p: PhantomData,
            _h: PhantomData,
        }
    }
}

unsafe impl<P, H, const PAGE_SIZE: usize> GlobalAlloc for GlobalHeapAllocator<P, H, PAGE_SIZE>
where
    P: RetrievePageFrameAllocator<PAGE_SIZE>,
    H: RetrieveHeapAllocator<PAGE_SIZE>,
{
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        let mut page_alloc = P::retrieve_allocator();
        let mut heap_alloc = H::retrieve_allocator();
        unsafe { heap_alloc.alloc(&mut page_alloc, layout) }
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        let mut heap_alloc = H::retrieve_allocator();
        unsafe { heap_alloc.dealloc(ptr, layout) }
    }
}

#[cfg(test)]
mod tests {
    use core::cell::UnsafeCell;
    use std::collections::HashSet;

    use super::*;

    #[test]
    fn test_bin_index() {
        for i in 1..=MIN_ALLOC_SIZE {
            assert_eq!(bin(i), (16, 0), "i = {i:#x}({i})");
        }
        for i in MIN_ALLOC_SIZE + 1..=2 * MIN_ALLOC_SIZE {
            assert_eq!(bin(i), (32, 1), "i = {i:#x}({i})");
        }
        for i in 2 * MIN_ALLOC_SIZE + 1..=4 * MIN_ALLOC_SIZE {
            assert_eq!(bin(i), (64, 2), "i = {i:#x}({i})");
        }
        assert_eq!(bin(65), (128, 3));
        assert_eq!(bin(128), (128, 3));
        assert_eq!(bin(129), (256, 4));
        assert_eq!(bin(256), (256, 4));
        assert_eq!(bin(257), (512, 5));
        assert_eq!(bin(512), (512, 5));
        assert_eq!(bin(513), (1024, 6));
        assert_eq!(bin(1024), (1024, 6));
        assert_eq!(bin(1025), (2048, 7));
        assert_eq!(bin(2048), (2048, 7));
        assert_eq!(bin(2049), (4096, 8));
        assert_eq!(bin(4096), (4096, 8));
    }

    const PAGE_SIZE: usize = 64;

    #[repr(align(64))]
    struct Heap(UnsafeCell<[u8; PAGE_SIZE * 100]>);
    unsafe impl Sync for Heap {}

    #[test]
    fn test_allocator() {
        let heap = Heap(UnsafeCell::new([0; PAGE_SIZE * 100]));
        let heap_range = unsafe { (*heap.0.get()).as_mut_ptr_range() };

        let mut page_allocator = unsafe { PageFrameAllocator::<PAGE_SIZE>::new(heap_range) };
        let mut heap_allocator = HeapAllocator::<PAGE_SIZE>::new();

        let layout = Layout::from_size_align(16, 16).unwrap();
        let mut ptrs = vec![];
        let mut addrs = HashSet::new();
        for i in 0..(PAGE_SIZE / 16) * 100 {
            let ptr = unsafe { heap_allocator.alloc(&mut page_allocator, layout) };
            assert!(!ptr.is_null(), "i = {i}  @ {ptr:#p}");
            assert!(addrs.insert(ptr.addr()));
            assert_eq!(ptr.addr() % 16, 0);
            ptrs.push(ptr);
        }
        assert!(unsafe { heap_allocator.alloc(&mut page_allocator, layout) }.is_null());

        for p in ptrs {
            unsafe { heap_allocator.dealloc(p, layout) };
        }

        assert!(!unsafe { heap_allocator.alloc(&mut page_allocator, layout) }.is_null());
    }

    #[test]
    fn test_aligned_allocation() {
        let heap = Heap(UnsafeCell::new([0; PAGE_SIZE * 100]));
        let heap_range = unsafe { (*heap.0.get()).as_mut_ptr_range() };

        let mut page_allocator = unsafe { PageFrameAllocator::<PAGE_SIZE>::new(heap_range) };
        let mut heap_allocator = HeapAllocator::<PAGE_SIZE>::new();

        let layout = Layout::from_size_align(16, 64).unwrap();
        let mut ptrs = vec![];
        let mut addrs = HashSet::new();
        for i in 0..100 {
            let ptr = unsafe { heap_allocator.alloc(&mut page_allocator, layout) };
            assert!(!ptr.is_null(), "i = {i}  @ {ptr:#p}");
            assert!(addrs.insert(ptr.addr()));
            assert_eq!(ptr.addr() % 64, 0);
            ptrs.push(ptr);
        }
        assert!(unsafe { heap_allocator.alloc(&mut page_allocator, layout) }.is_null());

        for p in ptrs {
            unsafe { heap_allocator.dealloc(p, layout) };
        }

        assert!(!unsafe { heap_allocator.alloc(&mut page_allocator, layout) }.is_null());
    }

    #[test]
    fn test_mixed_size_allocation() {
        let heap = Heap(UnsafeCell::new([0; PAGE_SIZE * 100]));
        let heap_range = unsafe { (*heap.0.get()).as_mut_ptr_range() };

        let mut page_allocator = unsafe { PageFrameAllocator::<PAGE_SIZE>::new(heap_range) };
        let mut heap_allocator = HeapAllocator::<PAGE_SIZE>::new();

        let layout0 = Layout::from_size_align(16, 16).unwrap(); // 4 per page
        let layout1 = Layout::from_size_align(32, 32).unwrap(); // 2 per page
        let layout2 = Layout::from_size_align(64, 64).unwrap(); // 1 per page
        let mut ptrs0 = vec![];
        let mut ptrs1 = vec![];
        let mut ptrs2 = vec![];
        let mut addrs = HashSet::new();
        for i in 0..10 {
            let ptr = unsafe { heap_allocator.alloc(&mut page_allocator, layout0) };
            assert!(!ptr.is_null(), "i = {i}  @ {ptr:#p}");
            assert!(addrs.insert(ptr.addr()));
            assert_eq!(ptr.addr() % 16, 0);
            ptrs0.push(ptr);

            let ptr = unsafe { heap_allocator.alloc(&mut page_allocator, layout1) };
            assert!(!ptr.is_null(), "i = {i}  @ {ptr:#p}");
            assert!(addrs.insert(ptr.addr()));
            assert_eq!(ptr.addr() % 32, 0);
            ptrs1.push(ptr);

            let ptr = unsafe { heap_allocator.alloc(&mut page_allocator, layout2) };
            assert!(!ptr.is_null(), "i = {i}  @ {ptr:#p}");
            assert!(addrs.insert(ptr.addr()));
            assert_eq!(ptr.addr() % 64, 0);
            ptrs2.push(ptr);
        }
    }
}
