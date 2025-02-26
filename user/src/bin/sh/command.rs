use core::{array, ffi::c_char, ptr};

use alloc::{boxed::Box, ffi::CString, sync::Arc};
use user::try_or_exit;
use xv6_user_lib::{
    fs::{File, OpenFlags},
    io::{STDIN_FD, STDOUT_FD},
    os::fd::RawFd,
    pipe,
    process::{self, ForkResult},
    sync::spin::Mutex,
};

use crate::util;

pub(super) const MAX_ARGS: usize = 10;

#[derive(Debug)]
pub(super) enum Command<'a> {
    Exec {
        argv: Arc<Mutex<[Option<&'a str>; MAX_ARGS]>>,
    },
    Redirect {
        cmd: Box<Command<'a>>,
        file: &'a str,
        mode: OpenFlags,
        fd: RawFd,
        fd_name: &'static str,
    },
    Pipe {
        left: Box<Command<'a>>,
        right: Box<Command<'a>>,
    },
    List {
        left: Box<Command<'a>>,
        right: Box<Command<'a>>,
    },
    Back {
        cmd: Box<Command<'a>>,
    },
}

impl Command<'_> {
    pub(super) fn run(self) -> ! {
        match self {
            Command::Exec { argv } => {
                let argv = argv.lock();
                if argv[0].is_none() {
                    process::exit(0);
                }
                let argv_cstring: [Option<CString>; 10] =
                    array::from_fn(|i| argv[i].map(|s| CString::new(s).unwrap()));
                let argv_ptr: [*const c_char; 10] = array::from_fn(|i| {
                    argv_cstring[i]
                        .as_ref()
                        .map(|s| s.as_ptr())
                        .unwrap_or(ptr::null())
                });
                try_or_exit!(
                    process::exec(argv_cstring[0].as_ref().unwrap(), &argv_ptr),
                    e => "exec {} failed: {e}", argv[0].unwrap(),
                );
            }
            Command::Redirect {
                cmd,
                file,
                mode,
                fd,
                fd_name,
            } => {
                unsafe { util::close_or_exit(fd, fd_name) };
                let path = CString::new(file).unwrap();
                let _file = try_or_exit!(
                    File::open(&path, mode),
                    e => "open {} failed: {e}", file
                );
                cmd.run();
            }
            Command::Pipe { left, right } => {
                let (rx, tx) = try_or_exit!(
                    pipe::pipe(),
                    e => "pipe create failed: {e}",
                );

                let ForkResult::Parent { child: left } = util::fork_or_exit() else {
                    unsafe { util::close_or_exit(STDOUT_FD, "stdout") };
                    let _stdout = try_or_exit!(
                        tx.try_clone(),
                        e => "cloning pipe failed: {e}",
                    );
                    drop(rx);
                    drop(tx);
                    left.run();
                };

                let ForkResult::Parent { child: right } = util::fork_or_exit() else {
                    unsafe { util::close_or_exit(STDIN_FD, "stdin") };
                    let _stdin = try_or_exit!(
                        rx.try_clone(),
                        e => "cloning pipe failed: {e}",
                    );
                    drop(rx);
                    drop(tx);
                    right.run();
                };
                drop(rx);
                drop(tx);
                util::wait_or_exit(&[left, right]);
                util::wait_or_exit(&[left, right]);
            }
            Command::List { left, right } => {
                let ForkResult::Parent { child } = util::fork_or_exit() else {
                    left.run();
                };
                util::wait_or_exit(&[child]);
                right.run();
            }
            Command::Back { cmd } => {
                if util::fork_or_exit().is_child() {
                    cmd.run();
                }
            }
        }

        process::exit(0);
    }
}
