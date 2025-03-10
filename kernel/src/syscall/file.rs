use ov6_syscall::{OpenFlags, ReturnType, SyscallError, syscall as sys};
use ov6_types::{fs::RawFd, os_str::OsStr, path::Path};

use crate::{
    error::KernelError,
    file::File,
    fs::{self, DeviceNo, Inode, T_DEVICE, T_DIR, T_FILE},
    memory::{PAGE_SIZE, VirtAddr, page, vm},
    param::{MAX_ARG, MAX_PATH},
    proc::{Proc, ProcPrivateData, ProcPrivateDataGuard, exec},
    syscall,
};

/// Allocates a file descriptor for the given `File`.
///
/// Takes over file reference from caller on success.
fn fd_alloc(private: &mut ProcPrivateData, file: File) -> Result<RawFd, KernelError> {
    private.add_ofile(file)
}

pub fn sys_dup(
    _p: &'static Proc,
    private: &mut Option<ProcPrivateDataGuard>,
) -> ReturnType<sys::Dup> {
    let private = private.as_mut().unwrap();
    let Ok((fd,)) = super::decode_arg::<sys::Dup>(private.trapframe().unwrap());
    let file = private.ofile(fd).ok_or(KernelError::Unknown)?;
    let file = file.clone();
    let fd = fd_alloc(private, file)?;
    Ok(fd)
}

pub fn sys_read(
    p: &'static Proc,
    private: &mut Option<ProcPrivateDataGuard>,
) -> ReturnType<sys::Read> {
    let private = private.as_mut().unwrap();
    let Ok((fd, data)) = super::decode_arg::<sys::Read>(private.trapframe().unwrap());
    let file = private.ofile(fd).ok_or(KernelError::Unknown)?;
    let n = file
        .clone()
        .read(p, private, VirtAddr::new(data.addr()), data.len())?;
    Ok(n)
}

pub fn sys_write(
    p: &'static Proc,
    private: &mut Option<ProcPrivateDataGuard>,
) -> ReturnType<sys::Write> {
    let private = private.as_mut().unwrap();
    let Ok((fd, data)) = super::decode_arg::<sys::Read>(private.trapframe().unwrap());
    let file = private.ofile(fd).ok_or(KernelError::Unknown)?;
    let n = file
        .clone()
        .write(p, private, VirtAddr::new(data.addr()), data.len())?;
    Ok(n)
}

pub fn sys_close(
    _p: &'static Proc,
    private: &mut Option<ProcPrivateDataGuard>,
) -> ReturnType<sys::Close> {
    let private = private.as_mut().unwrap();
    let Ok((fd,)) = super::decode_arg::<sys::Close>(private.trapframe().unwrap());
    let _file = private.ofile(fd).ok_or(KernelError::Unknown)?;
    private.unset_ofile(fd);
    Ok(())
}

pub fn sys_fstat(
    _p: &'static Proc,
    private: &mut Option<ProcPrivateDataGuard>,
) -> ReturnType<sys::Fstat> {
    let private = private.as_mut().unwrap();
    let Ok((fd, stat)) = super::decode_arg::<sys::Fstat>(private.trapframe().unwrap());
    let file = private.ofile(fd).ok_or(KernelError::Unknown)?;
    file.clone().stat(private, VirtAddr::new(stat.addr()))?;
    Ok(())
}

/// Creates the path `new` as a link to the same inode as `old`.
pub fn sys_link(
    _p: &'static Proc,
    private: &mut Option<ProcPrivateDataGuard>,
) -> ReturnType<sys::Link> {
    let private = private.as_mut().unwrap();
    let Ok((old_ptr, new_ptr)) = super::decode_arg::<sys::Link>(private.trapframe().unwrap());

    let mut new = [0; MAX_PATH];
    let mut old = [0; MAX_PATH];

    let old = super::fetch_str(private, VirtAddr::new(old_ptr.addr()), &mut old)?;
    let new = super::fetch_str(private, VirtAddr::new(new_ptr.addr()), &mut new)?;

    let old = Path::new(OsStr::from_bytes(old));
    let new = Path::new(OsStr::from_bytes(new));

    let tx = fs::begin_tx();
    fs::ops::link(&tx, private, old, new)?;
    Ok(())
}

pub fn sys_unlink(
    _p: &'static Proc,
    private: &mut Option<ProcPrivateDataGuard>,
) -> ReturnType<sys::Unlink> {
    let private = private.as_mut().unwrap();
    let Ok((path_ptr,)) = super::decode_arg::<sys::Unlink>(private.trapframe().unwrap());
    let mut path = [0; MAX_PATH];
    let path = super::fetch_str(private, VirtAddr::new(path_ptr.addr()), &mut path)?;
    let path = Path::new(OsStr::from_bytes(path));

    let tx = fs::begin_tx();
    fs::ops::unlink(&tx, private, path)?;
    Ok(())
}

pub fn sys_open(
    _p: &'static Proc,
    private: &mut Option<ProcPrivateDataGuard>,
) -> ReturnType<sys::Open> {
    let private = private.as_mut().unwrap();
    let mut path = [0; MAX_PATH];
    let (path_ptr, mode) =
        super::decode_arg::<sys::Open>(private.trapframe().unwrap()).map_err(KernelError::from)?;
    let path = super::fetch_str(private, VirtAddr::new(path_ptr.addr()), &mut path)?;
    let path = Path::new(OsStr::from_bytes(path));

    let tx = fs::begin_tx();
    let mut ip = if mode.contains(OpenFlags::CREATE) {
        fs::ops::create(&tx, private, path, T_FILE, DeviceNo::ROOT, 0)?
    } else {
        let mut ip = fs::path::resolve(&tx, private, path)?;
        let lip = ip.lock();
        if lip.is_dir() && mode != OpenFlags::READ_ONLY {
            return Err(KernelError::Unknown.into());
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

pub fn sys_mkdir(
    _p: &'static Proc,
    private: &mut Option<ProcPrivateDataGuard>,
) -> ReturnType<sys::Mkdir> {
    let private = private.as_mut().unwrap();
    let Ok((path_ptr,)) = super::decode_arg::<sys::Mkdir>(private.trapframe().unwrap());

    let mut path = [0; MAX_PATH];
    let path = super::fetch_str(private, VirtAddr::new(path_ptr.addr()), &mut path)?;
    let path = Path::new(OsStr::from_bytes(path));

    let tx = fs::begin_tx();
    let _ip = fs::ops::create(&tx, private, path, T_DIR, DeviceNo::ROOT, 0)?;

    Ok(())
}

pub fn sys_mknod(
    _p: &'static Proc,
    private: &mut Option<ProcPrivateDataGuard>,
) -> ReturnType<sys::Mknod> {
    let private = private.as_mut().unwrap();
    let (path_ptr, major, minor) =
        super::decode_arg::<sys::Mknod>(private.trapframe().unwrap()).map_err(KernelError::from)?;
    let mut path = [0; MAX_PATH];
    let path = super::fetch_str(private, VirtAddr::new(path_ptr.addr()), &mut path)?;
    let path = Path::new(OsStr::from_bytes(path));

    let tx = fs::begin_tx();
    let _ip = fs::ops::create(&tx, private, path, T_DEVICE, DeviceNo::new(major), minor)?;

    Ok(())
}

pub fn sys_chdir(
    _p: &'static Proc,
    private: &mut Option<ProcPrivateDataGuard>,
) -> ReturnType<sys::Chdir> {
    let private = private.as_mut().unwrap();
    let Ok((path_ptr,)) = super::decode_arg::<sys::Chdir>(private.trapframe().unwrap());
    let mut path = [0; MAX_PATH];
    let path = super::fetch_str(private, VirtAddr::new(path_ptr.addr()), &mut path)?;
    let path = Path::new(OsStr::from_bytes(path));

    let tx = fs::begin_tx();
    let mut ip = fs::path::resolve(&tx, private, path)?;
    if !ip.lock().is_dir() {
        return Err(KernelError::Unknown.into());
    }
    let old = private.update_cwd(Inode::from_tx(&ip));
    old.into_tx(&tx).put();

    Ok(())
}

pub fn sys_exec(
    p: &'static Proc,
    private: &mut Option<ProcPrivateDataGuard>,
) -> Result<(usize, usize), SyscallError> {
    let private = private.as_mut().unwrap();
    let Ok((path_ptr, argv_ptr)) = super::decode_arg::<sys::Exec>(private.trapframe().unwrap());
    let mut path = [0; MAX_PATH];
    let path = super::fetch_str(private, VirtAddr::new(path_ptr.addr()), &mut path)?;
    let path = Path::new(OsStr::from_bytes(path));
    let uargv = VirtAddr::new(argv_ptr.addr());

    let mut argv = [None; MAX_ARG];
    let res = (|| {
        for i in 0.. {
            if i > argv.len() {
                return Err(KernelError::Unknown);
            }

            let uarg = syscall::fetch_addr(private, uargv.byte_add(i * size_of::<usize>()))?;
            if uarg.addr() == 0 {
                argv[i] = None;
                break;
            }
            argv[i] = Some(page::alloc_page().ok_or(KernelError::Unknown)?);
            let buf =
                unsafe { core::slice::from_raw_parts_mut(argv[i].unwrap().as_ptr(), PAGE_SIZE) };
            syscall::fetch_str(private, uarg, buf)?;
        }
        Ok(())
    })();

    if res.is_err() {
        for arg in argv.iter().filter_map(|&a| a) {
            unsafe {
                page::free_page(arg);
            }
        }
        return Err(KernelError::Unknown.into());
    }

    let ret = exec::exec(p, private, path, argv.as_ptr().cast());

    for arg in argv.iter().filter_map(|&a| a) {
        unsafe {
            page::free_page(arg);
        }
    }

    let (argc, argv) = ret.map_err(SyscallError::from)?;
    Ok((argc, argv))
}

pub fn sys_pipe(
    _p: &'static Proc,
    private: &mut Option<ProcPrivateDataGuard>,
) -> ReturnType<sys::Pipe> {
    let private = private.as_mut().unwrap();
    let Ok((fd_array,)) = super::decode_arg::<sys::Pipe>(private.trapframe().unwrap());

    let (rf, wf) = File::new_pipe()?;

    let Ok(rfd) = fd_alloc(private, rf) else {
        return Err(KernelError::Unknown.into());
    };
    let Ok(wfd) = fd_alloc(private, wf) else {
        private.unset_ofile(rfd);
        return Err(KernelError::Unknown.into());
    };

    let fds = [rfd, wfd];
    if vm::copy_out(
        private.pagetable_mut().unwrap(),
        VirtAddr::new(fd_array.addr()),
        &fds,
    )
    .is_err()
    {
        private.unset_ofile(rfd);
        private.unset_ofile(wfd);
        return Err(KernelError::Unknown.into());
    }

    Ok(())
}
