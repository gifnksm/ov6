use alloc::borrow::Cow;

use ov6_user_lib::{
    env,
    os_str::OsStr,
    process::{ExitStatus, ProcessBuilder},
};
use ov6_utilities::message;

use crate::run::{self, RunError};

pub(super) fn run_builtin(
    argv: &[Cow<'_, OsStr>],
    background: bool,
) -> Result<Option<ExitStatus>, RunError> {
    let f = match argv[0].as_bytes() {
        b"cd" => builtin_cd,
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
        message!("cannot cd to '{}': {}", dir.display(), e);
        return ExitStatus::new(1);
    }
    ExitStatus::new(0)
}
