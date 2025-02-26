use crate::{error::Error, proc::Proc};

use super::{DIR_SIZE, DeviceNo, InodeNo, Tx, inode::TxInode};

/// Copies the next path element from path into name.
///
/// Returns a pair of the next path element and the remainder of the path.
/// The returned path has no leading slashes.
/// If no name to remove, return None.
///
/// # Examples
///
/// ```
/// assert_eq!(skip_elem(b"a/bb/c"), Some((b"a", b"bb/c")));
/// assert_eq!(skip_elem(b"///a//bb"), Some((b"a", b"bb")));
/// assert_eq!(skip_elem(b"a"), Some((b"a", b"")));
/// assert_eq!(skip_elem(b"a/"), Some((b"a", b"")));
/// assert_eq!(skip_elem(b""), None);
/// assert_eq!(skip_elem(b"///"), None);
/// ```
fn skip_elem(path: &[u8]) -> Option<(&[u8], &[u8])> {
    let start = path.iter().position(|&c| c != b'/')?;
    let path = &path[start..];
    let end = path.iter().position(|&c| c == b'/').unwrap_or(path.len());
    let elem = &path[..end];
    let path = &path[end..];
    let next = path.iter().position(|&c| c != b'/').unwrap_or(path.len());
    let path = &path[next..];
    Some((elem, path))
}

/// Looks up and returns the inode for a given path.
///
/// If `parent` is `true`, returns the inode for the parent and copy the final
/// path element into `name`, which must have room for `DIR_SIZE` bytes.
/// Must be called inside a transaction since it calls `inode_put()`.
fn resolve_impl<'a, const READ_ONLY: bool>(
    tx: &'a Tx<READ_ONLY>,
    p: &Proc,
    path: &[u8],
    parent: bool,
    mut name_out: Option<&mut [u8; DIR_SIZE]>,
) -> Result<TxInode<'a, READ_ONLY>, Error> {
    let mut ip: TxInode<'_, READ_ONLY> = if path.first() == Some(&b'/') {
        TxInode::get(tx, DeviceNo::ROOT, InodeNo::ROOT)
    } else {
        p.cwd().unwrap().clone().into_tx(tx)
    };

    let mut path = path;
    while let Some((name, rest)) = skip_elem(path) {
        path = rest;
        if let Some(name_out) = &mut name_out {
            let copy_len = usize::min(name.len(), name_out.len());
            name_out[..copy_len].copy_from_slice(&name[..copy_len]);
            name_out[copy_len..].fill(0);
        }

        let mut lip = ip.lock();
        let mut dip_opt = lip.as_dir();
        let Some(dip) = &mut dip_opt else {
            return Err(Error::Unknown);
        };

        if parent && path.is_empty() {
            // Stop one level early.
            drop(lip);
            return Ok(ip);
        }

        let Some((next, _off)) = dip.lookup(p, name) else {
            return Err(Error::Unknown);
        };

        drop(lip);
        ip = next;
    }

    if parent {
        return Err(Error::Unknown);
    }
    Ok(ip)
}

pub fn resolve<'a, const READ_ONLY: bool>(
    tx: &'a Tx<READ_ONLY>,
    p: &Proc,
    path: &[u8],
) -> Result<TxInode<'a, READ_ONLY>, Error> {
    resolve_impl(tx, p, path, false, None)
}

pub fn resolve_parent<'a, 'b, const READ_ONLY: bool>(
    tx: &'a Tx<READ_ONLY>,
    p: &Proc,
    path: &[u8],
    name: &'b mut [u8; DIR_SIZE],
) -> Result<(TxInode<'a, READ_ONLY>, &'b [u8]), Error> {
    let ip = resolve_impl(tx, p, path, true, Some(name))?;
    let len = name.iter().position(|b| *b == 0).unwrap_or(name.len());
    let name = &name[..len];
    Ok((ip, name))
}
