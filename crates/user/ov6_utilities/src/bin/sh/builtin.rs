use alloc::borrow::Cow;
use core::str::FromStr as _;

use ov6_user_lib::{
    env,
    os_str::OsStr,
    process::{self, ExitStatus, ProcId, ProcessBuilder},
};
use ov6_utilities::{message, message_err};

use crate::run::{self, RunError};

pub(super) fn run_builtin(
    argv: &[Cow<'_, OsStr>],
    background: bool,
) -> Result<Option<ExitStatus>, RunError> {
    let f = match argv[0].as_bytes() {
        b"cd" => builtin_cd,
        b"wait" => builtin_wait,
        _ => return Ok(None),
    };
    if background {
        let child = run::spawn_fn(ProcessBuilder::new(), || Ok(f(argv)))?;
        run::wait(child, background).map(Some)
    } else {
        let status = f(argv);
        Ok(Some(status))
    }
}

fn builtin_cd(argv: &[Cow<'_, OsStr>]) -> ExitStatus {
    if argv.len() != 2 {
        message!("Usage: cd <dir>");
        return ExitStatus::new(2);
    }
    let dir = &argv[1];
    if let Err(e) = env::set_current_directory(dir) {
        message_err!(e, "cannot cd to '{}'", dir.display());
        return ExitStatus::new(1);
    }
    ExitStatus::new(0)
}

fn builtin_wait(argv: &[Cow<'_, OsStr>]) -> ExitStatus {
    if argv.len() == 1 {
        match process::wait_any() {
            Ok((_pid, status)) => return status,
            Err(e) => {
                message_err!(e, "cannot wait");
                return ExitStatus::new(1);
            }
        }
    }

    let mut status = ExitStatus::new(0);
    for arg in &argv[1..] {
        let Some(pid_str) = arg.to_str() else {
            message!("invalid pid '{}'", arg.display());
            return ExitStatus::new(2);
        };
        let pid = match ProcId::from_str(pid_str) {
            Ok(pid) => pid,
            Err(e) => {
                message_err!(e, "invalid pid '{pid_str}'");
                return ExitStatus::new(2);
            }
        };
        match process::wait_pid(pid) {
            Ok(st) => status = st,
            Err(e) => {
                message_err!(e, "cannot wait '{pid}'");
                status = ExitStatus::new(1);
            }
        }
    }
    status
}
