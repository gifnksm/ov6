use core::ptr::NonNull;

use crate::{param::NBUF, proc::Proc, sleeplock::SleepLock, spinlock::Mutex, virtio_disk};

mod ffi {
    use super::*;

    #[unsafe(no_mangle)]
    extern "C" fn bread(dev: u32, blockno: u32) -> *mut Buf {
        super::read(dev, blockno)
    }

    #[unsafe(no_mangle)]
    extern "C" fn bwrite(buf: *mut Buf) {
        let p = Proc::myproc().unwrap();
        unsafe { super::write(p, buf.as_mut().unwrap()) }
    }

    #[unsafe(no_mangle)]
    extern "C" fn brelse(buf: *mut Buf) {
        unsafe { buf.as_mut().unwrap().release(Proc::myproc().unwrap()) }
    }

    #[unsafe(no_mangle)]
    extern "C" fn bpin(buf: *mut Buf) {
        unsafe { buf.as_mut().unwrap().pin() }
    }

    #[unsafe(no_mangle)]
    extern "C" fn bunpin(buf: *mut Buf) {
        unsafe { buf.as_mut().unwrap().unpin() }
    }
}

// Block size
pub const BLOCK_SIZE: usize = 1024;

#[repr(C)]
pub struct Buf {
    /// Has data been read from disk?
    valid: i32,
    /// Does disk "own" buf?
    disk: i32,
    dev: u32,
    blockno: u32,
    lock: SleepLock,
    refcnt: u32,
    // LRU cache list
    prev: NonNull<Buf>,
    next: NonNull<Buf>,
    data: [u8; BLOCK_SIZE],
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
            dev: 0,
            blockno: 0,
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
        dev: 0,
        blockno: 0,
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
fn get(dev: u32, blockno: u32) -> &'static mut Buf {
    let mut bcache = BCACHE.lock();

    // Is the block already cached?
    let mut b = bcache.head.prev;
    while b != (&mut bcache.head).into() {
        let bp = unsafe { b.as_mut() };
        unsafe { b = b.as_mut().prev };
        if bp.dev == dev && bp.blockno == blockno {
            bp.refcnt += 1;
            drop(bcache);

            let p = Proc::myproc().unwrap();
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
            bp.dev = dev;
            bp.blockno = blockno;
            bp.valid = 0;
            bp.refcnt = 1;
            drop(bcache);

            let p = Proc::myproc().unwrap();
            bp.lock.acquire(p);
            return bp;
        }
    }
    panic!("no buffers");
}

/// Returns a locked buf with the contents of the indicated block.
pub fn read(dev: u32, blockno: u32) -> &'static mut Buf {
    let b = get(dev, blockno);
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
}
