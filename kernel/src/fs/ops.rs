use dataview::PodMethods as _;

use crate::{error::Error, fs::repr, proc::ProcPrivateData};

use super::{
    DIR_SIZE, DeviceNo, Tx,
    inode::TxInode,
    path,
    repr::{T_DEVICE, T_FILE},
};

pub fn unlink(tx: &Tx<false>, private: &ProcPrivateData, path: &[u8]) -> Result<(), Error> {
    let mut name = [0; DIR_SIZE];
    let (mut parent_ip, name) = path::resolve_parent(tx, private, path, &mut name)?;

    // Cannot unlink "." of "..".
    if name == b".." || name == b"." {
        return Err(Error::Unknown);
    }

    let mut parent_lip = parent_ip.lock();
    let mut parent_dp = parent_lip.as_dir().ok_or(Error::Unknown)?;

    let (mut child_ip, off) = parent_dp.lookup(private, name).ok_or(Error::Unknown)?;
    let mut child_lip = child_ip.lock();

    assert!(child_lip.data().nlink > 0);
    if let Some(mut child_dp) = child_lip.as_dir() {
        if !child_dp.is_empty(private) {
            return Err(Error::Unknown);
        }
    }

    let de = repr::DirEntry::zeroed();
    parent_dp.get_inner().write_data(private, off, &de).unwrap();

    if child_lip.is_dir() {
        // decrement reference to parent directory.
        parent_dp.get_inner().data_mut().nlink -= 1;
        parent_dp.get_inner().update();
    }
    parent_lip.unlock();
    parent_ip.put();

    child_lip.data_mut().nlink -= 1;
    child_lip.update();

    Ok(())
}

pub fn create<'tx>(
    tx: &'tx Tx<'tx, false>,
    private: &ProcPrivateData,
    path: &[u8],
    ty: i16,
    major: DeviceNo,
    minor: i16,
) -> Result<TxInode<'tx, false>, Error> {
    let mut name = [0; DIR_SIZE];
    let (mut parent_ip, name) = path::resolve_parent(tx, private, path, &mut name)?;

    let mut parent_lip = parent_ip.lock();
    let Some(mut parent_dp) = parent_lip.as_dir() else {
        return Err(Error::Unknown);
    };

    if let Some((mut child_ip, _off)) = parent_dp.lookup(private, name) {
        let lip = child_ip.lock();
        if ty == T_FILE && (lip.data().ty == T_FILE || lip.data().ty == T_DEVICE) {
            drop(lip);
            return Ok(child_ip);
        }
        return Err(Error::Unknown);
    }

    let mut child_ip = TxInode::alloc(tx, parent_dp.dev(), ty)?;
    let mut child_lip = child_ip.lock();
    child_lip.data_mut().major = major;
    child_lip.data_mut().minor = minor;
    child_lip.data_mut().nlink = 0; // update after
    child_lip.update();

    if let Some(mut child_dp) = child_lip.as_dir() {
        // Create "." and ".." entries
        child_dp.link(private, b".", child_dp.ino())?;
        child_dp.link(private, b"..", parent_dp.ino())?;
    }

    parent_dp.link(private, name, child_lip.ino())?;

    if child_lip.is_dir() {
        // now that success is guaranteed:
        parent_lip.data_mut().nlink += 1; // for ".."
        parent_lip.update();
    }

    child_lip.data_mut().nlink = 1;
    child_lip.update();

    drop(child_lip);
    Ok(child_ip)
}

pub fn link(
    tx: &Tx<false>,
    private: &ProcPrivateData,
    old_path: &[u8],
    new_path: &[u8],
) -> Result<(), Error> {
    let mut old_ip = path::resolve(tx, private, old_path)?;
    let old_lip = old_ip.lock();
    if old_lip.is_dir() {
        return Err(Error::Unknown);
    }
    old_lip.unlock();

    let mut name = [0; DIR_SIZE];
    let (mut parent_ip, name) = path::resolve_parent(tx, private, new_path, &mut name)?;
    let mut parent_lip = parent_ip.lock();
    if parent_lip.dev() != old_ip.dev() {
        return Err(Error::Unknown);
    }
    let Some(mut parent_dp) = parent_lip.as_dir() else {
        return Err(Error::Unknown);
    };
    parent_dp.link(private, name, old_ip.ino())?;

    let mut old_lip = old_ip.lock();
    old_lip.data_mut().nlink += 1;
    old_lip.update();

    Ok(())
}
