#![no_std]

use ov6_user_lib::{env, print, println};

fn main() {
    for (i, arg) in env::args().enumerate() {
        if i > 0 {
            print!(" {arg}");
        } else {
            print!("{arg}");
        }
    }
    println!();
}
