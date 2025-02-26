#![no_std]

use user::try_or_exit;
use xv6_user_lib::{
    env,
    fs::{File, OpenFlags},
    io::{self, Read, Write},
    process,
};

fn cat<T>(mut input: T)
where
    T: Read,
{
    let mut stdout = io::stdout();
    let mut buf = [0; 512];
    loop {
        let nread = try_or_exit!(
            input.read(&mut buf),
            e => "read error: {e}",
        );
        if nread == 0 {
            break;
        }
        try_or_exit!(
            stdout.write_all(&buf[..nread]),
            e => "write error: {e}",
        );
    }
}

fn main() {
    let args = env::args_cstr();
    if args.len() == 0 {
        cat(io::stdin());
        process::exit(0);
    }

    for arg in args {
        let file = try_or_exit!(
            File::open(arg, OpenFlags::READ_ONLY),
            e => "cannot open file {file}: {e}", file = arg.to_str().unwrap(),
        );
        cat(&file);
    }
}
