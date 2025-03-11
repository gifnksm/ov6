use alloc::sync::Arc;
use core::{
    alloc::{AllocError, Allocator, Layout},
    mem::MaybeUninit,
    ops::Deref,
    ptr::NonNull,
};

use once_init::OnceInit;
use ov6_kernel_params::NFILE;
use slab_allocator::{ArcInnerLayout, SlabAllocator};

use super::FileData;
use crate::{error::KernelError, sync::SpinLock};

type FileDataLayout = ArcInnerLayout<FileData>;

static ALLOCATOR: OnceInit<SpinLock<SlabAllocator<FileDataLayout>>> = OnceInit::new();

pub(super) fn init() {
    static mut FILE_DATA_MEMORY: [MaybeUninit<FileDataLayout>; NFILE] =
        [const { MaybeUninit::uninit() }; NFILE];

    unsafe {
        let start = (&raw mut FILE_DATA_MEMORY[0]).cast::<FileDataLayout>();
        let end = start.add(NFILE);
        let alloc = SlabAllocator::new(start..end);
        ALLOCATOR.init(SpinLock::new(alloc))
    }
}

#[derive(Clone)]
struct FileAllocator;

unsafe impl Allocator for FileAllocator {
    fn allocate(&self, layout: Layout) -> Result<NonNull<[u8]>, AllocError> {
        assert_eq!(layout, Layout::new::<FileDataLayout>());
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
pub(super) struct FileDataArc(Arc<FileData, FileAllocator>);

impl Deref for FileDataArc {
    type Target = FileData;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl FileDataArc {
    pub(super) fn try_new(data: FileData) -> Result<Self, KernelError> {
        let data = Arc::try_new_in(data, FileAllocator)
            .map_err(|AllocError| KernelError::TooManyOpenFilesSystem)?;
        Ok(Self(data))
    }
}
