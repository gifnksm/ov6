#![no_std]

use user::{try_or, usage_and_exit};
use xv6_user_lib::{env, fs, process};

fn main() {
    let args = env::args_cstr();

    if args.len() < 1 {
        usage_and_exit!("files...");
    }

    for arg in args {
        try_or!(
            fs::create_dir(arg),
            break,
            e => "{} failed to create: {e}", arg.to_str().unwrap(),
        );
    }

    process::exit(0);
}
