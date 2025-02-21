use dataview::PodMethods as _;

use crate::{fs::repr, proc::Proc};

use super::{
    DIR_SIZE, Tx,
    inode::TxInode,
    path,
    stat::{T_DEVICE, T_FILE},
};

pub fn unlink(tx: &Tx<false>, p: &Proc, path: &[u8]) -> Result<(), ()> {
    let mut name = [0; DIR_SIZE];
    let (mut parent_ip, name) = path::resolve_parent(tx, p, path, &mut name)?;

    // Cannot unlink "." of "..".
    if name == b".." || name == b"." {
        return Err(());
    }

    let mut parent_lip = parent_ip.lock();
    let mut parent_dp = parent_lip.as_dir().ok_or(())?;

    let (mut child_ip, off) = parent_dp.lookup(p, name).ok_or(())?;
    let mut child_lip = child_ip.lock();

    assert!(child_lip.data().nlink > 0);
    if let Some(mut child_dp) = child_lip.as_dir() {
        if !child_dp.is_empty(p) {
            return Err(());
        }
    }

    let de = repr::DirEntry::zeroed();
    parent_dp.get_inner().write_data(p, off, &de).unwrap();

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
    p: &Proc,
    path: &[u8],
    ty: i16,
    major: i16,
    minor: i16,
) -> Result<TxInode<'tx, false>, ()> {
    let mut name = [0; DIR_SIZE];
    let (mut parent_ip, name) = path::resolve_parent(tx, p, path, &mut name)?;

    let mut parent_lip = parent_ip.lock();
    let Some(mut parent_dp) = parent_lip.as_dir() else {
        return Err(());
    };

    if let Some((mut child_ip, _off)) = parent_dp.lookup(p, name) {
        let lip = child_ip.lock();
        if ty == T_FILE && (lip.data().ty == T_FILE || lip.data().ty == T_DEVICE) {
            drop(lip);
            return Ok(child_ip);
        }
        return Err(());
    }

    let mut child_ip = TxInode::alloc(tx, parent_dp.dev(), ty)?;
    let mut child_lip = child_ip.lock();
    child_lip.data_mut().major = major;
    child_lip.data_mut().minor = minor;
    child_lip.data_mut().nlink = 1;
    child_lip.update();

    if let Some(mut child_dp) = child_lip.as_dir() {
        // Create "." and ".." entries
        if child_dp.link(p, b".", child_dp.inum()).is_err()
            || child_dp.link(p, b"..", parent_dp.inum()).is_err()
        {
            // TODO: refactor error handling. immediate closure pattern is denied by borrow check.
            child_lip.data_mut().nlink = 0;
            child_lip.update();
            return Err(());
        }
    }

    if parent_dp.link(p, name, child_lip.inum()).is_err() {
        // TODO: refactor error handling. immediate closure pattern is denied by borrow check.
        child_lip.data_mut().nlink = 0;
        child_lip.update();
        return Err(());
    }

    if child_lip.is_dir() {
        // now that success is guaranteed:
        parent_lip.data_mut().nlink += 1; // for ".."
        parent_lip.update();
    }

    drop(child_lip);
    Ok(child_ip)
}

pub fn link(tx: &Tx<false>, p: &Proc, old_path: &[u8], new_path: &[u8]) -> Result<(), ()> {
    let mut old_ip = path::resolve(tx, p, old_path)?;
    let mut old_lip = old_ip.lock();

    if old_lip.is_dir() {
        return Err(());
    }

    old_lip.data_mut().nlink += 1;
    old_lip.update();
    old_lip.unlock();

    let res = (|| {
        let mut name = [0; DIR_SIZE];
        let (mut parent_ip, name) = path::resolve_parent(tx, p, new_path, &mut name)?;
        let mut parent_lip = parent_ip.lock();
        if parent_lip.dev() != old_ip.dev() {
            return Err(());
        }
        let Some(mut parent_dp) = parent_lip.as_dir() else {
            return Err(());
        };
        parent_dp.link(p, name, old_ip.inum())?;

        Ok(())
    })();

    if res.is_err() {
        let mut old_lip = old_ip.lock();
        old_lip.data_mut().nlink -= 1;
        old_lip.update();
    }

    res
}
