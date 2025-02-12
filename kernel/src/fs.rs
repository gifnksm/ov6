use core::{ffi::CStr, ptr::NonNull};

use crate::{file::Inode, stat::Stat, vm::VirtAddr};

mod ffi {
    use core::ffi::{c_char, c_int, c_uint};

    use super::*;

    unsafe extern "C" {
        pub fn fsinit(dev: i32);
        pub fn namei(path: *const c_char) -> *mut Inode;
        pub fn idup(ip: *mut Inode) -> *mut Inode;
        pub fn iput(ip: *mut Inode);
        pub fn ilock(ip: *mut Inode);
        pub fn iunlock(ip: *mut Inode);
        pub fn stati(ip: *mut Inode, st: *mut Stat);
        pub fn readi(ip: *mut Inode, user_dst: c_int, addr: u64, off: c_uint, n: c_uint) -> c_int;
        pub fn writei(ip: *mut Inode, user_src: c_int, src: u64, off: c_uint, n: c_uint) -> c_int;
    }
}

pub const NDIRECT: usize = 12;

pub fn namei(path: &CStr) -> Option<NonNull<Inode>> {
    NonNull::new(unsafe { ffi::namei(path.as_ptr()) })
}

pub fn inode_dup(ip: NonNull<Inode>) -> Option<NonNull<Inode>> {
    unsafe { NonNull::new(ffi::idup(ip.as_ptr())) }
}

pub fn inode_put(ip: NonNull<Inode>) {
    unsafe { ffi::iput(ip.as_ptr()) }
}

pub fn init(dev: usize) {
    unsafe { ffi::fsinit(dev as i32) }
}

pub fn inode_lock(ip: NonNull<Inode>) {
    unsafe { ffi::ilock(ip.as_ptr()) }
}

pub fn inode_unlock(ip: NonNull<Inode>) {
    unsafe { ffi::iunlock(ip.as_ptr()) }
}

pub fn stat_inode(ip: NonNull<Inode>, st: &mut Stat) {
    unsafe { ffi::stati(ip.as_ptr(), st) }
}

pub fn read_inode(
    ip: NonNull<Inode>,
    user_dst: bool,
    dst: VirtAddr,
    off: u32,
    n: usize,
) -> Result<usize, ()> {
    let sz = unsafe {
        ffi::readi(
            ip.as_ptr(),
            user_dst.into(),
            dst.addr() as u64,
            off,
            n as u32,
        )
    };
    if sz < 0 {
        return Err(());
    }
    Ok(sz as usize)
}

pub fn write_inode(
    ip: NonNull<Inode>,
    user_src: bool,
    src: VirtAddr,
    off: u32,
    n: usize,
) -> Result<usize, ()> {
    let sz = unsafe {
        ffi::writei(
            ip.as_ptr(),
            user_src.into(),
            src.addr() as u64,
            off,
            n as u32,
        )
    };
    if sz < 0 {
        return Err(());
    }
    Ok(sz as usize)
}
