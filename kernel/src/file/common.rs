use xv6_fs_types::{T_DEVICE, T_DIR, T_FILE};
use xv6_syscall::{Stat, StatType};

use crate::{
    error::Error,
    fs::{self, Inode},
    memory::vm::{self, VirtAddr},
    proc::Proc,
};

pub(super) fn close_inode(inode: Inode) {
    let tx = fs::begin_tx();
    inode.into_tx(&tx).put();
}

pub(super) fn stat_inode(inode: &Inode, p: &Proc, addr: VirtAddr) -> Result<(), Error> {
    let tx = fs::begin_readonly_tx();
    let mut ip = inode.clone().into_tx(&tx);
    let lip = ip.lock();
    let ty = match lip.ty() {
        T_DIR => StatType::Dir,
        T_FILE => StatType::File,
        T_DEVICE => StatType::Dir,
        _ => return Err(Error::Unknown),
    };
    let st = Stat {
        dev: lip.dev().value().cast_signed(),
        ino: lip.ino().value(),
        ty: ty as _,
        nlink: lip.nlink(),
        size: u64::from(lip.size()),
    };
    drop(lip);
    drop(ip);
    vm::copy_out(p.pagetable().unwrap(), addr, &st)?;
    Ok(())
}
