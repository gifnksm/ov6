//! Cache for block I/O.

use core::convert::Infallible;

use block_io::{BlockData, BlockDevice, BlockIoCache, BufferList};
use once_init::OnceInit;

use crate::{
    fs::{BlockNo, DeviceNo},
    param::{NBUF, ROOT_DEV},
    sync::{SleepLock, SpinLock},
    virtio_disk,
};

/// Block size in bytes.
pub const BLOCK_SIZE: usize = 1024;

pub struct VirtioDiskDevice {}
impl BlockDevice<BLOCK_SIZE> for VirtioDiskDevice {
    type Error = Infallible;

    fn read(&self, index: usize, data: &mut [u8; BLOCK_SIZE]) -> Result<(), Self::Error> {
        virtio_disk::read(index * BLOCK_SIZE, data);
        Ok(())
    }

    fn write(&self, index: usize, data: &[u8; BLOCK_SIZE]) -> Result<(), Self::Error> {
        virtio_disk::write(index * BLOCK_SIZE, data);
        Ok(())
    }
}

type BlockDataMutex = SleepLock<BlockData<BLOCK_SIZE>>;
type BufferListMutex = SpinLock<BufferList<BlockDataMutex>>;

static VIRTIO_DISK_CACHE: OnceInit<BlockIoCache<VirtioDiskDevice, BufferListMutex>> =
    OnceInit::new();

pub type BlockRef<'a, const VALID: bool> =
    block_io::BlockRef<'a, VirtioDiskDevice, BufferListMutex, BlockDataMutex, BLOCK_SIZE, VALID>;

/// Initializes the global block I/O cache.
pub fn init() {
    VIRTIO_DISK_CACHE.init(BlockIoCache::new(VirtioDiskDevice {}));
    VIRTIO_DISK_CACHE.get().init(NBUF);
}

/// Gets the block buffer with the given device number and block number.
pub fn get(dev: DeviceNo, block_no: BlockNo) -> BlockRef<'static, false> {
    match dev {
        ROOT_DEV => VIRTIO_DISK_CACHE.get().get(block_no.value() as usize),
        _ => panic!("unknown device: dev={}", dev.value()),
    }
}
