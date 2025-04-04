#![cfg_attr(not(test), no_std)]

use ov6_user_lib::{
    env, eprintln,
    error::Ov6Error,
    fs::{self, File},
    process::{self, ProcessBuilder},
};

const CONSOLE: u32 = 1;

fn open_console() -> Result<File, Ov6Error> {
    File::options().read(true).write(true).open("console")
}

fn create_console() -> Result<(), Ov6Error> {
    fs::mknod("console", CONSOLE, 0)
}

fn main() {
    let arg0 = env::arg0();
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
        eprintln!("{}: starting sh", arg0.display());

        let Ok(sh) = ProcessBuilder::new()
            .spawn_fn(|| {
                let Ok(_) = process::exec("sh", &["sh"]).map_err(|e| {
                    panic!("{} exec sh failed: {e}", arg0.display());
                });
                unreachable!()
            })
            .map_err(|e| {
                panic!("{}: fork failed: {e}", arg0.display());
            });

        loop {
            // this call to wait() returns if the shell exits,
            // or if a parentless process exits.
            let Ok((wpid, _status)) = process::wait_any()
                .map_err(|e| panic!("{}: wait returned an error: {e}", arg0.display()));
            if wpid != sh.id() {
                // it was a parentless process; do nothing
                continue;
            }
            // the shell exited; restart it.
            break;
        }
    }
}
