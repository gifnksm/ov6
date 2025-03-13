use alloc::boxed::Box;
use core::alloc::AllocError;

use arrayvec::ArrayVec;
use ov6_syscall::{OpenFlags, UserSlice, syscall as sys};
use ov6_types::{fs::RawFd, os_str::OsStr, path::Path};

use super::SyscallExt;
use crate::{
    error::KernelError,
    file::File,
    fs::{self, DeviceNo, Inode, T_DEVICE, T_DIR, T_FILE},
    memory::{PAGE_SIZE, page::PageFrameAllocator},
    param::{MAX_ARG, MAX_PATH},
    proc::{Proc, ProcPrivateData, exec},
};

/// Allocates a file descriptor for the given `File`.
///
/// Takes over file reference from caller on success.
fn fd_alloc(private: &mut ProcPrivateData, file: File) -> Result<RawFd, KernelError> {
    private.add_ofile(file)
}

fn fetch_path<'a>(
    private: &ProcPrivateData,
    user_path: UserSlice<u8>,
    path_out: &'a mut [u8; MAX_PATH],
) -> Result<&'a Path, KernelError> {
    if user_path.len() > MAX_PATH {
        return Err(KernelError::PathTooLong);
    }

    let path_out = &mut path_out[..user_path.len()];
    private.pagetable().copy_in_bytes(path_out, user_path)?;
    Ok(Path::new(OsStr::from_bytes(path_out)))
}

impl SyscallExt for sys::Dup {
    type Private<'a> = ProcPrivateData;

    fn handle(_p: &'static Proc, private: &mut Self::Private<'_>) -> Self::Return {
        let Ok((fd,)) = Self::decode_arg(private.trapframe());
        let file = private.ofile(fd)?;
        let file = file.clone();
        let fd = fd_alloc(private, file)?;
        Ok(fd)
    }
}

impl SyscallExt for sys::Read {
    type Private<'a> = ProcPrivateData;

    fn handle(p: &'static Proc, private: &mut Self::Private<'_>) -> Self::Return {
        let Ok((fd, data)) = Self::decode_arg(private.trapframe());
        let file = private.ofile(fd)?;
        let n = file.clone().read(p, private, data)?;
        Ok(n)
    }
}

impl SyscallExt for sys::Write {
    type Private<'a> = ProcPrivateData;

    fn handle(p: &'static Proc, private: &mut Self::Private<'_>) -> Self::Return {
        let Ok((fd, data)) = Self::decode_arg(private.trapframe());
        let file = private.ofile(fd)?;
        let n = file.clone().write(p, private, data)?;
        Ok(n)
    }
}

impl SyscallExt for sys::Close {
    type Private<'a> = ProcPrivateData;

    fn handle(_p: &'static Proc, private: &mut Self::Private<'_>) -> Self::Return {
        let Ok((fd,)) = Self::decode_arg(private.trapframe());
        let _file = private.ofile(fd)?;
        private.unset_ofile(fd);
        Ok(())
    }
}

impl SyscallExt for sys::Fstat {
    type Private<'a> = ProcPrivateData;

    fn handle(_p: &'static Proc, private: &mut Self::Private<'_>) -> Self::Return {
        let Ok((fd, stat)) = Self::decode_arg(private.trapframe());
        let file = private.ofile(fd)?;
        file.clone().stat(private, stat)?;
        Ok(())
    }
}

impl SyscallExt for sys::Link {
    type Private<'a> = ProcPrivateData;

    fn handle(_p: &'static Proc, private: &mut Self::Private<'_>) -> Self::Return {
        let Ok((user_old, user_new)) = Self::decode_arg(private.trapframe());

        let mut old = [0; MAX_PATH];
        let mut new = [0; MAX_PATH];
        let old = fetch_path(private, user_old, &mut old)?;
        let new = fetch_path(private, user_new, &mut new)?;

        let tx = fs::begin_tx();
        fs::ops::link(&tx, private, old, new)?;
        Ok(())
    }
}

impl SyscallExt for sys::Unlink {
    type Private<'a> = ProcPrivateData;

    fn handle(_p: &'static Proc, private: &mut Self::Private<'_>) -> Self::Return {
        let Ok((user_path,)) = Self::decode_arg(private.trapframe());
        let mut path = [0; MAX_PATH];
        let path = fetch_path(private, user_path, &mut path)?;

        let tx = fs::begin_tx();
        fs::ops::unlink(&tx, private, path)?;
        Ok(())
    }
}

impl SyscallExt for sys::Open {
    type Private<'a> = ProcPrivateData;

    fn handle(_p: &'static Proc, private: &mut Self::Private<'_>) -> Self::Return {
        let (user_path, mode) = Self::decode_arg(private.trapframe()).map_err(KernelError::from)?;
        let mut path = [0; MAX_PATH];
        let path = fetch_path(private, user_path, &mut path)?;

        let tx = fs::begin_tx();
        let mut ip = if mode.contains(OpenFlags::CREATE) {
            fs::ops::create(&tx, private, path, T_FILE, DeviceNo::ROOT, 0)?
        } else {
            let mut ip = fs::path::resolve(&tx, private, path)?;
            let lip = ip.lock();
            if lip.is_dir() && mode != OpenFlags::READ_ONLY {
                return Err(KernelError::OpenDirAsWritable.into());
            }
            lip.unlock();
            ip
        };

        let mut lip = ip.lock();

        let readable = !mode.contains(OpenFlags::WRITE_ONLY);
        let writable = mode.contains(OpenFlags::WRITE_ONLY) || mode.contains(OpenFlags::READ_WRITE);
        let f = if lip.ty() == T_DEVICE {
            File::new_device(lip.major(), Inode::from_locked(&lip), readable, writable)?
        } else {
            File::new_inode(Inode::from_locked(&lip), readable, writable)?
        };

        if mode.contains(OpenFlags::TRUNC) && lip.ty() == T_FILE {
            lip.truncate();
        }

        let fd = fd_alloc(private, f)?;

        Ok(fd)
    }
}

impl SyscallExt for sys::Mkdir {
    type Private<'a> = ProcPrivateData;

    fn handle(_p: &'static Proc, private: &mut Self::Private<'_>) -> Self::Return {
        let Ok((user_path,)) = Self::decode_arg(private.trapframe());
        let mut path = [0; MAX_PATH];
        let path = fetch_path(private, user_path, &mut path)?;

        let tx = fs::begin_tx();
        let _ip = fs::ops::create(&tx, private, path, T_DIR, DeviceNo::ROOT, 0)?;

        Ok(())
    }
}

impl SyscallExt for sys::Mknod {
    type Private<'a> = ProcPrivateData;

    fn handle(_p: &'static Proc, private: &mut Self::Private<'_>) -> Self::Return {
        let (user_path, major, minor) =
            Self::decode_arg(private.trapframe()).map_err(KernelError::from)?;
        let mut path = [0; MAX_PATH];
        let path = fetch_path(private, user_path, &mut path)?;

        let tx = fs::begin_tx();
        let _ip = fs::ops::create(&tx, private, path, T_DEVICE, DeviceNo::new(major), minor)?;

        Ok(())
    }
}

impl SyscallExt for sys::Chdir {
    type Private<'a> = ProcPrivateData;

    fn handle(_p: &'static Proc, private: &mut Self::Private<'_>) -> Self::Return {
        let Ok((user_path,)) = Self::decode_arg(private.trapframe());
        let mut path = [0; MAX_PATH];
        let path = fetch_path(private, user_path, &mut path)?;

        let tx = fs::begin_tx();
        let mut ip = fs::path::resolve(&tx, private, path)?;
        if !ip.lock().is_dir() {
            return Err(KernelError::ChdirNotDir.into());
        }
        let old = private.update_cwd(Inode::from_tx(&ip));
        old.into_tx(&tx).put();

        Ok(())
    }
}

pub fn sys_exec(
    p: &'static Proc,
    private: &mut ProcPrivateData,
) -> Result<(usize, usize), KernelError> {
    let Ok((user_path, uargv)) = super::decode_arg::<sys::Exec>(private.trapframe());
    let mut path = [0; MAX_PATH];
    let path = fetch_path(private, user_path, &mut path)?;

    let mut argv: ArrayVec<(usize, Box<[u8; PAGE_SIZE], PageFrameAllocator>), { MAX_ARG - 1 }> =
        ArrayVec::new();

    for i in 0..uargv.len() {
        let uarg = private.pagetable().copy_in(uargv.nth(i))?;
        if uarg.len() > PAGE_SIZE {
            return Err(KernelError::ArgumentListTooLarge);
        }

        let mut buf = Box::try_new_in([0; PAGE_SIZE], PageFrameAllocator)
            .map_err(|AllocError| KernelError::NoFreePage)?;
        private
            .pagetable()
            .copy_in_bytes(&mut buf[..uarg.len()], uarg)?;

        if argv.try_push((uarg.len(), buf)).is_err() {
            return Err(KernelError::ArgumentListTooLong);
        }
    }

    exec::exec(p, private, path, &argv)
}

impl SyscallExt for sys::Pipe {
    type Private<'a> = ProcPrivateData;

    fn handle(_p: &'static Proc, private: &mut Self::Private<'_>) -> Self::Return {
        let Ok((fd_array,)) = Self::decode_arg(private.trapframe());

        let (rf, wf) = File::new_pipe()?;

        let rfd = fd_alloc(private, rf)?;
        let wfd = match fd_alloc(private, wf) {
            Ok(wfd) => wfd,
            Err(e) => {
                private.unset_ofile(rfd);
                return Err(e.into());
            }
        };

        let fds = [rfd, wfd];
        if let Err(e) = private.pagetable_mut().copy_out(fd_array, &fds) {
            private.unset_ofile(rfd);
            private.unset_ofile(wfd);
            return Err(e.into());
        }

        Ok(())
    }
}
