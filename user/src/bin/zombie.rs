#![no_std]

use ov6_user_lib::{process, thread};
use user::try_or_exit;

fn main() {
    try_or_exit!(
        process::fork_fn(|| process::exit(0)),
        e => "fork child process failed: {e}"
    );

    // let child exit before parent.
    thread::sleep(5);
}
