#![no_std]

use core::ptr;

use ov6_user_lib::{
    fs::{self, File},
    process,
};
use user::{message, try_or_panic};

const CONSOLE: i16 = 1;

fn main() {
    let console = match File::options().read(true).write(true).open(c"console") {
        Ok(console) => console,
        Err(_) => {
            // stdout/stderr are not created here, so we don't output error message here.
            fs::mknod(c"console", CONSOLE, 0).unwrap();
            File::options()
                .read(true)
                .write(true)
                .open(c"console")
                .unwrap()
        }
    };
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
