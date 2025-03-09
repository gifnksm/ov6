#![no_std]
#![no_main]

use core::{ffi::CStr, ptr};

use ov6_syscall::{RegisterValue as _, Syscall, UserRef, UserSlice, syscall};
use ov6_user_lib::os::ov6::syscall::ffi;

#[unsafe(link_section = ".text.init")]
#[unsafe(no_mangle)]
extern "C" fn main() {
    let init = *b"/init\0";
    let init = unsafe { CStr::from_ptr((&raw const init[0]).cast()) };
    let argv = [init.as_ptr().cast(), ptr::null()];
    let [a0, a1, a2] =
        <syscall::Exec as Syscall>::Arg::encode((UserRef::new(init), UserSlice::new(&argv))).a;
    let _ = ffi::exec(a0, a1, a2);
    let [a0] = <syscall::Exit as Syscall>::Arg::encode((-1,)).a;
    let _ = ffi::exit(a0);
}
