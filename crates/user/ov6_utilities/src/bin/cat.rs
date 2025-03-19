#![no_std]

use core::fmt;

use ov6_user_lib::{
    env,
    fs::File,
    io::{self, Read, Write as _},
    process,
};
use ov6_utilities::{OrExit as _, exit_err, message_err};

fn cat<T, P>(mut input: T, path: P)
where
    T: Read,
    P: fmt::Display,
{
    let mut stdout = io::stdout();
    let mut buf = [0; 512];
    loop {
        let Ok(nread) = input
            .read(&mut buf)
            .inspect_err(|e| message_err!(e, "cannot read '{path}'"))
        else {
            return;
        };
        if nread == 0 {
            break;
        }
        stdout
            .write_all(&buf[..nread])
            .or_exit(|e| exit_err!(e, "cannot write to standard output"));
    }
}

fn main() {
    let args = env::args_os();
    if args.len() == 0 {
        cat(io::stdin(), "standard input");
        process::exit(0);
    }

    let files = args.flat_map(|path| {
        File::open(path)
            .inspect_err(|e| message_err!(e, "cannot open file '{}'", path.display()))
            .map(|file| (file, path))
    });

    for (file, path) in files {
        cat(&file, path.display());
    }
}
