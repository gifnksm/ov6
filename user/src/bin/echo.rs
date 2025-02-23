#![no_std]
#![no_main]

use xv6_user_lib::{env, eprintln, error::Error, print, println, process};

#[unsafe(no_mangle)]
fn main() {
    match run() {
        Ok(()) => process::exit(0),
        Err(err) => {
            let prog = env::arg0();
            eprintln!("{prog}: {err}");
            process::exit(1);
        }
    }
}

fn run() -> Result<(), Error> {
    for (i, arg) in env::args().enumerate() {
        if i > 0 {
            print!(" {arg}");
        } else {
            print!("{arg}");
        }
    }
    println!();

    Ok(())
}
