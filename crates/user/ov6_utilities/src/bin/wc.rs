#![no_std]

use core::fmt;

use ov6_user_lib::{
    env,
    fs::File,
    io::{self, Read},
    println, process,
};
use ov6_utilities::{OrExit as _, exit_err, message_err};

fn wc<R, P, Q>(mut input: R, out_name: P, err_name: Q)
where
    R: Read,
    P: fmt::Display,
    Q: fmt::Display,
{
    let mut l = 0;
    let mut w = 0;
    let mut c = 0;
    let mut in_word = false;

    let mut buf = [0; 512];

    loop {
        let nread = input
            .read(&mut buf)
            .or_exit(|e| exit_err!(e, "read '{err_name}' error",));
        if nread == 0 {
            break;
        }

        c += nread;
        for &b in &buf[..nread] {
            if b == b'\n' {
                l += 1;
            }
            if b.is_ascii_whitespace() {
                in_word = false;
            } else if !in_word {
                w += 1;
                in_word = true;
            }
        }
    }

    println!("{l} {w} {c} {out_name}");
}

fn main() {
    let mut args = env::args_os();
    let _ = args.next(); // skip the program name

    if args.len() == 0 {
        wc(io::stdin(), "", "standard input");
        process::exit(0);
    }

    let files = args.flat_map(|path| {
        File::open(path)
            .inspect_err(|e| message_err!(e, "cannot open '{}'", path.display()))
            .map(|file| (file, path))
    });

    for (file, path) in files {
        wc(&file, path.display(), path.display());
    }
    process::exit(0);
}
