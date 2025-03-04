#![no_std]
#![no_main]

use core::ptr;

use xv6_user_lib::os::xv6::syscall::ffi;

#[unsafe(link_section = ".text.init")]
#[unsafe(no_mangle)]
fn main() {
    let init = *b"/init\0";
    let argv = [init.as_ptr().cast(), ptr::null()];
    let err = unsafe { ffi::exec(init.as_ptr().cast(), argv.as_ptr()) };
    ffi::exit(err as i32);
}
