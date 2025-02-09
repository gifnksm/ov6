use core::{ffi::CStr, ptr::NonNull};

use crate::file::Inode;

mod ffi {
    use core::ffi::c_char;

    use super::*;

    unsafe extern "C" {
        pub fn fsinit(dev: i32);
        pub fn namei(path: *const c_char) -> *mut Inode;
        pub fn idup(ip: *mut Inode) -> *mut Inode;
        pub fn iput(ip: *mut Inode);
    }
}

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
