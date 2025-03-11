use alloc::boxed::Box;
use core::alloc::AllocError;

use arrayvec::ArrayVec;
use ov6_syscall::{OpenFlags, ReturnType, syscall as sys};
use ov6_types::{fs::RawFd, os_str::OsStr, path::Path};

use crate::{
    error::KernelError,
    file::File,
    fs::{self, DeviceNo, Inode, T_DEVICE, T_DIR, T_FILE},
    memory::{PAGE_SIZE, VirtAddr, page::PageFrameAllocator, vm},
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
    let file = private.ofile(fd)?;
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
    let file = private.ofile(fd)?;
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
    let file = private.ofile(fd)?;
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
    let _file = private.ofile(fd)?;
    private.unset_ofile(fd);
    Ok(())
}

pub fn sys_fstat(
    _p: &'static Proc,
    private: &mut Option<ProcPrivateDataGuard>,
) -> ReturnType<sys::Fstat> {
    let private = private.as_mut().unwrap();
    let Ok((fd, stat)) = super::decode_arg::<sys::Fstat>(private.trapframe().unwrap());
    let file = private.ofile(fd)?;
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
        return Err(KernelError::ChdirNotDir.into());
    }
    let old = private.update_cwd(Inode::from_tx(&ip));
    old.into_tx(&tx).put();

    Ok(())
}

pub fn sys_exec(
    p: &'static Proc,
    private: &mut Option<ProcPrivateDataGuard>,
) -> Result<(usize, usize), KernelError> {
    let private = private.as_mut().unwrap();
    let Ok((path_ptr, argv_ptr)) = super::decode_arg::<sys::Exec>(private.trapframe().unwrap());
    let mut path = [0; MAX_PATH];
    let path = super::fetch_str(private, VirtAddr::new(path_ptr.addr()), &mut path)?;
    let path = Path::new(OsStr::from_bytes(path));
    let uargv = VirtAddr::new(argv_ptr.addr());

    let mut argv: ArrayVec<Box<[u8; PAGE_SIZE], PageFrameAllocator>, { MAX_ARG - 1 }> =
        ArrayVec::new();

    for i in 0.. {
        let uarg = syscall::fetch_addr(private, uargv.byte_add(i * size_of::<usize>()))?;
        if uarg.addr() == 0 {
            break;
        }

        let mut buf = Box::try_new_in([0; PAGE_SIZE], PageFrameAllocator)
            .map_err(|AllocError| KernelError::NoFreePage)?;
        syscall::fetch_str(private, uarg, buf.as_mut_slice())?;

        if argv.try_push(buf).is_err() {
            return Err(KernelError::ArgumentListTooLong);
        }
    }

    exec::exec(p, private, path, &argv)
}

pub fn sys_pipe(
    _p: &'static Proc,
    private: &mut Option<ProcPrivateDataGuard>,
) -> ReturnType<sys::Pipe> {
    let private = private.as_mut().unwrap();
    let Ok((fd_array,)) = super::decode_arg::<sys::Pipe>(private.trapframe().unwrap());

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
    if let Err(e) = vm::copy_out(
        private.pagetable_mut().unwrap(),
        VirtAddr::new(fd_array.addr()),
        &fds,
    ) {
        private.unset_ofile(rfd);
        private.unset_ofile(wfd);
        return Err(e.into());
    }

    Ok(())
}
