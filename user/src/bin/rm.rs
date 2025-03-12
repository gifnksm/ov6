#![no_std]

use ov6_user_lib::{env, fs, os_str::OsStr, process};
use user::{try_or, usage_and_exit};

fn main() {
    let args = env::args_cstr();

    if args.len() < 1 {
        usage_and_exit!("files...");
    }

    for arg in args {
        try_or!(
            fs::remove_file(OsStr::from_bytes(arg.to_bytes())),
            break,
            e => "{} failed to delete: {e}", arg.to_str().unwrap(),
        );
    }

    process::exit(0);
}
