mod ffi {
    unsafe extern "C" {
        pub fn begin_op();
        pub fn end_op();
    }
}

pub fn begin_op() {
    unsafe { ffi::begin_op() }
}

pub fn end_op() {
    unsafe { ffi::end_op() }
}
