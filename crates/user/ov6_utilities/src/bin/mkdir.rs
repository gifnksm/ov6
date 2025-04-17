#![no_std]

use ov6_user_lib::{env, fs, process};
use ov6_utilities::{message_err, usage_and_exit};

fn main() {
    let mut args = env::args_os();
    let _ = args.next(); // skip the program name

    if args.len() < 1 {
        usage_and_exit!("files...");
    }

    for arg in args {
        if let Err(e) = fs::create_dir(arg) {
            message_err!(e, "cannot create directory '{}'", arg.display());
        }
    }

    process::exit(0);
}
