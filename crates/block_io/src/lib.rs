//! Cache for block I/O.

#![feature(extract_if)]
#![cfg_attr(not(test), no_std)]

extern crate alloc;

use alloc::{boxed::Box, collections::linked_list::LinkedList, sync::Arc};
use dataview::{Pod, PodMethods as _};
use mutex_api::Mutex;

pub trait BlockDevice<const BLOCK_SIZE: usize> {
    type Error;

    fn read(&self, index: usize, data: &mut [u8; BLOCK_SIZE]) -> Result<(), Self::Error>;
    fn write(&self, index: usize, data: &[u8; BLOCK_SIZE]) -> Result<(), Self::Error>;
}

/// A buffer cache for block I/O.
pub struct BlockIoCache<Device, BufferListMutex> {
    device: Device,

    /// Linked list of all buffers, through prev/next.
    ///
    /// Sorted by how recently the buffer was used.
    /// `buffers.front()` is most recent, `buffesr.back()` is least.
    buffers: BufferListMutex,
}

pub struct BufferList<BlockDataMutex>(LinkedList<Arc<Block<BlockDataMutex>>>);

/// A block buffer.
struct Block<BlockDataMutex> {
    /// Block index.
    index: usize,

    /// Block data.
    data: BlockDataMutex,
}

pub struct BlockHandle<'a, Device, BufferListMutex, BlockDataMutex>
where
    BufferListMutex: Mutex<Data = BufferList<BlockDataMutex>>,
{
    index: usize,
    cache: &'a BlockIoCache<Device, BufferListMutex>,
    block: Arc<Block<BlockDataMutex>>,
}

/// A reference to a block buffer.
pub struct BlockGuard<
    'a,
    'b,
    Device,
    BufferListMutex,
    BlockDataMutex,
    const BLOCK_SIZE: usize,
    const VALID: bool,
> where
    BufferListMutex: Mutex<Data = BufferList<BlockDataMutex>>,
    BlockDataMutex: Mutex<Data = BlockData<BLOCK_SIZE>> + 'b,
{
    /// Block index.
    index: usize,

    /// Reference to the block I/O cache
    cache: &'a BlockIoCache<Device, BufferListMutex>,

    /// Reference to the block itself.
    block: Arc<Block<BlockDataMutex>>,

    /// Block data.
    data: BlockDataMutex::Guard<'b>,
}

/// A block cache data.
pub struct BlockData<const BLOCK_SIZE: usize> {
    index: usize,
    valid: bool,
    data: Box<[u8; BLOCK_SIZE]>,
}

impl<Device, BufferListMutex, BlockDataMutex, const BLOCK_SIZE: usize>
    BlockIoCache<Device, BufferListMutex>
where
    BufferListMutex: Mutex<Data = BufferList<BlockDataMutex>>,
    BlockDataMutex: Mutex<Data = BlockData<BLOCK_SIZE>>,
{
    pub fn new(device: Device) -> Self {
        Self {
            device,
            buffers: BufferListMutex::new(BufferList(LinkedList::new())),
        }
    }

    /// Initializes the block I/O cache with `num_block` buffers.
    ///
    /// # Panics
    ///
    /// Panics if:
    ///
    /// * `num_block` is 0.
    /// * The cache is already initialized.
    pub fn init(&self, num_block: usize) {
        assert!(num_block > 0);
        let mut buffers = self.buffers.lock();
        assert!(buffers.0.is_empty());

        // Create linked list of buffers
        for _ in 0..num_block {
            buffers.0.push_back(Arc::new(Block {
                index: usize::MAX,
                data: BlockDataMutex::new(BlockData {
                    index: usize::MAX,
                    valid: false,
                    data: Box::new([0; BLOCK_SIZE]),
                }),
            }))
        }
    }

    /// Returns a editable reference to the buffer with the given device number and block number.
    ///
    /// If the buffer is already in the cache, returns a reference to it.
    /// Otherwise, recycles the least recently used (LRU) unused buffer and returns a reference to it.
    /// If all buffers are in use, returns `None`.
    ///
    /// # Panic
    ///
    /// Panics if:
    ///
    /// * the buffer is not initialized
    pub fn try_get(
        &self,
        index: usize,
    ) -> Option<BlockHandle<'_, Device, BufferListMutex, BlockDataMutex>> {
        let mut buffers = self.buffers.lock();
        assert!(!buffers.0.is_empty());

        // Find the buffer with dev & block_no
        if let Some(buf) = buffers.0.iter().find(|b| b.index == index) {
            // NOTE: `buf.valid` may be `false` here.
            return Some(BlockHandle {
                index,
                cache: self,
                block: Arc::clone(buf),
            });
        }

        // Not cached.
        // Recycle the least recentrly used (LRU) unused buffer.
        if let Some(buf) = buffers.0.iter_mut().rev().find_map(|buf| {
            let buf_content = Arc::get_mut(buf)?;
            buf_content.index = index;
            Some(buf)
        }) {
            return Some(BlockHandle {
                index,
                cache: self,
                block: Arc::clone(buf),
            });
        }

        None
    }

    /// Returns a editable reference to the buffer with the given device number and block number.
    ///
    /// If the buffer is already in the cache, returns a reference to it.
    /// Otherwise, recycles the least recently used (LRU) unused buffer and returns a reference to it.
    /// If all buffers are in use, panics.
    ///
    /// # Panic
    ///
    /// Panics if:
    ///
    /// * the buffer is not initialized
    /// * all buffers are in use
    pub fn get(&self, index: usize) -> BlockHandle<'_, Device, BufferListMutex, BlockDataMutex> {
        match self.try_get(index) {
            Some(buf) => buf,
            None => panic!("block buffer exhausted"),
        }
    }
}

impl<Device, BufferListMutex, BlockDataMutex> Drop
    for BlockHandle<'_, Device, BufferListMutex, BlockDataMutex>
where
    BufferListMutex: Mutex<Data = BufferList<BlockDataMutex>>,
{
    fn drop(&mut self) {
        let mut buffers = self.cache.buffers.lock();
        let buf = buffers.0.extract_if(|buf| buf.index == self.index).next();
        if let Some(buf) = buf {
            buffers.0.push_front(buf);
        }
    }
}

impl<'a, Device, BufferListMutex, BlockDataMutex, const BLOCK_SIZE: usize>
    BlockHandle<'a, Device, BufferListMutex, BlockDataMutex>
where
    Device: BlockDevice<BLOCK_SIZE>,
    BufferListMutex: Mutex<Data = BufferList<BlockDataMutex>>,
    BlockDataMutex: Mutex<Data = BlockData<BLOCK_SIZE>> + 'a,
{
    pub fn index(&self) -> usize {
        self.index
    }

    pub unsafe fn pin(&self) {
        unsafe {
            Arc::increment_strong_count(&self.block);
        }
    }

    pub unsafe fn unpin(&self) {
        unsafe {
            Arc::decrement_strong_count(&self.block);
        }
    }

    pub fn lock<'b>(
        &'b mut self,
    ) -> BlockGuard<'a, 'b, Device, BufferListMutex, BlockDataMutex, BLOCK_SIZE, false> {
        let mut data = self.block.data.lock();

        if data.index != self.index {
            // data recycle occurred
            data.index = self.index;
            data.valid = false;
        }

        BlockGuard {
            index: self.index,
            cache: self.cache,
            block: Arc::clone(&self.block),
            data,
        }
    }
}

impl<'a, 'b, Device, BufferListMutex, BlockDataMutex, const BLOCK_SIZE: usize, const VALID: bool>
    BlockGuard<'a, 'b, Device, BufferListMutex, BlockDataMutex, BLOCK_SIZE, VALID>
where
    Device: BlockDevice<BLOCK_SIZE>,
    BufferListMutex: Mutex<Data = BufferList<BlockDataMutex>>,
    BlockDataMutex: Mutex<Data = BlockData<BLOCK_SIZE>> + 'a,
{
    /// Returns the block number.
    pub fn index(&self) -> usize {
        self.index
    }

    /// Reads the block from disk if cached data is not valid.
    pub fn read(
        mut self,
    ) -> Result<
        BlockGuard<'a, 'b, Device, BufferListMutex, BlockDataMutex, BLOCK_SIZE, true>,
        (Self, Device::Error),
    > {
        if !self.data.valid {
            if let Err(e) = self.cache.device.read(self.index, &mut self.data.data) {
                return Err((self, e));
            }
            self.data.valid = true;
        }

        Ok(BlockGuard {
            index: self.index,
            cache: self.cache,
            block: Arc::clone(&self.block),
            data: self.data,
        })
    }

    /// Sets the whole block data.
    pub fn set_data(
        mut self,
        data: &[u8],
    ) -> BlockGuard<'a, 'b, Device, BufferListMutex, BlockDataMutex, BLOCK_SIZE, true> {
        self.data.valid = true;
        self.data.data.copy_from_slice(data);
        BlockGuard {
            index: self.index,
            cache: self.cache,
            block: Arc::clone(&self.block),
            data: self.data,
        }
    }

    /// Fills the whole block data with zero.
    pub fn zeroed(
        mut self,
    ) -> BlockGuard<'a, 'b, Device, BufferListMutex, BlockDataMutex, BLOCK_SIZE, true> {
        self.data.valid = true;
        self.data.data.fill(0);
        BlockGuard {
            index: self.index,
            cache: self.cache,
            block: Arc::clone(&self.block),
            data: self.data,
        }
    }

    pub unsafe fn pin(&self) {
        unsafe {
            Arc::increment_strong_count(&self.block);
        }
    }

    pub unsafe fn unpin(&self) {
        unsafe {
            Arc::decrement_strong_count(&self.block);
        }
    }
}

impl<Device, BufferListMutex, BlockDataMutex, const BLOCK_SIZE: usize>
    BlockGuard<'_, '_, Device, BufferListMutex, BlockDataMutex, BLOCK_SIZE, true>
where
    Device: BlockDevice<BLOCK_SIZE>,
    BufferListMutex: Mutex<Data = BufferList<BlockDataMutex>>,
    BlockDataMutex: Mutex<Data = BlockData<BLOCK_SIZE>>,
{
    /// Returns a reference to the block data bytes.
    pub fn bytes(&self) -> &[u8; BLOCK_SIZE] {
        &self.data.data
    }

    /// Returns a mutable reference to the block data bytes.
    pub fn bytes_mut(&mut self) -> &mut [u8; BLOCK_SIZE] {
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
    /// # Panic
    ///
    /// Panics if cached data is not valid.
    pub fn write(&mut self) -> Result<(), Device::Error> {
        assert!(self.data.valid);
        self.cache.device.write(self.index, self.bytes())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use core::{
        convert::Infallible,
        ops::{Deref, DerefMut},
    };
    use std::sync::Arc;

    const BLOCK_SIZE: usize = 512;

    struct StdMutex<T>(std::sync::Mutex<T>);
    struct StdMutexGuard<'a, T>(std::sync::MutexGuard<'a, T>);

    impl<T> mutex_api::Mutex for StdMutex<T> {
        type Data = T;

        type Guard<'a>
            = StdMutexGuard<'a, T>
        where
            Self: 'a;

        fn new(data: Self::Data) -> Self {
            Self(std::sync::Mutex::new(data))
        }

        fn lock(&self) -> Self::Guard<'_> {
            StdMutexGuard(self.0.lock().unwrap())
        }
    }

    impl<T> Deref for StdMutexGuard<'_, T> {
        type Target = T;

        fn deref(&self) -> &Self::Target {
            &self.0
        }
    }

    impl<T> DerefMut for StdMutexGuard<'_, T> {
        fn deref_mut(&mut self) -> &mut Self::Target {
            &mut self.0
        }
    }

    #[derive(Clone)]
    struct MockDevice {
        data: Vec<Arc<StdMutex<MockData>>>,
    }

    struct MockData {
        data: [u8; BLOCK_SIZE],
        read: usize,
        write: usize,
    }

    type BlockIoCache = super::BlockIoCache<MockDevice, StdMutex<BufferList>>;
    type BufferList = super::BufferList<StdMutex<BlockData>>;
    type BlockData = super::BlockData<BLOCK_SIZE>;

    impl MockDevice {
        fn new(size: usize) -> Self {
            Self {
                data: (0..size)
                    .map(|_| {
                        Arc::new(Mutex::new(MockData {
                            data: [0; BLOCK_SIZE],
                            read: 0,
                            write: 0,
                        }))
                    })
                    .collect(),
            }
        }
    }

    impl BlockDevice<BLOCK_SIZE> for MockDevice {
        type Error = Infallible;

        fn read(&self, index: usize, data: &mut [u8; 512]) -> Result<(), Self::Error> {
            let mut mock = self.data[index].lock();
            mock.0.read += 1;
            data.copy_from_slice(&mock.0.data);
            Ok(())
        }

        fn write(&self, index: usize, data: &[u8; 512]) -> Result<(), Self::Error> {
            let mut mock = self.data[index].lock();
            mock.0.write += 1;
            mock.0.data.copy_from_slice(data);
            Ok(())
        }
    }

    #[test]
    fn test_block_io_cache_init() {
        let device = MockDevice::new(10);
        let cache = BlockIoCache::new(device);
        cache.init(5);
        let buffers = cache.buffers.lock();
        assert_eq!(buffers.0.0.len(), 5);
    }

    #[test]
    #[should_panic]
    fn test_block_io_cache_init_zero() {
        let device = MockDevice::new(10);
        let cache = BlockIoCache::new(device);
        cache.init(0);
    }

    #[test]
    fn test_block_io_cache_get() {
        let device = MockDevice::new(10);
        let cache = BlockIoCache::new(device.clone());
        cache.init(5);

        let block = cache.get(0);
        assert_eq!(block.index(), 0);

        // `cache::get()` does not read the block from the device.
        assert_eq!(device.data[0].lock().0.read, 0);
        assert_eq!(device.data[0].lock().0.write, 0);
    }

    #[test]
    fn test_block_io_cache_read_write() {
        let device = MockDevice::new(10);
        let cache = BlockIoCache::new(device.clone());
        cache.init(5);

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
        assert_eq!(device.data[0].lock().0.read, 1);
        assert_eq!(device.data[0].lock().0.write, 1);
    }

    #[test]
    fn test_block_io_cache_exhaustion() {
        let device = MockDevice::new(10);
        let cache = BlockIoCache::new(device);
        cache.init(1);

        {
            let _block1 = cache.get(0);
            assert!(cache.try_get(1).is_none());
        }

        let _block2 = cache.get(1);
    }

    #[test]
    fn test_block_io_cache_drop_from_old() {
        let device = MockDevice::new(10);
        let cache = BlockIoCache::new(device.clone());
        cache.init(5);

        for i in 0..10 {
            let mut block = cache.get(i);
            let Ok(_block) = block.lock().read();
        }
        // cache: 9 -> 8 -> 7 -> 6 -> 5

        // data is read from the device only once.
        for i in 0..10 {
            assert_eq!(device.data[i].lock().0.read, 1);
            assert_eq!(device.data[i].lock().0.write, 0);
        }

        // The least recently used buffer is recycled.
        let mut block = cache.get(0);
        let Ok(block) = block.lock().read(); // 0 is not cached, drops 5
        assert_eq!(device.data[0].lock().0.read, 2);
        drop(block);
        // cache: 0 -> 9 -> 8 -> 7 -> 6

        let x = (*cache.buffers.lock())
            .0
            .iter()
            .map(|block| block.index)
            .collect::<alloc::vec::Vec<_>>();
        println!("{x:?}");

        let mut block = cache.get(8);
        let Ok(block) = block.lock().read(); // 8 is cached
        assert_eq!(device.data[8].lock().0.read, 1);
        drop(block);
        // cache: 8 -> 0 -> 9 -> 7 -> 6

        let mut block = cache.get(3);
        let Ok(block) = block.lock().read(); // 3 is not cached, drops 6
        assert_eq!(device.data[3].lock().0.read, 2);
        drop(block);
        // cache: 3 -> 8 -> 0 -> 9 -> 7

        for (i, n) in [(3, 2), (8, 1), (0, 2), (9, 1), (7, 1)] {
            let mut block = cache.get(i);
            let Ok(_block) = block.lock().read();
            assert_eq!(device.data[i].lock().0.read, n);
        }
    }

    #[test]
    fn test_block_io_cache_pin_unpin() {
        let device = MockDevice::new(10);
        let cache = BlockIoCache::new(device.clone());
        cache.init(5);

        for i in 0..10 {
            let mut block = cache.get(i);
            let Ok(_block) = block.lock().read();
        }
        // cache: 9 -> 8 -> 7 -> 6 -> 5
        let mut block = cache.get(5);
        unsafe {
            block.pin();
        }
        let Ok(block) = block.lock().read();
        drop(block);

        for i in 0..10 {
            let mut block = cache.get(i);
            let Ok(_block) = block.lock().read();
        }

        for i in 0..10 {
            let n = if i == 5 { 1 } else { 2 };
            assert_eq!(device.data[i].lock().0.read, n);
        }
    }
}
