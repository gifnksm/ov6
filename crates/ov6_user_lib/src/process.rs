use core::convert::Infallible;

pub use crate::os::ov6::syscall::{exec, exit, fork, kill, wait};
use crate::{error::Ov6Error, os::ov6::syscall};

#[derive(Default, Debug, Clone, Copy, PartialEq, Eq)]
pub struct ExitStatus {
    status: i32,
}

impl ExitStatus {
    #[must_use]
    pub fn new(status: i32) -> Self {
        Self { status }
    }

    #[must_use]
    pub fn success(&self) -> bool {
        self.status == 0
    }

    #[must_use]
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
    #[must_use]
    pub fn as_parent(&self) -> Option<u32> {
        match self {
            Self::Parent { child } => Some(*child),
            Self::Child => None,
        }
    }

    #[must_use]
    pub fn is_parent(&self) -> bool {
        matches!(self, Self::Parent { .. })
    }

    #[must_use]
    pub fn is_child(&self) -> bool {
        matches!(self, Self::Child)
    }
}

#[must_use]
pub fn id() -> u32 {
    syscall::getpid().unwrap()
}

#[must_use]
pub fn current_break() -> *mut u8 {
    unsafe { syscall::sbrk(0) }.unwrap()
}

pub fn grow_break(size: usize) -> Result<*mut u8, Ov6Error> {
    unsafe { syscall::sbrk(size.try_into().unwrap()) }
}

/// # Safety
///
/// This function is unsafe because it may invalidate the region of memory that was previously allocated by the kernel.
pub unsafe fn shrink_break(size: usize) -> Result<*mut u8, Ov6Error> {
    unsafe { syscall::sbrk(-isize::try_from(size).unwrap()) }
}

pub struct ForkFnHandle {
    pid: u32,
}

impl ForkFnHandle {
    #[must_use]
    pub fn pid(&self) -> u32 {
        self.pid
    }

    pub fn wait(self) -> Result<ExitStatus, Ov6Error> {
        let (wpid, status) = wait()?;
        assert_eq!(
            self.pid, wpid,
            "The waited process ID does not match the forked process ID"
        );
        Ok(status)
    }
}

pub fn fork_fn<F>(child_fn: F) -> Result<ForkFnHandle, Ov6Error>
where
    F: FnOnce() -> Infallible,
{
    let Some(pid) = fork()?.as_parent() else {
        child_fn();
        unreachable!();
    };
    Ok(ForkFnHandle { pid })
}
