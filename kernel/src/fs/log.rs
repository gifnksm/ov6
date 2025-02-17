//! Simple logging that allows concurrent FS system calls.
//!
//! A log transaction contains the updates of multiple FS system
//! calls. The logging system only commits when there are
//! no FS system calls active. Thus there is never
//! any reasoning required about whether a commit might
//! write an uncommitted system call's data to disk.
//!
//! A system call should call [`begin_op()`]/[`end_op()`] to mark
//! its start and end. Usually [`begin_op()`] just increments
//! the count of in-progress FS system calls and returns.
//! But if it thinks the log is close to running out, it
//! sleeps until the last outstanding [`end_op()`] commits.
//!
//! The log is a physical re-do log containiing disk blocks.
//!
//! The on-disk log format:
//!
//! ```text
//! header block, containing block #s for block A, B, C, ...
//! block A
//! block B
//! block C
//! ...
//! ```

use core::ptr;

use crate::{
    fs::{
        BlockNo, DeviceNo, SuperBlock,
        block_io::{self, BlockRef},
    },
    param::{LOG_SIZE, MAX_OP_BLOCKS},
    proc,
    sync::RawSpinLock,
};

/// Contents of the header block, used for both the on-disk header block
/// and to keep track in memory of logged block# before commit.
#[repr(C)]
struct LogHeader {
    n: u32,
    block: [Option<BlockNo>; LOG_SIZE],
}

struct Log {
    lock: RawSpinLock,
    start: u32,
    size: u32,
    /// How many FS sys calls are executing.
    outstanding: usize,
    /// In commit(), please wait.
    committing: bool,
    dev: DeviceNo,
    lh: LogHeader,
}

static mut LOG: Log = Log {
    lock: RawSpinLock::new(),
    start: 0,
    size: 0,
    outstanding: 0,
    committing: false,
    dev: DeviceNo::INVALID,
    lh: LogHeader {
        n: 0,
        block: [None; LOG_SIZE],
    },
};

impl Log {
    pub fn init(&mut self, dev: DeviceNo, sb: &SuperBlock) {
        self.start = sb.logstart;
        self.size = sb.nlog;
        self.dev = dev;

        self.recover_from_log();
    }

    /// Copies committed blocks from log to their home location.
    fn install_trans(&mut self, recovering: bool) {
        for tail in 0..self.lh.n {
            let lbuf = block_io::get(self.dev, BlockNo::new(self.start + tail + 1).unwrap()).read(); // read log block
            let mut dbuf = block_io::get(self.dev, self.lh.block[tail as usize].unwrap())
                .set_data(lbuf.data()); // copy from log to dst
            dbuf.data_mut().copy_from_slice(lbuf.data());
            dbuf.write(); // write dst to disk
            if !recovering {
                unsafe {
                    dbuf.unpin();
                }
            }
        }
    }

    /// Reads the log header from disk into the in-memory log header.
    fn read_head(&mut self) {
        let buf = block_io::get(self.dev, BlockNo::new(self.start).unwrap()).read();
        let lh = buf.data().as_ptr().cast::<LogHeader>();
        unsafe {
            self.lh.n = (*lh).n;
            let n = self.lh.n as usize;
            self.lh.block[..n].copy_from_slice(&(*lh).block[..n]);
        }
    }

    /// Writes in-memory log header to disk.
    ///
    /// This is the true point at which the
    /// current transaction commits.
    fn write_head(&mut self) {
        let mut buf = block_io::get(self.dev, BlockNo::new(self.start).unwrap()).zeroed();
        let hb = buf.data_mut().as_mut_ptr().cast::<LogHeader>();
        unsafe {
            (*hb).n = self.lh.n;
            let n = self.lh.n as usize;
            (*hb).block[..n].copy_from_slice(&self.lh.block[..n]);
        }
        buf.write();
    }

    fn recover_from_log(&mut self) {
        self.read_head();
        self.install_trans(true); // if committed, copy from log to disk.
        self.lh.n = 0;
        self.write_head(); // clear the log
    }

    /// Starts FS transaction.
    ///
    /// Called at the start of each FS system call.
    fn begin_op(&mut self) {
        self.lock.acquire();
        loop {
            if self.committing {
                proc::sleep_raw(ptr::from_ref(self).cast(), &self.lock);
                continue;
            }
            if (self.lh.n as usize) + (self.outstanding + 1) * MAX_OP_BLOCKS > LOG_SIZE {
                // this op might exhaust log space; wait for commit.
                proc::sleep_raw(ptr::from_ref(self).cast(), &self.lock);
                continue;
            }
            self.outstanding += 1;
            self.lock.release();
            break;
        }
    }

    /// Ends FS transaction.
    ///
    /// Called at the end of each FS system call.
    /// Commits if this was the last outstanding operation.
    fn end_op(&mut self) {
        let mut do_commit = false;

        self.lock.acquire();
        self.outstanding -= 1;
        assert!(!self.committing);
        if self.outstanding == 0 {
            do_commit = true;
            self.committing = true;
        } else {
            // begin_op() may be waiting for log space,
            // and decrementing log.outstanding has decreased
            // the amount of reserved space.
            proc::wakeup(ptr::from_ref(self).cast());
        }
        self.lock.release();

        if do_commit {
            // call commit w/o holding locks, since not allowed
            // to sleep with locks.
            self.commit();
            self.lock.acquire();
            self.committing = false;
            proc::wakeup(ptr::from_ref(self).cast());
            self.lock.release();
        }
    }

    fn write_body(&mut self) {
        for tail in 0..self.lh.n {
            let from = block_io::get(self.dev, self.lh.block[tail as usize].unwrap()).read(); // cache block
            let mut to = block_io::get(self.dev, BlockNo::new(self.start + tail + 1).unwrap())
                .set_data(from.data()); // log block
            to.write(); // write the log
        }
    }

    fn commit(&mut self) {
        if self.lh.n > 0 {
            self.write_body(); // Write modified blocks from cache to log
            self.write_head(); // Write header to disk -- the real commit
            self.install_trans(false); // Now install writes to home locations
            self.lh.n = 0;
            self.write_head(); // Erase the transaction from the log
        }
    }

    fn write(&mut self, b: &mut BlockRef<true>) {
        self.lock.acquire();
        assert!((self.lh.n as usize) < LOG_SIZE && self.lh.n < self.size - 1);
        assert!(self.outstanding > 0);

        let i = (0..self.lh.n as usize)
            .find(|i| self.lh.block[*i] == Some(b.block_no())) // log absorption
            .unwrap_or(self.lh.n as usize);
        self.lh.block[i] = Some(b.block_no());
        if i == self.lh.n as usize {
            // Add new block to log
            b.pin();
            self.lh.n += 1;
        }
        self.lock.release();
    }
}

pub fn init(dev: DeviceNo, sb: &SuperBlock) {
    let log = unsafe { (&raw mut LOG).as_mut() }.unwrap();
    log.init(dev, sb);
}

/// Starts FS transaction.
///
/// Called at the start of each FS system call.
pub fn begin_op() {
    let log = unsafe { (&raw mut LOG).as_mut() }.unwrap();
    log.begin_op();
}

/// Ends FS transaction.
///
/// Called at the end of each FS system call.
/// Commits if this was the last outstanding operation.
pub fn end_op() {
    let log = unsafe { (&raw mut LOG).as_mut() }.unwrap();
    log.end_op();
}

/// Does FS transaction.
///
/// Commits if this was the last outstanding operation.
pub fn do_op<T, F>(f: F) -> T
where
    F: FnOnce() -> T,
{
    begin_op();
    let res = f();
    end_op();
    res
}

pub fn write(b: &mut BlockRef<true>) {
    let log = unsafe { (&raw mut LOG).as_mut() }.unwrap();
    log.write(b);
}
