#![no_std]

use ov6_user_lib::{env, os::ov6::syscall};
use ov6_utilities::{OrExit as _, exit_err, usage_and_exit};

fn main() {
    let mut args = env::args();
    let _ = args.next(); // skip the program name

    if args.len() > 2 {
        usage_and_exit!("[code]");
    }

    let code = args.next().map_or(0, |s| {
        s.parse().or_exit(|e| exit_err!(e, "invalid code '{s}'"))
    });

    syscall::halt(code).or_exit(|e| exit_err!(e, "halt failed"));
}
