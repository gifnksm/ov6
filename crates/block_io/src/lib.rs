//! Cache for block I/O.

#![feature(extract_if)]
#![cfg_attr(not(test), no_std)]

extern crate alloc;

use core::sync::atomic::{AtomicBool, Ordering};

use alloc::{boxed::Box, collections::linked_list::LinkedList};
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

pub struct BufferList<BlockDataMutex>(LinkedList<BlockOwn<BlockDataMutex>>);

/// A block buffer.
struct BlockOwn<BlockDataMutex> {
    /// Block index.
    index: usize,

    /// Reference count.
    ///
    /// If > 0, the buffer is in use and guaranteed to be in the cache.
    /// If 0, the buffer is not used and eventually will be recycled.
    refcnt: u32,

    /// `true` if data has been read from disk.
    ///
    /// This is used to avoid reading the same block multiple times.
    /// This flag can be read and write without holding the `data`'s lock.
    valid: AtomicBool,

    /// Block data.
    data: BlockDataMutex,
}

/// A reference to a block buffer.
pub struct BlockRef<
    'a,
    Device,
    BufferListMutex,
    BlockDataMutex,
    const BLOCK_SIZE: usize,
    const VALID: bool,
> where
    BufferListMutex: Mutex<Data = BufferList<BlockDataMutex>>,
    BlockDataMutex: Mutex<Data = BlockData<BLOCK_SIZE>> + 'a,
{
    /// Block index.
    index: usize,

    /// Reference to the block I/O cache
    cache: &'a BlockIoCache<Device, BufferListMutex>,

    /// `true` if data has been read from disk.
    ///
    /// This is used to avoid reading the same block multiple times.
    /// This flag can be read and write without holding the `data`'s lock.
    valid: &'a AtomicBool,

    /// Block data.
    data: Option<BlockDataMutex::Guard<'a>>,
}

/// A block cache data.
pub struct BlockData<const BLOCK_SIZE: usize>(Box<[u8; BLOCK_SIZE]>);

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
            buffers.0.push_back(BlockOwn {
                index: usize::MAX,
                valid: AtomicBool::new(false),
                data: BlockDataMutex::new(BlockData(Box::new([0; BLOCK_SIZE]))),
                refcnt: 0,
            })
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
    ) -> Option<BlockRef<'_, Device, BufferListMutex, BlockDataMutex, BLOCK_SIZE, false>> {
        let mut buffers = self.buffers.lock();
        assert!(!buffers.0.is_empty());

        // Find the buffer with dev & block_no
        if let Some(buf) = buffers.0.iter_mut().find(|b| b.index == index) {
            // NOTE: `buf.valid` may be `false` here.
            unsafe {
                // Safety: `buf.refcnt > 0` here, so `valid` and `data` exists after unlock (`drop(bcache)`)
                buf.refcnt += 1;
                let valid = (&raw const buf.valid).as_ref().unwrap();
                let data = (&raw const buf.data).as_ref().unwrap();
                drop(buffers);

                let buf = BlockRef {
                    index,
                    cache: self,
                    valid,
                    data: Some(data.lock()),
                };
                return Some(buf);
            }
        }

        // Not cached.
        // Recycle the least recentrly used (LRU) unused buffer.
        if let Some(buf) = buffers.0.iter_mut().rev().find(|buf| buf.refcnt == 0) {
            buf.index = index;
            buf.valid.store(false, Ordering::Release);
            unsafe {
                // Safety: `buf.refcnt > 0` here, so `valid` and `data` exists after unlock (`drop(bcache)`)
                buf.refcnt = 1;
                let valid = (&raw const buf.valid).as_ref().unwrap();
                let data = (&raw const buf.data).as_ref().unwrap();
                drop(buffers);

                let buf = BlockRef {
                    index,
                    cache: self,
                    valid,
                    data: Some(data.lock()),
                };
                return Some(buf);
            }
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
    pub fn get(
        &self,
        index: usize,
    ) -> BlockRef<'_, Device, BufferListMutex, BlockDataMutex, BLOCK_SIZE, false> {
        match self.try_get(index) {
            Some(buf) => buf,
            None => panic!("block buffer exhausted"),
        }
    }
}

impl<'a, Device, BufferListMutex, BlockDataMutex, const BLOCK_SIZE: usize, const VALID: bool> Drop
    for BlockRef<'a, Device, BufferListMutex, BlockDataMutex, BLOCK_SIZE, VALID>
where
    BufferListMutex: Mutex<Data = BufferList<BlockDataMutex>>,
    BlockDataMutex: Mutex<Data = BlockData<BLOCK_SIZE>> + 'a,
{
    fn drop(&mut self) {
        // unlock
        if self.data.take().is_none() {
            // delegated to another BlockRef
            return;
        }

        let mut buffers = self.cache.buffers.lock();

        // decrement refcnt & extract element if refcnt == 0
        let Some(buf) = buffers
            .0
            .extract_if(|buf| {
                buf.index == self.index && {
                    assert!(buf.refcnt > 0);
                    buf.refcnt -= 1;
                    buf.refcnt == 0
                }
            })
            .next()
        else {
            return;
        };
        assert_eq!(buf.refcnt, 0);

        // no one is waiting for it, move to head of the most-recently-used list.
        buffers.0.push_front(buf);
    }
}

impl<'a, Device, BufferListMutex, BlockDataMutex, const BLOCK_SIZE: usize, const VALID: bool>
    BlockRef<'a, Device, BufferListMutex, BlockDataMutex, BLOCK_SIZE, VALID>
where
    Device: BlockDevice<BLOCK_SIZE>,
    BufferListMutex: Mutex<Data = BufferList<BlockDataMutex>>,
    BlockDataMutex: Mutex<Data = BlockData<BLOCK_SIZE>> + 'a,
{
    /// Returns the block number.
    pub fn index(&self) -> usize {
        self.index
    }
    /// Increments the reference count of the buffer.
    ///
    /// If the reference count is > 0, the buffer is in use and guaranteed to be in the cache.
    pub fn pin(&mut self) {
        let mut buffers = self.cache.buffers.lock();
        let buf = buffers
            .0
            .iter_mut()
            .find(|buf| buf.index == self.index)
            .expect("buffer should be found, because refcnt must be > 0");
        buf.refcnt = buf.refcnt.checked_add(1).unwrap();
    }

    /// Decrements the reference count of the buffer.
    ///
    /// # Safety
    ///
    /// The caller must ensure that the buffer is pinned before.
    /// Otherwise, the referenced buffer may be recycled and possibly causes data corruption.
    pub unsafe fn unpin(&mut self) {
        let mut buffers = self.cache.buffers.lock();
        let buf = buffers
            .0
            .iter_mut()
            .find(|buf| buf.index == self.index)
            .expect("buffer should be found, because refcnt must be > 0");
        assert!(buf.refcnt > 1); // When BufRef exists, refcnt must be > 0
        buf.refcnt -= 1;
    }

    /// Reads the block from disk if cached data is not valid.
    pub fn read(
        mut self,
    ) -> Result<
        BlockRef<'a, Device, BufferListMutex, BlockDataMutex, BLOCK_SIZE, true>,
        (Self, Device::Error),
    > {
        if !self.valid.load(Ordering::Relaxed) {
            if let Err(e) = self
                .cache
                .device
                .read(self.index, &mut self.data.as_mut().unwrap().0)
            {
                return Err((self, e));
            }
            self.valid.store(true, Ordering::Relaxed)
        }

        Ok(BlockRef {
            index: self.index,
            cache: self.cache,
            valid: self.valid,
            data: self.data.take(),
        })
    }

    /// Sets the whole block data.
    pub fn set_data(
        mut self,
        data: &[u8],
    ) -> BlockRef<'a, Device, BufferListMutex, BlockDataMutex, BLOCK_SIZE, true> {
        self.valid.store(true, Ordering::Relaxed);
        self.data.as_mut().unwrap().0.copy_from_slice(data);
        BlockRef {
            index: self.index,
            cache: self.cache,
            valid: self.valid,
            data: self.data.take(),
        }
    }

    /// Fills the whole block data with zero.
    pub fn zeroed(
        mut self,
    ) -> BlockRef<'a, Device, BufferListMutex, BlockDataMutex, BLOCK_SIZE, true> {
        self.valid.store(true, Ordering::Relaxed);
        self.data.as_mut().unwrap().0.fill(0);
        BlockRef {
            index: self.index,
            cache: self.cache,
            valid: self.valid,
            data: self.data.take(),
        }
    }
}

impl<Device, BufferListMutex, BlockDataMutex, const BLOCK_SIZE: usize>
    BlockRef<'_, Device, BufferListMutex, BlockDataMutex, BLOCK_SIZE, true>
where
    Device: BlockDevice<BLOCK_SIZE>,
    BufferListMutex: Mutex<Data = BufferList<BlockDataMutex>>,
    BlockDataMutex: Mutex<Data = BlockData<BLOCK_SIZE>>,
{
    /// Returns a reference to the block data bytes.
    pub fn bytes(&self) -> &[u8; BLOCK_SIZE] {
        &self.data.as_ref().unwrap().0
    }

    /// Returns a mutable reference to the block data bytes.
    pub fn bytes_mut(&mut self) -> &mut [u8; BLOCK_SIZE] {
        &mut self.data.as_mut().unwrap().0
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
        assert!(self.valid.load(Ordering::Relaxed));
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

        let block_ref = cache.get(0);
        assert_eq!(block_ref.index(), 0);

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
            let Ok(mut block_ref) = cache.get(0).read();
            block_ref.bytes_mut().copy_from_slice(&[1; 512]);
            block_ref.write().unwrap();
        }

        {
            let Ok(block_ref) = cache.get(0).read();
            assert_eq!(block_ref.bytes(), &[1; 512]);
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
            let _block_ref1 = cache.get(0);
            assert!(cache.try_get(1).is_none());
        }

        let _block_ref2 = cache.get(1);
    }

    #[test]
    fn test_block_io_cache_drop_from_old() {
        let device = MockDevice::new(10);
        let cache = BlockIoCache::new(device.clone());
        cache.init(5);

        for i in 0..10 {
            let _block_ref = cache.get(i).read();
        }
        // cache: 9 -> 8 -> 7 -> 6 -> 5

        // data is read from the device only once.
        for i in 0..10 {
            assert_eq!(device.data[i].lock().0.read, 1);
            assert_eq!(device.data[i].lock().0.write, 0);
        }

        // The least recently used buffer is recycled.
        let block_ref = cache.get(0).read(); // 0 is not cached, drops 5
        assert_eq!(device.data[0].lock().0.read, 2);
        drop(block_ref);
        // cache: 0 -> 9 -> 8 -> 7 -> 6

        let block_ref = cache.get(8).read(); // 8 is cached
        assert_eq!(device.data[8].lock().0.read, 1);
        drop(block_ref);
        // cache: 8 -> 0 -> 9 -> 7 -> 6

        let block_ref = cache.get(3).read(); // 3 is not cached, drops 6
        assert_eq!(device.data[3].lock().0.read, 2);
        drop(block_ref);
        // cache: 3 -> 8 -> 0 -> 9 -> 7

        for (i, n) in [(3, 2), (8, 1), (0, 2), (9, 1), (7, 1)] {
            let _block_ref = cache.get(i).read();
            assert_eq!(device.data[i].lock().0.read, n);
        }
    }

    #[test]
    fn test_block_io_cache_pin_unpin() {
        let device = MockDevice::new(10);
        let cache = BlockIoCache::new(device.clone());
        cache.init(5);

        for i in 0..10 {
            let _block_ref = cache.get(i).read();
        }
        // cache: 9 -> 8 -> 7 -> 6 -> 5
        let Ok(mut block_ref) = cache.get(5).read();
        block_ref.pin();
        drop(block_ref);

        for i in 0..10 {
            let _block_ref = cache.get(i).read();
        }

        for i in 0..10 {
            let n = if i == 5 { 1 } else { 2 };
            assert_eq!(device.data[i].lock().0.read, n);
        }
    }
}
