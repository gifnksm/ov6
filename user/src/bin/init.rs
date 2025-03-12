#![no_std]

use core::ptr;

use ov6_user_lib::{
    error::Ov6Error,
    fs::{self, File},
    process,
};
use user::{message, try_or_panic};

const CONSOLE: u32 = 1;

fn open_console() -> Result<File, Ov6Error> {
    File::options().read(true).write(true).open("console")
}

fn create_console() -> Result<(), Ov6Error> {
    fs::mknod("console", CONSOLE, 0)
}

fn main() {
    let console = open_console()
        .or_else(|_| {
            // stdout/stderr are not created here, so we don't output error message here.
            create_console().unwrap();
            open_console()
        })
        .unwrap();
    let _stdout = console.try_clone().unwrap();
    let _stderr = console.try_clone().unwrap();

    loop {
        message!("starting sh");

        let sh = try_or_panic!(
            process::fork_fn(|| {
                let argv = [c"sh".as_ptr(), ptr::null()];
                try_or_panic!(
                    process::exec(c"sh", &argv),
                    e => "exec sh failed: {e}",
                );
                unreachable!()
            }),
            e => "fork failed: {e}",
        );

        loop {
            // this call to wait() returns if the shell exits,
            // or if a parentless process exits.
            let (wpid, _status) = try_or_panic!(
                process::wait(),
                e => "wait returned an error: {e}",
            );
            if wpid != sh.pid() {
                // it was a parentless process; do nothing
                continue;
            }
            // the shell exited; restart it.
            break;
        }
    }
}
