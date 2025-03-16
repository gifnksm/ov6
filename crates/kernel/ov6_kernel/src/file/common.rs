use ov6_fs_types::{T_DEVICE, T_DIR, T_FILE};
use ov6_syscall::{Stat, StatType};

use crate::{
    error::KernelError,
    fs::{self, Inode},
};

pub(super) fn close_inode(inode: Inode) {
    let tx = fs::force_begin_tx();
    inode.into_tx(&tx).put();
}

pub(super) fn stat_inode(inode: &Inode) -> Result<Stat, KernelError> {
    let tx = fs::begin_readonly_tx();
    let mut ip = inode.clone().into_tx(&tx);
    let lip = ip.wait_lock()?;
    let ty = match lip.ty() {
        T_DIR => StatType::Dir,
        T_FILE => StatType::File,
        T_DEVICE => StatType::Dev,
        ty => return Err(KernelError::CorruptedInodeType(lip.ino(), ty)),
    };
    let st = Stat {
        dev: lip.dev().value().cast_signed(),
        ino: lip.ino().value(),
        ty: ty as i16,
        nlink: lip.nlink(),
        padding: [0; 4],
        size: u64::from(lip.size()),
    };
    drop(lip);
    drop(ip);
    Ok(st)
}
