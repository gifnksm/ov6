pub use crate::os::xv6::syscall::{exec, exit, fork, kill, wait};
use crate::{error::Error, os::xv6::syscall};

#[derive(Default, Debug, Clone, Copy, PartialEq, Eq)]
pub struct ExitStatus {
    status: i32,
}

impl ExitStatus {
    pub fn new(status: i32) -> Self {
        Self { status }
    }

    pub fn success(&self) -> bool {
        self.status == 0
    }

    pub fn code(&self) -> i32 {
        self.status
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ForkResult {
    Parent { child: u32 },
    Child,
}

impl ForkResult {
    pub fn is_parent(&self) -> bool {
        matches!(self, Self::Parent { .. })
    }

    pub fn is_child(&self) -> bool {
        matches!(self, Self::Child)
    }
}

pub fn id() -> Result<u32, Error> {
    syscall::getpid()
}
