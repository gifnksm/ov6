use alloc::{borrow::Cow, boxed::Box, sync::Arc, vec::Vec};
use core::convert::AsRef;

use ov6_user_lib::{
    fs::File,
    io::{STDIN_FD, STDOUT_FD},
    process::{self, ProcessBuilder, Stdio},
    sync::spin::Mutex,
};
use ov6_utilities::{OrExit as _, exit_err};

use crate::util::{self, SpawnFnOrExit as _, WaitOrExit as _};

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
        argv: Arc<Mutex<Vec<Cow<'a, str>>>>,
    },
    Redirect {
        cmd: Box<Command<'a>>,
        file: Cow<'a, str>,
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
                if argv.is_empty() {
                    process::exit(0);
                }
                let argv = argv.iter().map(AsRef::as_ref).collect::<Vec<_>>();
                process::exec(argv[0], &argv)
                    .or_exit(|e| exit_err!(e, "exec '{}' failed", argv[0]));
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
                let _file = options
                    .open(file.as_ref())
                    .or_exit(|e| exit_err!(e, "open '{file}' failed"));
                cmd.run();
            }
            Command::Pipe { left, right } => {
                let mut left = ProcessBuilder::new()
                    .stdout(Stdio::Pipe)
                    .spawn_fn_or_exit(|| left.run());
                let left_out = left.stdout.take().unwrap();
                let mut right = ProcessBuilder::new()
                    .stdin(Stdio::Fd(left_out.into()))
                    .spawn_fn_or_exit(|| {
                        right.run();
                    });
                left.wait_or_exit();
                right.wait_or_exit();
            }
            Command::List { left, right } => {
                ProcessBuilder::new()
                    .spawn_fn_or_exit(|| left.run())
                    .wait_or_exit();
                right.run();
            }
            Command::Back { cmd } => {
                ProcessBuilder::new().spawn_fn_or_exit(|| cmd.run());
            }
        }

        process::exit(0);
    }
}
