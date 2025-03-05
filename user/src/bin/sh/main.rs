#![no_std]

extern crate alloc;

use core::mem;

use alloc::{ffi::CString, string::String};
use ov6_user_lib::{
    env, eprint,
    error::Error,
    fs::File,
    io::{self},
    os::fd::AsRawFd,
    process,
};
use user::{exit, message, try_or, try_or_exit};

mod command;
mod parser;
mod util;

fn get_cmd(buf: &mut String) -> Result<Option<&str>, Error> {
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
    while let Ok(file) = File::options().read(true).write(true).open(c"console") {
        if file.as_raw_fd() < 3 {
            mem::forget(file);
            continue;
        }
        break;
    }

    // Read and run input commands.
    let mut buf = String::new();
    loop {
        let mut cmd = match get_cmd(&mut buf) {
            Ok(Some(cmd)) => cmd,
            Ok(None) => break,
            Err(e) => {
                exit!("failed to read console: {e}");
            }
        };
        let mut parts = cmd.split_whitespace();
        if parts.next() == Some("cd") {
            // chdir must be called by the parent, not the child.
            let (Some(dir), None) = (parts.next(), parts.next()) else {
                message!("Usage: cd <dir>");
                continue;
            };
            try_or!(env::set_current_directory(&CString::new(dir).unwrap()),
                continue,
                e => "cannot cd {dir}: {e}",
            );
            continue;
        }

        let child = util::fork_fn_or_exit(|| {
            let cmd = try_or_exit!(
                parser::parse_cmd(&mut cmd),
                e => "syntax error: {e}",
            );
            let Some(cmd) = cmd else {
                process::exit(0);
            };
            cmd.run();
        });
        util::wait_or_exit(&[child.pid()]);
    }
    process::exit(0);
}
