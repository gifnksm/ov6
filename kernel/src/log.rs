use crate::{
    bio::Buf,
    fs::{DeviceNo, SuperBlock},
};

mod ffi {
    use core::ffi::c_int;

    use super::*;

    unsafe extern "C" {
        pub fn initlog(dev: c_int, sb: *const SuperBlock);
        pub fn begin_op();
        pub fn end_op();
        pub fn log_write(b: *mut Buf);
    }
}

pub fn begin_op() {
    unsafe { ffi::begin_op() }
}

pub fn end_op() {
    unsafe { ffi::end_op() }
}

pub fn do_op<T, F>(f: F) -> T
where
    F: FnOnce() -> T,
{
    begin_op();
    let res = f();
    end_op();
    res
}

pub fn init(dev: DeviceNo, sb: &SuperBlock) {
    unsafe { ffi::initlog(dev.value() as i32, sb) };
}

pub fn write(b: &mut Buf) {
    unsafe { ffi::log_write(b) }
}
