#![no_std]

use core::ptr;

use xv6_user_lib::{
    env, eprintln,
    fs::{self, File, OpenFlags},
    process,
};

const CONSOLE: i16 = 1;

fn main() {
    let prog = env::arg0();
    let console = match File::open(c"console", OpenFlags::READ_WRITE) {
        Ok(console) => console,
        Err(_) => {
            if fs::mknod(c"console", CONSOLE, 0).is_err() {
                panic!("{prog}: create console failed");
            }
            let Ok(console) = File::open(c"console", OpenFlags::READ_WRITE) else {
                panic!("{prog}: open console failed")
            };
            console
        }
    };
    let Ok(_stdout) = console.try_clone() else {
        panic!("{prog}: create stdout failed");
    };
    let Ok(_stderr) = console.try_clone() else {
        panic!("{prog}: create stderr failed");
    };

    loop {
        eprintln!("{prog}: starting sh");

        let Ok(pid) = process::fork() else {
            panic!("{prog}: fork failed");
        };

        if pid == 0 {
            let argv = [c"sh".as_ptr(), ptr::null()];
            let Err(_e) = process::exec(c"sh", &argv);
            panic!("{prog}: exec sh failed");
        }

        loop {
            // this call to wait() returns if the shell exits,
            // or if a parentless process exits.
            let Ok((wpid, _status)) = process::wait() else {
                panic!("{prog}: wait returned an error");
            };
            if wpid != pid {
                // it was a parentless process; do nothing
                continue;
            }
            // the shell exited; restart it.
            break;
        }
    }
}
