#![no_std]

use ov6_user_lib::{
    env,
    fs::File,
    io::{self, Read},
    path::Path,
    println, process,
};
use user::try_or_exit;

fn wc<R, P>(mut input: R, name: P)
where
    R: Read,
    P: AsRef<Path>,
{
    let name = name.as_ref();

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

    println!("{l} {w} {c} {name}", name = name.display());
}

fn main() {
    let args = env::args_os();

    if args.len() == 0 {
        wc(io::stdin(), "");
        process::exit(0);
    }

    for path in args {
        let file = try_or_exit!(
            File::open(path),
            e => "cannot open {}: {e}", path.display(),
        );
        wc(&file, path);
    }
    process::exit(0);
}
