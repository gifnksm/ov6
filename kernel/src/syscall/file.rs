use xv6_syscall::OpenFlags;

use crate::{
    error::Error,
    file::File,
    fs::{self, DeviceNo, Inode, T_DEVICE, T_DIR, T_FILE},
    memory::{
        page,
        vm::{self, PAGE_SIZE},
    },
    param::{MAX_ARG, MAX_PATH},
    proc::{Proc, exec},
    syscall,
};

/// Fetches the nth word-sized system call argument as a file descriptor
/// and returns the descriptor and the corresponding `File`.
fn arg_fd(p: &Proc, n: usize) -> Result<(usize, File), Error> {
    let fd = syscall::arg_int(p, n);
    let file = p.ofile(fd).ok_or(Error::Unknown)?;
    Ok((fd, file))
}

/// Allocates a file descriptor for the given `File`.
///
/// Takes over file reference from caller on success.
fn fd_alloc(p: &Proc, file: &File) -> Result<usize, Error> {
    p.add_ofile(file.clone()).ok_or(Error::Unknown)
}

pub fn sys_dup(p: &Proc) -> Result<usize, Error> {
    let (_fd, f) = arg_fd(p, 0)?;
    let fd = fd_alloc(p, &f)?;
    Ok(fd)
}

pub fn sys_read(p: &Proc) -> Result<usize, Error> {
    let va = syscall::arg_addr(p, 1);
    let n = syscall::arg_int(p, 2);
    let (_fd, f) = arg_fd(p, 0)?;
    f.read(p, va, n)
}

pub fn sys_write(p: &Proc) -> Result<usize, Error> {
    let va = syscall::arg_addr(p, 1);
    let n = syscall::arg_int(p, 2);
    let (_fd, f) = arg_fd(p, 0)?;
    f.write(p, va, n)
}

pub fn sys_close(p: &Proc) -> Result<usize, Error> {
    let (fd, _f) = arg_fd(p, 0)?;
    p.unset_ofile(fd);
    Ok(0)
}

pub fn sys_fstat(p: &Proc) -> Result<usize, Error> {
    let va = syscall::arg_addr(p, 1);
    let (_fd, f) = arg_fd(p, 0)?;
    f.stat(p, va)?;
    Ok(0)
}

/// Creates the path `new` as a link to the same inode as `old`.
pub fn sys_link(p: &Proc) -> Result<usize, Error> {
    let mut new = [0; MAX_PATH];
    let mut old = [0; MAX_PATH];

    let old = syscall::arg_str(p, 0, &mut old)?;
    let new = syscall::arg_str(p, 1, &mut new)?;

    let tx = fs::begin_tx();
    fs::ops::link(&tx, p, old, new)?;
    Ok(0)
}

pub fn sys_unlink(p: &Proc) -> Result<usize, Error> {
    let mut path = [0; MAX_PATH];
    let path = syscall::arg_str(p, 0, &mut path)?;

    let tx = fs::begin_tx();
    fs::ops::unlink(&tx, p, path)?;
    Ok(0)
}

pub fn sys_open(p: &Proc) -> Result<usize, Error> {
    let mode = OpenFlags::from_bits_retain(syscall::arg_int(p, 1));
    let mut path = [0; MAX_PATH];
    let path = syscall::arg_str(p, 0, &mut path)?;

    let tx = fs::begin_tx();
    let mut ip = if mode.contains(OpenFlags::CREATE) {
        fs::ops::create(&tx, p, path, T_FILE, DeviceNo::ROOT, 0)?
    } else {
        let mut ip = fs::path::resolve(&tx, p, path)?;
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

    let fd = fd_alloc(p, &f)?;

    Ok(fd)
}

pub fn sys_mkdir(p: &Proc) -> Result<usize, Error> {
    let mut path = [0; MAX_PATH];
    let path = syscall::arg_str(p, 0, &mut path)?;

    let tx = fs::begin_tx();
    let _ip = fs::ops::create(&tx, p, path, T_DIR, DeviceNo::ROOT, 0)?;

    Ok(0)
}

pub fn sys_mknod(p: &Proc) -> Result<usize, Error> {
    let mut path = [0; MAX_PATH];
    let path = syscall::arg_str(p, 0, &mut path)?;
    let major = syscall::arg_int(p, 1) as u32;
    let minor = syscall::arg_int(p, 2) as i16;

    let tx = fs::begin_tx();
    let _ip = fs::ops::create(&tx, p, path, T_DEVICE, DeviceNo::new(major), minor)?;

    Ok(0)
}

pub fn sys_chdir(p: &Proc) -> Result<usize, Error> {
    let mut path = [0; MAX_PATH];
    let path = syscall::arg_str(p, 0, &mut path)?;

    let tx = fs::begin_tx();
    let mut ip = fs::path::resolve(&tx, p, path)?;
    if !ip.lock().is_dir() {
        return Err(Error::Unknown);
    }
    let old = p.update_cwd(Inode::from_tx(&ip));
    old.into_tx(&tx).put();

    Ok(0)
}

pub fn sys_exec(p: &Proc) -> Result<usize, Error> {
    let mut path = [0; MAX_PATH];
    let path = syscall::arg_str(p, 0, &mut path)?;
    let uargv = syscall::arg_addr(p, 1);

    let mut argv = [None; MAX_ARG];
    let res = (|| {
        for i in 0.. {
            if i > argv.len() {
                return Err(Error::Unknown);
            }

            let uarg = syscall::fetch_addr(p, uargv.byte_add(i * size_of::<usize>()))?;
            if uarg.addr() == 0 {
                argv[i] = None;
                break;
            }
            argv[i] = Some(page::alloc_page().ok_or(Error::Unknown)?);
            let buf =
                unsafe { core::slice::from_raw_parts_mut(argv[i].unwrap().as_ptr(), PAGE_SIZE) };
            syscall::fetch_str(p, uarg, buf)?;
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

    let ret = exec::exec(path, argv.as_ptr().cast());

    for arg in argv.iter().filter_map(|&a| a) {
        unsafe {
            page::free_page(arg);
        }
    }

    ret
}

pub fn sys_pipe(p: &Proc) -> Result<usize, Error> {
    let fd_array = syscall::arg_addr(p, 0);

    let (rf, wf) = File::new_pipe()?;

    let Ok(rfd) = fd_alloc(p, &rf) else {
        return Err(Error::Unknown);
    };
    let Ok(wfd) = fd_alloc(p, &wf) else {
        p.unset_ofile(rfd);
        return Err(Error::Unknown);
    };

    let fds = [rfd as i32, wfd as i32];
    if vm::copy_out(p.pagetable().unwrap(), fd_array, &fds).is_err() {
        p.unset_ofile(rfd);
        p.unset_ofile(wfd);
        return Err(Error::Unknown);
    }

    Ok(0)
}
