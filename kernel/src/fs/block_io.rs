//! Cache for block I/O.

use core::{
    alloc::{AllocError, Allocator, Layout},
    convert::Infallible,
    mem::MaybeUninit,
    ptr::NonNull,
};

use block_io::{BlockData, BlockDevice, BlockIoCache, LruMap};
use once_init::OnceInit;
use ov6_kernel_params::LOG_SIZE;
use slab_allocator::SlabAllocator;

use crate::{
    param::NBUF,
    sync::{SleepLock, SpinLock},
};

use super::{DeviceNo, repr::FS_BLOCK_SIZE, virtio_disk};

pub(super) struct VirtioDiskDevice {}

impl BlockDevice<FS_BLOCK_SIZE> for VirtioDiskDevice {
    type Error = Infallible;

    fn read(&self, block_index: usize, data: &mut [u8; FS_BLOCK_SIZE]) -> Result<(), Self::Error> {
        virtio_disk::read(block_index * FS_BLOCK_SIZE, data);
        Ok(())
    }

    fn write(&self, block_index: usize, data: &[u8; FS_BLOCK_SIZE]) -> Result<(), Self::Error> {
        virtio_disk::write(block_index * FS_BLOCK_SIZE, data);
        Ok(())
    }
}

type BlockMutex = SleepLock<BlockData<FS_BLOCK_SIZE>>;
type LruMutex = SpinLock<LruMap<BlockMutex, BlockAllocator>>;

type LruMapAllocLayout = block_io::LruMapALlocLayout<BlockMutex, BlockAllocator>;
type LruValueAllocLayout = block_io::LruValueAllocLayout<BlockMutex>;

static VIRTIO_DISK_CACHE: OnceInit<BlockIoCache<VirtioDiskDevice, LruMutex>> = OnceInit::new();
static LRU_MAP_ALLOCATOR: OnceInit<SpinLock<SlabAllocator<LruMapAllocLayout>>> = OnceInit::new();
static LRU_VALUE_ALLOCATOR: OnceInit<SpinLock<SlabAllocator<LruValueAllocLayout>>> =
    OnceInit::new();

pub(super) type BlockRef =
    block_io::BlockRef<'static, VirtioDiskDevice, LruMutex, BlockMutex, BlockAllocator>;

pub(super) type BlockGuard<'block, const VALID: bool> = block_io::BlockGuard<
    'static,
    'block,
    VirtioDiskDevice,
    LruMutex,
    BlockMutex,
    FS_BLOCK_SIZE,
    VALID,
    BlockAllocator,
>;

/// Initializes the global block I/O cache.
pub(super) fn init() {
    static mut LRU_MAP_MEMORY: [MaybeUninit<LruMapAllocLayout>; LOG_SIZE] =
        [const { MaybeUninit::uninit() }; LOG_SIZE];
    static mut LRU_VALUE_MEMORY: [MaybeUninit<LruValueAllocLayout>; LOG_SIZE] =
        [const { MaybeUninit::uninit() }; LOG_SIZE];

    unsafe {
        let start = (&raw mut LRU_MAP_MEMORY[0]).cast::<LruMapAllocLayout>();
        let end = start.add(LOG_SIZE);
        let alloc = SlabAllocator::new(start..end);
        LRU_MAP_ALLOCATOR.init(SpinLock::new(alloc));
    }

    unsafe {
        let start = (&raw mut LRU_VALUE_MEMORY[0]).cast::<LruValueAllocLayout>();
        let end = start.add(LOG_SIZE);
        let alloc = SlabAllocator::new(start..end);
        LRU_VALUE_ALLOCATOR.init(SpinLock::new(alloc));
    }

    VIRTIO_DISK_CACHE.init(BlockIoCache::new_in(
        VirtioDiskDevice {},
        NBUF,
        BlockAllocator,
    ));
}

/// Gets the block buffer with the given device number and block number.
pub(super) fn get(dev: DeviceNo, block_index: usize) -> BlockRef {
    match dev {
        DeviceNo::ROOT => VIRTIO_DISK_CACHE.get().get(block_index),
        _ => panic!("unknown device: dev={}", dev.value()),
    }
}

#[derive(Clone)]
pub(super) struct BlockAllocator;

unsafe impl Allocator for BlockAllocator {
    fn allocate(&self, layout: Layout) -> Result<NonNull<[u8]>, AllocError> {
        let ptr = if layout == Layout::new::<LruMapAllocLayout>() {
            let Some(ptr) = LRU_MAP_ALLOCATOR.get().lock().allocate() else {
                return Err(AllocError);
            };
            NonNull::slice_from_raw_parts(ptr.cast(), layout.size())
        } else if layout == Layout::new::<LruValueAllocLayout>() {
            let Some(ptr) = LRU_VALUE_ALLOCATOR.get().lock().allocate() else {
                return Err(AllocError);
            };
            NonNull::slice_from_raw_parts(ptr.cast(), layout.size())
        } else {
            panic!("Unexpected layout")
        };
        Ok(ptr)
    }

    unsafe fn deallocate(&self, ptr: NonNull<u8>, layout: Layout) {
        if layout == Layout::new::<LruMapAllocLayout>() {
            unsafe { LRU_MAP_ALLOCATOR.get().lock().deallocate(ptr.cast()) }
        } else if layout == Layout::new::<LruValueAllocLayout>() {
            unsafe { LRU_VALUE_ALLOCATOR.get().lock().deallocate(ptr.cast()) }
        } else {
            panic!("Unexpected layout")
        }
    }
}
