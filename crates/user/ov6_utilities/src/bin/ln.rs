#![no_std]

use ov6_user_lib::{env, fs, process};
use ov6_utilities::{OrExit as _, exit_err, usage_and_exit};

fn main() {
    let mut args = env::args_os();
    if args.len() != 2 {
        usage_and_exit!("old new");
    }

    let old = args.next().unwrap();
    let new = args.next().unwrap();
    fs::link(old, new)
        .or_exit(|e| exit_err!(e, "link '{}' '{}' failed", old.display(), new.display(),));
    process::exit(0);
}
