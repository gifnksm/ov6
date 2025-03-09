#![no_std]
#![no_main]

use core::ptr;

use ov6_user_lib::os::ov6::syscall::ffi;

#[unsafe(link_section = ".text.init")]
#[unsafe(no_mangle)]
extern "C" fn main() {
    let init = *b"/init\0";
    let argv = [init.as_ptr().cast(), ptr::null()];
    let _ = unsafe { ffi::exec(init.as_ptr().cast(), argv.as_ptr()) };
    let _ = ffi::exit(-1);
}
