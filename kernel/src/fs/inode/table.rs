use alloc::sync::{Arc, Weak};
use xv6_fs_types::InodeNo;
use xv6_kernel_params::NINODE;

use crate::{
    error::Error,
    fs::DeviceNo,
    sync::{SleepLock, SpinLock, SpinLockGuard},
};

use super::{InodeDataPtr, InodeDataWeakPtr};

static INODE_TABLE: SpinLock<InodeTable> = SpinLock::new(InodeTable::new());

pub(super) fn get_or_insert(dev: DeviceNo, ino: InodeNo) -> Result<InodeDataPtr, Error> {
    INODE_TABLE.lock().get_or_insert(dev, ino)
}

pub(super) fn lock() -> SpinLockGuard<'static, InodeTable> {
    INODE_TABLE.lock()
}

pub(super) struct InodeTable {
    table: [Option<(DeviceNo, InodeNo, InodeDataWeakPtr)>; NINODE],
}

impl InodeTable {
    const fn new() -> Self {
        Self {
            table: [const { None }; NINODE],
        }
    }

    fn get_or_insert(&mut self, dev: DeviceNo, ino: InodeNo) -> Result<InodeDataPtr, Error> {
        let mut empty_idx = None;
        for (i, entry) in self.table.iter_mut().enumerate() {
            let Some(entry_body) = entry else {
                empty_idx = Some(i);
                continue;
            };

            if let Some(data) = Weak::upgrade(&entry_body.2) {
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
            return Err(Error::Unknown);
        };

        // insert new entry
        let data = Arc::new(SleepLock::new(None));
        self.table[empty_idx] = Some((dev, ino, Arc::downgrade(&data)));
        Ok(data)
    }
}
