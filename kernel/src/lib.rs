#![no_std]

use core::ffi::c_int;

#[panic_handler]
fn panic(_info: &core::panic::PanicInfo) -> ! {
    loop {}
}

unsafe extern "C" {
    fn consputc(c: c_int);
}

/// A function that prints "Hello from Rust!" to the console.
#[unsafe(no_mangle)]
pub extern "C" fn rust_hello() {
    unsafe {
        for &b in b"Hello from Rust!\n" {
            consputc(b as c_int);
        }
    }
}
