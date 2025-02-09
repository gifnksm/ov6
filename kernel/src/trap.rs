mod ffi {
    unsafe extern "C" {
        pub fn usertrapret();
    }
}
pub fn usertrapret() {
    unsafe { ffi::usertrapret() }
}
