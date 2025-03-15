//! Inode content
//!
//! The content (data) associated with each inode is stored
//! in blocks on the disk. The first `NUN_DIRECT_REFS` block numbers
//! are listed in `addrs[]`.  The next `NUM_INDIRECT_REFS` blocks are
//! listed in block `[NUM_DIRECT_REFS]`.

use dataview::{Pod, PodMethods as _};

use super::LockedTxInode;
use crate::{
    error::KernelError,
    fs::{
        BlockNo, SUPER_BLOCK, data_block,
        repr::{self, FS_BLOCK_SIZE, MAX_FILE, NUM_DIRECT_REFS, NUM_INDIRECT_REFS},
    },
    memory::{
        addr::{GenericMutSlice, GenericSlice},
        vm_user::UserPageTable,
    },
};

impl<const READ_ONLY: bool> LockedTxInode<'_, '_, READ_ONLY> {
    /// Returns the disk block address of the `i`th **direct** block in inode.
    ///
    /// If there is no such block, `get_data_block()` allocates one.
    /// Returns `None` if out of disk space.
    fn get_or_alloc_direct_data_block(&mut self, i: usize) -> Result<Option<BlockNo>, KernelError> {
        assert!(i < NUM_DIRECT_REFS);
        if let Some(bn) = self.data().addrs[i] {
            return Ok(Some(bn));
        }

        let Some(tx) = self.tx.to_writable() else {
            return Ok(None);
        };
        let bn = data_block::alloc(&tx, self.dev)?;
        self.data_mut().addrs[i] = Some(bn);
        Ok(Some(bn))
    }

    /// Returns the disk block address of the `i`th **indirect** block in inode.
    ///
    /// If there is no such block, `get_data_block()` allocates one.
    /// Returns `None` if out of disk space.
    fn get_or_alloc_indirect_data_block(
        &mut self,
        i: usize,
    ) -> Result<Option<BlockNo>, KernelError> {
        // Load indirect block, allocating if necessary.
        let (ind_bn, ind_newly_allocated) = if let Some(ind_bn) = self.data().addrs[NUM_DIRECT_REFS]
        {
            (ind_bn, false)
        } else {
            let Some(tx) = self.tx.to_writable() else {
                return Ok(None);
            };
            let ind_bn = data_block::alloc(&tx, self.dev)?;
            self.data_mut().addrs[NUM_DIRECT_REFS] = Some(ind_bn);
            (ind_bn, true)
        };

        if !ind_newly_allocated {
            let mut ind_br = self.tx.get_block(self.dev, ind_bn);
            let Ok(ind_bg) = ind_br.lock().read();
            if let Some(bn) = ind_bg.data::<repr::IndirectBlock>().get(i) {
                return Ok(Some(bn));
            }
        }

        let Some(tx) = self.tx.to_writable() else {
            return Ok(None);
        };
        let bn = data_block::alloc(&tx, self.dev)?;
        let mut ind_br = tx.get_block(self.dev, ind_bn);
        let Ok(mut ind_bg) = ind_br.lock().read();
        ind_bg.data_mut::<repr::IndirectBlock>().set(i, Some(bn));

        Ok(Some(bn))
    }

    /// Returns the disk block address of the `i`th block in inode.
    ///
    /// If there is no such block, `get_data_block()` allocates one.
    /// Returns `None` if out of disk space.
    fn get_or_alloc_data_block(&mut self, i: usize) -> Result<Option<BlockNo>, KernelError> {
        if i < NUM_DIRECT_REFS {
            return self.get_or_alloc_direct_data_block(i);
        }

        let i = i - NUM_DIRECT_REFS;
        if i < NUM_INDIRECT_REFS {
            return self.get_or_alloc_indirect_data_block(i);
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
    pub fn read(&mut self, mut dst: GenericMutSlice<u8>, off: usize) -> Result<usize, KernelError> {
        let data = self.data();
        let size = data.size as usize;
        if off > size || off.checked_add(dst.len()).is_none() {
            return Ok(0);
        }
        let len = usize::min(dst.len(), size - off);
        let mut dst = dst.take_mut(len);

        let mut tot = 0;
        while tot < dst.len() {
            let off = off + tot;
            let mut dst = dst.skip_mut(tot);
            let bn = match self.get_or_alloc_data_block(off / FS_BLOCK_SIZE) {
                Ok(Some(bn)) => bn,
                Ok(None) => break,
                Err(e) => {
                    if tot > 0 {
                        break;
                    }
                    return Err(e);
                }
            };
            let mut br = self.tx.get_block(self.dev, bn);
            let Ok(bg) = br.lock().read();
            let m = usize::min(dst.len(), FS_BLOCK_SIZE - off % FS_BLOCK_SIZE);
            let dst = dst.take_mut(m);
            if let Err(e) =
                UserPageTable::either_copy_out_bytes(dst, &bg.bytes()[off % FS_BLOCK_SIZE..][..m])
            {
                if tot > 0 {
                    break;
                }
                return Err(e);
            }
            tot += m;
        }
        Ok(tot)
    }

    /// Reads the inode's data as `T`.
    pub fn read_as<T>(&mut self, off: usize) -> Result<T, KernelError>
    where
        T: Pod,
    {
        let mut dst = T::zeroed();
        let read = self.read(dst.as_bytes_mut().into(), off)?;
        assert_eq!(read, size_of::<T>());
        Ok(dst)
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
    pub fn write(&mut self, src: GenericSlice<u8>, off: usize) -> Result<usize, KernelError> {
        if off
            .checked_add(src.len())
            .is_none_or(|end| end > MAX_FILE * FS_BLOCK_SIZE)
        {
            return Err(KernelError::FileTooLarge);
        }

        let size = self.data().size as usize;
        if off > size {
            // TODO: expand file as Linux does
            return Err(KernelError::WriteOffsetTooLarge);
        }

        let mut tot = 0;
        while tot < src.len() {
            let off = off + tot;
            let src = src.skip(tot);
            let bn = match self.get_or_alloc_data_block(off / FS_BLOCK_SIZE) {
                Ok(Some(bn)) => bn,
                Ok(None) => unreachable!(),
                Err(e) => {
                    if tot > 0 {
                        break;
                    }
                    return Err(e);
                }
            };

            let mut br = self.tx.get_block(self.dev, bn);
            let Ok(mut bg) = br.lock().read();
            let m = usize::min(src.len(), FS_BLOCK_SIZE - off % FS_BLOCK_SIZE);
            let src = src.take(m);
            if let Err(e) = UserPageTable::either_copy_in_bytes(
                &mut bg.bytes_mut()[off % FS_BLOCK_SIZE..][..m],
                src,
            ) {
                if tot > 0 {
                    break;
                }
                return Err(e);
            }

            tot += m;
        }

        if off + tot > size {
            self.data_mut().size = (off + tot).try_into().unwrap();
        }

        // write the i-node back to disk even if the size didn't change
        // because the loop above might have called inode_block_map() and added a new
        // block to `ip.addrs`.`
        self.update();

        Ok(tot)
    }

    /// Writes `data` to inode.
    pub fn write_data<T>(&mut self, off: usize, data: &T) -> Result<(), KernelError>
    where
        T: Pod,
    {
        let written = self.write(data.as_bytes().into(), off)?;
        assert_eq!(written, size_of::<T>());
        Ok(())
    }
}
