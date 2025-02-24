#![no_std]

use core::ptr;

use user::{message, try_or_panic};
use xv6_user_lib::{
    fs::{self, File, OpenFlags},
    process,
};

const CONSOLE: i16 = 1;

fn main() {
    let console = match File::open(c"console", OpenFlags::READ_WRITE) {
        Ok(console) => console,
        Err(_) => {
            try_or_panic!(
                fs::mknod(c"console", CONSOLE, 0),
                e => "create console failed: {e}",
            );
            try_or_panic!(
                File::open(c"console", OpenFlags::READ_WRITE),
                e => "open console failed: {e}",
            )
        }
    };
    let _stdout = try_or_panic!(
        console.try_clone(),
        e => "create stdout failed: {e}",
    );
    let _stderr = try_or_panic!(
        console.try_clone(),
        e => "create stderr failed: {e}",
    );

    loop {
        message!("starting sh");

        let pid = try_or_panic!(
            process::fork(),
            e => "fork failed: {e}",
        );

        if pid == 0 {
            let argv = [c"sh".as_ptr(), ptr::null()];
            try_or_panic!(
                process::exec(c"sh", &argv),
                e => "exec sh failed: {e}",
            );
        }

        loop {
            // this call to wait() returns if the shell exits,
            // or if a parentless process exits.
            let (wpid, _status) = try_or_panic!(
                process::wait(),
                e => "wait returned an error: {e}",
            );
            if wpid != pid {
                // it was a parentless process; do nothing
                continue;
            }
            // the shell exited; restart it.
            break;
        }
    }
}
