use dataview::PodMethods as _;
use ov6_types::{
    os_str::OsStr,
    path::{Component, Path},
};

use super::{
    DeviceNo, Tx,
    inode::TxInode,
    path,
    repr::{T_DEVICE, T_FILE},
};
use crate::{error::KernelError, fs::repr, proc::ProcPrivateData};

fn split_path(path: &Path) -> Option<(&Path, &OsStr)> {
    let mut it = path.components();
    let file_name = it
        .next_back()
        .filter(|p| *p != Component::RootDir)?
        .as_os_str();
    let dir_path = it.as_path();
    Some((dir_path, file_name))
}

pub fn unlink(
    tx: &Tx<false>,
    private: &mut ProcPrivateData,
    path: &Path,
) -> Result<(), KernelError> {
    let (dir_path, file_name) = split_path(path).ok_or(KernelError::UnlinkRootDir)?;
    let mut dir_ip = path::resolve(tx, private, dir_path)?;

    // Cannot unlink "." or "..".
    if file_name == ".." || file_name == "." {
        return Err(KernelError::Unknown);
    }

    let mut dir_lip = dir_ip.lock();
    let mut dir_dp = dir_lip
        .as_dir()
        .ok_or(KernelError::NonDirectoryPathComponent)?;

    let (mut file_ip, off) = dir_dp
        .lookup(private, file_name)
        .ok_or(KernelError::FsEntryNotFound)?;
    let mut file_lip = file_ip.lock();

    assert!(file_lip.data().nlink > 0);
    if let Some(mut file_dp) = file_lip.as_dir() {
        if !file_dp.is_empty(private) {
            return Err(KernelError::DirectoryNotEmpty);
        }
    }

    let de = repr::DirEntry::zeroed();
    dir_dp.get_inner().write_data(private, off, &de).unwrap();

    if file_lip.is_dir() {
        // decrement reference to parent directory.
        dir_dp.get_inner().data_mut().nlink -= 1;
        dir_dp.get_inner().update();
    }
    dir_lip.unlock();
    dir_ip.put();

    file_lip.data_mut().nlink -= 1;
    file_lip.update();

    Ok(())
}

pub fn create<'tx>(
    tx: &'tx Tx<'tx, false>,
    private: &mut ProcPrivateData,
    path: &Path,
    ty: i16,
    major: DeviceNo,
    minor: i16,
) -> Result<TxInode<'tx, false>, KernelError> {
    let (dir_path, file_name) = split_path(path).ok_or(KernelError::CreateRootDir)?;
    let mut dir_ip = path::resolve(tx, private, dir_path)?;

    let mut dir_lip = dir_ip.lock();
    let mut dir_dp = dir_lip
        .as_dir()
        .ok_or(KernelError::NonDirectoryPathComponent)?;

    if let Some((mut file_ip, _off)) = dir_dp.lookup(private, file_name) {
        let file_lip = file_ip.lock();
        if ty == T_FILE && (file_lip.data().ty == T_FILE || file_lip.data().ty == T_DEVICE) {
            drop(file_lip);
            return Ok(file_ip);
        }
        return Err(KernelError::CreateAlreadyExists);
    }

    let mut file_ip = TxInode::alloc(tx, dir_dp.dev(), ty)?;
    let mut file_lip = file_ip.lock();
    file_lip.data_mut().major = major;
    file_lip.data_mut().minor = minor;
    file_lip.data_mut().nlink = 0; // update after
    file_lip.update();

    if let Some(mut child_dp) = file_lip.as_dir() {
        // Create "." and ".." entries
        child_dp.link(private, OsStr::new("."), child_dp.ino())?;
        child_dp.link(private, OsStr::new(".."), dir_dp.ino())?;
    }

    dir_dp.link(private, file_name, file_lip.ino())?;

    if file_lip.is_dir() {
        // now that success is guaranteed:
        dir_lip.data_mut().nlink += 1; // for ".."
        dir_lip.update();
    }

    file_lip.data_mut().nlink = 1;
    file_lip.update();

    drop(file_lip);
    Ok(file_ip)
}

pub fn link(
    tx: &Tx<false>,
    private: &mut ProcPrivateData,
    old_path: &Path,
    new_path: &Path,
) -> Result<(), KernelError> {
    let (new_dir_path, new_file_name) = split_path(new_path).ok_or(KernelError::LinkRootDir)?;

    let mut old_ip = path::resolve(tx, private, old_path)?;
    let old_lip = old_ip.lock();
    if old_lip.is_dir() {
        return Err(KernelError::NonDirectoryPathComponent);
    }
    old_lip.unlock();

    let mut new_dir_ip = path::resolve(tx, private, new_dir_path)?;
    let mut new_dir_lip = new_dir_ip.lock();
    if new_dir_lip.dev() != old_ip.dev() {
        return Err(KernelError::LinkCrossDevices);
    }
    let Some(mut new_dir_dp) = new_dir_lip.as_dir() else {
        return Err(KernelError::LinkToNonDirectory);
    };
    new_dir_dp.link(private, new_file_name, old_ip.ino())?;

    let mut old_lip = old_ip.lock();
    old_lip.data_mut().nlink += 1;
    old_lip.update();

    Ok(())
}
