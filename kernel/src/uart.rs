use core::ffi::c_int;

mod ffi {
    use super::*;

    unsafe extern "C" {
        pub fn uartinit();
        pub fn uartputc(c: c_int);
        pub fn uartputc_sync(c: c_int);
    }
}

pub fn init() {
    unsafe { ffi::uartinit() }
}

pub fn putc(c: char) {
    unsafe { ffi::uartputc(c as u8 as i32) }
}

pub fn putc_sync(c: char) {
    unsafe { ffi::uartputc_sync(c as u8 as i32) }
}
