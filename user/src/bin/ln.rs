#![no_std]

use xv6_user_lib::{env, eprintln, fs, process};

fn main() {
    let prog = env::arg0();
    let mut args = env::args_cstr();
    if args.len() != 2 {
        eprintln!("Usage: {prog} old new");
        process::exit(1);
    }

    let old = args.next().unwrap();
    let new = args.next().unwrap();
    if let Err(e) = fs::link(old, new) {
        eprintln!(
            "link {} {} failed: {e}",
            old.to_str().unwrap(),
            new.to_str().unwrap()
        );
        process::exit(1);
    }

    process::exit(0);
}
