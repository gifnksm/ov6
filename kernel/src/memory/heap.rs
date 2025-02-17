use page_alloc::heap_allocator::{GlobalHeapAllocator, HeapAllocator, RetrieveHeapAllocator};

use crate::sync::{SpinLock, SpinLockGuard};

use super::{page::PageFrameAllocatorRetriever, vm::PAGE_SIZE};

static HEAP_ALLOCATOR: SpinLock<HeapAllocator<PAGE_SIZE>> = SpinLock::new(HeapAllocator::new());

pub struct HeapAllocatorRetriever;
impl RetrieveHeapAllocator<PAGE_SIZE> for HeapAllocatorRetriever {
    type AllocatorRef = SpinLockGuard<'static, HeapAllocator<PAGE_SIZE>>;

    fn retrieve_allocator() -> Self::AllocatorRef {
        HEAP_ALLOCATOR.lock()
    }
}

#[global_allocator]
static GLOBAL_ALLOCATOR: GlobalHeapAllocator<
    PageFrameAllocatorRetriever,
    HeapAllocatorRetriever,
    PAGE_SIZE,
> = GlobalHeapAllocator::new();
