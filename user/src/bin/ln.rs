#![no_std]

use ov6_user_lib::{env, fs, os_str::OsStr, process};
use user::{try_or_exit, usage_and_exit};

fn main() {
    let mut args = env::args_cstr();
    if args.len() != 2 {
        usage_and_exit!("old new");
    }

    let old = OsStr::from_bytes(args.next().unwrap().to_bytes());
    let new = OsStr::from_bytes(args.next().unwrap().to_bytes());
    try_or_exit!(
        fs::link(old, new),
        e => "link {} {} failed: {e}",
            old.to_str().unwrap(),
            new.to_str().unwrap()
    );

    process::exit(0);
}
