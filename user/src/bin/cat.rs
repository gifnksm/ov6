#![no_std]

use xv6_user_lib::{
    env, eprintln,
    fs::File,
    io::{self, Read, Write},
    process,
    syscall::OpenFlags,
};

fn cat<T>(mut input: T)
where
    T: Read,
{
    let mut stdout = io::stdout();
    let mut buf = [0; 512];
    loop {
        let Ok(nread) = input.read(&mut buf) else {
            eprintln!("cat: read error");
            process::exit(1);
        };
        if nread == 0 {
            break;
        }
        let Ok(nwrite) = stdout.write(&buf[..nread]) else {
            eprintln!("cat: write error");
            process::exit(1);
        };
        if nwrite != nread {
            eprintln!("cat: write error {nwrite} vs {nread}");
            process::exit(1);
        }
    }
}

fn main() {
    let args = env::args_cstr();
    if args.len() == 0 {
        cat(io::stdin());
        process::exit(0);
    }

    for arg in args {
        let Ok(file) = File::open(arg, OpenFlags::READ_ONLY) else {
            eprintln!("cat: cannot open file {}\n", arg.to_str().unwrap());
            process::exit(1);
        };

        cat(&file);
    }
}
