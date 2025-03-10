use ov6_fs_types::{T_DEVICE, T_DIR, T_FILE};
use ov6_syscall::{Stat, StatType};

use crate::{
    error::KernelError,
    fs::{self, Inode},
    memory::{VirtAddr, vm},
    proc::ProcPrivateData,
};

pub(super) fn close_inode(inode: Inode) {
    let tx = fs::begin_tx();
    inode.into_tx(&tx).put();
}

pub(super) fn stat_inode(
    inode: &Inode,
    private: &mut ProcPrivateData,
    addr: VirtAddr,
) -> Result<(), KernelError> {
    let tx = fs::begin_readonly_tx();
    let mut ip = inode.clone().into_tx(&tx);
    let lip = ip.lock();
    let ty = match lip.ty() {
        T_DIR => StatType::Dir,
        T_FILE => StatType::File,
        T_DEVICE => StatType::Dev,
        _ => return Err(KernelError::Unknown),
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
    vm::copy_out(private.pagetable_mut().unwrap(), addr, &st)?;
    Ok(())
}
