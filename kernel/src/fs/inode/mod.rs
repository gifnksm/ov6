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
//! * Allocation: an inode is allocated if its type (on disk) is non-zero.
//!   [`TxInode::alloc()`] allocates, and [`TxInode::drop()`] (destructor) or
//!   [`TxInode::put()`] frees if the reference and link counts have fallen to
//!   zero.
//!
//! * Referencing in table: an entry in the inode table is free if reference
//!   count is zero. Otherwise tracks the number of in-memory pointers to the
//!   entry (open files and current directories). [`TxInode::get()`] finds or
//!   creates a table entry and increments its ref; [`TxInode::drop()`]
//!   (destructor) or [`TxInode::put()`] decrements ref.
//!
//! * Valid: the information (type, size, &c) in an inode table entry is only
//!   correct when `data` is `Some`. [`TxInode::lock()`] reads the inode from
//!   the disk and sets [`TxInode::data`], while [`TxInode::put()`] clears
//!   [`TxInode::data`] if reference count has fallen to zero.
//!
//! * Locked: file system code may only examine and modify the information in an
//!   inode and its content if it has first locked the inode.
//!
//! Thus a typical sequence is:
//!
//!   ```
//!   let mut ip = Inode::get(dev, ino)
//!   let locked = ip.lock();
//!   ... examine and modify ip.xxx ...
//!
//!   // they are optional
//!   locked.unlock()
//!   ip.put()
//!    ```
//!
//! [`TxInode::lock()`] is separate from [`TxInode::get()`] so that system calls
//! can get a long-term reference to an inode (as for an open file)
//! and only lock it for short periods (e.g., in `read()`).
//! The separation also helps avoid deadlock and races during
//! pathname lookup. [`TxInode::get()`] increments reference count so that the
//! inode stays in the table and pointers to it remain valid.
//!
//! Many internal file system functions expect the caller to
//! have locked the inodes involved; this lets callers create
//! multi-step atomic operations.

use self::alloc::{InodeDataArc, InodeDataWeak};
use super::{
    BlockNo, DeviceNo, InodeNo, SUPER_BLOCK, Tx,
    repr::{self, NUM_DIRECT_REFS},
};
use crate::{error::KernelError, sync::SleepLockGuard};

mod alloc;
mod content;
mod directory;
mod table;

pub(super) fn init() {
    alloc::init();
}

type InodeDataGuard<'a> = SleepLockGuard<'a, Option<InodeData>>;

#[derive(Clone)]
pub struct Inode {
    dev: DeviceNo,
    ino: InodeNo,
    data: Option<InodeDataArc>,
}

/// In-memory copy of an inode.
#[derive(Clone)]
pub struct TxInode<'tx, const READ_ONLY: bool> {
    tx: &'tx Tx<'tx, READ_ONLY>,
    dev: DeviceNo,
    ino: InodeNo,
    data: InodeDataArc,
}

pub(super) struct InodeData {
    pub(super) ty: i16,
    pub(super) major: DeviceNo,
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
            major: DeviceNo::new(u32::try_from(r.major).unwrap()),
            minor: r.minor,
            nlink: r.nlink,
            size: r.size,
            addrs,
        }
    }

    fn write_repr(&self, r: &mut repr::Inode) {
        r.ty = self.ty;
        r.major = self.major.value().try_into().unwrap();
        r.nlink = self.nlink;
        r.size = self.size;
        r.write_addrs(&self.addrs);
    }
}

pub struct LockedTxInode<'tx, 'i, const READ_ONLY: bool> {
    tx: &'tx Tx<'tx, READ_ONLY>,
    dev: DeviceNo,
    ino: InodeNo,
    data: InodeDataArc,
    locked: InodeDataGuard<'i>,
}

impl<const READ_ONLY: bool> TxInode<'_, READ_ONLY> {
    pub fn dev(&self) -> DeviceNo {
        self.dev
    }

    pub fn ino(&self) -> InodeNo {
        self.ino
    }
}

impl Inode {
    pub fn from_tx<const READ_ONLY: bool>(tx: &TxInode<'_, READ_ONLY>) -> Self {
        Self {
            dev: tx.dev,
            ino: tx.ino,
            data: Some(InodeDataArc::clone(&tx.data)),
        }
    }

    pub fn from_locked<const READ_ONLY: bool>(locked: &LockedTxInode<'_, '_, READ_ONLY>) -> Self {
        Self {
            dev: locked.dev,
            ino: locked.ino,
            data: Some(InodeDataArc::clone(&locked.data)),
        }
    }

    pub fn into_tx<'a, const READ_ONLY: bool>(
        mut self,
        tx: &'a Tx<READ_ONLY>,
    ) -> TxInode<'a, READ_ONLY> {
        TxInode {
            tx,
            dev: self.dev,
            ino: self.ino,
            data: self.data.take().unwrap(),
        }
    }
}

impl Drop for Inode {
    fn drop(&mut self) {
        assert!(
            self.data.is_none(),
            "Inode::into_tx() must be called before dropped"
        );
    }
}

impl<'tx, const READ_ONLY: bool> TxInode<'tx, READ_ONLY> {
    fn new(tx: &'tx Tx<READ_ONLY>, dev: DeviceNo, ino: InodeNo, data: InodeDataArc) -> Self {
        TxInode { tx, dev, ino, data }
    }

    /// Finds the inode with number `ino` on device `dev`.
    ///
    /// Returns the in-memory inode copy.
    pub fn get(tx: &'tx Tx<READ_ONLY>, dev: DeviceNo, ino: InodeNo) -> Self {
        let Ok(data) = table::get_or_insert(dev, ino) else {
            panic!("cannot allocate in-memory inode entry any more");
        };
        TxInode::new(tx, dev, ino, data)
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
        let _ = self;
    }

    /// Attempts to lock the inode.
    ///
    /// This also reads the inode from disk if it is not already in memory.
    /// Returns `Err()` if the inode is already locked.
    #[expect(clippy::needless_pass_by_ref_mut)]
    pub fn try_lock<'a>(&'a mut self) -> Result<LockedTxInode<'tx, 'a, READ_ONLY>, KernelError> {
        let locked = self.data.try_lock()?;
        Ok(LockedTxInode::new(
            self.tx,
            self.dev,
            self.ino,
            InodeDataArc::clone(&self.data),
            locked,
        ))
    }

    /// Locks the inode.
    ///
    /// This also reads the inode from disk if it is not already in memory.
    #[expect(clippy::needless_pass_by_ref_mut)]
    pub fn lock<'a>(&'a mut self) -> LockedTxInode<'tx, 'a, READ_ONLY> {
        let locked = self.data.lock();
        LockedTxInode::new(
            self.tx,
            self.dev,
            self.ino,
            InodeDataArc::clone(&self.data),
            locked,
        )
    }
}

impl<'tx> TxInode<'tx, false> {
    /// Allocates an inode on device `dev`
    ///
    /// Returns a n unlocked but allocated and referenced inode,
    /// or `Err()` if there is no free inode.
    pub fn alloc(tx: &'tx Tx<false>, dev: DeviceNo, ty: i16) -> Result<Self, KernelError> {
        let ino = alloc_ino(tx, dev, ty)?;
        Ok(Self::get(tx, dev, ino))
    }
}

impl<const READ_ONLY: bool> Drop for TxInode<'_, READ_ONLY> {
    fn drop(&mut self) {
        let table = table::lock();
        if InodeDataArc::strong_count(&self.data) > 1 {
            return;
        }

        // strong_count == 1 means no other process can have self locked,
        // so this acquires won't block (or deadlock).
        let lip = self.try_lock().unwrap();

        // if the file is referenced in file system, do nothing
        if lip.data().nlink > 0 {
            return;
        }

        drop(table);

        // remove inode on disk
        if let Some(tx) = lip.tx.to_writable() {
            let mut lip = LockedTxInode {
                tx: &*tx,
                dev: lip.dev,
                ino: lip.ino,
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
        ino: InodeNo,
        data: InodeDataArc,
        mut locked: InodeDataGuard<'i>,
    ) -> Self {
        if locked.is_none() {
            // read data from disk
            let sb = SUPER_BLOCK.get();
            let mut br = tx.get_block(dev, sb.inode_block(ino));
            let Ok(bg) = br.lock().read();
            let dip = bg.data::<repr::InodeBlock>().inode(ino);
            *locked = Some(InodeData::from_repr(dip));
        }

        LockedTxInode {
            tx,
            dev,
            ino,
            data,
            locked,
        }
    }

    pub fn dev(&self) -> DeviceNo {
        self.dev
    }

    pub fn ino(&self) -> InodeNo {
        self.ino
    }

    pub fn ty(&self) -> i16 {
        self.data().ty
    }

    pub fn nlink(&self) -> i16 {
        self.data().nlink
    }

    pub fn size(&self) -> u32 {
        self.data().size
    }

    pub fn major(&self) -> DeviceNo {
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

    /// Unlocks the inode.
    pub fn unlock(self) {
        // this is a no-op because the guard is dropped
        let _ = self;
    }
}

/// Allocates an inode on device `dev`.
///
/// Marks it as allocated by giving it type `ty`.
/// Returns an allocated inode number or `Err()` if there is no free inode.
fn alloc_ino(tx: &Tx<false>, dev: DeviceNo, ty: i16) -> Result<InodeNo, KernelError> {
    let sb = SUPER_BLOCK.get();

    for ino in 1..(sb.ninodes) {
        let ino = InodeNo::new(ino);
        let mut br = tx.get_block(dev, sb.inode_block(ino));
        let Ok(mut bg) = br.lock().read();
        let disk_ip = bg.data_mut::<repr::InodeBlock>().inode_mut(ino);
        if disk_ip.is_free() {
            disk_ip.allocate(ty);
            return Ok(ino);
        }
    }
    crate::println!("no free inodes");
    Err(KernelError::Unknown)
}
