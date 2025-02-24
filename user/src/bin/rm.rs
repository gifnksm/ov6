#![no_std]

use xv6_user_lib::{env, eprintln, fs, process};

fn main() {
    let prog = env::arg0();
    let args = env::args_cstr();

    if args.len() < 1 {
        eprintln!("Usage: {prog} files...\n");
        process::exit(0);
    }

    for arg in args {
        if let Err(e) = fs::remove_file(arg) {
            eprintln!("{prog}: {} failed to delete: {e}", arg.to_str().unwrap());
            break;
        }
    }

    process::exit(0);
}
