mod ffi {
    unsafe extern "C" {
        pub fn syscall();
    }
}

pub fn syscall() {
    unsafe { ffi::syscall() }
}
