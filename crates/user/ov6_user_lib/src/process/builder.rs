use core::convert::Infallible;

use crate::{
    error::Ov6Error,
    os::fd::{BorrowedFd, OwnedFd},
    pipe::{PipeReader, PipeWriter},
};

#[derive(Debug)]
pub struct Builder {
    stdin: Option<Stdio>,
    stdout: Option<Stdio>,
    stderr: Option<Stdio>,
}

#[derive(Debug)]
pub enum Stdio {
    Inherit,
    Null,
    Pipe,
    OwnedFd(OwnedFd),
    BorrowedFd(BorrowedFd<'static>),
}

impl Builder {
    pub fn new() -> Self {
        Self {
            stdin: None,
            stdout: None,
            stderr: None,
        }
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

    pub fn spawn_fn<F>(&mut self, f: F) -> Result<Child, Ov6Error>
    where
        F: FnOnce() -> Infallible,
    {
        todo!()
    }
}

#[derive(Debug)]
pub struct Child {
    pub stdin: Option<PipeWriter>,
    pub stdout: Option<PipeReader>,
    pub stderr: Option<PipeReader>,
}
