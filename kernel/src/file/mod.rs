use crate::{
    error::Error,
    fs::{DeviceNo, Inode},
    memory::vm::VirtAddr,
    proc::Proc,
};

use self::{alloc::FileDataArc, device::DeviceFile, inode::InodeFile, pipe::PipeFile};

pub use self::device::{Device, register_device};

mod alloc;
mod common;
mod device;
mod inode;
mod pipe;

pub fn init() {
    alloc::init();
}

#[derive(Clone)]
pub struct File {
    data: FileDataArc,
}

struct FileData {
    readable: bool,
    writable: bool,
    data: SpecificData,
}

enum SpecificData {
    Pipe(PipeFile),
    Inode(InodeFile),
    Device(DeviceFile),
}

impl Drop for FileData {
    fn drop(&mut self) {
        match &self.data {
            SpecificData::Pipe(pipe) => pipe.close(self.writable),
            SpecificData::Inode(inode) => inode.close(),
            SpecificData::Device(device) => device.close(),
        }
    }
}

impl File {
    pub fn new_pipe() -> Result<(File, File), Error> {
        pipe::new_file()
    }

    pub fn new_device(
        major: DeviceNo,
        inode: Inode,
        readable: bool,
        writable: bool,
    ) -> Result<File, Error> {
        device::new_file(major, inode, readable, writable)
    }

    pub fn new_inode(inode: Inode, readable: bool, writable: bool) -> Result<File, Error> {
        inode::new_file(inode, readable, writable)
    }

    /// Increments ref count for the file.
    pub fn dup(&self) -> Self {
        self.clone()
    }

    /// Decrements ref count for the file.
    pub fn close(self) {
        // consume self to drop
    }

    /// Gets metadata about file `f`.
    ///
    /// `addr` is a user virtual address, pointing to a struct stat.
    pub fn stat(&self, p: &Proc, addr: VirtAddr) -> Result<(), Error> {
        match &self.data.data {
            SpecificData::Inode(inode) => inode.stat(p, addr),
            SpecificData::Device(device) => device.stat(p, addr),
            _ => Err(Error::Unknown),
        }
    }

    /// Reads from file `f`.
    ///
    /// `addr` is a user virtual address.
    pub fn read(&self, p: &Proc, addr: VirtAddr, n: usize) -> Result<usize, Error> {
        if !self.data.readable {
            return Err(Error::Unknown);
        }

        match &self.data.data {
            SpecificData::Pipe(pipe) => pipe.read(addr, n),
            SpecificData::Inode(inode) => inode.read(p, addr, n),
            SpecificData::Device(device) => device.read(addr, n),
        }
    }

    /// Writes to file `f`.
    ///
    /// `addr` is a user virtual address.
    pub fn write(&self, p: &Proc, addr: VirtAddr, n: usize) -> Result<usize, Error> {
        if !self.data.writable {
            return Err(Error::Unknown);
        }

        match &self.data.data {
            SpecificData::Pipe(pipe) => pipe.write(addr, n),
            SpecificData::Inode(inode) => inode.write(p, addr, n),
            SpecificData::Device(device) => device.write(addr, n),
        }
    }
}
