use core::alloc::GlobalAlloc;

struct PanicAllocator;

unsafe impl GlobalAlloc for PanicAllocator {
    unsafe fn alloc(&self, _layout: core::alloc::Layout) -> *mut u8 {
        panic!("global alloc is not supported")
    }

    unsafe fn dealloc(&self, _ptr: *mut u8, _layout: core::alloc::Layout) {
        panic!("global alloc is not supported")
    }
}

#[global_allocator]
static GLOBAL_ALLOCATOR: PanicAllocator = PanicAllocator;
