use core::sync::atomic::{AtomicUsize, Ordering};

use crate::{
    error::Error,
    fs::{self, FS_BLOCK_SIZE, Inode},
    memory::vm::VirtAddr,
    param::MAX_OP_BLOCKS,
    proc::Proc,
};

use super::{File, FileData, FileDataArc, SpecificData};

pub(super) struct InodeFile {
    inode: Inode,
    off: AtomicUsize,
}

pub fn new_file(inode: Inode, readable: bool, writable: bool) -> Result<File, Error> {
    let data = FileDataArc::try_new(FileData {
        readable,
        writable,
        data: Some(SpecificData::Inode(InodeFile {
            inode,
            off: AtomicUsize::new(0),
        })),
    })?;
    Ok(File { data })
}

impl InodeFile {
    pub(super) fn close(self) {
        super::common::close_inode(self.inode);
    }

    pub(super) fn stat(&self, p: &Proc, addr: VirtAddr) -> Result<(), Error> {
        super::common::stat_inode(&self.inode, p, addr)
    }

    pub(super) fn read(&self, p: &Proc, addr: VirtAddr, n: usize) -> Result<usize, Error> {
        let tx = fs::begin_readonly_tx();
        let mut ip = self.inode.clone().into_tx(&tx);
        let mut lip = ip.lock();
        let res = lip.read(p, true, addr, self.off.load(Ordering::Relaxed), n);
        if let Ok(sz) = res {
            self.off.fetch_add(sz, Ordering::Relaxed);
        }
        res
    }

    pub(super) fn write(&self, p: &Proc, addr: VirtAddr, n: usize) -> Result<usize, Error> {
        // write a few blocks at a time to avoid exceeding
        // the maximum log transaction size, including
        // i-node, indirect block, allocation blocks,
        // and 2 blocks of slop for non-aligned writes.
        // this really belongs lower down, since write_inode()
        // might be writing a device like the console.
        let max = ((MAX_OP_BLOCKS - 1 - 1 - 2) / 2) * FS_BLOCK_SIZE;
        let mut i = 0;
        while i < n {
            let mut n1 = n - i;
            if n1 > max {
                n1 = max;
            }

            let tx = fs::begin_tx();
            let mut ip = self.inode.clone().into_tx(&tx);
            let mut lip = ip.lock();
            let res = lip.write(
                p,
                true,
                addr.byte_add(i),
                self.off.load(Ordering::Relaxed),
                n1,
            );
            if let Ok(sz) = res {
                self.off.fetch_add(sz, Ordering::Relaxed);
            }
            lip.unlock();
            ip.put();
            tx.end();

            if !res.is_ok_and(|n| n == n1) {
                // error from write_inode
                break;
            }
            i += n1;
        }
        if i == n { Ok(n) } else { Err(Error::Unknown) }
    }
}
