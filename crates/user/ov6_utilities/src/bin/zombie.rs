#![no_std]

use core::time::Duration;

use ov6_user_lib::{
    process::{self, ProcessBuilder},
    thread,
};
use ov6_utilities::{OrExit as _, exit_err};

fn main() {
    ProcessBuilder::new()
        .spawn_fn(|| process::exit(0))
        .or_exit(|e| exit_err!(e, "fork child process failed"));

    // let child exit before parent.
    thread::sleep(Duration::from_millis(500));
}
