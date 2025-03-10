#![no_std]
#![no_main]

use core::{ffi::CStr, ptr};

use ov6_syscall::{UserRef, UserSlice, syscall};
use ov6_user_lib::os::ov6::syscall::ffi::SyscallExt as _;

#[unsafe(link_section = ".text.init")]
#[unsafe(no_mangle)]
extern "C" fn main() {
    let init = *b"/init\0";
    let init = unsafe { CStr::from_ptr((&raw const init[0]).cast()) };
    let argv = [init.as_ptr().cast(), ptr::null()];
    let _ = syscall::Exec::call_raw((UserRef::new(init), UserSlice::new(&argv)));
    let _ = syscall::Exit::call_raw((-1,));
}
