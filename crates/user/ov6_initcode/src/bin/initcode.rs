#![no_std]
#![no_main]

use core::hint;

use ov6_syscall::{UserSlice, syscall};
use ov6_user_lib::os::ov6::syscall::ffi::SyscallExt as _;

#[unsafe(link_section = ".text.init")]
#[unsafe(no_mangle)]
extern "C" fn main() {
    let argv = [UserSlice::new(&INIT)];
    let _ = syscall::Exec::call_raw((UserSlice::new(&INIT), UserSlice::new(&argv)));
    let _ = syscall::Exit::call_raw((-1,));
    loop {
        hint::spin_loop();
    }
}

#[unsafe(link_section = ".text.init")]
static INIT: [u8; 5] = *b"/init";
