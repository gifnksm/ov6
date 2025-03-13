use alloc::{boxed::Box, sync::Arc, vec::Vec};

use ov6_user_lib::{
    fs::File,
    io::{STDIN_FD, STDOUT_FD},
    pipe,
    process::{self, ForkResult},
    sync::spin::Mutex,
};
use user::try_or_exit;

use crate::util;

pub(super) const MAX_ARGS: usize = 10;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum RedirectMode {
    Input,
    OutputTrunc,
    OutputAppend,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum RedirectFd {
    Stdin,
    Stdout,
}

#[derive(Debug)]
pub(super) enum Command<'a> {
    Exec {
        argv: Arc<Mutex<[Option<&'a str>; MAX_ARGS]>>,
    },
    Redirect {
        cmd: Box<Command<'a>>,
        file: &'a str,
        mode: RedirectMode,
        fd: RedirectFd,
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
                let argv = argv.iter().flatten().collect::<Vec<_>>();
                try_or_exit!(
                    process::exec(argv[0], &argv),
                    e => "exec {} failed: {e}", argv[0],
                );
            }
            Command::Redirect {
                cmd,
                file,
                mode,
                fd,
            } => {
                let (fd, fd_name) = match fd {
                    RedirectFd::Stdin => (STDIN_FD, "stdin"),
                    RedirectFd::Stdout => (STDOUT_FD, "stdout"),
                };
                let mut options = File::options();
                match mode {
                    RedirectMode::Input => options.read(true),
                    RedirectMode::OutputTrunc => options.write(true).create(true).truncate(true),
                    RedirectMode::OutputAppend => options.read(true).write(true).create(true),
                };
                unsafe { util::close_or_exit(fd, fd_name) }
                let _file = try_or_exit!(
                    options.open(file),
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
                    unsafe {
                        util::close_or_exit(STDOUT_FD, "stdout");
                    }
                    let _stdout = try_or_exit!(
                        tx.try_clone(),
                        e => "cloning pipe failed: {e}",
                    );
                    drop(rx);
                    drop(tx);
                    left.run();
                };

                let ForkResult::Parent { child: right } = util::fork_or_exit() else {
                    unsafe {
                        util::close_or_exit(STDIN_FD, "stdin");
                    }
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
                let child = util::fork_fn_or_exit(|| left.run());
                util::wait_or_exit(&[child.pid()]);
                right.run();
            }
            Command::Back { cmd } => {
                util::fork_fn_or_exit(|| cmd.run());
            }
        }

        process::exit(0);
    }
}
