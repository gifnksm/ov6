use core::convert::Infallible;

use ov6_types::{fs::RawFd, process::ProcId};

use super::{Child, ExitStatus};
use crate::{
    error::Ov6Error,
    io::{STDERR_FD, STDIN_FD, STDOUT_FD},
    os::{
        fd::{AsFd as _, AsRawFd as _, BorrowedFd, IntoRawFd as _, OwnedFd},
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

enum ChildStdio<'a> {
    Borrowed(BorrowedFd<'a>),
    Owned(OwnedFd),
}

impl ChildStdio<'_> {
    unsafe fn set(self, target_fd: RawFd) -> Result<(), Ov6Error> {
        match self {
            Self::Borrowed(src_fd) => {
                if src_fd.as_raw_fd() != target_fd {
                    let _ = unsafe { syscall::close(target_fd) };
                    let fd = src_fd.try_clone_to_owned()?.into_raw_fd();
                    assert_eq!(fd.as_raw_fd(), target_fd);
                    let _ = unsafe { syscall::close(src_fd.as_raw_fd()) };
                }
                Ok(())
            }
            Self::Owned(src_fd) => {
                if src_fd.as_raw_fd() != target_fd {
                    let _ = unsafe { syscall::close(target_fd) };
                    let fd = src_fd.try_clone()?.into_raw_fd();
                    assert_eq!(fd.as_raw_fd(), target_fd);
                    drop(src_fd);
                }
                Ok(())
            }
        }
    }
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
            Some(Stdio::Inherit) | None => (
                None,
                ChildStdio::Borrowed(unsafe { BorrowedFd::borrow_raw(STDIN_FD) }),
            ),
            Some(Stdio::Pipe) => {
                let (rx, tx) = pipe::pipe()?;
                (Some(tx), ChildStdio::Owned(rx.into()))
            }
            Some(Stdio::Fd(fd)) => (None, ChildStdio::Borrowed(fd.as_fd())),
        };
        let (parent_stdout, child_stdout) = match &self.stdout {
            Some(Stdio::Inherit) | None => (
                None,
                ChildStdio::Borrowed(unsafe { BorrowedFd::borrow_raw(STDOUT_FD) }),
            ),
            Some(Stdio::Pipe) => {
                let (rx, tx) = pipe::pipe()?;
                (Some(rx), ChildStdio::Owned(tx.into()))
            }
            Some(Stdio::Fd(fd)) => (None, ChildStdio::Borrowed(fd.as_fd())),
        };
        let (parent_stderr, child_stderr) = match &self.stderr {
            Some(Stdio::Inherit) | None => (
                None,
                ChildStdio::Borrowed(unsafe { BorrowedFd::borrow_raw(STDERR_FD) }),
            ),
            Some(Stdio::Pipe) => {
                let (rx, tx) = pipe::pipe()?;
                (Some(rx), ChildStdio::Owned(tx.into()))
            }
            Some(Stdio::Fd(fd)) => (None, ChildStdio::Borrowed(fd.as_fd())),
        };

        let Some(pid) = super::fork()?.into_parent() else {
            drop(parent_stdin);
            drop(parent_stdout);
            drop(parent_stderr);
            unsafe { child_stdin.set(STDIN_FD) }?;
            unsafe { child_stdout.set(STDOUT_FD) }?;
            unsafe { child_stderr.set(STDERR_FD) }?;
            let _ = self.stdin.take();
            let _ = self.stdout.take();
            let _ = self.stderr.take();
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
