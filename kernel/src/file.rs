use core::{ffi::c_int, ptr::NonNull};

pub const CONSOLE: usize = 1;
const NDEV: usize = 10;

mod ffi {
    unsafe extern "C" {
        pub type File;
        pub type Inode;
        pub fn filedup(f: *mut File) -> *mut File;
        pub fn fileclose(f: *mut File);
    }
}

pub use ffi::{File, Inode};

/// Maps major device number to device functions.
#[repr(C)]
pub struct DevSw {
    pub read: extern "C" fn(c_int, u64, c_int) -> c_int,
    pub write: extern "C" fn(c_int, u64, c_int) -> c_int,
}

unsafe extern "C" {
    pub static mut devsw: [DevSw; NDEV];
}

pub fn dup(f: NonNull<File>) -> Option<NonNull<File>> {
    unsafe { NonNull::new(ffi::filedup(f.as_ptr())) }
}

pub fn close(of: NonNull<File>) {
    unsafe { ffi::fileclose(of.as_ptr()) }
}
