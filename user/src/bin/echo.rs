#![no_std]
#![no_main]

use core::{ffi::CStr, slice};

use xv6_user_lib::{eprintln, error::Error, print, println, process};

#[unsafe(no_mangle)]
fn main(argc: i32, argv: *const *const u8) {
    let args = unsafe { slice::from_raw_parts(argv, argc as usize) };
    match run(args) {
        Ok(()) => process::exit(0),
        Err(err) => {
            let prog = unsafe { CStr::from_ptr(args[0]).to_str().unwrap() };
            eprintln!("{prog}: {err}",);
            process::exit(1);
        }
    }
}

fn run(args: &[*const u8]) -> Result<(), Error> {
    for (i, arg) in args[1..].iter().enumerate() {
        print!("{}", unsafe { CStr::from_ptr(*arg).to_str().unwrap() });
        if i + 2 < args.len() {
            print!(" ");
        } else {
            println!("");
        }
    }

    Ok(())
}
