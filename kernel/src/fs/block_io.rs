//! Cache for block I/O.
//!
use core::{
    ptr,
    sync::atomic::{AtomicBool, Ordering},
};

use alloc::{boxed::Box, collections::linked_list::LinkedList};

use crate::{
    fs::{BlockNo, DInode, DeviceNo, INODE_PER_BLOCK, NINDIRECT},
    param::NBUF,
    sync::{SleepLock, SleepLockGuard, SpinLock},
    virtio_disk,
};

/// Block size in bytes.
pub const BLOCK_SIZE: usize = 1024;

impl BlockNo {
    /// Returns the offset of the block in the disk.
    fn to_offset(self) -> usize {
        self.value() as usize * BLOCK_SIZE
    }
}

/// A buffer cache for block I/O.
struct BlockIoCache {
    /// Linked list of all buffers, through prev/next.
    ///
    /// Sorted by how recently the buffer was used.
    /// `buffers.front()` is most recent, `buffesr.back()` is least.
    buffers: SpinLock<LinkedList<BlockOwn>>,
}

/// A block buffer.
struct BlockOwn {
    /// Device number.
    dev: DeviceNo,
    /// Block number.
    block_no: BlockNo,
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
    data: SleepLock<Box<[u8; BLOCK_SIZE]>>,
}

/// A reference to a block buffer.
pub struct BlockRef<'a, const VALID: bool> {
    /// Device number.
    dev: DeviceNo,
    /// Block number.
    block_no: BlockNo,
    /// `true` if data has been read from disk.
    ///
    /// This is used to avoid reading the same block multiple times.
    /// This flag can be read and write without holding the `data`'s lock.
    valid: &'a AtomicBool,

    /// Block data.
    data: Option<SleepLockGuard<'a, Box<[u8; BLOCK_SIZE]>>>,
}

impl BlockIoCache {
    /// Initializes the block I/O cache with `num_block` buffers.
    ///
    /// # Panics
    ///
    /// Panics if:
    ///
    /// * `num_block` is 0.
    /// * The cache is already initialized.
    fn init(&self, num_block: usize) {
        assert!(num_block > 0);
        let mut buffers = self.buffers.lock();
        assert!(buffers.is_empty());

        // Create linked list of buffers
        for _ in 0..num_block {
            buffers.push_back(BlockOwn {
                dev: DeviceNo::INVALID,
                block_no: BlockNo::INVALID,
                valid: AtomicBool::new(false),
                data: SleepLock::new(Box::new([0; BLOCK_SIZE])),
                refcnt: 0,
            })
        }
    }

    /// Returns a editable reference to the buffer with the given device number and block number.
    ///
    /// If the buffer is already in the cache, returns a reference to it.
    /// Otherwise, recycles the least recently used (LRU) unused buffer and returns a reference to it.
    /// If all buffers are in use, panics.
    fn get(&self, dev: DeviceNo, block_no: BlockNo) -> BlockRef<'_, false> {
        let mut buffers = self.buffers.lock();

        // Find the buffer with dev & block_no
        if let Some(buf) = buffers
            .iter_mut()
            .find(|b| b.dev == dev && b.block_no == block_no)
        {
            // NOTE: `buf.valid` may be `false` here.
            unsafe {
                // Safety: `buf.refcnt > 0` here, so `valid` and `data` exists after unlock (`drop(bcache)`)
                buf.refcnt += 1;
                let valid = (&raw const buf.valid).as_ref().unwrap();
                let data = (&raw const buf.data).as_ref().unwrap();
                drop(buffers);

                let buf = BlockRef {
                    dev,
                    block_no,
                    valid,
                    data: Some(data.lock()),
                };
                return buf;
            }
        }

        // Not cached.
        // Recycle the least recentrly used (LRU) unused buffer.
        if let Some(buf) = buffers.iter_mut().rev().find(|buf| buf.refcnt == 0) {
            buf.dev = dev;
            buf.block_no = block_no;
            buf.valid.store(false, Ordering::Release);
            unsafe {
                // Safety: `buf.refcnt > 0` here, so `valid` and `data` exists after unlock (`drop(bcache)`)
                buf.refcnt = 1;
                let valid = (&raw const buf.valid).as_ref().unwrap();
                let data = (&raw const buf.data).as_ref().unwrap();
                drop(buffers);

                let buf = BlockRef {
                    dev,
                    block_no,
                    valid,
                    data: Some(data.lock()),
                };
                return buf;
            }
        }

        panic!("block buffer exhausted");
    }
}

impl<const VALID: bool> Drop for BlockRef<'_, VALID> {
    fn drop(&mut self) {
        // unlock
        if self.data.take().is_none() {
            // delegated to another BlockRef
            return;
        }

        let mut buffers = BLOCK_IO_CACHE.buffers.lock();

        // decrement refcnt & extract element if refcnt == 0
        let Some(buf) = buffers
            .extract_if(|buf| {
                buf.dev == self.dev && buf.block_no == self.block_no && {
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
        buffers.push_front(buf);
    }
}

impl<'a, const VALID: bool> BlockRef<'a, VALID> {
    /// Returns the block number.
    pub fn block_no(&self) -> BlockNo {
        self.block_no
    }
    /// Increments the reference count of the buffer.
    ///
    /// If the reference count is > 0, the buffer is in use and guaranteed to be in the cache.
    pub fn pin(&mut self) {
        let mut buffers = BLOCK_IO_CACHE.buffers.lock();
        let buf = buffers
            .iter_mut()
            .find(|buf| buf.dev == self.dev && buf.block_no == self.block_no)
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
        let mut buffers = BLOCK_IO_CACHE.buffers.lock();
        let buf = buffers
            .iter_mut()
            .find(|buf| buf.dev == self.dev && buf.block_no == self.block_no)
            .expect("buffer should be found, because refcnt must be > 0");
        assert!(buf.refcnt > 0);
        buf.refcnt -= 1;
    }

    /// Reads the block from disk if cached data is not valid.
    pub fn read(mut self) -> BlockRef<'a, true> {
        if !self.valid.load(Ordering::Relaxed) {
            let offset = self.block_no.to_offset();
            virtio_disk::read(offset, self.data.as_mut().unwrap().as_mut());
            self.valid.store(true, Ordering::Relaxed);
        }

        BlockRef {
            dev: self.dev,
            block_no: self.block_no,
            valid: self.valid,
            data: self.data.take(),
        }
    }

    /// Sets the whole block data.
    pub fn set_data(mut self, data: &[u8]) -> BlockRef<'a, true> {
        self.valid.store(true, Ordering::Relaxed);
        self.data.as_mut().unwrap().copy_from_slice(data);
        BlockRef {
            dev: self.dev,
            block_no: self.block_no,
            valid: self.valid,
            data: self.data.take(),
        }
    }

    /// Fills the whole block data with zero.
    pub fn zeroed(mut self) -> BlockRef<'a, true> {
        self.valid.store(true, Ordering::Relaxed);
        self.data.as_mut().unwrap().fill(0);
        BlockRef {
            dev: self.dev,
            block_no: self.block_no,
            valid: self.valid,
            data: self.data.take(),
        }
    }
}

impl BlockRef<'_, true> {
    /// Returns a reference to the block data.
    pub fn data(&self) -> &[u8; BLOCK_SIZE] {
        self.data.as_ref().unwrap()
    }

    /// Returns a mutable reference to the block data.
    pub fn data_mut(&mut self) -> &mut [u8; BLOCK_SIZE] {
        self.data.as_mut().unwrap()
    }

    /// Writes the block to disk.
    ///
    /// # Panic
    ///
    /// Panics if cached data is not valid.
    pub fn write(&mut self) {
        assert!(self.valid.load(Ordering::Relaxed));

        let offset = self.block_no.to_offset();
        virtio_disk::write(offset, self.data());
    }

    fn as_mut_array<T, const N: usize>(&mut self) -> &mut [T; N] {
        const {
            assert!(size_of::<[T; N]>() <= BLOCK_SIZE);
        }
        let data = ptr::from_mut(self.data_mut()).cast::<[T; N]>();
        unsafe { &mut *data }
    }

    pub fn as_dinodes_mut(&mut self) -> &mut [DInode; INODE_PER_BLOCK] {
        self.as_mut_array()
    }

    pub fn as_indirect_blocks_mut(&mut self) -> &mut [Option<BlockNo>; NINDIRECT] {
        self.as_mut_array()
    }
}

/// The global block I/O cache.
static BLOCK_IO_CACHE: BlockIoCache = BlockIoCache {
    buffers: SpinLock::new(LinkedList::new()),
};

/// Initializes the global block I/O cache.
pub fn init() {
    BLOCK_IO_CACHE.init(NBUF);
}

/// Gets the block buffer with the given device number and block number.
pub fn get(dev: DeviceNo, block_no: BlockNo) -> BlockRef<'static, false> {
    BLOCK_IO_CACHE.get(dev, block_no)
}
