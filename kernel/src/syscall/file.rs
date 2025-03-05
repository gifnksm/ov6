use ov6_syscall::OpenFlags;

use crate::{
    error::Error,
    file::File,
    fs::{self, DeviceNo, Inode, T_DEVICE, T_DIR, T_FILE},
    memory::{PAGE_SIZE, page, vm},
    param::{MAX_ARG, MAX_PATH},
    proc::{Proc, ProcPrivateData, ProcPrivateDataGuard, exec},
    syscall,
};

/// Fetches the nth word-sized system call argument as a file descriptor
/// and returns the descriptor and the corresponding `File`.
fn arg_fd(private: &ProcPrivateData, n: usize) -> Result<(usize, &File), Error> {
    let fd = syscall::arg_int(private, n);
    let file = private.ofile(fd).ok_or(Error::Unknown)?;
    Ok((fd, file))
}

/// Allocates a file descriptor for the given `File`.
///
/// Takes over file reference from caller on success.
fn fd_alloc(private: &mut ProcPrivateData, file: File) -> Result<usize, Error> {
    private.add_ofile(file)
}

pub fn sys_dup(
    _p: &'static Proc,
    private: &mut Option<ProcPrivateDataGuard>,
) -> Result<usize, Error> {
    let private = private.as_mut().unwrap();
    let (_fd, f) = arg_fd(private, 0)?;
    let f = f.clone();
    let fd = fd_alloc(private, f)?;
    Ok(fd)
}

pub fn sys_read(
    p: &'static Proc,
    private: &mut Option<ProcPrivateDataGuard>,
) -> Result<usize, Error> {
    let private = private.as_mut().unwrap();
    let va = syscall::arg_addr(private, 1);
    let n = syscall::arg_int(private, 2);
    let (_fd, f) = arg_fd(private, 0)?;
    f.clone().read(p, private, va, n)
}

pub fn sys_write(
    p: &'static Proc,
    private: &mut Option<ProcPrivateDataGuard>,
) -> Result<usize, Error> {
    let private = private.as_mut().unwrap();
    let va = syscall::arg_addr(private, 1);
    let n = syscall::arg_int(private, 2);
    let (_fd, f) = arg_fd(private, 0)?;
    f.clone().write(p, private, va, n)
}

pub fn sys_close(
    _p: &'static Proc,
    private: &mut Option<ProcPrivateDataGuard>,
) -> Result<usize, Error> {
    let private = private.as_mut().unwrap();
    let (fd, _f) = arg_fd(private, 0)?;
    private.unset_ofile(fd);
    Ok(0)
}

pub fn sys_fstat(
    _p: &'static Proc,
    private: &mut Option<ProcPrivateDataGuard>,
) -> Result<usize, Error> {
    let private = private.as_mut().unwrap();
    let va = syscall::arg_addr(private, 1);
    let (_fd, f) = arg_fd(private, 0)?;
    f.stat(private, va)?;
    Ok(0)
}

/// Creates the path `new` as a link to the same inode as `old`.
pub fn sys_link(
    _p: &'static Proc,
    private: &mut Option<ProcPrivateDataGuard>,
) -> Result<usize, Error> {
    let private = private.as_mut().unwrap();
    let mut new = [0; MAX_PATH];
    let mut old = [0; MAX_PATH];

    let old = syscall::arg_str(private, 0, &mut old)?;
    let new = syscall::arg_str(private, 1, &mut new)?;

    let tx = fs::begin_tx();
    fs::ops::link(&tx, private, old, new)?;
    Ok(0)
}

pub fn sys_unlink(
    _p: &'static Proc,
    private: &mut Option<ProcPrivateDataGuard>,
) -> Result<usize, Error> {
    let private = private.as_mut().unwrap();
    let mut path = [0; MAX_PATH];
    let path = syscall::arg_str(private, 0, &mut path)?;

    let tx = fs::begin_tx();
    fs::ops::unlink(&tx, private, path)?;
    Ok(0)
}

pub fn sys_open(
    _p: &'static Proc,
    private: &mut Option<ProcPrivateDataGuard>,
) -> Result<usize, Error> {
    let private = private.as_mut().unwrap();
    let mode = OpenFlags::from_bits_retain(syscall::arg_int(private, 1));
    let mut path = [0; MAX_PATH];
    let path = syscall::arg_str(private, 0, &mut path)?;

    let tx = fs::begin_tx();
    let mut ip = if mode.contains(OpenFlags::CREATE) {
        fs::ops::create(&tx, private, path, T_FILE, DeviceNo::ROOT, 0)?
    } else {
        let mut ip = fs::path::resolve(&tx, private, path)?;
        let lip = ip.lock();
        if lip.is_dir() && mode != OpenFlags::READ_ONLY {
            return Err(Error::Unknown);
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
) -> Result<usize, Error> {
    let private = private.as_mut().unwrap();
    let mut path = [0; MAX_PATH];
    let path = syscall::arg_str(private, 0, &mut path)?;

    let tx = fs::begin_tx();
    let _ip = fs::ops::create(&tx, private, path, T_DIR, DeviceNo::ROOT, 0)?;

    Ok(0)
}

pub fn sys_mknod(
    _p: &'static Proc,
    private: &mut Option<ProcPrivateDataGuard>,
) -> Result<usize, Error> {
    let private = private.as_mut().unwrap();
    let mut path = [0; MAX_PATH];
    let path = syscall::arg_str(private, 0, &mut path)?;
    let major = syscall::arg_int(private, 1) as u32;
    let minor = syscall::arg_int(private, 2) as i16;

    let tx = fs::begin_tx();
    let _ip = fs::ops::create(&tx, private, path, T_DEVICE, DeviceNo::new(major), minor)?;

    Ok(0)
}

pub fn sys_chdir(
    _p: &'static Proc,
    private: &mut Option<ProcPrivateDataGuard>,
) -> Result<usize, Error> {
    let private = private.as_mut().unwrap();
    let mut path = [0; MAX_PATH];
    let path = syscall::arg_str(private, 0, &mut path)?;

    let tx = fs::begin_tx();
    let mut ip = fs::path::resolve(&tx, private, path)?;
    if !ip.lock().is_dir() {
        return Err(Error::Unknown);
    }
    let old = private.update_cwd(Inode::from_tx(&ip));
    old.into_tx(&tx).put();

    Ok(0)
}

pub fn sys_exec(
    p: &'static Proc,
    private: &mut Option<ProcPrivateDataGuard>,
) -> Result<usize, Error> {
    let private = private.as_mut().unwrap();
    let mut path = [0; MAX_PATH];
    let path = syscall::arg_str(private, 0, &mut path)?;
    let uargv = syscall::arg_addr(private, 1);

    let mut argv = [None; MAX_ARG];
    let res = (|| {
        for i in 0.. {
            if i > argv.len() {
                return Err(Error::Unknown);
            }

            let uarg = syscall::fetch_addr(private, uargv.byte_add(i * size_of::<usize>()))?;
            if uarg.addr() == 0 {
                argv[i] = None;
                break;
            }
            argv[i] = Some(page::alloc_page().ok_or(Error::Unknown)?);
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
        return Err(Error::Unknown);
    }

    let ret = exec::exec(p, private, path, argv.as_ptr().cast());

    for arg in argv.iter().filter_map(|&a| a) {
        unsafe {
            page::free_page(arg);
        }
    }

    ret
}

pub fn sys_pipe(
    _p: &'static Proc,
    private: &mut Option<ProcPrivateDataGuard>,
) -> Result<usize, Error> {
    let private = private.as_mut().unwrap();
    let fd_array = syscall::arg_addr(private, 0);

    let (rf, wf) = File::new_pipe()?;

    let Ok(rfd) = fd_alloc(private, rf) else {
        return Err(Error::Unknown);
    };
    let Ok(wfd) = fd_alloc(private, wf) else {
        private.unset_ofile(rfd);
        return Err(Error::Unknown);
    };

    let fds = [rfd as i32, wfd as i32];
    if vm::copy_out(private.pagetable().unwrap(), fd_array, &fds).is_err() {
        private.unset_ofile(rfd);
        private.unset_ofile(wfd);
        return Err(Error::Unknown);
    }

    Ok(0)
}
