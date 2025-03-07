//! Data types for ov6 file system.
//!
//! The data layout:
//!
//! | block no.                      | # of blocks        | content     | type                                          |
//! |--------------------------------|--------------------|-------------|-----------------------------------------------|
//! |  0                             | 1                  | Boot Block  | (unused)                                      |
//! |  1                             | 1                  | Super Block | [`SuperBlock`]                                |
//! | `sb.logstart`                  | `1 + sb.nlog`      | Log         | [`LogHeader`] & `[u8; BLOCK_SIZE]` (log body) |
//! | `sb.inodestart`                | `sb.ninodes / IPB` | inode table | [`InodeBlock`]                                |
//! | `sb.bmapstart`                 | `sb.size / BPB`    | bitmap      | [`BmapBlock`]                                 |
//! | `sb.bmapstart + sb.size / BPB` | `sb.nblocks`       | data blocks | [`[u8; BLOCK_SIZE]`] (data)                   |

#![no_std]

use core::{fmt, mem};

use dataview::{Pod, PodMethods as _};
use ov6_types::os_str::OsStr;

/// Block size.
pub const FS_BLOCK_SIZE: usize = 1024;

/// Number of blocks directly referenced by an inode.
pub const NUM_DIRECT_REFS: usize = 12;

/// Number of blocks indirectly referenced by an inode.
pub const NUM_INDIRECT_REFS: usize = FS_BLOCK_SIZE / size_of::<u32>();

pub const MAX_FILE: usize = NUM_DIRECT_REFS + NUM_INDIRECT_REFS;

/// File system block number.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Pod)]
#[repr(transparent)]
pub struct BlockNo(u32);

impl fmt::Display for BlockNo {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl BlockNo {
    #[must_use]
    pub const fn new(n: u32) -> Self {
        Self(n)
    }

    #[must_use]
    pub const fn value(&self) -> u32 {
        self.0
    }

    #[must_use]
    pub fn as_index(&self) -> usize {
        usize::try_from(self.0).unwrap()
    }
}

/// File system inode number.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Pod)]
#[repr(transparent)]
pub struct InodeNo(u32);

impl fmt::Display for InodeNo {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl InodeNo {
    pub const ROOT: Self = Self::new(1);

    #[must_use]
    pub const fn new(n: u32) -> Self {
        Self(n)
    }

    #[must_use]
    pub const fn value(&self) -> u32 {
        self.0
    }

    #[must_use]
    pub fn as_index(&self) -> usize {
        usize::try_from(self.0).unwrap()
    }
}

/// Super block of the file system.
#[derive(Pod)]
#[repr(C)]
pub struct SuperBlock {
    /// Magic number. Must be FSMAGIC
    pub magic: u32,
    /// Size of file system image (blocks)
    pub size: u32,
    /// Number of data blocks
    pub nblocks: u32,
    /// Number of inodes.
    pub ninodes: u32,
    /// Number of log blocks.
    pub nlog: u32,
    /// Block number of first log block.
    pub logstart: u32,
    /// Block number of first inode block.
    pub inodestart: u32,
    /// Block number of first free map block.
    pub bmapstart: u32,
}

impl SuperBlock {
    pub const FS_MAGIC: u32 = 0x1020_3040;

    pub const SUPER_BLOCK_NO: BlockNo = BlockNo::new(1);

    /// Returns the block number that contains the specified inode.
    #[must_use]
    pub fn inode_block(&self, inode_no: InodeNo) -> BlockNo {
        let block_index = u32::try_from(inode_no.as_index() / INODE_PER_BLOCK).unwrap();
        BlockNo::new(self.inodestart + block_index)
    }

    /// Returns the block number that contains the specified bitmap.
    #[must_use]
    pub fn bmap_block(&self, bn: usize) -> BlockNo {
        let block_index = u32::try_from(bn / BITS_PER_BLOCK).unwrap();
        BlockNo::new(self.bmapstart + block_index)
    }

    #[must_use]
    pub fn max_log_len(&self) -> usize {
        usize::try_from(self.nlog).unwrap()
    }

    #[must_use]
    pub fn log_header_block(&self) -> BlockNo {
        BlockNo::new(self.logstart)
    }

    #[must_use]
    pub fn log_body_block(&self, i: usize) -> BlockNo {
        BlockNo::new(self.logstart + u32::try_from(i).unwrap())
    }
}

const MAX_LOG_COUNT: usize = FS_BLOCK_SIZE / size_of::<u32>() - 1;

/// Contents of the header block, used for both the on-disk header block
/// and to keep track in memory of logged block# before commit.
#[derive(Pod)]
#[repr(C)]
pub struct LogHeader {
    len: u32,
    block_indices: [u32; MAX_LOG_COUNT],
}
const _: () = const { assert!(size_of::<LogHeader>() == FS_BLOCK_SIZE) };

impl LogHeader {
    #[must_use]
    pub fn len(&self) -> usize {
        usize::try_from(self.len).unwrap()
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    pub fn set_len(&mut self, len: usize) {
        self.len = u32::try_from(len).unwrap();
    }

    #[must_use]
    pub fn block_indices(&self) -> &[u32] {
        &self.block_indices[..self.len()]
    }

    #[must_use]
    pub fn block_indices_mut(&mut self) -> &mut [u32] {
        let len = self.len();
        &mut self.block_indices[..len]
    }
}

/// Directory
pub const T_DIR: i16 = 1;
/// File
pub const T_FILE: i16 = 2;
/// Device
pub const T_DEVICE: i16 = 3;

#[derive(Pod)]
#[repr(C)]
pub struct Inode {
    /// File type
    pub ty: i16,
    /// Major device number ([`T_DEVICE`] only)
    pub major: i16,
    /// Minor device number ([`T_DEVICE`] only)
    pub minor: i16,
    /// Number of links to inode in file system
    pub nlink: i16,
    /// Size of file (bytes)
    pub size: u32,
    /// Data block addresses
    pub addrs: [u32; NUM_DIRECT_REFS + 1],
}

impl Inode {
    #[must_use]
    pub fn is_free(&self) -> bool {
        self.ty == 0
    }

    pub fn allocate(&mut self, ty: i16) {
        assert_eq!(self.ty, 0);
        *self = Self::zeroed();
        self.ty = ty;
    }

    pub fn write_addrs(&mut self, addrs: &[Option<BlockNo>; NUM_DIRECT_REFS + 1]) {
        for (dst, src) in self.addrs.iter_mut().zip(addrs) {
            if let Some(bn) = src {
                assert_ne!(bn.0, 0);
                *dst = bn.0;
            } else {
                *dst = 0;
            }
        }
    }

    pub fn read_addrs(&self, addrs: &mut [Option<BlockNo>; NUM_DIRECT_REFS + 1]) {
        for (dst, src) in addrs.iter_mut().zip(&self.addrs) {
            if *src == 0 {
                *dst = None;
            } else {
                *dst = Some(BlockNo(*src));
            }
        }
    }
}

/// Inodes per block.
pub const INODE_PER_BLOCK: usize = FS_BLOCK_SIZE / size_of::<Inode>();

#[derive(Pod)]
#[repr(transparent)]
pub struct InodeBlock([Inode; INODE_PER_BLOCK]);
const _: () = const { assert!(size_of::<InodeBlock>() == FS_BLOCK_SIZE) };

impl InodeBlock {
    #[must_use]
    pub fn inode(&self, ino: InodeNo) -> &Inode {
        &self.0[ino.value() as usize % INODE_PER_BLOCK]
    }

    #[must_use]
    pub fn inode_mut(&mut self, ino: InodeNo) -> &mut Inode {
        &mut self.0[ino.value() as usize % INODE_PER_BLOCK]
    }
}

/// Bitmap bits per block
pub const BITS_PER_BLOCK: usize = FS_BLOCK_SIZE * 8;

#[derive(Pod)]
#[repr(transparent)]
pub struct BmapBlock([u8; FS_BLOCK_SIZE]);
const _: () = const { assert!(size_of::<BmapBlock>() == FS_BLOCK_SIZE) };

impl BmapBlock {
    #[must_use]
    pub fn bit(&self, n: usize) -> bool {
        assert!(n < BITS_PER_BLOCK);
        self.0[n / 8] & (1 << (n % 8)) != 0
    }

    pub fn set_bit(&mut self, n: usize) {
        assert!(n < BITS_PER_BLOCK);
        self.0[n / 8] |= 1 << (n % 8);
    }

    pub fn clear_bit(&mut self, n: usize) {
        assert!(n < BITS_PER_BLOCK);
        self.0[n / 8] &= !(1 << (n % 8));
    }
}

#[derive(Pod)]
#[repr(transparent)]
pub struct IndirectBlock([u32; NUM_INDIRECT_REFS]);

impl IndirectBlock {
    #[must_use]
    pub fn get(&self, i: usize) -> Option<BlockNo> {
        if self.0[i] == 0 {
            None
        } else {
            Some(BlockNo::new(self.0[i]))
        }
    }

    pub fn set(&mut self, i: usize, n: Option<BlockNo>) {
        self.0[i] = n.map_or(0, |n| {
            assert_ne!(n.value(), 0);
            n.value()
        });
    }

    pub fn drain(&mut self) -> impl Iterator<Item = Option<BlockNo>> + '_ {
        self.0.iter_mut().map(|n| {
            let n = mem::take(n);
            if n == 0 { None } else { Some(BlockNo::new(n)) }
        })
    }
}

// Directory is a file containing a sequence of dirent structures.
pub const DIR_SIZE: usize = 14;

#[repr(C)]
#[derive(Debug, Pod)]
pub struct DirEntry {
    ino: u16,
    name: [u8; DIR_SIZE],
}

impl DirEntry {
    #[must_use]
    pub fn ino(&self) -> Option<InodeNo> {
        if self.ino == 0 {
            None
        } else {
            Some(InodeNo::new(self.ino.into()))
        }
    }

    pub fn set_ino(&mut self, ino: Option<InodeNo>) {
        if let Some(ino) = ino {
            assert_ne!(ino.0, 0);
            self.ino = ino.0.try_into().unwrap();
        } else {
            self.ino = 0;
        }
    }

    #[must_use]
    pub fn name(&self) -> &OsStr {
        let len = self
            .name
            .iter()
            .position(|&c| c == 0)
            .unwrap_or(self.name.len());
        OsStr::from_bytes(&self.name[..len])
    }

    #[must_use]
    pub fn is_same_name(&self, name: &OsStr) -> bool {
        let len = usize::min(name.len(), DIR_SIZE);
        self.name().as_bytes() == &name.as_bytes()[..len]
    }

    pub fn set_name(&mut self, name: &OsStr) {
        let len = usize::min(name.len(), self.name.len());
        self.name[..len].copy_from_slice(&name.as_bytes()[..len]);
        self.name[len..].fill(0);
    }
}
