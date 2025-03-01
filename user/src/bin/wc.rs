#![no_std]

use core::ffi::CStr;

use user::try_or_exit;
use xv6_user_lib::{
    env,
    fs::File,
    io::{self, Read},
    println, process,
};

fn wc<R>(mut input: R, name: &CStr)
where
    R: Read,
{
    let mut l = 0;
    let mut w = 0;
    let mut c = 0;
    let mut in_word = false;

    let mut buf = [0; 512];

    loop {
        let nread = try_or_exit!(
            input.read(&mut buf),
            e => "wc: read error: {e}",
        );
        if nread == 0 {
            break;
        }

        c += nread;
        for &b in buf[..nread].iter() {
            if b == b'\n' {
                l += 1;
            }
            if b.is_ascii_whitespace() {
                in_word = false
            } else if !in_word {
                w += 1;
                in_word = true;
            }
        }
    }

    println!("{l} {w} {c} {name}", name = name.to_str().unwrap());
}

fn main() {
    let args = env::args_cstr();

    if args.len() == 0 {
        wc(io::stdin(), c"");
        process::exit(0);
    }

    for path in args {
        let file = try_or_exit!(
            File::open(path),
            e => "cannot open {}: {e}", path.to_str().unwrap(),
        );
        wc(&file, path);
    }
    process::exit(0);
}
