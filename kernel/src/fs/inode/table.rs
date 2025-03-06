use ov6_fs_types::InodeNo;
use ov6_kernel_params::NINODE;

use crate::{
    error::KernelError,
    fs::DeviceNo,
    sync::{SleepLock, SpinLock, SpinLockGuard},
};

use super::{InodeDataArc, InodeDataWeak};

static INODE_TABLE: SpinLock<InodeTable> = SpinLock::new(InodeTable::new());

pub(super) fn get_or_insert(dev: DeviceNo, ino: InodeNo) -> Result<InodeDataArc, KernelError> {
    INODE_TABLE.lock().get_or_insert(dev, ino)
}

pub(super) fn lock() -> SpinLockGuard<'static, InodeTable> {
    INODE_TABLE.lock()
}

pub(super) struct InodeTable {
    table: [Option<(DeviceNo, InodeNo, InodeDataWeak)>; NINODE],
}

impl InodeTable {
    const fn new() -> Self {
        Self {
            table: [const { None }; NINODE],
        }
    }

    fn get_or_insert(&mut self, dev: DeviceNo, ino: InodeNo) -> Result<InodeDataArc, KernelError> {
        let mut empty_idx = None;
        for (i, entry) in self.table.iter_mut().enumerate() {
            let Some(entry_body) = entry else {
                empty_idx = Some(i);
                continue;
            };

            if let Some(data) = InodeDataWeak::upgrade(&entry_body.2) {
                if entry_body.0 == dev && entry_body.1 == ino {
                    return Ok(data);
                }
                continue;
            }

            // If reference is not available, remove it
            *entry = None;
            empty_idx = Some(i);
        }

        let Some(empty_idx) = empty_idx else {
            return Err(KernelError::Unknown);
        };

        // insert new entry
        let data = InodeDataArc::try_new(SleepLock::new(None))?;
        self.table[empty_idx] = Some((dev, ino, InodeDataArc::downgrade(&data)));
        Ok(data)
    }
}
