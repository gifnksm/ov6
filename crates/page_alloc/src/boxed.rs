use core::{
    marker::PhantomData,
    num::NonZero,
    ops::{Deref, DerefMut},
    ptr::{self, NonNull},
};

use crate::RetrievePageFrameAllocator;

/// A pointer type that uniquely owns a page of type `T`.
pub struct PageBox<T, A, const PAGE_SIZE: usize>
where
    A: RetrievePageFrameAllocator<PAGE_SIZE>,
{
    ptr: NonNull<T>,
    _allocator: PhantomData<A>,
}

impl<T, A, const PAGE_SIZE: usize> Deref for PageBox<T, A, PAGE_SIZE>
where
    A: RetrievePageFrameAllocator<PAGE_SIZE>,
{
    type Target = T;

    fn deref(&self) -> &Self::Target {
        unsafe { self.ptr.as_ref() }
    }
}

impl<T, A, const PAGE_SIZE: usize> DerefMut for PageBox<T, A, PAGE_SIZE>
where
    A: RetrievePageFrameAllocator<PAGE_SIZE>,
{
    fn deref_mut(&mut self) -> &mut Self::Target {
        unsafe { self.ptr.as_mut() }
    }
}

impl<T, A, const PAGE_SIZE: usize> PageBox<T, A, PAGE_SIZE>
where
    A: RetrievePageFrameAllocator<PAGE_SIZE>,
{
    /// Allocates a page and then places `x` into it.
    pub fn new(x: T) -> Self {
        Self::try_new(x).unwrap()
    }

    /// Allocates a page and then places `value` into it, returning an error if the allocation fails.
    pub fn try_new(value: T) -> Option<Self> {
        assert!(size_of::<T>() < PAGE_SIZE);
        assert_eq!(PAGE_SIZE % align_of::<T>(), 0);

        let mut allocator = A::retrieve_allocator();
        let ptr = allocator.alloc()?.cast();
        unsafe {
            ptr.write(value);
        }
        Some(Self {
            ptr,
            _allocator: PhantomData,
        })
    }

    /// Returns a raw pointer to the `PageBox`'s contents.
    pub fn as_ptr(this: &Self) -> *const T {
        this.ptr.as_ptr()
    }

    /// Returns a mutable raw pointer to the `PageBox`'s contents.
    pub fn as_mut_ptr(this: &mut Self) -> *mut T {
        this.ptr.as_ptr()
    }

    /// Returns the address of the `PageBox`'s contents.
    pub fn addr(this: &Self) -> NonZero<usize> {
        this.ptr.addr()
    }
}

impl<T, A, const PAGE_SIZE: usize> Drop for PageBox<T, A, PAGE_SIZE>
where
    A: RetrievePageFrameAllocator<PAGE_SIZE>,
{
    fn drop(&mut self) {
        let mut allocator = A::retrieve_allocator();
        unsafe {
            ptr::drop_in_place(self.ptr.as_ptr());
            allocator.free(self.ptr.cast());
        }
    }
}

#[cfg(test)]
mod tests {
    use core::cell::UnsafeCell;
    use std::sync::{Mutex, MutexGuard, OnceLock};

    use super::*;
    use crate::PageFrameAllocator;

    const PAGE_SIZE: usize = 64;

    static ALLOCATOR: OnceLock<Mutex<PageFrameAllocator<PAGE_SIZE>>> = OnceLock::new();

    #[repr(align(64))]
    struct Heap(UnsafeCell<[u8; PAGE_SIZE * 100]>);
    unsafe impl Sync for Heap {}

    static HEAP: Heap = Heap(UnsafeCell::new([0; PAGE_SIZE * 100]));

    struct Retriever;
    impl RetrievePageFrameAllocator<PAGE_SIZE> for Retriever {
        type AllocatorRef = MutexGuard<'static, PageFrameAllocator<PAGE_SIZE>>;

        fn retrieve_allocator() -> Self::AllocatorRef {
            ALLOCATOR.get().unwrap().lock().unwrap()
        }
    }

    type MyPageBox<T> = PageBox<T, Retriever, PAGE_SIZE>;

    #[test]
    fn test_page_box() {
        ALLOCATOR
            .set(Mutex::new(unsafe {
                PageFrameAllocator::new((*HEAP.0.get()).as_mut_ptr_range())
            }))
            .unwrap();

        let page = MyPageBox::new(0);
        assert_eq!(PageBox::addr(&page).get() % PAGE_SIZE, 0);
        assert_eq!(*page, 0);
        drop(page); // page must be freed.

        let mut pages = vec![];
        for i in 0..100 {
            pages.push(MyPageBox::new(i));
        }
        assert!(MyPageBox::try_new(1000).is_none());

        for _ in 0..50 {
            pages.pop();
        }
        for i in 0..50 {
            pages.push(MyPageBox::new(i));
        }
        assert!(MyPageBox::try_new(1000).is_none());
    }
}
