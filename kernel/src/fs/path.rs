use ov6_types::path::{Component, Path};

use super::{DeviceNo, InodeNo, Tx, inode::TxInode};
use crate::{error::KernelError, proc::ProcPrivateData};

/// Looks up and returns the inode for a given path.
pub fn resolve<'a, const READ_ONLY: bool>(
    tx: &'a Tx<READ_ONLY>,
    private: &mut ProcPrivateData,
    path: &Path,
) -> Result<TxInode<'a, READ_ONLY>, KernelError> {
    let mut components = path.components().peekable();
    let mut ip: TxInode<'_, READ_ONLY> = if components.next_if_eq(&Component::RootDir).is_some() {
        TxInode::get(tx, DeviceNo::ROOT, InodeNo::ROOT)
    } else {
        private.cwd().unwrap().clone().into_tx(tx)
    };

    for comp in components {
        let name = comp.as_os_str();

        let mut lip = ip.lock();
        let mut dip_opt = lip.as_dir();
        let Some(dip) = &mut dip_opt else {
            return Err(KernelError::Unknown);
        };

        let Some((next, _off)) = dip.lookup(private, name) else {
            return Err(KernelError::Unknown);
        };

        drop(lip);
        ip = next;
    }

    Ok(ip)
}
