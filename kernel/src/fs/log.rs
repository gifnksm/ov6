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

use alloc::boxed::Box;
use dataview::Pod;
use once_init::OnceInit;

use crate::{
    fs::{
        BlockNo, DeviceNo, SuperBlock,
        block_io::{self},
    },
    param::{LOG_SIZE, MAX_OP_BLOCKS},
    sync::{SpinLock, SpinLockCondVar},
};

use super::block_io::BlockGuard;

/// Contents of the header block, used for both the on-disk header block
/// and to keep track in memory of logged block# before commit.
#[repr(C)]
#[derive(Pod)]
struct LogHeader {
    len: u32,
    block_indices: [u32; LOG_SIZE],
}

impl LogHeader {
    const fn new() -> Self {
        Self {
            len: 0,
            block_indices: [0; LOG_SIZE],
        }
    }

    fn len(&self) -> usize {
        self.len as usize
    }

    fn copy_from(&mut self, src: &LogHeader) {
        self.len = src.len;
        let len = self.len as usize;
        self.block_indices[..len].copy_from_slice(&src.block_indices[..len]);
    }

    fn block_indices(&self) -> &[u32] {
        &self.block_indices[..self.len as usize]
    }

    fn push(&mut self, block_index: u32) {
        self.block_indices[self.len as usize] = block_index;
        self.len += 1;
    }
}

struct Commit<'h> {
    dev: DeviceNo,
    start: u32,
    head: &'h mut LogHeader,
}

impl Commit<'_> {
    fn recover_from_log(&mut self) {
        self.read_head();
        self.install_trans(true); // if committed, copy from log to disk.
        self.head.len = 0;
        self.write_head(); // clear the log
    }

    fn commit(&mut self) {
        if self.head.len > 0 {
            self.write_body(); // Write modified blocks from cache to log
            self.write_head(); // Write header to disk -- the real commit
            self.install_trans(false); // Now install writes to home locations
            self.head.len = 0;
            self.write_head(); // Erase the transaction from the log
        }
    }

    /// Reads the log header from disk into the in-memory log header.
    fn read_head(&mut self) {
        let mut bh = block_io::get(self.dev, BlockNo::new(self.start).unwrap());
        let Ok(bg) = bh.lock().read();
        let lh = bg.data::<LogHeader>();
        self.head.copy_from(lh);
    }

    /// Writes in-memory log header to disk.
    ///
    /// This is the true point at which the
    /// current transaction commits.
    fn write_head(&self) {
        let mut br = block_io::get(self.dev, BlockNo::new(self.start).unwrap());
        let mut bg = br.lock().zeroed();
        bg.data_mut::<LogHeader>().copy_from(self.head);
        let Ok(()) = bg.write(); // infallible
    }

    fn write_body(&self) {
        for (bn, tail) in self.head.block_indices().iter().zip(0..) {
            let mut from_br = block_io::get(self.dev, BlockNo::new(*bn).unwrap()); // cache block
            let Ok(from_bg) = from_br.lock().read(); // read block
            let mut to_br = block_io::get(self.dev, BlockNo::new(self.start + tail + 1).unwrap());
            let mut to_bg = to_br.lock().set_data(from_bg.bytes());
            let Ok(()) = to_bg.write(); // log block
        }
    }

    /// Copies committed blocks from log to their home location.
    fn install_trans(&self, recovering: bool) {
        for (bn, tail) in self.head.block_indices().iter().zip(0..) {
            let mut from_br = block_io::get(self.dev, BlockNo::new(self.start + tail + 1).unwrap());
            let Ok(from_bg) = from_br.lock().read(); // read log block
            let mut to_br = block_io::get(self.dev, BlockNo::new(*bn).unwrap());
            let mut to_bg = to_br.lock().set_data(from_bg.bytes());
            let Ok(()) = to_bg.write(); // copy from log to dst and write dst to disk
            if !recovering {
                unsafe {
                    assert!(to_bg.pin_count() > 2);
                    to_bg.unpin();
                }
            }
        }
    }
}

struct Log {
    start: u32,
    size: u32,
    dev: DeviceNo,
    data: SpinLock<LogData>,
    cond: SpinLockCondVar,
}

struct LogData {
    outstanding: usize,
    header: Option<Box<LogHeader>>, // If None, data is committing.
}

static LOG: OnceInit<Log> = OnceInit::new();

impl Log {
    pub fn new(dev: DeviceNo, sb: &SuperBlock) -> Self {
        let start = sb.logstart;

        let mut header = Box::new(LogHeader::new());
        let mut commit = Commit {
            dev,
            start,
            head: &mut header,
        };
        commit.recover_from_log();

        Self {
            start,
            size: sb.nlog,
            dev,
            data: SpinLock::new(LogData {
                outstanding: 0,
                header: Some(Box::new(LogHeader::new())),
            }),
            cond: SpinLockCondVar::new(),
        }
    }

    /// Starts FS transaction.
    ///
    /// Called at the start of each FS system call.
    fn begin_op(&self) {
        let mut data = self.data.lock();
        loop {
            let Some(header) = &data.header else {
                // header is under committing
                data = self.cond.wait(data);
                continue;
            };
            if header.len() + (data.outstanding + 1) * MAX_OP_BLOCKS > LOG_SIZE {
                // this op might exhaust log space; wait for commit.
                data = self.cond.wait(data);
                continue;
            }
            data.outstanding += 1;
            break;
        }
    }

    /// Ends FS transaction.
    ///
    /// Called at the end of each FS system call.
    /// Commits if this was the last outstanding operation.
    fn end_op(&self) {
        let mut to_commit = None;

        let mut data = self.data.lock();
        data.outstanding -= 1;
        assert!(data.header.is_some()); // not under committing
        if data.outstanding == 0 {
            to_commit = data.header.take();
        } else {
            // begin_op() may be waiting for log space,
            // and decrementing log.outstanding has decreased
            // the amount of reserved space.
            self.cond.notify();
        }
        drop(data); // unlock here

        if let Some(mut to_commit) = to_commit {
            let mut commit = Commit {
                dev: self.dev,
                start: self.start,
                head: &mut to_commit,
            };
            // call commit w/o holding locks, since not allowed
            // to sleep with locks.
            commit.commit();
            let mut data = self.data.lock();
            assert!(data.header.is_none());
            data.header = Some(to_commit);
            self.cond.notify();
        }
    }

    fn write(&self, b: &mut BlockGuard<true>) {
        let data = &mut *self.data.lock();
        let header = data.header.as_mut().unwrap();
        assert!(header.len() < LOG_SIZE && header.len < self.size - 1);
        assert!(data.outstanding > 0);

        let bn = b.index() as u32;
        if header.block_indices().iter().all(|bbn| *bbn != bn) {
            // Add new block to log
            b.pin();
            header.push(bn);
        }
    }
}

pub fn init(dev: DeviceNo, sb: &SuperBlock) {
    LOG.init(Log::new(dev, sb));
}

/// Starts FS transaction.
///
/// Called at the start of each FS system call.
pub fn begin_op() {
    LOG.get().begin_op();
}

/// Ends FS transaction.
///
/// Called at the end of each FS system call.
/// Commits if this was the last outstanding operation.
pub fn end_op() {
    LOG.get().end_op();
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

pub fn write(b: &mut BlockGuard<true>) {
    LOG.get().write(b);
}
