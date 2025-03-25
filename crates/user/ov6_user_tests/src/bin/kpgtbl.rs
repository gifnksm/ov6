#![cfg_attr(not(test), no_std)]

use ov6_user_lib::os::ov6::syscall;

fn main() {
    syscall::dump_kernel_page_table();
}
