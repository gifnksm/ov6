#![no_std]
#![no_main]

use core::{ffi::CStr, slice};

use xv6_user_lib::{
    eprintln,
    error::Error,
    fs::File,
    io::{self, Read, Write},
    process,
    syscall::OpenFlags,
};

fn cat<T>(mut input: T) -> Result<(), Error>
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

    Ok(())
}

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
    if args.len() <= 1 {
        cat(io::stdin())?;
        process::exit(0);
    }

    for arg in &args[1..] {
        let arg = unsafe { CStr::from_ptr(*arg) };
        let Ok(file) = File::open(arg, OpenFlags::READ_ONLY) else {
            eprintln!("cat: cannot open file {}\n", arg.to_str().unwrap());
            process::exit(1);
        };

        cat(&file)?;
    }

    Ok(())
}
