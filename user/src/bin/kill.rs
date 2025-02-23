#![no_std]

use xv6_user_lib::{env, eprintln, process};

fn main() {
    let prog = env::arg0();
    let args = env::args();

    if args.len() == 0 {
        eprintln!("Usage: {prog} pid...");
        process::exit(1);
    }

    for arg in args {
        let pid = match arg.parse() {
            Ok(pid) => pid,
            Err(e) => {
                eprintln!("{prog}: invalid pid: {e}");
                continue;
            }
        };
        if let Err(e) = process::kill(pid) {
            eprintln!("{prog}: kill process {pid} failed: {e}");
            continue;
        }
    }

    process::exit(0);
}
