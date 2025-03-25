#![cfg_attr(not(test), no_std)]

use ov6_user_lib::{env, os::ov6::syscall, process};
use ov6_user_tests::{OrExit as _, exit_err, usage_and_exit};

fn main() {
    let mut args = env::args();

    if args.len() > 2 {
        usage_and_exit!("[sbrk_size]");
    }

    let sbrk_size = args.next().map_or(0, |s| {
        s.parse().or_exit(|e| exit_err!(e, "invalid code '{s}'"))
    });

    process::grow_break(sbrk_size).or_exit(|e| exit_err!(e, "grow break failed"));

    syscall::dump_user_page_table();
}
