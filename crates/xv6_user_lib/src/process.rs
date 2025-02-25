pub use crate::os::xv6::syscall::{exec, exit, fork, kill, wait};

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
