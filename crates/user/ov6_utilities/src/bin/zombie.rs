#![no_std]

use core::time::Duration;

use ov6_user_lib::{
    process::{self, ProcessBuilder},
    thread,
};
use ov6_utilities::try_or_exit;

fn main() {
    try_or_exit!(
        ProcessBuilder::new().spawn_fn(|| process::exit(0)),
        e => "fork child process failed: {e}"
    );

    // let child exit before parent.
    thread::sleep(Duration::from_millis(500));
}
