use ov6_syscall::{Stat, UserMutSlice, UserSlice};

use super::{File, FileData, FileDataArc, SpecificData};
use crate::{
    error::KernelError,
    fs::{DeviceNo, Inode},
    memory::{
        addr::{GenericMutSlice, GenericSlice, Validated},
        vm_user::UserPageTable,
    },
    param::NDEV,
    sync::SpinLock,
};

pub struct Device {
    pub read: fn(dst: &mut GenericMutSlice<u8>) -> Result<usize, KernelError>,
    pub write: fn(src: &GenericSlice<u8>) -> Result<usize, KernelError>,
}

struct DeviceTable {
    devices: [Option<Device>; NDEV],
}

impl DeviceTable {
    const fn new() -> Self {
        Self {
            devices: [const { None }; NDEV],
        }
    }

    fn register(&mut self, no: DeviceNo, dev: Device) {
        self.devices[no.value() as usize] = Some(dev);
    }

    fn get_device(&self, no: DeviceNo) -> Option<&Device> {
        self.devices.get(no.as_index()).and_then(Option::as_ref)
    }
}

static DEVICE_TABLE: SpinLock<DeviceTable> = SpinLock::new(DeviceTable::new());

pub fn register_device(no: DeviceNo, dev: Device) {
    DEVICE_TABLE.lock().register(no, dev);
}

pub(super) struct DeviceFile {
    major: DeviceNo,
    inode: Inode,
}

pub(super) fn new_file(
    major: DeviceNo,
    inode: Inode,
    readable: bool,
    writable: bool,
) -> Result<File, KernelError> {
    let data = FileDataArc::try_new(FileData {
        readable,
        writable,
        data: Some(SpecificData::Device(DeviceFile { major, inode })),
    })?;
    Ok(File { data })
}

impl DeviceFile {
    pub(super) fn close(self) {
        super::common::close_inode(self.inode);
    }

    pub(super) fn stat(&self) -> Result<Stat, KernelError> {
        super::common::stat_inode(&self.inode)
    }

    pub(super) fn read(
        &self,
        pt: &mut UserPageTable,
        dst: &mut Validated<UserMutSlice<u8>>,
    ) -> Result<usize, KernelError> {
        let read = DEVICE_TABLE
            .lock()
            .get_device(self.major)
            .ok_or(KernelError::DeviceNotFound(self.major))?
            .read;
        read(&mut (pt, dst).into())
    }

    pub(super) fn write(
        &self,
        pt: &UserPageTable,
        src: &Validated<UserSlice<u8>>,
    ) -> Result<usize, KernelError> {
        let write = DEVICE_TABLE
            .lock()
            .get_device(self.major)
            .ok_or(KernelError::DeviceNotFound(self.major))?
            .write;
        write(&(pt, src).into())
    }
}
