use dataview::PodMethods as _;
use ov6_types::{fs::RawFd, os_str::OsStr, path::Path};
pub use syscall::StatType;

use crate::{
    error::Ov6Error,
    io::{Read, Write},
    os::{
        fd::{AsFd, AsRawFd, BorrowedFd, FromRawFd, IntoRawFd, OwnedFd},
        ov6::syscall::{self, OpenFlags},
    },
};

pub struct Metadata {
    dev: u32,
    ino: u32,
    ty: StatType,
    nlink: u16,
    size: u64,
}

impl Metadata {
    #[must_use]
    pub fn ty(&self) -> StatType {
        self.ty
    }

    #[must_use]
    pub fn is_file(&self) -> bool {
        self.ty == StatType::File
    }

    #[must_use]
    pub fn is_device(&self) -> bool {
        self.ty == StatType::Dev
    }

    #[must_use]
    pub fn is_dir(&self) -> bool {
        self.ty == StatType::Dir
    }

    #[must_use]
    pub fn dev(&self) -> u32 {
        self.dev
    }

    #[must_use]
    pub fn ino(&self) -> u32 {
        self.ino
    }

    #[must_use]
    pub fn nlink(&self) -> u16 {
        self.nlink
    }

    #[must_use]
    pub fn size(&self) -> u64 {
        self.size
    }
}

#[derive(Default, Debug, Clone)]
pub struct OpenOptions {
    read: bool,
    write: bool,
    create: bool,
    truncate: bool,
}

impl OpenOptions {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    pub fn read(&mut self, read: bool) -> &mut Self {
        self.read = read;
        self
    }

    pub fn write(&mut self, write: bool) -> &mut Self {
        self.write = write;
        self
    }

    pub fn create(&mut self, create: bool) -> &mut Self {
        self.create = create;
        self
    }

    pub fn truncate(&mut self, truncate: bool) -> &mut Self {
        self.truncate = truncate;
        self
    }

    pub fn open<P>(&self, path: P) -> Result<File, Ov6Error>
    where
        P: AsRef<Path>,
    {
        let Self {
            read,
            write,
            create,
            truncate,
        } = self;
        let mut flags = OpenFlags::empty();
        match (read, write) {
            (true, true) => flags |= OpenFlags::READ_WRITE,
            (true, false) => flags |= OpenFlags::READ_ONLY,
            (false, true) => flags |= OpenFlags::WRITE_ONLY,
            (false, false) => return Err(Ov6Error::Unknown),
        }
        flags.set(OpenFlags::CREATE, *create);
        flags.set(OpenFlags::TRUNC, *truncate);
        let fd = syscall::open(path.as_ref(), flags)?;
        Ok(File { fd })
    }
}

#[derive(Debug)]
pub struct File {
    fd: OwnedFd,
}

impl File {
    #[must_use]
    pub fn options() -> OpenOptions {
        OpenOptions::new()
    }

    pub fn open<P>(path: P) -> Result<Self, Ov6Error>
    where
        P: AsRef<Path>,
    {
        OpenOptions::new().read(true).open(path)
    }

    pub fn create<P>(path: P) -> Result<Self, Ov6Error>
    where
        P: AsRef<Path>,
    {
        OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(true)
            .open(path)
    }

    pub fn try_clone(&self) -> Result<Self, Ov6Error> {
        let fd = syscall::dup(self.fd.as_fd())?;
        Ok(Self { fd })
    }

    pub fn metadata(&self) -> Result<Metadata, Ov6Error> {
        let stat = syscall::fstat(self.fd.as_fd())?;
        Ok(Metadata {
            dev: stat.dev.cast_unsigned(),
            ino: stat.ino,
            ty: StatType::from_repr(stat.ty).ok_or(Ov6Error::Unknown)?,
            nlink: stat.nlink.cast_unsigned(),
            size: stat.size,
        })
    }
}

impl AsFd for File {
    fn as_fd(&self) -> BorrowedFd<'_> {
        self.fd.as_fd()
    }
}

impl AsRawFd for File {
    fn as_raw_fd(&self) -> RawFd {
        self.fd.as_raw_fd()
    }
}

impl FromRawFd for File {
    unsafe fn from_raw_fd(fd: RawFd) -> Self {
        Self {
            fd: unsafe { OwnedFd::from_raw_fd(fd) },
        }
    }
}

impl IntoRawFd for File {
    fn into_raw_fd(self) -> RawFd {
        self.fd.into_raw_fd()
    }
}

impl Write for File {
    fn write(&mut self, buf: &[u8]) -> Result<usize, Ov6Error> {
        syscall::write(self.fd.as_fd(), buf)
    }
}

impl Write for &'_ File {
    fn write(&mut self, buf: &[u8]) -> Result<usize, Ov6Error> {
        syscall::write(self.fd.as_fd(), buf)
    }
}

impl Read for File {
    fn read(&mut self, buf: &mut [u8]) -> Result<usize, Ov6Error> {
        syscall::read(self.fd.as_fd(), buf)
    }
}

impl Read for &'_ File {
    fn read(&mut self, buf: &mut [u8]) -> Result<usize, Ov6Error> {
        syscall::read(self.fd.as_fd(), buf)
    }
}

pub fn mknod<P>(path: P, major: u32, minor: i16) -> Result<(), Ov6Error>
where
    P: AsRef<Path>,
{
    syscall::mknod(path.as_ref(), major, minor)
}

pub fn link<P, Q>(old: P, new: Q) -> Result<(), Ov6Error>
where
    P: AsRef<Path>,
    Q: AsRef<Path>,
{
    syscall::link(old.as_ref(), new.as_ref())
}

pub fn metadata<P>(path: P) -> Result<Metadata, Ov6Error>
where
    P: AsRef<Path>,
{
    let fd = syscall::open(path.as_ref(), OpenFlags::READ_ONLY)?;
    let stat = syscall::fstat(fd.as_fd())?;
    Ok(Metadata {
        dev: stat.dev.cast_unsigned(),
        ino: stat.ino,
        ty: StatType::from_repr(stat.ty).ok_or(Ov6Error::Unknown)?,
        nlink: stat.nlink.cast_unsigned(),
        size: stat.size,
    })
}

pub fn remove_file<P>(path: P) -> Result<(), Ov6Error>
where
    P: AsRef<Path>,
{
    syscall::unlink(path.as_ref())
}

pub fn create_dir<P>(path: P) -> Result<(), Ov6Error>
where
    P: AsRef<Path>,
{
    syscall::mkdir(path.as_ref())
}

pub fn read_dir<P>(path: P) -> Result<ReadDir, Ov6Error>
where
    P: AsRef<Path>,
{
    let fd = syscall::open(path.as_ref(), OpenFlags::READ_ONLY)?;
    let st = syscall::fstat(fd.as_fd())?;
    if st.ty != StatType::Dir as i16 {
        return Err(Ov6Error::NotADirectory);
    }
    Ok(ReadDir { fd })
}

pub struct ReadDir {
    fd: OwnedFd,
}

impl Iterator for ReadDir {
    type Item = Result<DirEntry, Ov6Error>;

    fn next(&mut self) -> Option<Self::Item> {
        loop {
            let mut ent = ov6_fs_types::DirEntry::zeroed();
            let Ok(size) = syscall::read(self.fd.as_fd(), ent.as_bytes_mut()) else {
                return Some(Err(Ov6Error::Unknown));
            };
            if size == 0 {
                return None;
            }
            if ent.ino().is_none() {
                continue;
            }
            assert_eq!(size, size_of::<ov6_fs_types::DirEntry>());
            return Some(Ok(DirEntry { ent }));
        }
    }
}

pub struct DirEntry {
    ent: ov6_fs_types::DirEntry,
}

impl DirEntry {
    #[must_use]
    pub fn name(&self) -> &OsStr {
        self.ent.name()
    }
}
