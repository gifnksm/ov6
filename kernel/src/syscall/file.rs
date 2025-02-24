use core::ptr::NonNull;

use xv6_syscall::OpenFlags;

use crate::{
    error::Error,
    file::{self, File, pipe},
    fs::{self, Inode, T_DEVICE, T_DIR, T_FILE},
    memory::{
        page,
        vm::{self, PAGE_SIZE},
    },
    param::{MAX_ARG, MAX_PATH, NDEV},
    proc::{Proc, exec},
    syscall,
};

/// Fetches the nth word-sized system call argument as a file descriptor
/// and returns the descriptor and the corresponding `File`.
fn arg_fd(p: &Proc, n: usize) -> Result<(usize, NonNull<File>), Error> {
    let fd = syscall::arg_int(p, n);
    let file = p.ofile(fd).ok_or(Error::Unknown)?;
    Ok((fd, file))
}

/// Allocates a file descriptor for the given `File`.
///
/// Takes over file reference from caller on success.
fn fd_alloc(p: &Proc, file: NonNull<File>) -> Result<usize, Error> {
    p.add_ofile(file).ok_or(Error::Unknown)
}

pub fn sys_dup(p: &Proc) -> Result<usize, Error> {
    let (_fd, f) = arg_fd(p, 0)?;
    let fd = fd_alloc(p, f)?;
    file::dup(unsafe { f.as_ref() });
    Ok(fd)
}

pub fn sys_read(p: &Proc) -> Result<usize, Error> {
    let va = syscall::arg_addr(p, 1);
    let n = syscall::arg_int(p, 2);
    let (_fd, f) = arg_fd(p, 0)?;
    file::read(p, unsafe { f.as_ref() }, va, n).map_err(|()| Error::Unknown)
}

pub fn sys_write(p: &Proc) -> Result<usize, Error> {
    let va = syscall::arg_addr(p, 1);
    let n = syscall::arg_int(p, 2);
    let (_fd, f) = arg_fd(p, 0)?;
    file::write(p, unsafe { f.as_ref() }, va, n).map_err(|()| Error::Unknown)
}

pub fn sys_close(p: &Proc) -> Result<usize, Error> {
    let (fd, f) = arg_fd(p, 0)?;
    p.unset_ofile(fd);
    file::close(unsafe { f.as_ref() });
    Ok(0)
}

pub fn sys_fstat(p: &Proc) -> Result<usize, Error> {
    let va = syscall::arg_addr(p, 1);
    let (_fd, f) = arg_fd(p, 0)?;
    file::stat(p, unsafe { f.as_ref() }, va).map_err(|()| Error::Unknown)?;
    Ok(0)
}

/// Creates the path `new` as a link to the same inode as `old`.
pub fn sys_link(p: &Proc) -> Result<usize, Error> {
    let mut new = [0; MAX_PATH];
    let mut old = [0; MAX_PATH];

    let old = syscall::arg_str(p, 0, &mut old)?;
    let new = syscall::arg_str(p, 1, &mut new)?;

    let tx = fs::begin_tx();
    fs::ops::link(&tx, p, old, new).map_err(|()| Error::Unknown)?;
    Ok(0)
}

pub fn sys_unlink(p: &Proc) -> Result<usize, Error> {
    let mut path = [0; MAX_PATH];
    let path = syscall::arg_str(p, 0, &mut path)?;

    let tx = fs::begin_tx();
    fs::ops::unlink(&tx, p, path).map_err(|()| Error::Unknown)?;
    Ok(0)
}

pub fn sys_open(p: &Proc) -> Result<usize, Error> {
    let mode = OpenFlags::from_bits_retain(syscall::arg_int(p, 1));
    let mut path = [0; MAX_PATH];
    let path = syscall::arg_str(p, 0, &mut path)?;

    let tx = fs::begin_tx();
    let mut ip = if mode.contains(OpenFlags::CREATE) {
        fs::ops::create(&tx, p, path, T_FILE, 0, 0).map_err(|()| Error::Unknown)?
    } else {
        let mut ip = fs::path::resolve(&tx, p, path).map_err(|()| Error::Unknown)?;
        let lip = ip.lock();
        if lip.is_dir() && mode != OpenFlags::READ_ONLY {
            return Err(Error::Unknown);
        }
        lip.unlock();
        ip
    };

    let mut lip = ip.lock();
    if lip.ty() == T_DEVICE && (lip.major() < 0 || lip.major() as usize >= NDEV) {
        return Err(Error::Unknown);
    }

    let Some(f) = file::alloc() else {
        return Err(Error::Unknown);
    };

    let Ok(fd) = fd_alloc(p, f.into()) else {
        file::close(f);
        return Err(Error::Unknown);
    };

    let readable = !mode.contains(OpenFlags::WRITE_ONLY);
    let writable = mode.contains(OpenFlags::WRITE_ONLY) || mode.contains(OpenFlags::READ_WRITE);
    if lip.ty() == T_DEVICE {
        f.init_device(lip.major(), Inode::from_locked(&lip), readable, writable);
    } else {
        f.init_inode(Inode::from_locked(&lip), readable, writable);
    }

    if mode.contains(OpenFlags::TRUNC) && lip.ty() == T_FILE {
        lip.truncate();
    }

    Ok(fd)
}

pub fn sys_mkdir(p: &Proc) -> Result<usize, Error> {
    let mut path = [0; MAX_PATH];
    let path = syscall::arg_str(p, 0, &mut path)?;

    let tx = fs::begin_tx();
    let _ip = fs::ops::create(&tx, p, path, T_DIR, 0, 0).map_err(|()| Error::Unknown)?;

    Ok(0)
}

pub fn sys_mknod(p: &Proc) -> Result<usize, Error> {
    let mut path = [0; MAX_PATH];
    let path = syscall::arg_str(p, 0, &mut path)?;
    let major = syscall::arg_int(p, 1) as i16;
    let minor = syscall::arg_int(p, 2) as i16;

    let tx = fs::begin_tx();
    let _ip = fs::ops::create(&tx, p, path, T_DEVICE, major, minor).map_err(|()| Error::Unknown)?;

    Ok(0)
}

pub fn sys_chdir(p: &Proc) -> Result<usize, Error> {
    let mut path = [0; MAX_PATH];
    let path = syscall::arg_str(p, 0, &mut path)?;

    let tx = fs::begin_tx();
    let mut ip = fs::path::resolve(&tx, p, path).map_err(|()| Error::Unknown)?;
    if !ip.lock().is_dir() {
        return Err(Error::Unknown);
    }
    let _old = p.update_cwd(Inode::from_tx(&ip));

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

    let ret = exec::exec(path, argv.as_ptr().cast()).map_err(|()| Error::Unknown);

    for arg in argv.iter().filter_map(|&a| a) {
        unsafe {
            page::free_page(arg);
        }
    }

    ret
}

pub fn sys_pipe(p: &Proc) -> Result<usize, Error> {
    let fd_array = syscall::arg_addr(p, 0);

    let (rf, wf) = pipe::alloc().map_err(|()| Error::Unknown)?;

    let Ok(rfd) = fd_alloc(p, rf.into()) else {
        file::close(rf);
        file::close(wf);
        return Err(Error::Unknown);
    };
    let Ok(wfd) = fd_alloc(p, wf.into()) else {
        p.unset_ofile(rfd);
        file::close(rf);
        file::close(wf);
        return Err(Error::Unknown);
    };

    let fds = [rfd as i32, wfd as i32];
    if vm::copy_out(p.pagetable().unwrap(), fd_array, &fds).is_err() {
        p.unset_ofile(rfd);
        p.unset_ofile(wfd);
        file::close(rf);
        file::close(wf);
        return Err(Error::Unknown);
    }

    Ok(0)
}
