#![no_std]

use ov6_user_lib::os::ov6::syscall;
use ov6_utilities::{OrExit as _, exit_err};

fn main() {
    syscall::reboot().or_exit(|e| exit_err!(e, "reboot failed"));
}
