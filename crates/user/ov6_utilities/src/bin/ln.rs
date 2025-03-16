#![no_std]

use ov6_user_lib::{env, fs, process};
use ov6_utilities::{try_or_exit, usage_and_exit};

fn main() {
    let mut args = env::args_os();
    if args.len() != 2 {
        usage_and_exit!("old new");
    }

    let old = args.next().unwrap();
    let new = args.next().unwrap();
    try_or_exit!(
        fs::link(old, new),
        e => "link {} {} failed: {e}",
            old.display(),
            new.display(),
    );

    process::exit(0);
}
