#![no_std]

use ov6_user_lib::{env, print, println};

fn main() {
    let mut args = env::args();
    let _ = args.next(); // skip the program name

    if let Some(arg) = args.next() {
        print!("{arg}");
    }

    for arg in args {
        print!(" {arg}");
    }

    println!();
}
