#![no_std]

use ov6_user_lib::{env, process};
use ov6_utilities::{message_err, usage_and_exit};

fn main() {
    let mut args = env::args();
    let _ = args.next(); // skip the program name

    if args.len() == 0 {
        usage_and_exit!("pid...");
    }

    let pids = args.flat_map(|arg| {
        arg.parse()
            .inspect_err(|e| message_err!(e, "invalid pid '{arg}'"))
    });

    for pid in pids {
        if let Err(e) = process::kill(pid) {
            message_err!(e, "cannot kill process '{pid}'");
        }
    }

    process::exit(0);
}
