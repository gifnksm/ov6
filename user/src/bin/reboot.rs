#![no_std]

use ov6_user_lib::os::ov6::syscall;
use user::try_or_exit;

fn main() {
    try_or_exit!(
        syscall::reboot(),
        e => "reboot failed: {e}"
    );
}
