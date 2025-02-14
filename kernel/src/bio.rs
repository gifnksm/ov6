use core::ptr::{self, NonNull};

use crate::{
    fs::{BlockNo, DInode, DeviceNo, INODE_PER_BLOCK, NINDIRECT},
    param::NBUF,
    proc::Proc,
    sleeplock::SleepLock,
    spinlock::Mutex,
    virtio_disk,
};

// Block size
pub const BLOCK_SIZE: usize = 1024;

#[repr(C)]
pub struct Buf {
    /// Has data been read from disk?
    valid: i32,
    /// Does disk "own" buf?
    disk: i32,
    dev: Option<DeviceNo>,
    pub block_no: Option<BlockNo>,
    lock: SleepLock,
    refcnt: u32,
    // LRU cache list
    prev: NonNull<Buf>,
    next: NonNull<Buf>,
    pub data: [u8; BLOCK_SIZE],
}

unsafe impl Send for Buf {}

#[repr(C)]
struct BlockCache {
    buf: [Buf; NBUF],

    /// Linked list of all buffers, through prev/next.
    ///
    /// Sorted by how recently the buffer was used.
    /// head.next is most recent, head.prev is least.
    head: Buf,
}

static BCACHE: Mutex<BlockCache> = Mutex::new(BlockCache {
    buf: [const {
        Buf {
            valid: 0,
            disk: 0,
            dev: None,
            block_no: None,
            lock: SleepLock::new(c"buffer"),
            refcnt: 0,
            prev: NonNull::dangling(),
            next: NonNull::dangling(),
            data: [0; BLOCK_SIZE],
        }
    }; NBUF],
    head: Buf {
        valid: 0,
        disk: 0,
        dev: None,
        block_no: None,
        lock: SleepLock::new(c"buffer"),
        refcnt: 0,
        prev: NonNull::dangling(),
        next: NonNull::dangling(),
        data: [0; BLOCK_SIZE],
    },
});

pub fn init() {
    let bcache = &mut *BCACHE.lock();

    // Create linked list of buffers
    unsafe {
        bcache.head.prev = (&mut bcache.head).into();
        bcache.head.next = (&mut bcache.head).into();
        for b in &mut bcache.buf {
            b.next = bcache.head.next;
            b.prev = (&mut bcache.head).into();
            bcache.head.next.as_mut().prev = b.into();
            bcache.head.next = b.into();
        }
    }
}

/// Looks through buffer cache for block on device dev.
///
/// If not found, allocate a buffer.
/// In either case, return locked buffer.
fn get(p: &Proc, dev: DeviceNo, block_no: BlockNo) -> &'static mut Buf {
    let mut bcache = BCACHE.lock();

    // Is the block already cached?
    let mut b = bcache.head.prev;
    while b != (&mut bcache.head).into() {
        let bp = unsafe { b.as_mut() };
        unsafe { b = b.as_mut().prev };
        if bp.dev == Some(dev) && bp.block_no == Some(block_no) {
            bp.refcnt += 1;
            drop(bcache);

            bp.lock.acquire(p);
            return bp;
        }
    }

    // Not cached.
    // Recycle the least recentrly used (LRU) unused buffer.
    let mut b = bcache.head.prev;
    while b != (&mut bcache.head).into() {
        let bp = unsafe { b.as_mut() };
        unsafe { b = b.as_mut().prev };
        if bp.refcnt == 0 {
            bp.dev = Some(dev);
            bp.block_no = Some(block_no);
            bp.valid = 0;
            bp.refcnt = 1;
            drop(bcache);

            bp.lock.acquire(p);
            return bp;
        }
    }
    panic!("no buffers");
}

/// Returns a locked buf with the contents of the indicated block.
pub fn read(p: &Proc, dev: DeviceNo, block_no: BlockNo) -> &'static mut Buf {
    let b = get(p, dev, block_no);
    if b.valid == 0 {
        virtio_disk::read(b);
        b.valid = 1;
    }
    b
}

/// Writes b's contains to disk.
///
/// Must be locked.
pub fn write(p: &Proc, b: &mut Buf) {
    assert!(b.lock.holding(p));
    virtio_disk::write(b);
}

impl Buf {
    /// Releases a locked buffer.
    ///
    /// Moves to the head of the most-recently-used list.
    pub fn release(&mut self, p: &Proc) {
        assert!(self.lock.holding(p));
        self.lock.release();

        let bcache = &mut *BCACHE.lock();
        self.refcnt -= 1;

        if self.refcnt == 0 {
            // no one is waiting for it.
            unsafe {
                self.next.as_mut().prev = self.prev;
                self.prev.as_mut().next = self.next;
                self.next = bcache.head.next;
                self.prev = (&mut bcache.head).into();
                bcache.head.next.as_mut().prev = self.into();
                bcache.head.next = self.into();
            }
        }
    }

    pub fn pin(&mut self) {
        let _bcache = BCACHE.lock();
        self.refcnt += 1;
    }

    pub fn unpin(&mut self) {
        assert!(self.refcnt > 0);
        let _bcache = BCACHE.lock();
        self.refcnt -= 1;
    }

    fn as_mut_array<T, const N: usize>(&mut self) -> &mut [T; N] {
        const {
            assert!(size_of::<[T; N]>() <= BLOCK_SIZE);
        }
        let data = ptr::from_mut(&mut self.data).cast::<[T; N]>();
        unsafe { &mut *data }
    }

    pub fn as_dinodes_mut(&mut self) -> &mut [DInode; INODE_PER_BLOCK] {
        self.as_mut_array()
    }

    pub fn as_indirect_blocks_mut(&mut self) -> &mut [Option<BlockNo>; NINDIRECT] {
        self.as_mut_array()
    }
}

pub fn with_buf<F, T>(p: &Proc, dev: DeviceNo, block_no: BlockNo, f: F) -> T
where
    F: FnOnce(&mut Buf) -> T,
{
    let buf = read(p, dev, block_no);
    let res = f(buf);
    buf.release(p);
    res
}
