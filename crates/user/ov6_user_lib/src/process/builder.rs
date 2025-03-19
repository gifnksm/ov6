use core::convert::Infallible;

use ov6_types::{fs::RawFd, process::ProcId};

use super::{Child, ExitStatus};
use crate::{
    error::Ov6Error,
    io::{STDERR_FD, STDIN_FD, STDOUT_FD},
    os::{
        fd::{AsRawFd as _, IntoRawFd as _, OwnedFd},
        ov6::syscall,
    },
    pipe::{self, PipeReader, PipeWriter},
};

#[derive(Debug, Default)]
pub struct ProcessBuilder {
    stdin: Option<Stdio>,
    stdout: Option<Stdio>,
    stderr: Option<Stdio>,
}

#[derive(Debug)]
pub enum Stdio {
    Inherit,
    Pipe,
    Fd(OwnedFd),
}

impl ProcessBuilder {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    pub fn stdin(&mut self, stdio: Stdio) -> &mut Self {
        self.stdin = Some(stdio);
        self
    }

    pub fn stdout(&mut self, stdio: Stdio) -> &mut Self {
        self.stdout = Some(stdio);
        self
    }

    pub fn stderr(&mut self, stdio: Stdio) -> &mut Self {
        self.stderr = Some(stdio);
        self
    }

    pub fn spawn_fn<F>(&mut self, f: F) -> Result<ChildWithIo, Ov6Error>
    where
        F: FnOnce() -> Infallible,
    {
        let (parent_stdin, child_stdin) = match &self.stdin {
            Some(Stdio::Inherit) | None => (None, None),
            Some(Stdio::Pipe) => {
                let (rx, tx) = pipe::pipe()?;
                (Some(tx), Some(rx.into()))
            }
            Some(Stdio::Fd(fd)) => (None, Some(fd.try_clone()?)),
        };
        let (parent_stdout, child_stdout) = match &self.stdout {
            Some(Stdio::Inherit) | None => (None, None),
            Some(Stdio::Pipe) => {
                let (rx, tx) = pipe::pipe()?;
                (Some(rx), Some(tx.into()))
            }
            Some(Stdio::Fd(fd)) => (None, Some(fd.try_clone()?)),
        };
        let (parent_stderr, child_stderr) = match &self.stderr {
            Some(Stdio::Inherit) | None => (None, None),
            Some(Stdio::Pipe) => {
                let (rx, tx) = pipe::pipe()?;
                (Some(rx), Some(tx.into()))
            }
            Some(Stdio::Fd(fd)) => (None, Some(fd.try_clone()?)),
        };

        let Some(pid) = super::fork()?.into_parent() else {
            let _ = self.stdin.take();
            let _ = self.stdout.take();
            let _ = self.stderr.take();
            drop(parent_stdin);
            drop(parent_stdout);
            drop(parent_stderr);
            if let Some(child_stdin) = child_stdin {
                unsafe {
                    set_stdio_fd(child_stdin, STDIN_FD).unwrap();
                }
            }
            if let Some(child_stdout) = child_stdout {
                unsafe {
                    set_stdio_fd(child_stdout, STDOUT_FD).unwrap();
                }
            }
            if let Some(child_stderr) = child_stderr {
                unsafe {
                    set_stdio_fd(child_stderr, STDERR_FD).unwrap();
                }
            }
            let _: Infallible = f();
            unreachable!()
        };

        drop(child_stdin);
        drop(child_stdout);
        drop(child_stderr);
        Ok(ChildWithIo {
            child: pid,
            stdin: parent_stdin,
            stdout: parent_stdout,
            stderr: parent_stderr,
        })
    }
}

unsafe fn set_stdio_fd(src: OwnedFd, dst: RawFd) -> Result<(), Ov6Error> {
    unsafe {
        syscall::close(dst)?;
    }
    let stdio = src.try_clone()?;
    drop(src);
    assert_eq!(stdio.as_raw_fd(), dst);
    let _ = stdio.into_raw_fd();
    Ok(())
}

#[derive(Debug)]
pub struct ChildWithIo {
    pub child: Child,
    pub stdin: Option<PipeWriter>,
    pub stdout: Option<PipeReader>,
    pub stderr: Option<PipeReader>,
}

impl ChildWithIo {
    #[must_use]
    pub fn id(&self) -> ProcId {
        self.child.id()
    }

    pub fn kill(&mut self) -> Result<(), Ov6Error> {
        self.child.kill()
    }

    pub fn wait(&mut self) -> Result<ExitStatus, Ov6Error> {
        self.child.wait()
    }
}
