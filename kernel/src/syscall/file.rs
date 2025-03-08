use ov6_syscall::{OpenFlags, ReturnType, SyscallError, syscall as sys};
use ov6_types::{fs::RawFd, os_str::OsStr, path::Path};

use crate::{
    error::KernelError,
    file::File,
    fs::{self, DeviceNo, Inode, T_DEVICE, T_DIR, T_FILE},
    memory::{PAGE_SIZE, page, vm},
    param::{MAX_ARG, MAX_PATH},
    proc::{Proc, ProcPrivateData, ProcPrivateDataGuard, exec},
    syscall,
};

/// Fetches the nth word-sized system call argument as a file descriptor
/// and returns the descriptor and the corresponding `File`.
fn arg_fd(private: &ProcPrivateData, n: usize) -> Result<(RawFd, &File), KernelError> {
    let fd = RawFd::new(syscall::arg_int(private, n));
    let file = private.ofile(fd).ok_or(KernelError::Unknown)?;
    Ok((fd, file))
}

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
    let (_fd, f) = arg_fd(private, 0)?;
    let f = f.clone();
    let fd = fd_alloc(private, f)?;
    Ok(fd)
}

pub fn sys_read(
    p: &'static Proc,
    private: &mut Option<ProcPrivateDataGuard>,
) -> ReturnType<sys::Read> {
    let private = private.as_mut().unwrap();
    let va = syscall::arg_addr(private, 1);
    let n = syscall::arg_int(private, 2);
    let (_fd, f) = arg_fd(private, 0)?;
    let n = f.clone().read(p, private, va, n)?;
    Ok(n)
}

pub fn sys_write(
    p: &'static Proc,
    private: &mut Option<ProcPrivateDataGuard>,
) -> ReturnType<sys::Write> {
    let private = private.as_mut().unwrap();
    let va = syscall::arg_addr(private, 1);
    let n = syscall::arg_int(private, 2);
    let (_fd, f) = arg_fd(private, 0)?;
    let n = f.clone().write(p, private, va, n)?;
    Ok(n)
}

pub fn sys_close(
    _p: &'static Proc,
    private: &mut Option<ProcPrivateDataGuard>,
) -> ReturnType<sys::Close> {
    let private = private.as_mut().unwrap();
    let (fd, _f) = arg_fd(private, 0)?;
    private.unset_ofile(fd);
    Ok(())
}

pub fn sys_fstat(
    _p: &'static Proc,
    private: &mut Option<ProcPrivateDataGuard>,
) -> ReturnType<sys::Fstat> {
    let private = private.as_mut().unwrap();
    let va = syscall::arg_addr(private, 1);
    let (_fd, f) = arg_fd(private, 0)?;
    f.clone().stat(private, va)?;
    Ok(())
}

/// Creates the path `new` as a link to the same inode as `old`.
pub fn sys_link(
    _p: &'static Proc,
    private: &mut Option<ProcPrivateDataGuard>,
) -> ReturnType<sys::Link> {
    let private = private.as_mut().unwrap();
    let mut new = [0; MAX_PATH];
    let mut old = [0; MAX_PATH];

    let old = syscall::arg_str(private, 0, &mut old)?;
    let new = syscall::arg_str(private, 1, &mut new)?;

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
    let mut path = [0; MAX_PATH];
    let path = syscall::arg_str(private, 0, &mut path)?;
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
    let mode = OpenFlags::from_bits_retain(syscall::arg_int(private, 1));
    let mut path = [0; MAX_PATH];
    let path = syscall::arg_str(private, 0, &mut path)?;
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
    let mut path = [0; MAX_PATH];
    let path = syscall::arg_str(private, 0, &mut path)?;
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
    let mut path = [0; MAX_PATH];
    let path = syscall::arg_str(private, 0, &mut path)?;
    let path = Path::new(OsStr::from_bytes(path));
    let major = u32::try_from(syscall::arg_int(private, 1)).unwrap();
    let minor = i16::try_from(syscall::arg_int(private, 2)).unwrap();

    let tx = fs::begin_tx();
    let _ip = fs::ops::create(&tx, private, path, T_DEVICE, DeviceNo::new(major), minor)?;

    Ok(())
}

pub fn sys_chdir(
    _p: &'static Proc,
    private: &mut Option<ProcPrivateDataGuard>,
) -> ReturnType<sys::Chdir> {
    let private = private.as_mut().unwrap();
    let mut path = [0; MAX_PATH];
    let path = syscall::arg_str(private, 0, &mut path)?;
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
    let mut path = [0; MAX_PATH];
    let path = syscall::arg_str(private, 0, &mut path)?;
    let path = Path::new(OsStr::from_bytes(path));
    let uargv = syscall::arg_addr(private, 1);

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
    let fd_array = syscall::arg_addr(private, 0);

    let (rf, wf) = File::new_pipe()?;

    let Ok(rfd) = fd_alloc(private, rf) else {
        return Err(KernelError::Unknown.into());
    };
    let Ok(wfd) = fd_alloc(private, wf) else {
        private.unset_ofile(rfd);
        return Err(KernelError::Unknown.into());
    };

    let fds = [rfd, wfd];
    if vm::copy_out(private.pagetable_mut().unwrap(), fd_array, &fds).is_err() {
        private.unset_ofile(rfd);
        private.unset_ofile(wfd);
        return Err(KernelError::Unknown.into());
    }

    Ok(())
}
