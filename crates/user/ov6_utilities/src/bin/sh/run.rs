use alloc::{borrow::Cow, vec::Vec};
use core::convert::Infallible;

use ov6_user_lib::{
    error::Ov6Error,
    fs::File,
    os_str::{OsStr, OsString},
    path::Path,
    process::{self, ChildWithIo, ExitStatus, ProcessBuilder, Stdio},
};
use ov6_utilities::{message, message_err};

use crate::{
    builtin,
    command::{Command, CommandKind, OutputMode, Redirect},
};

pub(super) trait ToCode {
    fn to_code(&self) -> i32;
}

impl ToCode for ExitStatus {
    fn to_code(&self) -> i32 {
        self.code()
    }
}

#[derive(Debug, thiserror::Error)]
pub(super) enum RunError {
    #[error("cannot open '{}': {}", file.display(), err)]
    OpenFile { file: OsString, err: Ov6Error },
    #[error("cannot fork child process: {err}")]
    Fork { err: Ov6Error },
    #[error("cannot exec '{}': {}", arg0.display(), err)]
    Exec { arg0: OsString, err: Ov6Error },
    #[error("cannot wait child process: {err}")]
    Wait { err: Ov6Error },
}

impl ToCode for RunError {
    fn to_code(&self) -> i32 {
        match self {
            Self::OpenFile { .. } | Self::Fork { .. } | Self::Wait { .. } => 1,
            Self::Exec { .. } => 127,
        }
    }
}

impl Redirect<'_> {
    fn open(self) -> Result<ProcessBuilder, RunError> {
        let mut builder = ProcessBuilder::new();
        if let Some(file) = self.stdin {
            let stdin = File::open(Path::new(&file)).map_err(|err| RunError::OpenFile {
                file: file.into_owned(),
                err,
            })?;
            builder.stdin(Stdio::Fd(stdin.into()));
        }
        if let Some((file, mode)) = self.stdout {
            let mut options = File::options();
            match mode {
                OutputMode::Truncate => options.write(true).create(true).truncate(true),
                OutputMode::Append => options.read(true).write(true).create(true),
            };
            let stdout = options
                .open(Path::new(&file))
                .map_err(|err| RunError::OpenFile {
                    file: file.into_owned(),
                    err,
                })?;
            builder.stdout(Stdio::Fd(stdout.into()));
        }
        Ok(builder)
    }
}

pub(super) fn run(cmd: Command<'_>) -> Result<ExitStatus, RunError> {
    match *cmd.kind {
        CommandKind::Subshell { list, redirect } => {
            let builder = redirect.open()?;
            let child = spawn_fn(builder, || Ok(run_list(list)))?;
            wait(child, cmd.background)
        }
        CommandKind::Exec { argv, redirect } => {
            if let Some(status) = builtin::run_builtin(&argv, cmd.background)? {
                return Ok(status);
            }
            run_external(&argv, redirect, cmd.background)
        }
        CommandKind::Pipe { left, right } => {
            let mut left_builder = ProcessBuilder::new();
            left_builder.stdout(Stdio::Pipe);
            let mut left = spawn_fn(left_builder, || run(left));
            let left_out = left.as_mut().ok().and_then(|l| l.stdout.take());

            let mut right_builder = ProcessBuilder::new();
            if let Some(left_out) = left_out {
                right_builder.stdin(Stdio::Fd(left_out.into()));
            }
            let right = spawn_fn(right_builder, || run(right));

            if let Err(e) = left.and_then(|left| wait(left, cmd.background)) {
                message_err!(e);
            }
            wait(right?, cmd.background)
        }
        CommandKind::LogicalAnd { left, right } => {
            let left_status = run(left)?;
            if !left_status.success() {
                return Ok(left_status);
            }
            run(right)
        }
        CommandKind::LogicalOr { left, right } => {
            match run(left) {
                Ok(status) if status.success() => return Ok(status),
                Ok(_status) => {}
                Err(e) => message_err!(e),
            }
            run(right)
        }
    }
}

pub(super) fn run_list(list: Vec<Command<'_>>) -> ExitStatus {
    let mut status = ExitStatus::new(0);
    for cmd in list {
        status = match run(cmd) {
            Ok(s) => s,
            Err(e) => {
                message_err!(e);
                ExitStatus::new(e.to_code())
            }
        };
    }
    status
}

pub(super) fn spawn_fn<F>(mut builder: ProcessBuilder, f: F) -> Result<ChildWithIo, RunError>
where
    F: FnOnce() -> Result<ExitStatus, RunError>,
{
    builder
        .spawn_fn(|| {
            let code = match f() {
                Ok(status) => status.to_code(),
                Err(e) => {
                    message_err!(e);
                    e.to_code()
                }
            };
            process::exit(code);
        })
        .map_err(|err| RunError::Fork { err })
}

pub(super) fn wait(mut child: ChildWithIo, background: bool) -> Result<ExitStatus, RunError> {
    if background {
        return Ok(ExitStatus::new(0));
    }

    let status = child.wait().map_err(|err| RunError::Wait { err })?;
    if !status.success() {
        message!("command exited with status {}", status.code());
    }
    Ok(status)
}

fn run_external(
    argv: &[Cow<'_, OsStr>],
    redirect: Redirect<'_>,
    background: bool,
) -> Result<ExitStatus, RunError> {
    let builder = redirect.open()?;
    let child = spawn_fn(builder, || {
        let _: Infallible = process::exec(&argv[0], argv).map_err(|err| RunError::Exec {
            arg0: argv[0].clone().into_owned(),
            err,
        })?;
        unreachable!()
    })?;
    wait(child, background)
}
