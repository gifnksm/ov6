#![no_std]
#![no_main]

use core::hint;

use ov6_syscall::{UserSlice, syscall};
use ov6_user_lib::os::ov6::syscall::ffi::SyscallExt as _;

#[unsafe(link_section = ".text.init")]
#[unsafe(no_mangle)]
extern "C" fn main() {
    let init = *b"/init";
    let argv = [UserSlice::new(&init)];
    let _ = syscall::Exec::call_raw((UserSlice::new(&init), UserSlice::new(&argv)));
    let _ = syscall::Exit::call_raw((-1,));
    loop {
        hint::spin_loop();
    }
}
