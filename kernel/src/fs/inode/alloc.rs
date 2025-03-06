use core::{
    alloc::{AllocError, Allocator, Layout},
    mem::MaybeUninit,
    ops::Deref,
    ptr::NonNull,
};

use alloc::sync::{Arc, Weak};
use once_init::OnceInit;
use ov6_kernel_params::NINODE;
use slab_allocator::{ArcInnerLayout, SlabAllocator};

use crate::{
    error::KernelError,
    sync::{SleepLock, SpinLock},
};

use super::InodeData;

type InodeDataLayout = ArcInnerLayout<SleepLock<Option<InodeData>>>;

static ALLOCATOR: OnceInit<SpinLock<SlabAllocator<InodeDataLayout>>> = OnceInit::new();

pub(super) fn init() {
    static mut INODE_DATA_MEMORY: [MaybeUninit<InodeDataLayout>; NINODE] =
        [const { MaybeUninit::uninit() }; NINODE];

    unsafe {
        let start = (&raw mut INODE_DATA_MEMORY[0]).cast::<InodeDataLayout>();
        let end = start.add(NINODE);
        let alloc = SlabAllocator::new(start..end);
        ALLOCATOR.init(SpinLock::new(alloc))
    }
}

#[derive(Clone)]
struct InodeDataAllocator;

unsafe impl Allocator for InodeDataAllocator {
    fn allocate(&self, layout: Layout) -> Result<NonNull<[u8]>, AllocError> {
        assert_eq!(layout, Layout::new::<InodeDataLayout>());
        let Some(ptr) = ALLOCATOR.get().lock().allocate() else {
            return Err(AllocError);
        };
        Ok(NonNull::slice_from_raw_parts(ptr.cast(), layout.size()))
    }

    unsafe fn deallocate(&self, ptr: NonNull<u8>, _layout: Layout) {
        unsafe { ALLOCATOR.get().lock().deallocate(ptr.cast()) }
    }
}

#[derive(Clone)]
pub(super) struct InodeDataArc(Arc<SleepLock<Option<InodeData>>, InodeDataAllocator>);

pub(super) struct InodeDataWeak(Weak<SleepLock<Option<InodeData>>, InodeDataAllocator>);

impl Deref for InodeDataArc {
    type Target = SleepLock<Option<InodeData>>;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl InodeDataArc {
    pub(super) fn try_new(data: SleepLock<Option<InodeData>>) -> Result<Self, KernelError> {
        let data =
            Arc::try_new_in(data, InodeDataAllocator).map_err(|AllocError| KernelError::Unknown)?;
        Ok(Self(data))
    }

    pub(super) fn strong_count(this: &Self) -> usize {
        Arc::strong_count(&this.0)
    }

    pub(super) fn downgrade(this: &Self) -> InodeDataWeak {
        InodeDataWeak(Arc::downgrade(&this.0))
    }
}

impl InodeDataWeak {
    pub(super) fn upgrade(this: &Self) -> Option<InodeDataArc> {
        this.0.upgrade().map(InodeDataArc)
    }
}
