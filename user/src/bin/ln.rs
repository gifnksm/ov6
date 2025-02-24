#![no_std]

use user::{try_or_exit, usage_and_exit};
use xv6_user_lib::{env, fs, process};

fn main() {
    let mut args = env::args_cstr();
    if args.len() != 2 {
        usage_and_exit!("old new");
    }

    let old = args.next().unwrap();
    let new = args.next().unwrap();
    try_or_exit!(
        fs::link(old, new),
        e => "link {} {} failed: {e}",
            old.to_str().unwrap(),
            new.to_str().unwrap()
    );

    process::exit(0);
}
