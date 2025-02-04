use core::ffi::c_int;

pub const CONSOLE: usize = 1;
const NDEV: usize = 10;

/// Maps major device number to device functions.
#[repr(C)]
pub struct DevSw {
    pub read: extern "C" fn(c_int, u64, c_int) -> c_int,
    pub write: extern "C" fn(c_int, u64, c_int) -> c_int,
}

unsafe extern "C" {
    pub static mut devsw: [DevSw; NDEV];
}
