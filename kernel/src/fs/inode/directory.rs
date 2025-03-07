//! Directories

use dataview::PodMethods as _;
use ov6_types::os_str::OsStr;

use crate::{
    error::KernelError,
    fs::{
        DeviceNo, InodeNo,
        repr::{self, T_DIR},
    },
    proc::ProcPrivateData,
};

use super::{LockedTxInode, TxInode};

// TODO: refactoring. Add some utility methods to access DirEntry.

impl<'tx, 'i, const READ_ONLY: bool> LockedTxInode<'tx, 'i, READ_ONLY> {
    pub fn is_dir(&self) -> bool {
        self.data().ty == T_DIR
    }

    pub fn as_dir<'l>(&'l mut self) -> Option<DirInode<'tx, 'i, 'l, READ_ONLY>> {
        self.is_dir().then_some(DirInode(self))
    }
}

pub struct DirInode<'tx, 'i, 'l, const READ_ONLY: bool>(&'l mut LockedTxInode<'tx, 'i, READ_ONLY>);

impl<'tx, 'i, 'l, const READ_ONLY: bool> DirInode<'tx, 'i, 'l, READ_ONLY> {
    pub fn dev(&self) -> DeviceNo {
        self.0.dev()
    }

    pub fn ino(&self) -> InodeNo {
        self.0.ino()
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
    pub fn is_empty(&mut self, private: &mut ProcPrivateData) -> bool {
        let de_size = size_of::<repr::DirEntry>();
        let size = self.0.data().size as usize;
        // skip first two entry ("." and "..").
        for off in (2 * de_size..size).step_by(de_size) {
            let de = self.0.read_as::<repr::DirEntry>(private, off).unwrap();
            if de.ino().is_some() {
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
    pub fn lookup(
        &mut self,
        private: &mut ProcPrivateData,
        name: &OsStr,
    ) -> Option<(TxInode<'tx, READ_ONLY>, usize)> {
        for off in (0..self.0.data().size as usize).step_by(size_of::<repr::DirEntry>()) {
            let de = self.0.read_as::<repr::DirEntry>(private, off).unwrap();
            let Some(ino) = de.ino() else { continue };
            if !de.is_same_name(name) {
                continue;
            }
            let ip = TxInode::get(self.0.tx, self.0.dev, ino);
            return Some((ip, off));
        }
        None
    }
}

impl DirInode<'_, '_, '_, false> {
    /// Writes a new directory entry (`name` and `ino`) into the directory.
    pub fn link(
        &mut self,
        private: &mut ProcPrivateData,
        name: &OsStr,
        ino: InodeNo,
    ) -> Result<(), KernelError> {
        // Check that name is not present.
        if self.lookup(private, name).is_some() {
            return Err(KernelError::Unknown);
        }

        // Looks for an empty dirent.
        let size = self.0.data().size as usize;
        assert_eq!(size % size_of::<repr::DirEntry>(), 0);

        let (mut de, off) = (0..size)
            .step_by(size_of::<repr::DirEntry>())
            .map(|off| {
                let de = self.0.read_as::<repr::DirEntry>(private, off).unwrap();
                (de, off)
            })
            .find(|(de, _)| de.ino().is_none())
            .unwrap_or_else(|| (repr::DirEntry::zeroed(), size));

        de.set_name(name);
        de.set_ino(Some(ino));
        self.0.write_data(private, off, &de)?;
        // write_inode_data(tx, p, NonNull::new(dp).unwrap(), off, de)?;
        Ok(())
    }
}
