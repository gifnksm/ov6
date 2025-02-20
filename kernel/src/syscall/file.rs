use core::ptr::NonNull;

use crate::{
    file::{self, File, pipe},
    fs::{
        self, DIR_SIZE, inode_unlock_put,
        stat::{T_DEVICE, T_DIR, T_FILE},
    },
    memory::{
        page,
        vm::{self, PAGE_SIZE},
    },
    param::{MAX_ARG, MAX_PATH, NDEV},
    proc::{Proc, exec},
    syscall,
    syscall::fcntl::{O_CREATE, O_RDONLY, O_RDWR, O_TRUNC, O_WRONLY},
};

/// Fetches the nth word-sized system call argument as a file descriptor
/// and returns the descriptor and the corresponding `File`.
fn arg_fd(p: &Proc, n: usize) -> Result<(usize, NonNull<File>), ()> {
    let fd = syscall::arg_int(p, n);
    let file = p.ofile(fd).ok_or(())?;
    Ok((fd, file))
}

/// Allocates a file descriptor for the given `File`.
///
/// Takes over file reference from caller on success.
fn fd_alloc(p: &Proc, file: NonNull<File>) -> Result<usize, ()> {
    p.add_ofile(file).ok_or(())
}

pub fn sys_dup(p: &Proc) -> Result<usize, ()> {
    let (_fd, f) = arg_fd(p, 0)?;
    let fd = fd_alloc(p, f)?;
    file::dup(unsafe { f.as_ref() });
    Ok(fd)
}

pub fn sys_read(p: &Proc) -> Result<usize, ()> {
    let va = syscall::arg_addr(p, 1);
    let n = syscall::arg_int(p, 2);
    let (_fd, f) = arg_fd(p, 0)?;
    file::read(p, unsafe { f.as_ref() }, va, n)
}

pub fn sys_write(p: &Proc) -> Result<usize, ()> {
    let va = syscall::arg_addr(p, 1);
    let n = syscall::arg_int(p, 2);
    let (_fd, f) = arg_fd(p, 0)?;
    file::write(p, unsafe { f.as_ref() }, va, n)
}

pub fn sys_close(p: &Proc) -> Result<usize, ()> {
    let (fd, f) = arg_fd(p, 0)?;
    p.unset_ofile(fd);
    file::close(unsafe { f.as_ref() });
    Ok(0)
}

pub fn sys_fstat(p: &Proc) -> Result<usize, ()> {
    let va = syscall::arg_addr(p, 1);
    let (_fd, f) = arg_fd(p, 0)?;
    file::stat(p, unsafe { f.as_ref() }, va)?;
    Ok(0)
}

/// Creates the path `new` as a link to the same inode as `old`.
pub fn sys_link(p: &Proc) -> Result<usize, ()> {
    let mut new = [0; MAX_PATH];
    let mut old = [0; MAX_PATH];

    let old = syscall::arg_str(p, 0, &mut old)?;
    let new = syscall::arg_str(p, 1, &mut new)?;

    let tx = fs::begin_tx();
    let mut ip = fs::resolve_path(&tx, p, old)?;

    fs::inode_lock(&tx, ip);
    unsafe {
        if ip.as_ref().ty == T_DIR {
            fs::inode_unlock_put(&tx, ip);
            return Err(());
        }

        ip.as_mut().nlink += 1;
        fs::inode_update(&tx, ip);
        fs::inode_unlock(ip);

        let res = (|| {
            let mut name = [0; DIR_SIZE];
            let (dp, name) = fs::resolve_path_parent(&tx, p, new, &mut name)?;

            fs::inode_lock(&tx, dp);
            if dp.as_ref().dev != ip.as_ref().dev
                || fs::dir_link(&tx, p, dp, name, ip.as_ref().inum.unwrap()).is_err()
            {
                fs::inode_unlock_put(&tx, dp);
                return Err(());
            }
            fs::inode_unlock_put(&tx, dp);
            fs::inode_put(&tx, ip);

            Ok(())
        })();

        if res.is_err() {
            fs::inode_lock(&tx, ip);
            ip.as_mut().nlink -= 1;
            fs::inode_update(&tx, ip);
            fs::inode_unlock_put(&tx, ip);
            return Err(());
        }
    }

    Ok(0)
}

pub fn sys_unlink(p: &Proc) -> Result<usize, ()> {
    let mut path = [0; MAX_PATH];
    let path = syscall::arg_str(p, 0, &mut path)?;

    let tx = fs::begin_tx();
    fs::unlink(&tx, p, path)?;
    Ok(0)
}

pub fn sys_open(p: &Proc) -> Result<usize, ()> {
    let o_mode = syscall::arg_int(p, 1);
    let mut path = [0; MAX_PATH];
    let path = syscall::arg_str(p, 0, &mut path)?;

    let tx = fs::begin_tx();
    unsafe {
        let ip = if (o_mode & O_CREATE) != 0 {
            fs::create(&tx, p, path, T_FILE, 0, 0)?
        } else {
            let ip = fs::resolve_path(&tx, p, path)?;
            fs::inode_lock(&tx, ip);
            if ip.as_ref().ty == T_DIR && o_mode != O_RDONLY {
                fs::inode_unlock_put(&tx, ip);
                return Err(());
            }
            ip
        };

        if ip.as_ref().ty == T_DEVICE
            && (ip.as_ref().major < 0 || ip.as_ref().major as usize >= NDEV)
        {
            fs::inode_unlock_put(&tx, ip);
            return Err(());
        }

        let Some(f) = file::alloc() else {
            fs::inode_unlock_put(&tx, ip);
            return Err(());
        };
        let Ok(fd) = fd_alloc(p, f.into()) else {
            file::close(f);
            fs::inode_unlock_put(&tx, ip);
            return Err(());
        };

        let readable = (o_mode & O_WRONLY) == 0;
        let writable = (o_mode & O_WRONLY) != 0 || (o_mode & O_RDWR) != 0;
        if ip.as_ref().ty == T_DEVICE {
            f.init_device(ip.as_ref().major, ip, readable, writable);
        } else {
            f.init_inode(ip, readable, writable);
        }

        if (o_mode & O_TRUNC) != 0 && ip.as_ref().ty == T_FILE {
            fs::inode_trunc(&tx, ip);
        }

        fs::inode_unlock(ip);

        Ok(fd)
    }
}

pub fn sys_mkdir(p: &Proc) -> Result<usize, ()> {
    let mut path = [0; MAX_PATH];
    let path = syscall::arg_str(p, 0, &mut path)?;

    let tx = fs::begin_tx();
    let ip = fs::create(&tx, p, path, T_DIR, 0, 0)?;
    fs::inode_unlock_put(&tx, ip);

    Ok(0)
}

pub fn sys_mknod(p: &Proc) -> Result<usize, ()> {
    let mut path = [0; MAX_PATH];
    let path = syscall::arg_str(p, 0, &mut path)?;
    let major = syscall::arg_int(p, 1) as i16;
    let minor = syscall::arg_int(p, 2) as i16;

    let tx = fs::begin_tx();
    let ip = fs::create(&tx, p, path, T_DEVICE, major, minor)?;
    inode_unlock_put(&tx, ip);

    Ok(0)
}

pub fn sys_chdir(p: &Proc) -> Result<usize, ()> {
    let mut path = [0; MAX_PATH];
    let path = syscall::arg_str(p, 0, &mut path)?;

    unsafe {
        let tx = fs::begin_tx();
        let ip = fs::resolve_path(&tx, p, path)?;
        fs::inode_lock(&tx, ip);
        if ip.as_ref().ty != T_DIR {
            fs::inode_unlock_put(&tx, ip);
            return Err(());
        }
        fs::inode_unlock(ip);
        let old = p.update_cwd(ip);
        fs::inode_put(&tx, old);
    }
    Ok(0)
}

pub fn sys_exec(p: &Proc) -> Result<usize, ()> {
    let mut path = [0; MAX_PATH];
    let path = syscall::arg_str(p, 0, &mut path)?;
    let uargv = syscall::arg_addr(p, 1);

    let mut argv = [None; MAX_ARG];
    let res = (|| {
        for i in 0.. {
            if i > argv.len() {
                return Err(());
            }

            let uarg = syscall::fetch_addr(p, uargv.byte_add(i * size_of::<usize>()))?;
            if uarg.addr() == 0 {
                argv[i] = None;
                break;
            }
            argv[i] = Some(page::alloc_page().ok_or(())?);
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
        return Err(());
    }

    let ret = exec::exec(path, argv.as_ptr().cast());

    for arg in argv.iter().filter_map(|&a| a) {
        unsafe {
            page::free_page(arg);
        }
    }

    ret
}

pub fn sys_pipe(p: &Proc) -> Result<usize, ()> {
    let fd_array = syscall::arg_addr(p, 0);

    let (rf, wf) = pipe::alloc()?;

    let Ok(rfd) = fd_alloc(p, rf.into()) else {
        file::close(rf);
        file::close(wf);
        return Err(());
    };
    let Ok(wfd) = fd_alloc(p, wf.into()) else {
        p.unset_ofile(rfd);
        file::close(rf);
        file::close(wf);
        return Err(());
    };

    let fds = [rfd as i32, wfd as i32];
    if vm::copy_out(p.pagetable().unwrap(), fd_array, &fds).is_err() {
        p.unset_ofile(rfd);
        p.unset_ofile(wfd);
        file::close(rf);
        file::close(wf);
        return Err(());
    }

    Ok(0)
}
