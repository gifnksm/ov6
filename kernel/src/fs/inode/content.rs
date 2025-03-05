//! Inode content
//!
//! The content (data) associated with each inode is stored
//! in blocks on the disk. The first `NUN_DIRECT_REFS` block numbers
//! are listed in `addrs[]`.  The next `NUM_INDIRECT_REFS` blocks are
//! listed in block `[NUM_DIRECT_REFS]`.

use core::{mem::MaybeUninit, ptr};

use dataview::Pod;

use crate::{
    error::Error,
    fs::{
        BlockNo, SUPER_BLOCK, data_block,
        repr::{self, FS_BLOCK_SIZE, MAX_FILE, NUM_DIRECT_REFS, NUM_INDIRECT_REFS},
    },
    memory::VirtAddr,
    proc::{self, ProcPrivateData},
};

use super::LockedTxInode;

impl<const READ_ONLY: bool> LockedTxInode<'_, '_, READ_ONLY> {
    /// Returns the disk block address of the `i`th **direct** block in inode.
    ///
    /// If there is no such block, `get_data_block()` allocates one.
    /// Returns `None` if out of disk space.
    fn get_direct_data_block(&mut self, i: usize) -> Option<BlockNo> {
        assert!(i < NUM_DIRECT_REFS);
        if let Some(bn) = self.data().addrs[i] {
            return Some(bn);
        }

        let tx = self.tx.to_writable()?;
        let bn = data_block::alloc(&tx, self.dev)?;
        self.data_mut().addrs[i] = Some(bn);
        Some(bn)
    }

    /// Returns the disk block address of the `i`th **indirect** block in inode.
    ///
    /// If there is no such block, `get_data_block()` allocates one.
    /// Returns `None` if out of disk space.
    fn get_indirect_data_block(&mut self, i: usize) -> Option<BlockNo> {
        // Load indirect block, allocating if necessary.
        let (ind_bn, ind_newly_allocated) = match self.data().addrs[NUM_DIRECT_REFS] {
            Some(ind_bn) => (ind_bn, false),
            None => {
                let tx = self.tx.to_writable()?;
                let ind_bn = data_block::alloc(&tx, self.dev)?;
                self.data_mut().addrs[NUM_DIRECT_REFS] = Some(ind_bn);
                (ind_bn, true)
            }
        };

        if !ind_newly_allocated {
            let mut ind_br = self.tx.get_block(self.dev, ind_bn);
            let Ok(ind_bg) = ind_br.lock().read();
            if let Some(bn) = ind_bg.data::<repr::IndirectBlock>().get(i) {
                return Some(bn);
            }
        }

        let tx = self.tx.to_writable()?;
        let bn = data_block::alloc(&tx, self.dev)?;
        let mut ind_br = tx.get_block(self.dev, ind_bn);
        let Ok(mut ind_bg) = ind_br.lock().read();
        ind_bg.data_mut::<repr::IndirectBlock>().set(i, Some(bn));

        Some(bn)
    }

    /// Returns the disk block address of the `i`th block in inode.
    ///
    /// If there is no such block, `get_data_block()` allocates one.
    /// Returns `None` if out of disk space.
    fn get_data_block(&mut self, i: usize) -> Option<BlockNo> {
        if i < NUM_DIRECT_REFS {
            return self.get_direct_data_block(i);
        }

        let i = i - NUM_DIRECT_REFS;
        if i < NUM_INDIRECT_REFS {
            return self.get_indirect_data_block(i);
        }

        panic!("out of range: ibn={i}");
    }
}

impl LockedTxInode<'_, '_, false> {
    /// Truncates inode (discard contents).
    pub fn truncate(&mut self) {
        for bn in &mut self.locked.as_mut().unwrap().addrs[..NUM_DIRECT_REFS] {
            if let Some(bn) = bn.take() {
                data_block::free(self.tx, self.dev, bn);
            }
        }

        if let Some(bn) = self.data_mut().addrs[NUM_DIRECT_REFS].take() {
            let mut br = self.tx.get_block(self.dev, bn);
            let Ok(mut bg) = br.lock().read();
            for bn in bg.data_mut::<repr::IndirectBlock>().drain().flatten() {
                data_block::free(self.tx, self.dev, bn);
            }
            drop(bg);
            data_block::free(self.tx, self.dev, bn);
        }

        self.data_mut().size = 0;
        self.update();
    }

    /// Copies a modified in-memory inode to disk.
    ///
    /// Must be called after every change to an in-memory data
    /// that lives on disk.
    pub fn update(&self) {
        let sb = SUPER_BLOCK.get();
        let mut br = self.tx.get_block(self.dev, sb.inode_block(self.ino));
        let Ok(mut bg) = br.lock().read();
        let dip = bg.data_mut::<repr::InodeBlock>().inode_mut(self.ino);
        self.data().write_repr(dip);
    }

    pub fn free(mut self) {
        self.data_mut().ty = 0;
        self.update();
        *self.locked = None;
    }
}

impl<const READ_ONLY: bool> LockedTxInode<'_, '_, READ_ONLY> {
    /// Reads the inode's data.
    ///
    /// If `user_dst` is true, `dst` is a user virtual address;
    /// otherwise, it is a kernel address.
    /// Returns the number of bytes successfully read.
    /// If the return value is less than the requested `n`,
    /// there was an error of some kind.
    pub fn read(
        &mut self,
        private: &mut ProcPrivateData,
        user_dst: bool,
        dst: VirtAddr,
        off: usize,
        mut n: usize,
    ) -> Result<usize, Error> {
        let data = self.data();
        let size = data.size as usize;
        if off > size || off.checked_add(n).is_none() {
            return Ok(0);
        }
        if off + n > size {
            n = size - off;
        }

        let mut tot = 0;
        while tot < n {
            let off = off + tot;
            let dst = dst.byte_add(tot);
            let Some(bn) = self.get_data_block(off / FS_BLOCK_SIZE) else {
                break;
            };
            let mut br = self.tx.get_block(self.dev, bn);
            let Ok(bg) = br.lock().read();
            let m = usize::min(n - tot, FS_BLOCK_SIZE - off % FS_BLOCK_SIZE);
            // TODO: check if this is correct
            // if tot > 0, return Ok(tot)?
            proc::either_copy_out_bytes(
                private,
                user_dst,
                dst.addr(),
                &bg.bytes()[off % FS_BLOCK_SIZE..][..m],
            )?;
            tot += m;
        }
        Ok(tot)
    }

    /// Reads the inode's data as `T`.
    pub fn read_as<T>(&mut self, private: &mut ProcPrivateData, off: usize) -> Result<T, Error>
    where
        T: Pod,
    {
        let mut dst = MaybeUninit::<T>::uninit();
        let read = self.read(
            private,
            false,
            VirtAddr::new(dst.as_mut_ptr().addr()),
            off,
            size_of::<T>(),
        )?;
        if read != size_of::<T>() {
            return Err(Error::Unknown);
        }
        Ok(unsafe { dst.assume_init() })
    }
}

impl LockedTxInode<'_, '_, false> {
    /// Writes data to inode.
    ///
    /// If `user_src` is `true`, then `src` is a user virtual address;
    /// otherwise, `src` is a kernel address.
    /// Returns the number of bytes successfully written.
    /// If the return value is less than the requested `n`,
    /// there was an error of some kind.
    pub fn write(
        &mut self,
        private: &ProcPrivateData,
        user_src: bool,
        src: VirtAddr,
        off: usize,
        n: usize,
    ) -> Result<usize, Error> {
        let size = self.data().size as usize;
        if off > size || off.checked_add(n).is_none() {
            return Err(Error::Unknown);
        }
        if off + n > MAX_FILE * FS_BLOCK_SIZE {
            return Err(Error::Unknown);
        }

        let mut tot = 0;
        while tot < n {
            let off = off + tot;
            let src = src.byte_add(tot);
            let Some(bn) = self.get_data_block(off / FS_BLOCK_SIZE) else {
                break;
            };

            let mut br = self.tx.get_block(self.dev, bn);
            let Ok(mut bg) = br.lock().read();
            let m = usize::min(n - tot, FS_BLOCK_SIZE - off % FS_BLOCK_SIZE);
            // TODO: check if this is correct.
            // if tot > 0, return Ok(tot)?
            proc::either_copy_in_bytes(
                private,
                &mut bg.bytes_mut()[off % FS_BLOCK_SIZE..][..m],
                user_src,
                src.addr(),
            )?;

            tot += m;
        }

        if off + tot > size {
            self.data_mut().size = (off + tot) as u32;
        }

        // write the i-node back to disk even if the size didn't change
        // because the loop above might have called inode_block_map() and added a new
        // block to `ip.addrs`.`
        self.update();

        Ok(tot)
    }

    /// Writes `data` to inode.
    pub fn write_data<T>(
        &mut self,
        private: &ProcPrivateData,
        off: usize,
        data: &T,
    ) -> Result<(), Error>
    where
        T: Pod,
    {
        let written = self.write(
            private,
            false,
            VirtAddr::new(ptr::from_ref(data).addr()),
            off,
            size_of::<T>(),
        )?;
        if written != size_of::<T>() {
            return Err(Error::Unknown);
        }
        Ok(())
    }
}
