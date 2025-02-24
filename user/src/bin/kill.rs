#![no_std]

use user::{try_or, usage_and_exit};
use xv6_user_lib::{env, process};

fn main() {
    let args = env::args();

    if args.len() == 0 {
        usage_and_exit!("pid...");
    }

    for arg in args {
        let pid = try_or!(
            arg.parse(),
            continue,
            e => "invalid pid: {e}",
        );
        try_or!(
            process::kill(pid),
            continue,
            e => "kill process {pid} failed: {e}"
        );
    }

    process::exit(0);
}
