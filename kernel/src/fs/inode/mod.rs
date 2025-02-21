//! Inodes.
//!
//! An inode describes a single unnamed file.
//! The inode disk structure holds metadata: the file's type,
//! its size, the number of links referring to it, and the
//! list of blocks holding the file's content.
//!
//! The inodes are laid out sequentially on disk at block
//! `sb.inodestart`. Each inode has a number, indicating its
//! position on the disk.
//!
//! The kernel keeps a table of in-use inodes in memory
//! to provide a place for synchronizing access
//! to inodes used by multiple processes. The in-memory
//! inodes include book-keeping information that is
//! not stored on disk.
//!
//! An inode and its in-memory representation go through a
//! sequence of states before they can be used by the
//! rest of the file system code.
//!
//! * Allocation: an inode is allocated if its type (on disk)
//!   is non-zero. [`TxInode::alloc()`] allocates, and
//!   [`TxInode::drop()`] (destructor) or [`TxInode::put()`] frees if
//!   the reference and link counts have fallen to zero.
//!
//! * Referencing in table: an entry in the inode table
//!   is free if reference count is zero. Otherwise tracks
//!   the number of in-memory pointers to the entry (open
//!   files and current directories). [`TxInode::get()`] finds or
//!   creates a table entry and increments its ref;
//!   [`TxInode::drop()`] (destructor) or [`TxInode::put()`]
//!   decrements ref.
//!
//! * Valid: the information (type, size, &c) in an inode
//!   table entry is only correct when `data` is `Some`.
//!   [`TxInode::lock()`] reads the inode from
//!   the disk and sets [`TxInode::data`], while [`TxInode::put()`] clears
//!   [`TxInode::data`] if reference count has fallen to zero.
//!
//! * Locked: file system code may only examine and modify
//!   the information in an inode and its content if it
//!   has first locked the inode.
//!
//! Thus a typical sequence is:
//!
//!   ```
//!   let mut ip = Inode::get(dev, inum)
//!   let locked = ip.lock();
//!   ... examine and modify ip.xxx ...
//!
//!   // they are optional
//!   locked.unlock()
//!   ip.put()
//!    ```
//!
//! [`TxInode::lock()`] is separate from [`TxInode::get()`] so that system calls can
//! get a long-term reference to an inode (as for an open file)
//! and only lock it for short periods (e.g., in `read()`).
//! The separation also helps avoid deadlock and races during
//! pathname lookup. [`TxInode::get()`] increments reference count so that the inode
//! stays in the table and pointers to it remain valid.
//!
//! Many internal file system functions expect the caller to
//! have locked the inodes involved; this lets callers create
//! multi-step atomic operations.

use alloc::sync::Arc;

use crate::{
    param::NINODE,
    sync::{SleepLock, SleepLockGuard, SpinLock},
};

use super::{
    BlockNo, DeviceNo, InodeNo, SUPER_BLOCK, Tx,
    repr::{self, NUM_DIRECT_REFS},
    stat::Stat,
};

mod content;
mod directory;

type InodeDataPtr = Arc<SleepLock<Option<InodeData>>>;
type InodeDataGuard<'a> = SleepLockGuard<'a, Option<InodeData>>;

#[derive(Clone)]
pub struct Inode {
    dev: DeviceNo,
    inum: InodeNo,
    data: InodeDataPtr,
}

/// In-memory copy of an inode.
#[derive(Clone)]
pub struct TxInode<'tx, const READ_ONLY: bool> {
    tx: &'tx Tx<'tx, READ_ONLY>,
    dev: DeviceNo,
    inum: InodeNo,
    data: InodeDataPtr,
}

pub(super) struct InodeData {
    pub(super) ty: i16,
    pub(super) major: i16,
    pub(super) minor: i16,
    pub(super) nlink: i16,
    size: u32,
    addrs: [Option<BlockNo>; NUM_DIRECT_REFS + 1],
}

impl InodeData {
    fn from_repr(r: &repr::Inode) -> Self {
        let mut addrs = [None; NUM_DIRECT_REFS + 1];
        r.read_addrs(&mut addrs);
        Self {
            ty: r.ty,
            major: r.major,
            minor: r.minor,
            nlink: r.nlink,
            size: r.size,
            addrs,
        }
    }

    fn write_repr(&self, r: &mut repr::Inode) {
        r.ty = self.ty;
        r.major = self.major;
        r.nlink = self.nlink;
        r.size = self.size;
        r.write_addrs(&self.addrs);
    }
}

pub struct LockedTxInode<'tx, 'i, const READ_ONLY: bool> {
    tx: &'tx Tx<'tx, READ_ONLY>,
    dev: DeviceNo,
    inum: InodeNo,
    data: InodeDataPtr,
    locked: InodeDataGuard<'i>,
}

struct InodeEntry {
    dev: DeviceNo,
    inum: InodeNo,
    data: InodeDataPtr,
}

impl InodeEntry {
    fn new(dev: DeviceNo, inum: InodeNo) -> Self {
        Self {
            dev,
            inum,
            data: Arc::new(SleepLock::new(None)),
        }
    }

    /// Resets the `InodeEntry`.
    ///
    /// Caller must ensures that no other reference to this entry exist.
    fn reset(&mut self, dev: DeviceNo, inum: InodeNo) -> Result<(), ()> {
        let data = Arc::get_mut(&mut self.data).ok_or(())?;
        *data.try_lock()? = None;
        self.dev = dev;
        self.inum = inum;
        Ok(())
    }
}

static INODE_TABLE: SpinLock<[Option<InodeEntry>; NINODE]> =
    SpinLock::new([const { None }; NINODE]);

impl<const READ_ONLY: bool> TxInode<'_, READ_ONLY> {
    pub fn dev(&self) -> DeviceNo {
        self.dev
    }

    pub fn inum(&self) -> InodeNo {
        self.inum
    }
}

impl Inode {
    pub fn from_tx<const READ_ONLY: bool>(tx: &TxInode<'_, READ_ONLY>) -> Self {
        Self {
            dev: tx.dev,
            inum: tx.inum,
            data: Arc::clone(&tx.data),
        }
    }

    pub fn from_locked<const READ_ONLY: bool>(locked: &LockedTxInode<'_, '_, READ_ONLY>) -> Self {
        Self {
            dev: locked.dev,
            inum: locked.inum,
            data: Arc::clone(&locked.data),
        }
    }

    pub fn to_tx<'a, const READ_ONLY: bool>(
        &self,
        tx: &'a Tx<READ_ONLY>,
    ) -> TxInode<'a, READ_ONLY> {
        TxInode {
            tx,
            dev: self.dev,
            inum: self.inum,
            data: Arc::clone(&self.data),
        }
    }
}

impl<'tx, const READ_ONLY: bool> TxInode<'tx, READ_ONLY> {
    fn new(tx: &'tx Tx<READ_ONLY>, dev: DeviceNo, inum: InodeNo, data: InodeDataPtr) -> Self {
        TxInode {
            tx,
            dev,
            inum,
            data,
        }
    }

    /// Finds the inode with number `inum` on device `dev`.
    ///
    /// Returns the in-memory inode copy.
    pub fn get(tx: &'tx Tx<READ_ONLY>, dev: DeviceNo, inum: InodeNo) -> Self {
        let mut table = INODE_TABLE.lock();

        let mut empty = None;
        let iter = table.iter_mut().find_map(|entry_ref| {
            // Recycle empty entry
            let Some(entry) = entry_ref else {
                empty = Some(entry_ref);
                return None;
            };

            // Recycle unreferred entry
            if Arc::get_mut(&mut entry.data).is_some() {
                empty = Some(entry_ref);
                return None;
            }

            if entry.dev != dev || entry.inum != inum {
                return None;
            }
            let data = Arc::clone(&entry.data);
            Some(TxInode::new(tx, dev, inum, data))
        });

        if let Some(found) = iter {
            return found;
        }

        let Some(empty) = empty else {
            panic!("no inodes");
        };

        let data = match empty {
            Some(entry) => {
                entry.reset(dev, inum).unwrap();
                Arc::clone(&entry.data)
            }
            None => {
                let entry = InodeEntry::new(dev, inum);
                let data = Arc::clone(&entry.data);
                *empty = Some(entry);
                data
            }
        };

        TxInode::new(tx, dev, inum, data)
    }

    /// Drops a reference to an in-memory inode.
    ///
    /// If that was the last reference, the inode table entry can
    /// be recycled.
    /// If that was the last reference and the inode has no links
    /// to it, free the inode (and its content) on disk.
    /// All calls to `put()` must inside a transaction in
    /// case it has to free the inode.
    pub fn put(self) {
        // this is a no-op because the inode is dropped
    }

    /// Attempts to lock the inode.
    ///
    /// This also reads the inode from disk if it is not already in memory.
    /// Returns `Err()` if the inode is already locked.
    pub fn try_lock<'a>(&'a mut self) -> Result<LockedTxInode<'tx, 'a, READ_ONLY>, ()> {
        let locked = self.data.try_lock()?;
        Ok(LockedTxInode::new(
            self.tx,
            self.dev,
            self.inum,
            Arc::clone(&self.data),
            locked,
        ))
    }

    /// Locks the inode.
    ///
    /// This also reads the inode from disk if it is not already in memory.
    pub fn lock<'a>(&'a mut self) -> LockedTxInode<'tx, 'a, READ_ONLY> {
        let locked = self.data.lock();
        LockedTxInode::new(self.tx, self.dev, self.inum, Arc::clone(&self.data), locked)
    }
}

impl<'tx> TxInode<'tx, false> {
    /// Allocates an inode on device `dev`
    ///
    /// Returns a n unlocked but allocated and referenced inode,
    /// or `Err()` if there is no free inode.
    pub fn alloc(tx: &'tx Tx<false>, dev: DeviceNo, ty: i16) -> Result<TxInode<'tx, false>, ()> {
        let inum = alloc_inum(tx, dev, ty)?;
        Ok(Self::get(tx, dev, inum))
    }
}

impl<const READ_ONLY: bool> Drop for TxInode<'_, READ_ONLY> {
    fn drop(&mut self) {
        let mut _table = INODE_TABLE.lock();

        if Arc::strong_count(&self.data) > 2 {
            return;
        }

        // strong_count == 2 means no other process can have self locked,
        // so this acquires won't block (or deadlock).
        let lip = self.try_lock().unwrap();

        if lip.data().nlink > 0 {
            return;
        }

        // inode has no links and no other references: truncate and free

        drop(_table);

        if let Some(tx) = lip.tx.to_writable() {
            let mut lip = LockedTxInode {
                tx: &*tx,
                dev: lip.dev,
                inum: lip.inum,
                data: lip.data,
                locked: lip.locked,
            };
            lip.truncate();
            lip.free();
        }
    }
}

impl<'tx, 'i, const READ_ONLY: bool> LockedTxInode<'tx, 'i, READ_ONLY> {
    fn new(
        tx: &'tx Tx<'tx, READ_ONLY>,
        dev: DeviceNo,
        inum: InodeNo,
        data: InodeDataPtr,
        mut locked: InodeDataGuard<'i>,
    ) -> LockedTxInode<'tx, 'i, READ_ONLY> {
        if locked.is_none() {
            // read data from disk
            let sb = SUPER_BLOCK.get();
            let mut br = tx.get_block(dev, sb.inode_block(inum));
            let Ok(bg) = br.lock().read();
            let dip = bg.data::<repr::InodeBlock>().inode(inum);
            *locked = Some(InodeData::from_repr(dip));
        }

        LockedTxInode {
            tx,
            dev,
            inum,
            data,
            locked,
        }
    }

    pub fn dev(&self) -> DeviceNo {
        self.dev
    }

    pub fn inum(&self) -> InodeNo {
        self.inum
    }

    pub fn ty(&self) -> i16 {
        self.data().ty
    }

    pub fn major(&self) -> i16 {
        self.data().major
    }

    // pub fn minor(&self) -> i16 {
    //     self.data().minor
    // }

    pub(super) fn data(&self) -> &InodeData {
        self.locked.as_ref().unwrap()
    }

    pub(super) fn data_mut(&mut self) -> &mut InodeData {
        self.locked.as_mut().unwrap()
    }

    /// Copies stat information from inode.
    pub fn stat(&self) -> Stat {
        let data = self.data();
        Stat {
            dev: self.dev,
            ino: self.inum,
            ty: data.ty,
            nlink: data.nlink,
            size: u64::from(data.size),
        }
    }

    /// Unlocks the inode.
    pub fn unlock(self) {
        // this is a no-op because the guard is dropped
    }
}

/// Allocates an inode on device `dev`.
///
/// Marks it as allocated by giving it type `ty`.
/// Returns an allocated inode number or Err() if there is no free inode.
fn alloc_inum(tx: &Tx<false>, dev: DeviceNo, ty: i16) -> Result<InodeNo, ()> {
    let sb = SUPER_BLOCK.get();

    for inum in 1..(sb.ninodes) {
        let inum = InodeNo::new(inum);
        let mut br = tx.get_block(dev, sb.inode_block(inum));
        let Ok(mut bg) = br.lock().read();
        let disk_ip = bg.data_mut::<repr::InodeBlock>().inode_mut(inum);
        if disk_ip.is_free() {
            disk_ip.allocate(ty);
            return Ok(inum);
        }
    }
    crate::println!("no free inodes");
    Err(())
}
