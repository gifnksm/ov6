#![no_std]

use ov6_user_lib::{env, process};
use ov6_utilities::{message, try_or, usage_and_exit};

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

        match process::kill(pid) {
            Ok(()) => {}
            Err(e) => {
                message!("kill process {pid} failed: {e}");
            }
        }
    }

    process::exit(0);
}
