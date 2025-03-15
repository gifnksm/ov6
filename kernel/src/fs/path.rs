use ov6_types::path::{Component, Path};

use super::{Tx, inode::TxInode};
use crate::error::KernelError;

/// Looks up and returns the inode for a given path.
pub fn resolve<'tx, const READ_ONLY: bool>(
    tx: &'tx Tx<READ_ONLY>,
    cwd: TxInode<'tx, READ_ONLY>,
    path: &Path,
) -> Result<TxInode<'tx, READ_ONLY>, KernelError> {
    let mut components = path.components().peekable();
    let mut ip: TxInode<'_, READ_ONLY> = if components.next_if_eq(&Component::RootDir).is_some() {
        TxInode::root(tx)
    } else {
        cwd
    };

    for comp in components {
        let name = comp.as_os_str();

        let mut lip = ip.lock();
        let mut dip_opt = lip.as_dir();
        let Some(dip) = &mut dip_opt else {
            return Err(KernelError::NonDirectoryPathComponent);
        };

        let Some((next, _off)) = dip.lookup(name) else {
            return Err(KernelError::FsEntryNotFound);
        };

        drop(lip);
        ip = next;
    }

    Ok(ip)
}
