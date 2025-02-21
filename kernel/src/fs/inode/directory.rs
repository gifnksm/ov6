//! Directories

use dataview::PodMethods as _;

use crate::{
    fs::{DeviceNo, InodeNo, repr, stat::T_DIR},
    proc::Proc,
};

use super::{LockedTxInode, TxInode};

// TODO: refactoring. Add some utility methods to access DirEntry.

impl<'tx, 'i, const READ_ONLY: bool> LockedTxInode<'tx, 'i, READ_ONLY> {
    pub fn is_dir(&self) -> bool {
        self.data().ty == T_DIR
    }

    pub fn as_dir<'l>(&'l mut self) -> Option<DirInode<'tx, 'i, 'l, READ_ONLY>> {
        if self.is_dir() {
            Some(DirInode(self))
        } else {
            None
        }
    }
}

pub struct DirInode<'tx, 'i, 'l, const READ_ONLY: bool>(&'l mut LockedTxInode<'tx, 'i, READ_ONLY>);

impl<'tx, 'i, 'l, const READ_ONLY: bool> DirInode<'tx, 'i, 'l, READ_ONLY> {
    pub fn dev(&self) -> DeviceNo {
        self.0.dev()
    }

    pub fn inum(&self) -> InodeNo {
        self.0.inum()
    }

    pub fn get_inner<'s>(&'s mut self) -> &'s mut LockedTxInode<'tx, 'i, READ_ONLY>
    where
        'l: 's,
    {
        self.0
    }
}

impl<const READ_ONLY: bool> DirInode<'_, '_, '_, READ_ONLY> {
    /// Returns `true` if the directory is empty except for `"."` and `".."`.
    pub fn is_empty(&mut self, p: &Proc) -> bool {
        let de_size = size_of::<repr::DirEntry>();
        let size = self.0.data().size as usize;
        // skip first two entry ("." and "..").
        for off in (2 * de_size..size).step_by(de_size) {
            let de = self.0.read_as::<repr::DirEntry>(p, off).unwrap();
            if de.inum().is_some() {
                return false;
            }
        }
        true
    }
}

impl<'tx, const READ_ONLY: bool> DirInode<'tx, '_, '_, READ_ONLY> {
    /// Looks up for a directory entry by given `name`.
    ///
    /// Returns a inode that contains the entry and its offset from inode data head.
    pub fn lookup(&mut self, p: &Proc, name: &[u8]) -> Option<(TxInode<'tx, READ_ONLY>, usize)> {
        for off in (0..self.0.data().size as usize).step_by(size_of::<repr::DirEntry>()) {
            let de = self.0.read_as::<repr::DirEntry>(p, off).unwrap();
            let Some(inum) = de.inum() else { continue };
            if !de.is_same_name(name) {
                continue;
            }
            let ip = TxInode::get(self.0.tx, self.0.dev, inum);
            return Some((ip, off));
        }
        None
    }
}

impl DirInode<'_, '_, '_, false> {
    /// Writes a new directory entry (`name` and `inum`) into the directory.
    pub fn link(&mut self, p: &Proc, name: &[u8], inum: InodeNo) -> Result<(), ()> {
        // Check that name is not present.
        if self.lookup(p, name).is_some() {
            return Err(());
        }

        // Looks for an empty dirent.
        let size = self.0.data().size as usize;
        assert_eq!(size % size_of::<repr::DirEntry>(), 0);

        let (mut de, off) = (0..size)
            .step_by(size_of::<repr::DirEntry>())
            .map(|off| {
                let de = self.0.read_as::<repr::DirEntry>(p, off).unwrap();
                (de, off)
            })
            .find(|(de, _)| de.inum().is_none())
            .unwrap_or((repr::DirEntry::zeroed(), size));

        de.set_name(name);
        de.set_inum(Some(inum));
        self.0.write_data(p, off, &de)?;
        // write_inode_data(tx, p, NonNull::new(dp).unwrap(), off, de)?;
        Ok(())
    }
}
