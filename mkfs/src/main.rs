// Workaround for `cargo doc --workspace --target riscv64gc-unknown-none-elf` to work
#![cfg_attr(target_os = "none", no_std)]
#![cfg(not(target_os = "none"))]

use std::{
    env,
    fs::File,
    io::{self, Read, Seek, SeekFrom, Write as _},
    mem,
    path::Path,
    process,
};

use dataview::{Pod, PodMethods as _};
use xv6_fs_types::{
    BITS_PER_BLOCK, BlockNo, DIR_SIZE, DirEntry, FS_BLOCK_SIZE, INODE_PER_BLOCK, Inode, InodeNo,
    MAX_FILE, NUM_DIRECT_REFS, NUM_INDIRECT_REFS, SuperBlock, T_DIR, T_FILE,
};
use xv6_kernel_params::{FS_LOG_SIZE, FS_SIZE, NUM_FS_INODES};

const _: () = const {
    assert!(FS_BLOCK_SIZE % size_of::<Inode>() == 0);
    assert!(FS_BLOCK_SIZE % size_of::<DirEntry>() == 0);
};

fn main() -> io::Result<()> {
    let args = env::args().collect::<Vec<String>>();
    if args.len() < 2 {
        eprintln!("Usage: {} fs.img files...", args[0]);
        process::exit(1);
    }

    let image_file = &args[1];
    let contents = &args[2..];

    let mut fs = FileSystem::new(Path::new(image_file))?;
    fs.clear_all_sections()?;
    fs.write_super_block()?;
    let root_ino = fs.create_directory()?;
    assert_eq!(root_ino, InodeNo::ROOT);

    for name in contents {
        let mut short_name = name.as_str();
        short_name = short_name.strip_prefix("user/").unwrap_or(short_name);
        short_name = short_name.strip_prefix("_").unwrap_or(short_name);

        let mut buf = vec![];
        File::open(name)?.read_to_end(&mut buf)?;
        let ino = fs.create_file(&buf)?;
        fs.add_directory_entry(root_ino, ino, short_name)?;
    }

    // fix size of root inode dir
    let mut inode = Inode::zeroed();
    fs.read_inode(root_ino, &mut inode)?;
    let size = u32::from_le(inode.size);
    let size = size.next_multiple_of(u32::try_from(FS_BLOCK_SIZE).unwrap());
    inode.size = size.to_le();
    fs.write_inode(root_ino, &inode)?;

    fs.write_bitmap()?;

    Ok(())
}

struct FileSystem {
    img: File,
    num_bmap_blocks: u32,
    num_inode_blocks: u32,
    num_log_blocks: u32,
    /// Number of meta blocks (boot, sb, nlog, inode, bitmap)
    num_meta_blocks: u32,
    /// Number of data blocks
    num_blocks: u32,
    num_inodes: u32,
    next_free_inode: InodeNo,
    next_free_block: BlockNo,
    total_blocks: u32,
    sb: SuperBlock,
}

impl FileSystem {
    fn new(image_file: &Path) -> io::Result<Self> {
        let total_blocks = u32::try_from(FS_SIZE).unwrap();
        let mut fs = FileSystem {
            img: File::options()
                .read(true)
                .write(true)
                .truncate(true)
                .create(true)
                .open(image_file)?,
            num_bmap_blocks: u32::try_from(FS_SIZE / BITS_PER_BLOCK + 1).unwrap(),
            num_inode_blocks: u32::try_from(NUM_FS_INODES / INODE_PER_BLOCK + 1).unwrap(),
            num_log_blocks: u32::try_from(FS_LOG_SIZE).unwrap(),
            num_meta_blocks: 0,
            num_blocks: 0,
            num_inodes: u32::try_from(NUM_FS_INODES).unwrap(),
            next_free_inode: InodeNo::new(1),
            next_free_block: BlockNo::new(2),
            total_blocks,
            sb: SuperBlock::zeroed(),
        };

        fs.num_meta_blocks = 2 + fs.num_log_blocks + fs.num_inode_blocks + fs.num_bmap_blocks;
        fs.num_blocks = total_blocks - fs.num_meta_blocks;
        fs.next_free_block = BlockNo::new(fs.num_meta_blocks);

        fs.sb = SuperBlock {
            magic: SuperBlock::FS_MAGIC,
            size: fs.total_blocks,
            nblocks: fs.num_blocks,
            ninodes: fs.num_inodes,
            nlog: fs.num_log_blocks,
            logstart: 2u32,
            inodestart: (2 + fs.num_log_blocks),
            bmapstart: (2 + fs.num_log_blocks + fs.num_inode_blocks),
        };

        eprintln!(
            "nmeta {} (boot, super, log blocks {} inode blocsk {}, bitmap blocks {}) blocks {} total {}",
            fs.num_meta_blocks,
            fs.num_log_blocks,
            fs.num_inode_blocks,
            fs.num_bmap_blocks,
            fs.num_blocks,
            fs.total_blocks,
        );

        Ok(fs)
    }

    fn clear_all_sections(&mut self) -> io::Result<()> {
        for i in 0..self.total_blocks {
            self.write_section(BlockNo::new(i), &[0u8; FS_BLOCK_SIZE])?;
        }
        Ok(())
    }

    fn write_super_block(&mut self) -> io::Result<()> {
        let sb = SuperBlock {
            magic: self.sb.magic.to_le(),
            size: self.sb.size.to_le(),
            nblocks: self.sb.nblocks.to_le(),
            ninodes: self.sb.ninodes.to_le(),
            nlog: self.sb.nlog.to_le(),
            logstart: self.sb.logstart.to_le(),
            inodestart: self.sb.inodestart.to_le(),
            bmapstart: self.sb.bmapstart.to_le(),
        };

        let mut buf = [0u8; FS_BLOCK_SIZE];
        let sb_bytes = sb.as_bytes();
        buf[..sb_bytes.len()].copy_from_slice(sb_bytes);
        self.write_section(BlockNo::new(1), &buf)?;

        Ok(())
    }

    fn create_directory(&mut self) -> io::Result<InodeNo> {
        let dir_ino = self.alloc_inode(T_DIR)?;

        self.add_directory_entry(dir_ino, dir_ino, ".")?;
        self.add_directory_entry(dir_ino, dir_ino, "..")?;

        Ok(dir_ino)
    }

    fn create_file(&mut self, content: &[u8]) -> io::Result<InodeNo> {
        let ino = self.alloc_inode(T_FILE)?;
        self.append_inode(ino, content)?;
        Ok(ino)
    }

    fn add_directory_entry(
        &mut self,
        dir_ino: InodeNo,
        ino: InodeNo,
        name: &str,
    ) -> io::Result<()> {
        assert!(name.len() < DIR_SIZE);
        let mut de = DirEntry::zeroed();
        de.set_inum(Some(InodeNo::new(ino.value().to_le())));
        de.set_name(name.as_bytes());
        self.append_inode(dir_ino, &de)?;
        Ok(())
    }

    fn write_bitmap(&mut self) -> io::Result<()> {
        let mut buf = [0u8; FS_BLOCK_SIZE];

        let used = usize::try_from(self.next_free_block.value()).unwrap();
        println!("balloc: first {used} blocks have been allocated");
        for i in 0..used {
            buf[i / 8] |= 1 << (i % 8);
        }
        println!("balloc: write bitmap block at sector {}", self.sb.bmapstart);
        self.write_section(BlockNo::new(self.sb.bmapstart), &buf)?;

        Ok(())
    }

    fn write_section<T>(&mut self, bn: BlockNo, data: &T) -> io::Result<()>
    where
        T: Pod + ?Sized,
    {
        let data = data.as_bytes();
        assert_eq!(data.len(), FS_BLOCK_SIZE);
        let offset = u64::from(bn.value()) * u64::try_from(FS_BLOCK_SIZE).unwrap();
        self.img.seek(SeekFrom::Start(offset))?;
        self.img.write_all(data)?;
        Ok(())
    }

    fn read_section<T>(&mut self, bn: BlockNo, data: &mut T) -> io::Result<()>
    where
        T: Pod + ?Sized,
    {
        let data = data.as_bytes_mut();
        assert_eq!(data.len(), FS_BLOCK_SIZE);
        let offset = u64::from(bn.value()) * u64::try_from(FS_BLOCK_SIZE).unwrap();
        self.img.seek(SeekFrom::Start(offset))?;
        self.img.read_exact(data)?;
        Ok(())
    }

    fn write_inode(&mut self, ino: InodeNo, data: &Inode) -> io::Result<()> {
        let mut buf = [const { unsafe { mem::zeroed::<Inode>() } }; INODE_PER_BLOCK];

        let bn = self.sb.inode_block(ino);
        self.read_section(bn, &mut buf)?;
        buf[ino.as_index() % INODE_PER_BLOCK]
            .as_bytes_mut()
            .copy_from_slice(data.as_bytes());
        self.write_section(bn, &buf)?;
        Ok(())
    }

    fn read_inode(&mut self, ino: InodeNo, data: &mut Inode) -> io::Result<()> {
        let mut buf = [const { unsafe { mem::zeroed::<Inode>() } }; INODE_PER_BLOCK];

        let bn = self.sb.inode_block(ino);
        self.read_section(bn, &mut buf)?;
        data.as_bytes_mut()
            .copy_from_slice(buf[ino.as_index() % INODE_PER_BLOCK].as_bytes());
        Ok(())
    }

    fn alloc_inode(&mut self, ty: i16) -> io::Result<InodeNo> {
        let ino = self.next_free_inode;
        self.next_free_inode = InodeNo::new(self.next_free_inode.value() + 1);

        let inode = Inode {
            ty: ty.to_le(),
            nlink: 1i16.to_le(),
            size: 0u32.to_le(),
            ..Inode::zeroed()
        };
        self.write_inode(ino, &inode)?;
        Ok(ino)
    }

    fn alloc_block(&mut self) -> BlockNo {
        let bn = self.next_free_block;
        self.next_free_block = BlockNo::new(self.next_free_block.value() + 1);
        bn
    }

    fn append_inode<T>(&mut self, ino: InodeNo, data: &T) -> io::Result<()>
    where
        T: Pod + ?Sized,
    {
        let mut data = data.as_bytes();

        let mut inode = Inode::zeroed();
        self.read_inode(ino, &mut inode)?;
        let mut file_off = usize::try_from(u32::from_le(inode.size)).unwrap();
        // println!("append inum {ino} at off {file_off} {} sz", data.len());

        while !data.is_empty() {
            let file_bidx = file_off / FS_BLOCK_SIZE;
            assert!(file_bidx < MAX_FILE);
            let bn = if file_bidx < NUM_DIRECT_REFS {
                if inode.addrs[file_bidx] == 0 {
                    inode.addrs[file_bidx] = self.alloc_block().value().to_le();
                }
                BlockNo::new(u32::from_le(inode.addrs[file_bidx]))
            } else {
                if inode.addrs[NUM_DIRECT_REFS] == 0 {
                    inode.addrs[NUM_DIRECT_REFS] = self.alloc_block().value().to_le();
                }
                let ind_bn = BlockNo::new(u32::from_le(inode.addrs[NUM_DIRECT_REFS]));
                let mut ind_buf = [0; NUM_INDIRECT_REFS];
                self.read_section(ind_bn, &mut ind_buf)?;
                if ind_buf[file_bidx - NUM_DIRECT_REFS] == 0 {
                    ind_buf[file_bidx - NUM_DIRECT_REFS] = self.alloc_block().value().to_le();
                    self.write_section(ind_bn, &ind_buf)?;
                }
                BlockNo::new(u32::from_le(ind_buf[file_bidx - NUM_DIRECT_REFS]))
            };

            let mut buf = [0u8; FS_BLOCK_SIZE];
            self.read_section(bn, &mut buf)?;

            let block_start = file_bidx * FS_BLOCK_SIZE;
            let block_end = (file_bidx + 1) * FS_BLOCK_SIZE;
            let copy_len = usize::min(data.len(), block_end - file_off);
            buf[file_off - block_start..][..copy_len].copy_from_slice(&data[..copy_len]);
            self.write_section(bn, &buf)?;

            file_off += copy_len;
            data = &data[copy_len..];
        }

        inode.size = u32::try_from(file_off).unwrap().to_le();
        self.write_inode(ino, &inode)?;
        Ok(())
    }
}
