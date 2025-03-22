#![no_std]

extern crate alloc;

use alloc::string::String;
use core::mem;

use once_init::OnceInit;
use ov6_user_lib::{
    eprint,
    error::Ov6Error,
    fs::{File, StatType},
    io::{self, STDIN_FD},
    os::{fd::AsRawFd as _, ov6::syscall},
    process::{self},
};
use ov6_utilities::{OrExit as _, exit_err, message_err};

use self::parser::Parser;

mod builtin;
mod command;
mod parser;
mod run;
mod tokenizer;

static SHOW_PROMPT: OnceInit<bool> = OnceInit::new();

fn get_cmd(buf: &mut String) -> Result<Option<&str>, Ov6Error> {
    if *SHOW_PROMPT.get() {
        eprint!("$ ");
    }
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

    let isatty = syscall::fstat(STDIN_FD)
        .ok()
        .is_some_and(|meta| meta.ty == StatType::Dev as u16);
    SHOW_PROMPT.init(isatty);

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
