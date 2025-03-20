//! LRU (Lease Recently Used) cache for block I/O.
#![feature(allocator_api)]
#![cfg_attr(not(test), no_std)]

extern crate alloc;

use alloc::alloc::Global;
use core::alloc::Allocator;

use dataview::{Pod, PodMethods as _};
use lru::Lru;
use mutex_api::Mutex;

/// A trait representing a block device with a fixed block size.
///
/// # Constants
///
/// * `BLOCK_SIZE`: The size of each block in bytes.
pub trait BlockDevice<const BLOCK_SIZE: usize> {
    /// The error type that can be returned by the block device operations.
    type Error;

    /// Reads a block of data from the device at the specified index into the
    /// provided buffer.
    ///
    /// Returns `Ok(())` if the read operation is successful, or an error of
    /// type `Self::Error` if it fails.
    fn read(&self, block_index: usize, data: &mut [u8; BLOCK_SIZE]) -> Result<(), Self::Error>;

    /// Writes a block of data to the device at the specified index from the
    /// provided buffer.
    ///
    /// Returns `Ok(())` if the write operation is successful, or an error of
    /// type `Self::Error` if it fails.
    fn write(&self, block_index: usize, data: &[u8; BLOCK_SIZE]) -> Result<(), Self::Error>;
}

/// A LRU (Least Recently Used) cache for block I/O.
pub struct BlockIoCache<Device, LruMutex> {
    device: Device,
    lru: Lru<LruMutex>,
}

/// A type alias for an LRU (Least Recently Used) map where the keys are block
/// indices.
///
/// This type alias simplifies the usage of the [`lru::LruMap`] type with block
/// indices.
///
/// # Type Parameters
///
/// * `BlockMutex`: The mutex type used to protect access to the block data.
pub type LruMap<BlockMutex, A = Global> = lru::LruMap<usize, BlockMutex, A>;

/// Allocation layout for [`LruMap`].
pub type LruMapALlocLayout<BlockMutex, A = Global> = lru::LruMapAllocLayout<usize, BlockMutex, A>;

/// A type alias for an LRU (Least Recently Used) value in the cache.
///
/// This type alias simplifies the usage of the [`lru::LruValue`] type with
/// block indices and block data.
///
/// # Type Parameters
///
/// * `'list`: The lifetime of the LRU list.
/// * `LruMutex`: The mutex type used to protect access to the LRU list.
/// * `BlockMutex`: The mutex type used to protect access to the block data.
pub type LruValue<'list, LruMutex, BlockMutex, A = Global> =
    lru::LruValue<'list, LruMutex, usize, BlockMutex, A>;

/// Allocation layout for [`LruValue`].
pub type LruValueAllocLayout<BlockMutex> = lru::LruValueAllocLayout<BlockMutex>;

/// A reference to a block cache.
pub struct BlockRef<'list, Device, LruMutex, BlockMutex, A = Global>
where
    LruMutex: Mutex<Data = LruMap<BlockMutex, A>>,
    A: Allocator,
{
    index: usize,
    device: &'list Device,
    block: LruValue<'list, LruMutex, BlockMutex, A>,
}

/// A lock guard of a block cache providing exclusive access.
pub struct BlockGuard<
    'list,
    'block,
    Device,
    LruMutex,
    BlockMutex,
    const BLOCK_SIZE: usize,
    const VALID: bool,
    A = Global,
> where
    LruMutex: Mutex<Data = LruMap<BlockMutex, A>>,
    BlockMutex: Mutex<Data = BlockData<BLOCK_SIZE>> + 'block,
    A: Allocator,
{
    index: usize,
    device: &'list Device,
    block: LruValue<'list, LruMutex, BlockMutex, A>,
    data: BlockMutex::Guard<'block>,
}

/// A cached data of a block.
pub struct BlockData<const BLOCK_SIZE: usize> {
    index: usize,
    valid: bool,
    dirty: bool,
    data: [u8; BLOCK_SIZE],
}

impl<const BLOCK_SIZE: usize> Default for BlockData<BLOCK_SIZE> {
    fn default() -> Self {
        Self {
            index: 0,
            valid: false,
            dirty: true,
            data: [0; BLOCK_SIZE],
        }
    }
}

impl<Device, LruMutex, BlockMutex, const BLOCK_SIZE: usize> BlockIoCache<Device, LruMutex>
where
    LruMutex: Mutex<Data = LruMap<BlockMutex>>,
    BlockMutex: Mutex<Data = BlockData<BLOCK_SIZE>> + Default,
{
    /// Creates a new [`BlockIoCache`] instance.
    ///
    /// # Panics
    ///
    /// Panics if `num_block` is `0`.
    pub fn new(device: Device, num_block: usize) -> Self {
        Self {
            device,
            lru: Lru::new(num_block),
        }
    }
}

impl<Device, LruMutex, BlockMutex, const BLOCK_SIZE: usize, A> BlockIoCache<Device, LruMutex>
where
    LruMutex: Mutex<Data = LruMap<BlockMutex, A>>,
    BlockMutex: Mutex<Data = BlockData<BLOCK_SIZE>> + Default,
    A: Allocator + Clone,
{
    /// Creates a new [`BlockIoCache`] instance.
    ///
    /// # Panics
    ///
    /// Panics if `num_block` is `0`.
    pub fn new_in(device: Device, num_block: usize, alloc: A) -> Self {
        Self {
            device,
            lru: Lru::new_in(num_block, alloc),
        }
    }

    /// Returns a reference to the cached block with the given block index.
    ///
    /// If the block is cached, returns a reference to it.
    /// Otherwise, the value is not cached, recycles the least recently used
    /// (LRU) unreferenced cache and returns a reference to it.
    /// If all caches are referenced, returns `None`.
    pub fn try_get(&self, index: usize) -> Option<BlockRef<'_, Device, LruMutex, BlockMutex, A>> {
        let block = self.lru.get(index)?;
        Some(BlockRef {
            index,
            device: &self.device,
            block,
        })
    }

    /// Returns a reference to the cached block with the given block index.
    ///
    /// If the block is cached, returns a reference to it.
    /// Otherwise, the value is not cached, recycles the least recently used
    /// (LRU) unreferenced cache and returns a reference to it.
    /// If all caches are referenced, panics.
    ///
    /// # Panics
    ///
    /// Panics if all buffers are referenced.
    pub fn get(&self, index: usize) -> BlockRef<'_, Device, LruMutex, BlockMutex, A> {
        let Some(buf) = self.try_get(index) else {
            panic!("block buffer exhausted");
        };
        buf
    }
}

impl<'list, Device, LruMutex, BlockMutex, const BLOCK_SIZE: usize, A>
    BlockRef<'list, Device, LruMutex, BlockMutex, A>
where
    Device: BlockDevice<BLOCK_SIZE>,
    LruMutex: Mutex<Data = LruMap<BlockMutex, A>>,
    BlockMutex: Mutex<Data = BlockData<BLOCK_SIZE>> + 'list,
    A: Allocator + Clone,
{
    /// Returns the index number of the block.
    pub fn index(&self) -> usize {
        self.index
    }

    /// Acquires a block's lock and provides a mutable exclusive access to it.
    pub fn lock<'b>(
        &'b mut self,
    ) -> BlockGuard<'list, 'b, Device, LruMutex, BlockMutex, BLOCK_SIZE, false, A> {
        let block = self.block.value();
        let mut block = block.lock();

        if block.index != self.index {
            // data recycle occurred
            block.index = self.index;
            block.valid = false;
        }

        BlockGuard {
            index: self.index,
            device: self.device,
            block: self.block.clone(),
            data: block,
        }
    }
}

impl<'list, Device, LruMutex, BlockMutex, const BLOCK_SIZE: usize, A> Clone
    for BlockRef<'list, Device, LruMutex, BlockMutex, A>
where
    Device: BlockDevice<BLOCK_SIZE>,
    LruMutex: Mutex<Data = LruMap<BlockMutex, A>>,
    BlockMutex: Mutex<Data = BlockData<BLOCK_SIZE>> + 'list,
    A: Allocator + Clone,
{
    fn clone(&self) -> Self {
        Self {
            index: self.index,
            device: self.device,
            block: self.block.clone(),
        }
    }
}

impl<'list, 'block, Device, LruMutex, BlockMutex, const BLOCK_SIZE: usize, const VALID: bool, A>
    BlockGuard<'list, 'block, Device, LruMutex, BlockMutex, BLOCK_SIZE, VALID, A>
where
    Device: BlockDevice<BLOCK_SIZE>,
    LruMutex: Mutex<Data = LruMap<BlockMutex, A>>,
    BlockMutex: Mutex<Data = BlockData<BLOCK_SIZE>> + 'list,
    A: Allocator + Clone,
{
    /// Returns the index number of the block.
    pub fn index(&self) -> usize {
        self.index
    }

    /// Returns the reference to the block cache.
    pub fn block(&self) -> BlockRef<'list, Device, LruMutex, BlockMutex, A> {
        BlockRef {
            index: self.index,
            device: self.device,
            block: self.block.clone(),
        }
    }

    /// Reads the block from disk if cached data is not valid.
    #[expect(clippy::type_complexity)]
    pub fn read(
        mut self,
    ) -> Result<
        BlockGuard<'list, 'block, Device, LruMutex, BlockMutex, BLOCK_SIZE, true, A>,
        (Self, Device::Error),
    > {
        if !self.data.valid {
            self.data.valid = true;
            self.data.dirty = false;
            if let Err(e) = self.device.read(self.index, &mut self.data.data) {
                return Err((self, e));
            }
        }

        Ok(BlockGuard {
            index: self.index,
            device: self.device,
            block: self.block.clone(),
            data: self.data,
        })
    }

    /// Sets the whole block data.
    pub fn set_data(
        mut self,
        data: &[u8],
    ) -> BlockGuard<'list, 'block, Device, LruMutex, BlockMutex, BLOCK_SIZE, true, A> {
        self.data.valid = true;
        self.data.dirty = true;
        self.data.data.copy_from_slice(data);
        BlockGuard {
            index: self.index,
            device: self.device,
            block: self.block.clone(),
            data: self.data,
        }
    }

    /// Fills the whole block data with zero.
    pub fn zeroed(
        mut self,
    ) -> BlockGuard<'list, 'block, Device, LruMutex, BlockMutex, BLOCK_SIZE, true, A> {
        self.data.valid = true;
        self.data.dirty = true;
        self.data.data.fill(0);
        BlockGuard {
            index: self.index,
            device: self.device,
            block: self.block.clone(),
            data: self.data,
        }
    }

    /// Returns `true` if the block cache is dirty.
    pub fn is_dirty(&self) -> bool {
        self.data.dirty
    }

    pub fn try_validate(
        self,
    ) -> Result<BlockGuard<'list, 'block, Device, LruMutex, BlockMutex, BLOCK_SIZE, true, A>, Self>
    {
        if self.data.valid {
            Ok(BlockGuard {
                index: self.index,
                device: self.device,
                block: self.block,
                data: self.data,
            })
        } else {
            Err(self)
        }
    }
}

impl<Device, LruMutex, BlockMutex, const BLOCK_SIZE: usize, A>
    BlockGuard<'_, '_, Device, LruMutex, BlockMutex, BLOCK_SIZE, true, A>
where
    Device: BlockDevice<BLOCK_SIZE>,
    LruMutex: Mutex<Data = LruMap<BlockMutex, A>>,
    BlockMutex: Mutex<Data = BlockData<BLOCK_SIZE>>,
    A: Allocator,
{
    /// Returns a reference to the bytes of block cache.
    pub fn bytes(&self) -> &[u8; BLOCK_SIZE] {
        &self.data.data
    }

    /// Returns a mutable reference to the bytes of block cache.
    pub fn bytes_mut(&mut self) -> &mut [u8; BLOCK_SIZE] {
        self.data.dirty = true;
        &mut self.data.data
    }

    /// Returns a reference to the block data as POD.
    pub fn data<T>(&self) -> &T
    where
        T: Pod,
    {
        self.bytes().as_data_view().get(0)
    }

    /// Returns a mutable reference to the block data as POD.
    pub fn data_mut<T>(&mut self) -> &mut T
    where
        T: Pod,
    {
        self.bytes_mut().as_data_view_mut().get_mut(0)
    }

    /// Writes the block to disk.
    ///
    /// # Panics
    ///
    /// Panics if cached data is not valid.
    pub fn write(&mut self) -> Result<(), Device::Error> {
        assert!(self.data.valid);
        self.device.write(self.index, self.bytes())?;
        self.data.dirty = false;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use core::{convert::Infallible, iter};
    use std::sync::{Arc, Mutex};

    use super::*;

    const BLOCK_SIZE: usize = 512;

    #[derive(Clone)]
    struct MockDevice {
        data: Vec<Arc<Mutex<MockData>>>,
    }

    struct MockData {
        data: [u8; BLOCK_SIZE],
        read: usize,
        write: usize,
    }

    impl Default for MockData {
        fn default() -> Self {
            Self {
                data: [0; BLOCK_SIZE],
                read: 0,
                write: 0,
            }
        }
    }

    type BlockIoCache = super::BlockIoCache<MockDevice, Mutex<LruList>>;
    type LruList = super::LruMap<Mutex<BlockData>>;
    type BlockData = super::BlockData<BLOCK_SIZE>;

    impl MockDevice {
        fn new(size: usize) -> Self {
            Self {
                data: iter::repeat_with(|| {
                    Arc::new(Mutex::new(MockData {
                        data: [0; BLOCK_SIZE],
                        read: 0,
                        write: 0,
                    }))
                })
                .take(size)
                .collect(),
            }
        }
    }

    impl BlockDevice<BLOCK_SIZE> for MockDevice {
        type Error = Infallible;

        fn read(&self, block_index: usize, data: &mut [u8; 512]) -> Result<(), Self::Error> {
            let mut mock = self.data[block_index].lock().unwrap();
            mock.read += 1;
            data.copy_from_slice(&mock.data);
            Ok(())
        }

        fn write(&self, block_index: usize, data: &[u8; 512]) -> Result<(), Self::Error> {
            let mut mock = self.data[block_index].lock().unwrap();
            mock.write += 1;
            mock.data.copy_from_slice(data);
            Ok(())
        }
    }

    #[test]
    #[should_panic(expected = "size must be greater than 0")]
    fn test_block_io_cache_init_zero() {
        let device = MockDevice::new(10);
        BlockIoCache::new(device, 0);
    }

    #[test]
    fn test_block_io_cache_get() {
        let device = MockDevice::new(10);
        let cache = BlockIoCache::new(device.clone(), 5);

        let block = cache.get(0);
        assert_eq!(block.index(), 0);

        // `cache::get()` does not read the block from the device.
        assert_eq!(device.data[0].lock().unwrap().read, 0);
        assert_eq!(device.data[0].lock().unwrap().write, 0);
    }

    #[test]
    fn test_block_io_cache_read_write() {
        let device = MockDevice::new(10);
        let cache = BlockIoCache::new(device.clone(), 5);

        {
            let mut block = cache.get(0);
            let Ok(mut block) = block.lock().read();
            block.bytes_mut().copy_from_slice(&[1; 512]);
            block.write().unwrap();
        }

        {
            let mut block = cache.get(0);
            let Ok(block) = block.lock().read();
            assert_eq!(block.bytes(), &[1; 512]);
        }

        // data is read from the device only once.
        assert_eq!(device.data[0].lock().unwrap().read, 1);
        assert_eq!(device.data[0].lock().unwrap().write, 1);
    }

    #[test]
    fn test_block_io_cache_exhaustion() {
        let device = MockDevice::new(10);
        let cache = BlockIoCache::new(device, 1);

        {
            let _block1 = cache.get(0);
            assert!(cache.try_get(1).is_none());
        }

        let _block2 = cache.get(1);
    }

    #[test]
    fn test_block_io_cache_drop_from_old() {
        let device = MockDevice::new(10);
        let cache = BlockIoCache::new(device.clone(), 5);

        for i in 0..10 {
            let mut block = cache.get(i);
            let Ok(_block) = block.lock().read();
        }
        // cache: 9 -> 8 -> 7 -> 6 -> 5

        // data is read from the device only once.
        for i in 0..10 {
            assert_eq!(device.data[i].lock().unwrap().read, 1);
            assert_eq!(device.data[i].lock().unwrap().write, 0);
        }

        // The least recently used buffer is recycled.
        let mut block = cache.get(0);
        let Ok(block) = block.lock().read(); // 0 is not cached, drops 5
        assert_eq!(device.data[0].lock().unwrap().read, 2);
        drop(block);
        // cache: 0 -> 9 -> 8 -> 7 -> 6

        let mut block = cache.get(8);
        let Ok(block) = block.lock().read(); // 8 is cached
        assert_eq!(device.data[8].lock().unwrap().read, 1);
        drop(block);
        // cache: 8 -> 0 -> 9 -> 7 -> 6

        let mut block = cache.get(3);
        let Ok(block) = block.lock().read(); // 3 is not cached, drops 6
        assert_eq!(device.data[3].lock().unwrap().read, 2);
        drop(block);
        // cache: 3 -> 8 -> 0 -> 9 -> 7

        for (i, n) in [(3, 2), (8, 1), (0, 2), (9, 1), (7, 1)] {
            let mut block = cache.get(i);
            let Ok(_block) = block.lock().read();
            assert_eq!(device.data[i].lock().unwrap().read, n);
        }
    }
}
