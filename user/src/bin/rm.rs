#![no_std]

use ov6_user_lib::{env, fs, process};
use user::{try_or, usage_and_exit};

fn main() {
    let args = env::args_os();

    if args.len() < 1 {
        usage_and_exit!("files...");
    }

    for arg in args {
        try_or!(
            fs::remove_file(arg),
            break,
            e => "{} failed to delete: {e}", arg.display(),
        );
    }

    process::exit(0);
}
