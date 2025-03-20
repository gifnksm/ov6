#![no_std]

use core::time::Duration;

use ov6_user_lib::{os::ov6::syscall, println};

fn main() {
    let nanos = syscall::uptime();
    let uptime = Duration::from_nanos(nanos);
    println!("{}.{:09}", uptime.as_secs(), uptime.subsec_nanos());
}
