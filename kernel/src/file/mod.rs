use ov6_syscall::{Stat, UserMutRef, UserMutSlice, UserSlice};

pub use self::device::{Device, register_device};
use self::{alloc::FileDataArc, device::DeviceFile, inode::InodeFile, pipe::PipeFile};
use crate::{
    error::KernelError,
    fs::{DeviceNo, Inode},
    proc::{Proc, ProcPrivateData},
};

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
    data: Option<SpecificData>,
}

enum SpecificData {
    Pipe(PipeFile),
    Inode(InodeFile),
    Device(DeviceFile),
}

impl Drop for FileData {
    fn drop(&mut self) {
        match self.data.take() {
            Some(SpecificData::Pipe(pipe)) => pipe.close(self.writable),
            Some(SpecificData::Inode(inode)) => inode.close(),
            Some(SpecificData::Device(device)) => device.close(),
            None => {}
        }
    }
}

impl File {
    pub fn new_pipe() -> Result<(Self, Self), KernelError> {
        pipe::new_file()
    }

    pub fn new_device(
        major: DeviceNo,
        inode: Inode,
        readable: bool,
        writable: bool,
    ) -> Result<Self, KernelError> {
        device::new_file(major, inode, readable, writable)
    }

    pub fn new_inode(inode: Inode, readable: bool, writable: bool) -> Result<Self, KernelError> {
        inode::new_file(inode, readable, writable)
    }

    /// Increments ref count for the file.
    pub fn dup(&self) -> Self {
        self.clone()
    }

    /// Decrements ref count for the file.
    pub fn close(self) {
        // consume self to drop
        let _ = self;
    }

    /// Gets metadata about file `f`.
    ///
    /// `addr` is a user virtual address, pointing to a struct stat.
    pub fn stat(
        &self,
        private: &mut ProcPrivateData,
        dst: UserMutRef<Stat>,
    ) -> Result<(), KernelError> {
        match &self.data.data {
            Some(SpecificData::Inode(inode)) => inode.stat(private, dst),
            Some(SpecificData::Device(device)) => device.stat(private, dst),
            Some(SpecificData::Pipe(_)) => Err(KernelError::StatOnNonFsEntry),
            None => unreachable!(),
        }
    }

    /// Reads from file `f`.
    ///
    /// `addr` is a user virtual address.
    pub fn read(
        &self,
        p: &Proc,
        private: &mut ProcPrivateData,
        dst: UserMutSlice<u8>,
    ) -> Result<usize, KernelError> {
        if !self.data.readable {
            return Err(KernelError::FileDescriptorNotReadable);
        }

        match &self.data.data {
            Some(SpecificData::Pipe(pipe)) => pipe.read(p, private, dst),
            Some(SpecificData::Inode(inode)) => inode.read(private, dst),
            Some(SpecificData::Device(device)) => device.read(p, private, dst),
            None => unreachable!(),
        }
    }

    /// Writes to file `f`.
    ///
    /// `addr` is a user virtual address.
    pub fn write(
        &self,
        p: &Proc,
        private: &mut ProcPrivateData,
        src: UserSlice<u8>,
    ) -> Result<usize, KernelError> {
        if !self.data.writable {
            return Err(KernelError::FileDescriptorNotWritable);
        }

        match &self.data.data {
            Some(SpecificData::Pipe(pipe)) => pipe.write(p, private, src),
            Some(SpecificData::Inode(inode)) => inode.write(private, src),
            Some(SpecificData::Device(device)) => device.write(p, private, src),
            _ => unreachable!(),
        }
    }
}
