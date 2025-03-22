#![no_std]

extern crate alloc;

use alloc::string::String;
use core::mem;

use ov6_user_lib::{
    eprint,
    error::Ov6Error,
    fs::File,
    io::{self},
    os::fd::AsRawFd as _,
    process::{self},
};
use ov6_utilities::{OrExit as _, exit_err, message_err};

use self::parser::Parser;

mod builtin;
mod command;
mod parser;
mod run;
mod tokenizer;

fn get_cmd(buf: &mut String) -> Result<Option<&str>, Ov6Error> {
    eprint!("$ ");
    buf.clear();
    let n = io::stdin().read_line(buf)?;
    if n == 0 {
        return Ok(None);
    }
    Ok(Some(&buf[..n]))
}

fn main() {
    // Ensure that three file descriptors are open.
    while let Ok(file) = File::options().read(true).write(true).open("console") {
        if file.as_raw_fd().get() < 3 {
            mem::forget(file);
            continue;
        }
        break;
    }

    // Read and run input commands.
    let mut buf = String::new();
    loop {
        let Some(cmd) = get_cmd(&mut buf).or_exit(|e| exit_err!(e, "cannot read console")) else {
            process::exit(0);
        };
        let Ok(list) = Parser::new(cmd).parse().inspect_err(|e| {
            message_err!(e, "syntax error");
        }) else {
            continue;
        };
        run::run_list(list);
    }
}
