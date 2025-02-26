#![no_std]

use user::try_or_exit;
use xv6_user_lib::{process, thread};

fn main() {
    let res = try_or_exit!(
        process::fork(),
        e => "fork child process failed: {e}"
    );

    if res.is_parent() {
        // let child exit before parent.
        try_or_exit!(
            thread::sleep(5),
            e => "sleep failed: {e}",
        );
    }

    process::exit(0);
}
