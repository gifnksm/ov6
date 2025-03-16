use core::mem;

use ov6_syscall::{OpenFlags, UserSlice, syscall};
use ov6_types::{os_str::OsStr, path::Path};

use super::SyscallExt;
use crate::{
    error::KernelError,
    file::File,
    fs::{self, DeviceNo, Inode, T_DEVICE, T_DIR, T_FILE},
    memory::addr::{Validate as _, Validated},
    param::MAX_PATH,
    proc::{Proc, ProcPrivateData, exec},
};

fn fetch_path<'a>(
    private: &ProcPrivateData,
    user_path: UserSlice<u8>,
    path_out: &'a mut [u8; MAX_PATH],
) -> Result<&'a Path, KernelError> {
    if user_path.len() > MAX_PATH {
        return Err(KernelError::PathTooLong);
    }
    let user_path = user_path.validate(private.pagetable())?;

    let path_out = &mut path_out[..user_path.len()];
    private.pagetable().copy_u2k_bytes(path_out, &user_path);
    if path_out.contains(&0) {
        return Err(KernelError::NullInPath);
    }
    Ok(Path::new(OsStr::from_bytes(path_out)))
}

impl SyscallExt for syscall::Dup {
    type Private<'a> = ProcPrivateData;

    fn handle(_p: &'static Proc, private: &mut Self::Private<'_>) -> Self::Return {
        let Ok((fd,)) = Self::decode_arg(private.trapframe());
        let file = private.ofile(fd)?;
        let file = file.clone();
        let fd = private.add_ofile(file)?;
        Ok(fd)
    }
}

impl SyscallExt for syscall::Read {
    type Private<'a> = ProcPrivateData;

    fn handle(_p: &'static Proc, private: &mut Self::Private<'_>) -> Self::Return {
        let Ok((fd, data)) = Self::decode_arg(private.trapframe());
        let mut data = data.validate(private.pagetable())?;
        let file = private.ofile(fd)?;
        let n = file.clone().read(private.pagetable_mut(), &mut data)?;
        Ok(n)
    }
}

impl SyscallExt for syscall::Write {
    type Private<'a> = ProcPrivateData;

    fn handle(_p: &'static Proc, private: &mut Self::Private<'_>) -> Self::Return {
        let Ok((fd, data)) = Self::decode_arg(private.trapframe());
        let data = data.validate(private.pagetable())?;
        let file = private.ofile(fd)?;
        let n = file.clone().write(private.pagetable(), &data)?;
        Ok(n)
    }
}

impl SyscallExt for syscall::Close {
    type Private<'a> = ProcPrivateData;

    fn handle(_p: &'static Proc, private: &mut Self::Private<'_>) -> Self::Return {
        let Ok((fd,)) = Self::decode_arg(private.trapframe());
        let _file = private.unset_ofile(fd)?;
        Ok(())
    }
}

impl SyscallExt for syscall::Fstat {
    type Private<'a> = ProcPrivateData;

    fn handle(_p: &'static Proc, private: &mut Self::Private<'_>) -> Self::Return {
        let Ok((fd, user_stat)) = Self::decode_arg(private.trapframe());
        let mut user_stat = user_stat.validate(private.pagetable_mut())?;
        let file = private.ofile(fd)?;
        let stat = file.clone().stat()?;
        private.pagetable_mut().copy_k2u(&mut user_stat, &stat);
        Ok(())
    }
}

impl SyscallExt for syscall::Link {
    type Private<'a> = ProcPrivateData;

    fn handle(_p: &'static Proc, private: &mut Self::Private<'_>) -> Self::Return {
        let Ok((user_old, user_new)) = Self::decode_arg(private.trapframe());

        let mut old = [0; MAX_PATH];
        let mut new = [0; MAX_PATH];
        let old = fetch_path(private, user_old, &mut old)?;
        let new = fetch_path(private, user_new, &mut new)?;

        let tx = fs::begin_tx().map_err(KernelError::from)?;
        let cwd = private.cwd().clone().into_tx(&tx);
        fs::ops::link(&tx, cwd, old, new)?;
        Ok(())
    }
}

impl SyscallExt for syscall::Unlink {
    type Private<'a> = ProcPrivateData;

    fn handle(_p: &'static Proc, private: &mut Self::Private<'_>) -> Self::Return {
        let Ok((user_path,)) = Self::decode_arg(private.trapframe());
        let mut path = [0; MAX_PATH];
        let path = fetch_path(private, user_path, &mut path)?;

        let tx = fs::begin_tx().map_err(KernelError::from)?;
        let cwd = private.cwd().clone().into_tx(&tx);
        fs::ops::unlink(&tx, cwd, path)?;
        Ok(())
    }
}

impl SyscallExt for syscall::Open {
    type Private<'a> = ProcPrivateData;

    fn handle(_p: &'static Proc, private: &mut Self::Private<'_>) -> Self::Return {
        let (user_path, mode) = Self::decode_arg(private.trapframe()).map_err(KernelError::from)?;
        let mut path = [0; MAX_PATH];
        let path = fetch_path(private, user_path, &mut path)?;

        let tx = fs::begin_tx().map_err(KernelError::from)?;
        let cwd = private.cwd().clone().into_tx(&tx);
        let mut ip = if mode.contains(OpenFlags::CREATE) {
            fs::ops::create(&tx, cwd, path, T_FILE, DeviceNo::ROOT, 0)?
        } else {
            let mut ip = fs::path::resolve(&tx, cwd, path)?;
            let lip = ip.force_wait_lock();
            if lip.is_dir() && mode != OpenFlags::READ_ONLY {
                return Err(KernelError::OpenDirAsWritable.into());
            }
            lip.unlock();
            ip
        };

        let mut lip = ip.force_wait_lock();

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

        let fd = private.add_ofile(f)?;

        Ok(fd)
    }
}

impl SyscallExt for syscall::Mkdir {
    type Private<'a> = ProcPrivateData;

    fn handle(_p: &'static Proc, private: &mut Self::Private<'_>) -> Self::Return {
        let Ok((user_path,)) = Self::decode_arg(private.trapframe());
        let mut path = [0; MAX_PATH];
        let path = fetch_path(private, user_path, &mut path)?;

        let tx = fs::begin_tx().map_err(KernelError::from)?;
        let cwd = private.cwd().clone().into_tx(&tx);
        let _ip = fs::ops::create(&tx, cwd, path, T_DIR, DeviceNo::ROOT, 0)?;

        Ok(())
    }
}

impl SyscallExt for syscall::Mknod {
    type Private<'a> = ProcPrivateData;

    fn handle(_p: &'static Proc, private: &mut Self::Private<'_>) -> Self::Return {
        let (user_path, major, minor) =
            Self::decode_arg(private.trapframe()).map_err(KernelError::from)?;
        let mut path = [0; MAX_PATH];
        let path = fetch_path(private, user_path, &mut path)?;

        let tx = fs::begin_tx().map_err(KernelError::from)?;
        let cwd = private.cwd().clone().into_tx(&tx);
        let _ip = fs::ops::create(&tx, cwd, path, T_DEVICE, DeviceNo::new(major), minor)?;

        Ok(())
    }
}

impl SyscallExt for syscall::Chdir {
    type Private<'a> = ProcPrivateData;

    fn handle(_p: &'static Proc, private: &mut Self::Private<'_>) -> Self::Return {
        let Ok((user_path,)) = Self::decode_arg(private.trapframe());
        let mut path = [0; MAX_PATH];
        let path = fetch_path(private, user_path, &mut path)?;

        let tx = fs::begin_tx().map_err(KernelError::from)?;
        let cwd = private.cwd().clone().into_tx(&tx);
        let mut ip = fs::path::resolve(&tx, cwd, path)?;
        if !ip.force_wait_lock().is_dir() {
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
    let Ok((user_path, uargv)) = super::decode_arg::<syscall::Exec>(private.trapframe());
    let mut path = [0; MAX_PATH];
    let path = fetch_path(private, user_path, &mut path)?;
    let uargv = uargv.validate(private.pagetable())?;

    let mut arg_data_size = 0;
    for i in 0..uargv.len() {
        let uarg = private.pagetable().copy_u2k(&uargv.nth(i));
        let uarg = uarg.validate(private.pagetable())?;
        arg_data_size += uarg.len() + 1; // +1 for '\0'
    }

    let uargv: Validated<UserSlice<Validated<UserSlice<u8>>>> = unsafe { mem::transmute(uargv) };

    exec::exec(p, private, path, &uargv, arg_data_size)
}

impl SyscallExt for syscall::Pipe {
    type Private<'a> = ProcPrivateData;

    fn handle(_p: &'static Proc, private: &mut Self::Private<'_>) -> Self::Return {
        let Ok((fd_array,)) = Self::decode_arg(private.trapframe());
        let mut fd_array = fd_array.validate(private.pagetable())?;

        let (rf, wf) = File::new_pipe()?;

        let rfd = private.add_ofile(rf)?;
        let wfd = match private.add_ofile(wf) {
            Ok(wfd) => wfd,
            Err(e) => {
                private.unset_ofile(rfd).unwrap();
                return Err(e.into());
            }
        };

        let fds = [rfd, wfd];
        private.pagetable_mut().copy_k2u(&mut fd_array, &fds);

        Ok(())
    }
}
