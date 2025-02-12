use crate::{param::NBUF, proc::Proc, sleeplock::SleepLock, spinlock::SpinLock, virtio_disk};

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
const BLOCK_SIZE: usize = 1024;

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
    prev: *mut Buf,
    next: *mut Buf,
    data: [u8; BLOCK_SIZE],
}

#[repr(C)]
struct BlockCache {
    lock: SpinLock,
    buf: [Buf; NBUF],

    /// Linked list of all buffers, through prev/next.
    ///
    /// Sorted by how recently the buffer was used.
    /// head.next is most recent, head.prev is least.
    head: Buf,
}

static mut BCACHE: BlockCache = BlockCache {
    lock: SpinLock::new(c"bcache"),
    buf: [const {
        Buf {
            valid: 0,
            disk: 0,
            dev: 0,
            blockno: 0,
            lock: SleepLock::new(c"buffer"),
            refcnt: 0,
            prev: 0 as *mut Buf,
            next: 0 as *mut Buf,
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
        prev: 0 as *mut Buf,
        next: 0 as *mut Buf,
        data: [0; BLOCK_SIZE],
    },
};

pub fn init() {
    unsafe {
        let bcache = (&raw mut BCACHE).as_mut().unwrap();

        // Create linked list of buffers
        bcache.head.prev = &raw mut bcache.head;
        bcache.head.next = &raw mut bcache.head;
        for b in bcache.buf.iter_mut() {
            b.next = bcache.head.next;
            b.prev = &raw mut bcache.head;
            (*bcache.head.next).prev = b;
            bcache.head.next = b;
        }
    }
}

/// Looks through buffer cache for block on device dev.
///
/// If not found, allocate a buffer.
/// In either case, return locked buffer.
fn get(dev: u32, blockno: u32) -> &'static mut Buf {
    let bcache = unsafe { (&raw mut BCACHE).as_mut() }.unwrap();
    bcache.lock.acquire();

    // Is the block already cached?
    let mut b = bcache.head.prev;
    while b != &raw mut bcache.head {
        let bp = unsafe { b.as_mut() }.unwrap();
        unsafe { b = (*b).prev };
        if bp.dev == dev && bp.blockno == blockno {
            bp.refcnt += 1;
            bcache.lock.release();
            let p = Proc::myproc().unwrap();
            bp.lock.acquire(p);
            return bp;
        }
    }

    // Not cached.
    // Recycle the least recentrly used (LRU) unused buffer.
    let mut b = bcache.head.prev;
    while b != &raw mut bcache.head {
        let bp = unsafe { b.as_mut() }.unwrap();
        unsafe { b = (*b).prev };
        if bp.refcnt == 0 {
            bp.dev = dev;
            bp.blockno = blockno;
            bp.valid = 0;
            bp.refcnt = 1;
            bcache.lock.release();
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

        let bcache = unsafe { (&raw mut BCACHE).as_mut() }.unwrap();
        bcache.lock.acquire();
        self.refcnt -= 1;

        if self.refcnt == 0 {
            // no one is waiting for it.
            unsafe {
                (*self.next).prev = self.prev;
                (*self.prev).next = self.next;
                self.next = bcache.head.next;
                self.prev = &raw mut bcache.head;
                (*bcache.head.next).prev = self;
                bcache.head.next = self;
            }
        }

        bcache.lock.release();
    }

    pub fn pin(&mut self) {
        let bcache = unsafe { (&raw mut BCACHE).as_mut() }.unwrap();
        bcache.lock.acquire();
        self.refcnt += 1;
        bcache.lock.release();
    }

    pub fn unpin(&mut self) {
        assert!(self.refcnt > 0);
        let bcache = unsafe { (&raw mut BCACHE).as_mut() }.unwrap();
        bcache.lock.acquire();
        self.refcnt -= 1;
        bcache.lock.release();
    }
}
