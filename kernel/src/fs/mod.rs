//! File system implementation.
//!
//! Five layers:
//!   + Blocks: allocator for raw disk blocks.
//!   + Log: crash recovery for multi-step updates.
//!   + Files: inode allocator, reading, writing, metadata.
//!   + Directories: inode with special contents (list of other inodes!)
//!   + Names: paths like /usr/rtm/xv6/fs.c for convenient naming.
//!
//! This file contains the low-level file system manipulation
//! routines. The (higher-level) system call implementations
//! are in syscall_file.rs

use core::{
    cell::UnsafeCell,
    mem::{self, MaybeUninit},
    num::{NonZeroU16, NonZeroU32},
    ptr::{self, NonNull},
};

use crate::{
    file::Inode,
    fs::stat::{Stat, T_DEVICE, T_DIR, T_FILE},
    memory::vm::VirtAddr,
    param::{NINODE, ROOT_DEV},
    proc::{self, Proc},
    sync::SpinLock,
};

pub mod block_io;
pub mod log;
pub mod stat;
pub mod virtio;
pub mod virtio_disk;

/// Super block of the file system.
///
/// Disk layout:
/// `[ boot block | super block | log | inode blocks | free bit map | data blocks ]`
///
/// mkfs computes the super block and builds an initial file system. The
/// super block describes the disk layout:
#[repr(C)]
pub struct SuperBlock {
    /// Magic number. Must be FSMAGIC
    magic: u32,
    /// Size of file system image (blocks)
    size: u32,
    /// Number of data blocks
    nblocks: u32,
    /// Number of inodes.
    ninodes: u32,
    /// Number of log blocks.
    pub nlog: u32,
    /// Block number of first log block.
    pub logstart: u32,
    /// Block number of first inode block.
    inodestart: u32,
    /// Block number of first free map block.
    bmapstart: u32,
}

/// Root i-number
const ROOT_INO: InodeNo = InodeNo::new(1).unwrap();
/// Block size
const BLOCK_SIZE: usize = 1024;

const FS_MAGIC: u32 = 0x10203040;
pub const NDIRECT: usize = 12;
pub const NINDIRECT: usize = BLOCK_SIZE / size_of::<u32>();
const MAX_FILE: usize = NDIRECT + NINDIRECT;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
#[repr(transparent)]
pub struct DeviceNo(NonZeroU32);

impl DeviceNo {
    pub const INVALID: Self = Self(NonZeroU32::new(u32::MAX).unwrap());

    pub const fn new(n: u32) -> Option<Self> {
        let Some(n) = NonZeroU32::new(n) else {
            return None;
        };
        Some(Self(n))
    }

    // pub const fn value(&self) -> u32 {
    //     self.0.get()
    // }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
#[repr(transparent)]
pub struct BlockNo(NonZeroU32);

impl BlockNo {
    pub const INVALID: Self = Self(NonZeroU32::new(u32::MAX).unwrap());

    pub const fn new(n: u32) -> Option<Self> {
        let Some(n) = NonZeroU32::new(n) else {
            return None;
        };
        Some(Self(n))
    }

    pub const fn value(&self) -> u32 {
        self.0.get()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
#[repr(transparent)]
pub struct InodeNo(NonZeroU32);

impl InodeNo {
    pub const fn new(n: u32) -> Option<Self> {
        let Some(n) = NonZeroU32::new(n) else {
            return None;
        };
        Some(Self(n))
    }

    pub const fn value(&self) -> u32 {
        self.0.get()
    }
}

#[repr(C)]
pub struct DInode {
    /// File type
    ty: i16,
    /// Major device number (T_DEVICE only)
    major: i16,
    /// Minor device number (T_DEVICE only)
    minor: i16,
    /// Number of links to inode in file system
    nlink: i16,
    /// Size of file (bytes)
    size: u32,
    /// Data block addresses
    addrs: [Option<BlockNo>; NDIRECT + 1],
}

/// Inodes per block.
pub const INODE_PER_BLOCK: usize = BLOCK_SIZE / size_of::<DInode>();

/// Block containing inode `inum`
const fn inode_block(inum: InodeNo, sb: &SuperBlock) -> BlockNo {
    BlockNo::new(inum.0.get() / (INODE_PER_BLOCK as u32) + sb.inodestart).unwrap()
}

/// Bitmap bits per block
const BITS_PER_BLOCK: usize = BLOCK_SIZE * 8;

/// Blocks of free map containing bit for block b
const fn bit_block(bn: usize, sb: &SuperBlock) -> BlockNo {
    BlockNo::new((bn as u32) / (BITS_PER_BLOCK as u32) + sb.bmapstart).unwrap()
}

// Directory is a file containing a sequence of dirent structures.
pub const DIR_SIZE: usize = 14;

#[repr(C)]
#[derive(Debug)]
struct DirEntry {
    inum: Option<NonZeroU16>,
    name: [u8; DIR_SIZE],
}

impl DirEntry {
    const fn zeroed() -> Self {
        Self {
            inum: None,
            name: [0; DIR_SIZE],
        }
    }

    fn name(&self) -> &[u8] {
        let len = self
            .name
            .iter()
            .position(|&c| c == 0)
            .unwrap_or(self.name.len());
        &self.name[..len]
    }

    fn is_same_name(&self, name: &[u8]) -> bool {
        let len = usize::min(name.len(), DIR_SIZE);
        self.name() == &name[..len]
    }

    fn set_name(&mut self, name: &[u8]) {
        let len = usize::min(name.len(), self.name.len());
        self.name[..len].copy_from_slice(&name[..len]);
        self.name[len..].fill(0);
    }
}

// there should be one superblock per disk device, but we run with
// only one device
static mut SUPER_BLOCK: SuperBlock = unsafe { mem::zeroed() };

/// Reads the super block.
unsafe fn read_superblock(dev: DeviceNo, sb: *mut SuperBlock) {
    let bp = block_io::get(dev, BlockNo::new(1).unwrap()).read();
    unsafe {
        sb.copy_from(bp.data().as_ptr().cast(), 1);
    }
}

pub fn init(dev: DeviceNo) {
    let sb = &raw mut SUPER_BLOCK;
    unsafe {
        read_superblock(dev, sb);
        assert_eq!((*sb).magic, FS_MAGIC);
        log::init(dev, &(*sb));
    }
}

/// Zeros a block.
fn block_zero(dev: DeviceNo, block_no: BlockNo) {
    let mut bp = block_io::get(dev, block_no).zeroed();
    log::write(&mut bp);
}

/// Allocates a zeroed disk block.
///
/// Returns None if out of disk space.
fn block_alloc(dev: DeviceNo) -> Option<BlockNo> {
    let sb = unsafe { (&raw const SUPER_BLOCK).as_ref() }.unwrap();
    let sb_size = sb.size as usize;
    for bn0 in (0..sb_size).step_by(BITS_PER_BLOCK) {
        let mut bp = block_io::get(dev, bit_block(bn0, sb)).read();
        let Some((bni, m)) = (0..BITS_PER_BLOCK)
            .take_while(|bni| bn0 + *bni < sb_size)
            .map(|bni| (bni, 1 << (bni % 8)))
            .find(|(bni, m)| {
                bp.data_mut()[bni / 8] & m == 0 // block is free
            })
        else {
            continue;
        };

        bp.data_mut()[bni / 8] |= m; // mark block in use
        log::write(&mut bp);
        let bn = BlockNo::new((bn0 + bni) as u32).unwrap();
        block_zero(dev, bn);
        return Some(bn);
    }
    crate::println!("out of blocks");
    None
}

/// Frees a disk block.
fn block_free(dev: DeviceNo, b: BlockNo) {
    let sb = unsafe { (&raw const SUPER_BLOCK).as_ref() }.unwrap();
    let mut bp = block_io::get(dev, bit_block(b.value() as usize, sb)).read();
    let bi = b.value() as usize % BITS_PER_BLOCK;
    let m = 1 << (bi % 8);
    assert_ne!(bp.data()[bi / 8] & m, 0, "freeing free block");
    bp.data_mut()[bi / 8] &= !m;
    log::write(&mut bp);
}

// Inodes.
//
// An inode describes a single unnamed file.
// The inode disk structure holds metadata: the file's type,
// its size, the number of links referring to it, and the
// list of blocks holding the file's content.
//
// The inodes are laid out sequentially on disk at block
// sb.inodestart. Each inode has a number, indicating its
// position on the disk.
//
// The kernel keeps a table of in-use inodes in memory
// to provide a place for synchronizing access
// to inodes used by multiple processes. The in-memory
// inodes include book-keeping information that is
// not stored on disk: ip->ref and ip->valid.
//
// An inode and its in-memory representation go through a
// sequence of states before they can be used by the
// rest of the file system code.
//
// * Allocation: an inode is allocated if its type (on disk)
//   is non-zero. inode_alloc() allocates, and inode_put() frees if
//   the reference and link counts have fallen to zero.
//
// * Referencing in table: an entry in the inode table
//   is free if ip->ref is zero. Otherwise ip->ref tracks
//   the number of in-memory pointers to the entry (open
//   files and current directories). inode_get() finds or
//   creates a table entry and increments its ref; inode_put()
//   decrements ref.
//
// * Valid: the information (type, size, &c) in an inode
//   table entry is only correct when ip->valid is 1.
//   inode_lock() reads the inode from
//   the disk and sets ip->valid, while inode_put() clears
//   ip->valid if ip->ref has fallen to zero.
//
// * Locked: file system code may only examine and modify
//   the information in an inode and its content if it
//   has first locked the inode.
//
// Thus a typical sequence is:
//   ip = inode_get(dev, inum)
//   inode_lock(ip)
//   ... examine and modify ip->xxx ...
//   inode_unlock(ip)
//   inode_put(ip)
//
// inode_lock() is separate from inode_get() so that system calls can
// get a long-term reference to an inode (as for an open file)
// and only lock it for short periods (e.g., in read()).
// The separation also helps avoid deadlock and races during
// pathname lookup. inode_get() increments ip->ref so that the inode
// stays in the table and pointers to it remain valid.
//
// Many internal file system functions expect the caller to
// have locked the inodes involved; this lets callers create
// multi-step atomic operations.
//
// The itable.lock spin-lock protects the allocation of itable
// entries. Since ip->ref indicates whether an entry is free,
// and ip->dev and ip->inum indicate which i-node an entry
// holds, one must hold itable.lock while using any of those fields.
//
// An ip->lock sleep-lock protects all ip-> fields other than ref,
// dev, and inum.  One must hold ip->lock in order to
// read or write that inode's ip->valid, ip->size, ip->type, &c.

static INODE_TABLE: SpinLock<[UnsafeCell<Inode>; NINODE]> =
    SpinLock::new([const { UnsafeCell::new(Inode::zero()) }; NINODE]);

/// Allocates an inode on device dev.
///
/// Marks it as allocated by giving it type `ty`.
/// Returns a n unlocked but allocated and referenced inode,
/// or None if there is no free inode.
fn inode_alloc(dev: DeviceNo, ty: i16) -> Result<NonNull<Inode>, ()> {
    let sb = unsafe { (&raw const SUPER_BLOCK).as_ref() }.unwrap();

    for inum in 1..(sb.ninodes) {
        let inum = InodeNo::new(inum).unwrap();
        let mut bp = block_io::get(dev, inode_block(inum, sb)).read();
        unsafe {
            let dip = &mut bp.as_dinodes_mut()[inum.value() as usize % INODE_PER_BLOCK];
            if dip.ty == 0 {
                // a free inode
                *dip = mem::zeroed();
                dip.ty = ty;
                log::write(&mut bp); // mark it allocated on the disk
                drop(bp);
                return inode_get(dev, inum);
            }
        }
    }
    crate::println!("no inodes");
    Err(())
}

/// Copies a modified in-memory inode to disk.
///
/// Must be called after every change to an ip.xxx field
/// that lives on disk.
/// Caller must hoold ip.lock.
pub fn inode_update(ip: NonNull<Inode>) {
    let sb = unsafe { (&raw const SUPER_BLOCK).as_ref() }.unwrap();

    unsafe {
        let ip = ip.as_ref();
        let mut bp = block_io::get(ip.dev.unwrap(), inode_block(ip.inum.unwrap(), sb)).read();
        let dip = &mut bp.as_dinodes_mut()[ip.inum.unwrap().value() as usize % INODE_PER_BLOCK];
        dip.ty = ip.ty;
        dip.major = ip.major;
        dip.minor = ip.minor;
        dip.nlink = ip.nlink;
        dip.size = ip.size;
        dip.addrs = ip.addrs;
        log::write(&mut bp);
    }
}

/// Finds the inode with number inum on device dev
/// and returns the in-memory copy.
///
/// Does not lock the inode and does not read it from disk.
fn inode_get(dev: DeviceNo, inum: InodeNo) -> Result<NonNull<Inode>, ()> {
    let itable = INODE_TABLE.lock();

    // Is the inode already in the table?
    let mut empty = None;
    for ic in &*itable {
        let ip = unsafe { &mut *ic.get() };
        if ip.refcount > 0 && ip.dev == Some(dev) && ip.inum == Some(inum) {
            ip.refcount += 1;
            return Ok(ip.into());
        }
        if empty.is_none() && ip.refcount == 0 {
            empty = Some(ic);
        }
    }
    let Some(ic) = empty else {
        panic!("no inodes");
    };

    let ip = unsafe { &mut *ic.get() };
    ip.dev = Some(dev);
    ip.inum = Some(inum);
    ip.refcount = 1;
    ip.valid = 0;
    Ok(NonNull::new(ic.get()).unwrap())
}

/// Increments reference count for `ip`.
///
/// Returns `ip` to enable `let ip = inode_dup(ip);` idiom.
pub fn inode_dup(ip: NonNull<Inode>) -> NonNull<Inode> {
    let _lock = INODE_TABLE.lock();
    unsafe {
        (*ip.as_ptr()).refcount += 1;
    }
    ip
}

/// Locks the given inode.
///
/// Reads the inode from disk if necessary.
pub fn inode_lock(ip: NonNull<Inode>) {
    let sb = unsafe { (&raw const SUPER_BLOCK).as_ref() }.unwrap();

    unsafe {
        let ip = ip.as_ptr();
        assert!((*ip).refcount > 0);
        (*ip).lock.acquire();

        if (*ip).valid == 0 {
            let mut bp =
                block_io::get((*ip).dev.unwrap(), inode_block((*ip).inum.unwrap(), sb)).read();
            let dip =
                &mut bp.as_dinodes_mut()[(*ip).inum.unwrap().value() as usize % INODE_PER_BLOCK];
            (*ip).ty = dip.ty;
            (*ip).major = dip.major;
            (*ip).minor = dip.minor;
            (*ip).nlink = dip.nlink;
            (*ip).size = dip.size;
            (*ip).addrs = dip.addrs;
            drop(bp);
            (*ip).valid = 1;
            assert_ne!((*ip).ty, 0);
        }
    }
}

/// Unlocks the given inode.
pub fn inode_unlock(ip: NonNull<Inode>) {
    unsafe {
        let ip = ip.as_ptr();
        assert!((*ip).lock.holding() && (*ip).refcount > 0);
        (*ip).lock.release();
    }
}

pub fn inode_with_lock<F, T>(ip: NonNull<Inode>, f: F) -> T
where
    F: FnOnce(NonNull<Inode>) -> T,
{
    inode_lock(ip);
    let res = f(ip);
    inode_unlock(ip);
    res
}

/// Drops a reference to an in-memory inode.
///
/// If that was the last reference, the inode table entry can
/// be recycled.
/// If that was the last reference and the inode has no links
/// to it, free the inode (and its content) on disk.
/// All calls to inode_put() must inside a transaction in
/// case it has to free the inode.
pub fn inode_put(ip: NonNull<Inode>) {
    let mut _lock = INODE_TABLE.lock();

    unsafe {
        let ip = ip.as_ptr();

        if (*ip).refcount == 1 && (*ip).valid != 0 && (*ip).nlink == 0 {
            // inode has no links and no other references: truncate and free.

            // (*ip).refcount == 1 means no other process can have ip locked,
            // so this acquires won't block (or deadlock).
            (*ip).lock.acquire();

            drop(_lock);

            inode_trunc(NonNull::new(ip).unwrap());
            (*ip).ty = 0;
            inode_update(NonNull::new(ip).unwrap());
            (*ip).valid = 0;
            (*ip).lock.release();

            _lock = INODE_TABLE.lock();
        }

        (*ip).refcount -= 1;
    }
}

/// Unlocks, then puts an inode.
pub fn inode_unlock_put(ip: NonNull<Inode>) {
    inode_unlock(ip);
    inode_put(ip);
}

// Inode content
//
// The content (data) associated with each inode is stored
// in blocks on the disk. The first NDIRECT block numbers
// are listed in ip->addrs[].  The next NINDIRECT blocks are
// listed in block ip->addrs[NDIRECT].

/// Returns the disk block address of the nth block in inode ip.
///
/// If there is no such block, `inode_block_map` allocates one.
/// Returns `None` if out of disk space.
fn inode_block_map(ip: NonNull<Inode>, ibn: usize) -> Option<BlockNo> {
    let ip = ip.as_ptr();
    unsafe {
        if ibn < NDIRECT {
            match (*ip).addrs[ibn] {
                Some(bn) => return Some(bn),
                None => {
                    let bn = block_alloc((*ip).dev.unwrap())?;
                    (*ip).addrs[ibn] = Some(bn);
                    return Some(bn);
                }
            }
        }

        let ibn = ibn - NDIRECT;
        if ibn < NINDIRECT {
            // Load indirect block, allocating if necessary.
            let bn = match (*ip).addrs[NDIRECT] {
                Some(bn) => bn,
                None => {
                    let bn = block_alloc((*ip).dev.unwrap())?;
                    (*ip).addrs[NDIRECT] = Some(bn);
                    bn
                }
            };

            let mut bp = block_io::get((*ip).dev.unwrap(), bn).read();
            let bnp = &mut bp.as_indirect_blocks_mut()[ibn];
            if let Some(bn) = *bnp {
                return Some(bn);
            }

            let bn = block_alloc((*ip).dev.unwrap())?;
            *bnp = Some(bn);
            log::write(&mut bp);
            return Some(bn);
        }

        panic!("out of range: bn={ibn}");
    }
}

/// Truncates inode (discard contents).
///
/// Caller must hold `ip.lock`.
pub fn inode_trunc(ip: NonNull<Inode>) {
    let ip = ip.as_ptr();
    unsafe {
        assert!((*ip).lock.holding());
        for bn in &mut (*ip).addrs[..NDIRECT] {
            if let Some(bn) = bn.take() {
                block_free((*ip).dev.unwrap(), bn);
            }
        }

        if let Some(bn) = (*ip).addrs[NDIRECT].take() {
            let mut bp = block_io::get((*ip).dev.unwrap(), bn).read();
            let bnp = bp.as_indirect_blocks_mut();
            for bn in bnp.iter_mut() {
                if let Some(bn) = bn.take() {
                    block_free((*ip).dev.unwrap(), bn);
                }
            }
            drop(bp);
            block_free((*ip).dev.unwrap(), bn);
        }

        (*ip).size = 0;
        inode_update(NonNull::new(ip).unwrap());
    }
}

/// Copies stat information from inode.
///
/// Caller must hold `ip.lock`.
pub fn stat_inode(ip: NonNull<Inode>) -> Stat {
    let ip = ip.as_ptr();
    unsafe {
        assert!((*ip).lock.holding());

        Stat {
            dev: (*ip).dev.unwrap(),
            ino: (*ip).inum.unwrap(),
            ty: (*ip).ty,
            nlink: (*ip).nlink,
            size: (*ip).size as u64,
        }
    }
}

/// Reads data from inode.
///
/// Caller must hold `ip.lock`.
/// If `user_dst == true`, then `dst` is a user virtual address;
/// otherwise, dst is a kernel address.
pub fn read_inode(
    p: &Proc,
    ip: NonNull<Inode>,
    user_dst: bool,
    dst: VirtAddr,
    off: usize,
    mut n: usize,
) -> Result<usize, ()> {
    let ip = ip.as_ptr();
    unsafe {
        assert!((*ip).lock.holding());

        let size = (*ip).size as usize;
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
            let Some(bn) = inode_block_map(NonNull::new(ip).unwrap(), off / BLOCK_SIZE) else {
                break;
            };
            let bp = block_io::get((*ip).dev.unwrap(), bn).read();
            let m = usize::min(n - tot, BLOCK_SIZE - off % BLOCK_SIZE);
            proc::either_copy_out_bytes(
                p,
                user_dst,
                dst.addr(),
                &bp.data()[off % BLOCK_SIZE..][..m],
            )?;
            tot += m;
        }
        Ok(tot)
    }
}

pub fn read_inode_as<T>(p: &Proc, ip: NonNull<Inode>, off: usize) -> Result<T, ()> {
    let mut dst = MaybeUninit::<T>::uninit();
    let read = read_inode(
        p,
        ip,
        false,
        VirtAddr::new(dst.as_mut_ptr().addr()),
        off,
        size_of::<T>(),
    )?;
    if read != size_of::<T>() {
        return Err(());
    }
    Ok(unsafe { dst.assume_init() })
}

/// Writes data to inode.
///
/// Caller must hold `ip.lock`.
/// If `user_src == true`, then `src` is a user virtual address;
/// otherwise, `src` is a kernel address.
/// Returns the number of bytes successfully written.
/// If the return value is less than the requested `n`,
/// there was an error of some kind.
pub fn write_inode(
    p: &Proc,
    ip: NonNull<Inode>,
    user_src: bool,
    src: VirtAddr,
    off: usize,
    n: usize,
) -> Result<usize, ()> {
    let ip = ip.as_ptr();
    unsafe {
        assert!((*ip).lock.holding());

        let size = (*ip).size as usize;
        if off > size || off.checked_add(n).is_none() {
            return Err(());
        }
        if off + n > MAX_FILE * BLOCK_SIZE {
            return Err(());
        }

        let mut tot = 0;
        while tot < n {
            let off = off + tot;
            let src = src.byte_add(tot);
            let Some(bn) = inode_block_map(NonNull::new(ip).unwrap(), off / BLOCK_SIZE) else {
                break;
            };

            let mut bp = block_io::get((*ip).dev.unwrap(), bn).read();
            let m = usize::min(n - tot, BLOCK_SIZE - off % BLOCK_SIZE);
            proc::either_copy_in_bytes(
                p,
                &mut bp.data_mut()[off % BLOCK_SIZE..][..m],
                user_src,
                src.addr(),
            )?;
            log::write(&mut bp);

            tot += m;
        }

        if off + tot > size {
            (*ip).size = (off + tot) as u32;
        }
        // write the i-node back to disk even if the size didn't change
        // because the loop above might have called inode_block_map() and added a new
        // block to `ip.addrs`.`
        inode_update(NonNull::new(ip).unwrap());

        Ok(tot)
    }
}

pub fn write_inode_data<T>(p: &Proc, ip: NonNull<Inode>, off: usize, data: T) -> Result<(), ()> {
    let written = write_inode(
        p,
        ip,
        false,
        VirtAddr::new(ptr::from_ref(&data).addr()),
        off,
        size_of::<T>(),
    )?;
    if written != size_of::<T>() {
        return Err(());
    }
    Ok(())
}

// Directories

/// Looks up for a directory entry in a directory inode.
pub fn dir_lookup(
    p: &Proc,
    dp: NonNull<Inode>,
    name: &[u8],
) -> Result<(NonNull<Inode>, usize), ()> {
    let dp = dp.as_ptr();
    unsafe {
        assert_eq!((*dp).ty, T_DIR); // must be a directory

        for off in (0..(*dp).size as usize).step_by(size_of::<DirEntry>()) {
            let de = read_inode_as::<DirEntry>(p, NonNull::new(dp).unwrap(), off).unwrap();
            let Some(inum) = de.inum else { continue };
            if !de.is_same_name(name) {
                continue;
            }
            let inum = InodeNo::new(inum.get() as u32).unwrap();
            let ip = inode_get((*dp).dev.unwrap(), inum)?;
            return Ok((ip, off));
        }
        Err(())
    }
}

/// Writes a new directory entry (name, inum) into the directory dp.
pub fn dir_link(p: &Proc, dp: NonNull<Inode>, name: &[u8], inum: InodeNo) -> Result<(), ()> {
    let dp = dp.as_ptr();
    unsafe {
        assert_eq!((*dp).ty, T_DIR); // must be a directory

        // Check that name is not present.
        if let Ok((ip, _)) = dir_lookup(p, NonNull::new(dp).unwrap(), name) {
            inode_put(ip);
            return Err(());
        }

        // Look for an empty dirent.
        assert_eq!((*dp).size as usize % size_of::<DirEntry>(), 0);
        let (mut de, off) = (0..(*dp).size as usize)
            .step_by(size_of::<DirEntry>())
            .map(|off| {
                let de = read_inode_as::<DirEntry>(p, NonNull::new(dp).unwrap(), off).unwrap();
                (de, off)
            })
            .find(|(de, _)| de.inum.is_none())
            .unwrap_or((DirEntry::zeroed(), (*dp).size as usize));

        de.set_name(name);
        de.inum = Some(NonZeroU16::new(inum.value() as u16).unwrap());
        write_inode_data(p, NonNull::new(dp).unwrap(), off, de)?;
        Ok(())
    }
}

/// Returns if the directory `dp` is empty except for `"."` and `"..."`.
fn dir_is_empty(p: &Proc, dp: NonNull<Inode>) -> bool {
    let de_size = size_of::<DirEntry>();
    unsafe {
        assert_eq!(dp.as_ref().ty, T_DIR);
        // skip first two entry ("." and "..").
        for off in (2 * de_size..(dp.as_ref().size as usize)).step_by(de_size) {
            let de = read_inode_as::<DirEntry>(p, dp, off).unwrap();
            if de.inum.is_some() {
                return false;
            }
        }
    }
    true
}

/// Copies the next path element from path into name.
///
/// Returns a pair of the next path element and the remainder of the path.
/// The returned path has no leading slashes.
/// If no name to remove, return None.
///
/// # Examples
///
/// ```
/// assert_eq!(skip_elem(b"a/bb/c"), Some((b"a", b"bb/c")));
/// assert_eq!(skip_elem(b"///a//bb"), Some((b"a", b"bb")));
/// assert_eq!(skip_elem(b"a"), Some((b"a", b"")));
/// assert_eq!(skip_elem(b"a/"), Some((b"a", b"")));
/// assert_eq!(skip_elem(b""), None);
/// assert_eq!(skip_elem(b"///"), None);
/// ```
fn skip_elem(path: &[u8]) -> Option<(&[u8], &[u8])> {
    let start = path.iter().position(|&c| c != b'/')?;
    let path = &path[start..];
    let end = path.iter().position(|&c| c == b'/').unwrap_or(path.len());
    let elem = &path[..end];
    let path = &path[end..];
    let next = path.iter().position(|&c| c != b'/').unwrap_or(path.len());
    let path = &path[next..];
    Some((elem, path))
}

/// Looks up and returns the inode for a given path.
///
/// If `parent` is `true`, returns the inode for the parent and copy the final
/// path element into `name`, which must have room for `DIR_SIZE` bytes.
/// Must be called inside a transaction since it calls `inode_put()`.
fn resolve_path_impl(
    p: &Proc,
    path: &[u8],
    parent: bool,
    mut name_out: Option<&mut [u8; DIR_SIZE]>,
) -> Result<NonNull<Inode>, ()> {
    let mut ip = if path.first() == Some(&b'/') {
        inode_get(ROOT_DEV, ROOT_INO)?
    } else {
        inode_dup(p.cwd().unwrap())
    };

    unsafe {
        let mut path = path;
        while let Some((name, rest)) = skip_elem(path) {
            path = rest;
            if let Some(name_out) = &mut name_out {
                let copy_len = usize::min(name.len(), name_out.len());
                name_out[..copy_len].copy_from_slice(&name[..copy_len]);
                name_out[copy_len..].fill(0);
            }

            inode_lock(ip);
            if ip.as_ref().ty != T_DIR {
                inode_unlock_put(ip);
                return Err(());
            }

            if parent && path.is_empty() {
                // Stop one level early.
                inode_unlock(ip);
                return Ok(ip);
            }
            let Ok((next, _off)) = dir_lookup(p, ip, name) else {
                inode_unlock_put(ip);
                return Err(());
            };
            inode_unlock_put(ip);
            ip = next;
        }
    }

    if parent {
        inode_put(ip);
        return Err(());
    }
    Ok(ip)
}

pub fn resolve_path(p: &Proc, path: &[u8]) -> Result<NonNull<Inode>, ()> {
    resolve_path_impl(p, path, false, None)
}

pub fn resolve_path_parent<'a>(
    p: &Proc,
    path: &[u8],
    name: &'a mut [u8; DIR_SIZE],
) -> Result<(NonNull<Inode>, &'a [u8]), ()> {
    let ip = resolve_path_impl(p, path, true, Some(name))?;
    let len = name.iter().position(|b| *b == 0).unwrap_or(name.len());
    let name = &name[..len];
    Ok((ip, name))
}

pub fn unlink(p: &Proc, path: &[u8]) -> Result<(), ()> {
    unsafe {
        log::do_op(|| {
            let mut name = [0; DIR_SIZE];
            let (mut dp, name) = resolve_path_parent(p, path, &mut name)?;

            inode_lock(dp);

            let res = (|| {
                // Cannot unlink "." of "..".
                if name == b".." || name == b"." {
                    return Err(());
                }

                let (mut ip, off) = dir_lookup(p, dp, name)?;
                inode_lock(ip);

                assert!(ip.as_ref().nlink > 0);
                if ip.as_ref().ty == T_DIR && !dir_is_empty(p, ip) {
                    inode_unlock_put(ip);
                    return Err(());
                }

                let de = DirEntry::zeroed();
                write_inode_data(p, dp, off, de).unwrap();
                if ip.as_ref().ty == T_DIR {
                    dp.as_mut().nlink -= 1;
                    inode_update(dp);
                }
                inode_unlock_put(dp);

                ip.as_mut().nlink -= 1;
                inode_update(ip);
                inode_unlock_put(ip);

                Ok(())
            })();

            if res.is_err() {
                inode_unlock_put(dp);
                return Err(());
            }

            Ok(())
        })?;

        Ok(())
    }
}

pub fn create(
    p: &Proc,
    path: &[u8],
    ty: i16,
    major: i16,
    minor: i16,
) -> Result<NonNull<Inode>, ()> {
    unsafe {
        let mut name = [0; DIR_SIZE];
        let (mut dp, name) = resolve_path_parent(p, path, &mut name)?;

        inode_lock(dp);

        if let Ok((ip, _off)) = dir_lookup(p, dp, name) {
            // Inode already exists
            inode_unlock_put(dp);
            inode_lock(ip);
            if ty == T_FILE && (ip.as_ref().ty == T_FILE || ip.as_ref().ty == T_DEVICE) {
                return Ok(ip);
            }
            inode_unlock_put(ip);
            return Err(());
        }

        let Ok(mut ip) = inode_alloc(dp.as_ref().dev.unwrap(), ty) else {
            inode_unlock_put(dp);
            return Err(());
        };

        inode_lock(ip);
        ip.as_mut().major = major;
        ip.as_mut().minor = minor;
        ip.as_mut().nlink = 1;
        inode_update(ip);

        let res = (|| {
            if ty == T_DIR {
                // Create "." and ".." entries
                dir_link(p, ip, b".", ip.as_ref().inum.unwrap())?;
                dir_link(p, ip, b"..", dp.as_ref().inum.unwrap())?;
            }

            dir_link(p, dp, name, ip.as_ref().inum.unwrap())?;

            if ty == T_DIR {
                // now that success is guaranteed:
                dp.as_mut().nlink += 1; // for ".."
                inode_update(dp);
            }

            inode_unlock_put(dp);

            Ok(ip)
        })();

        if res.is_err() {
            ip.as_mut().nlink = 0;
            inode_update(ip);
            inode_unlock_put(ip);
            inode_unlock_put(dp);
            return Err(());
        }

        res
    }
}
