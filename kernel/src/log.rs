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

pub fn do_op<T, F>(f: F) -> T
where
    F: FnOnce() -> T,
{
    begin_op();
    let res = f();
    end_op();
    res
}
