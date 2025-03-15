#![no_std]

use ov6_user_lib::{env, os::ov6::syscall};
use user::{try_or_exit, usage_and_exit};

fn main() {
    let mut args = env::args();

    if args.len() > 2 {
        usage_and_exit!("[code]");
    }

    let code = try_or_exit!(
        args.next().map(str::parse).transpose(),
        e => "invalid code: {e}"
    )
    .unwrap_or(255);

    try_or_exit!(
        syscall::abort(code),
        e => "abort failed: {e}"
    );
}
