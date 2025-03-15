use core::sync::atomic::{AtomicUsize, Ordering};

use ov6_syscall::{Stat, UserMutSlice, UserSlice};

use super::{File, FileData, FileDataArc, SpecificData};
use crate::{
    error::KernelError,
    fs::{self, FS_BLOCK_SIZE, Inode},
    memory::vm_user::UserPageTable,
    param::MAX_OP_BLOCKS,
};

pub(super) struct InodeFile {
    inode: Inode,
    off: AtomicUsize,
}

pub fn new_file(inode: Inode, readable: bool, writable: bool) -> Result<File, KernelError> {
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

    pub(super) fn stat(&self) -> Result<Stat, KernelError> {
        super::common::stat_inode(&self.inode)
    }

    pub(super) fn read(
        &self,
        pt: &mut UserPageTable,
        dst: UserMutSlice<u8>,
    ) -> Result<usize, KernelError> {
        let tx = fs::begin_readonly_tx();
        let mut ip = self.inode.clone().into_tx(&tx);
        let mut lip = ip.wait_lock()?;
        let res = lip.read((pt, dst).into(), self.off.load(Ordering::Relaxed));
        if let Ok(sz) = res {
            self.off.fetch_add(sz, Ordering::Relaxed);
        }
        res
    }

    pub(super) fn write(
        &self,
        pt: &UserPageTable,
        src: UserSlice<u8>,
    ) -> Result<usize, KernelError> {
        // write a few blocks at a time to avoid exceeding
        // the maximum log transaction size, including
        // i-node, indirect block, allocation blocks,
        // and 2 blocks of slop for non-aligned writes.
        // this really belongs lower down, since write_inode()
        // might be writing a device like the console.
        let max = ((MAX_OP_BLOCKS - 1 - 1 - 2) / 2) * FS_BLOCK_SIZE;
        let mut i = 0;
        while i < src.len() {
            let src = src.skip(i);
            let len = usize::min(src.len(), max);
            let src = src.take(len);

            let tx = fs::begin_tx()?;
            let mut ip = self.inode.clone().into_tx(&tx);
            let mut lip = ip.force_wait_lock();
            let res = lip.write((pt, src).into(), self.off.load(Ordering::Relaxed));
            if let Ok(sz) = res {
                self.off.fetch_add(sz, Ordering::Relaxed);
            }
            lip.unlock();
            ip.put();
            tx.end();

            match res {
                Err(e) => return Err(e),
                Ok(n) if n != src.len() => break,
                Ok(_) => {}
            }

            i += src.len();
        }
        Ok(src.len())
    }
}
