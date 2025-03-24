use core::convert::Infallible;

use alloc_crate::vec::Vec;
use ov6_syscall::{UserSlice, WaitTarget};
pub use ov6_types::process::ProcId;
use ov6_types::{os_str::OsStr, path::Path};

pub use self::builder::{ChildWithIo, ProcessBuilder, Stdio};
use crate::{error::Ov6Error, os::ov6::syscall};

mod builder;

/// Represents the exit status of a process.
#[derive(Default, Debug, Clone, Copy, PartialEq, Eq)]
pub struct ExitStatus {
    status: i32,
}

impl ExitStatus {
    /// Creates a new `ExitStatus` with the given status code.
    #[must_use]
    pub fn new(status: i32) -> Self {
        Self { status }
    }

    /// Checks if the process exited successfully.
    #[must_use]
    pub fn success(&self) -> bool {
        self.status == 0
    }

    /// Returns the status code of the process.
    #[must_use]
    pub fn code(&self) -> i32 {
        self.status
    }
}

/// Represents a child process.
#[derive(Debug)]
pub struct Child {
    pid: ProcId,
}

impl Child {
    /// Returns the process ID of the child process.
    #[must_use]
    pub fn id(&self) -> ProcId {
        self.pid
    }

    /// Sends a kill signal to the child process.
    pub fn kill(&mut self) -> Result<(), Ov6Error> {
        kill(self.pid)
    }

    /// Waits for the child process to exit and returns its exit status.
    pub fn wait(&mut self) -> Result<ExitStatus, Ov6Error> {
        wait_pid(self.pid)
    }
}

/// Represents a handle to a forked process, indicating whether it is the parent
/// or child process.
#[derive(Debug)]
pub enum JoinHandle {
    Parent { child: Child },
    Child,
}

impl JoinHandle {
    /// Converts the `JoinHandle` into an `Option<Child>`, returning
    /// `Some(child)` if it is the parent, or `None` if it is the child.
    #[must_use]
    pub fn into_parent(self) -> Option<Child> {
        match self {
            Self::Parent { child } => Some(child),
            Self::Child => None,
        }
    }

    /// Checks if the handle is for the parent process.
    #[must_use]
    pub fn is_parent(&self) -> bool {
        matches!(self, Self::Parent { .. })
    }

    /// Checks if the handle is for the child process.
    #[must_use]
    pub fn is_child(&self) -> bool {
        matches!(self, Self::Child)
    }

    /// Waits for the child process to exit and returns its exit status.
    pub fn join(self) -> Result<ExitStatus, Ov6Error> {
        match self {
            Self::Parent { mut child } => child.wait(),
            Self::Child => exit(0),
        }
    }
}

/// Forks the current process, creating a new child process.
pub fn fork() -> Result<JoinHandle, Ov6Error> {
    let pid = syscall::fork()?;
    Ok(pid.map_or(JoinHandle::Child, |pid| JoinHandle::Parent {
        child: Child { pid },
    }))
}

/// Returns the process ID of the current process.
#[must_use]
pub fn id() -> ProcId {
    syscall::getpid()
}

/// Returns the current program break (end of the process's data segment).
///
/// # Panics
///
/// This function will panic if the underlying syscall fails.
#[must_use]
pub fn current_break() -> *mut u8 {
    unsafe { syscall::sbrk(0) }.unwrap()
}

/// Increases the program break by the specified size.
///
/// # Panics
///
/// This function will panic if `size` is greater than `isize::MAX`.
pub fn grow_break(size: usize) -> Result<*mut u8, Ov6Error> {
    unsafe { syscall::sbrk(size.try_into().unwrap()) }
}

/// Decreases the program break by the specified size.
///
/// # Safety
///
/// This function is unsafe because it may invalidate the region of memory that
/// was previously allocated by the kernel.
///
/// # Panics
///
/// This function will panic if `-size` is less than `isize::MIN`.
pub unsafe fn shrink_break(size: usize) -> Result<*mut u8, Ov6Error> {
    unsafe { syscall::sbrk(-isize::try_from(size).unwrap()) }
}

/// Replaces the current process image with a new process image specified by the
/// path and arguments.
pub fn exec<P, A>(path: P, argv: &[A]) -> Result<Infallible, Ov6Error>
where
    P: AsRef<Path>,
    A: AsRef<OsStr>,
{
    if argv.len() < 10 {
        let mut new_argv = [const { UserSlice::from_raw_parts(0, 0) }; 10];
        for (dst, src) in new_argv.iter_mut().zip(argv) {
            *dst = UserSlice::new(src.as_ref().as_bytes());
        }
        syscall::exec(path.as_ref(), &new_argv[..argv.len()])
    } else {
        let argv = argv
            .iter()
            .map(|s| UserSlice::new(s.as_ref().as_bytes()))
            .collect::<Vec<_>>();

        syscall::exec(path.as_ref(), &argv)
    }
}

/// Sends a kill signal to the process with the specified process ID.
pub fn kill(pid: ProcId) -> Result<(), Ov6Error> {
    syscall::kill(pid)
}

/// Exits the current process with the specified status code.
pub fn exit(status: i32) -> ! {
    crate::rt::cleanup();
    syscall::exit(status)
}

/// Waits for any child process to exit and returns its process ID and exit
/// status.
pub fn wait_any() -> Result<(ProcId, ExitStatus), Ov6Error> {
    syscall::wait(WaitTarget::AnyProcess)
}

/// Waits for the specified child process to exit and returns its exit status.
///
/// # Panics
///
/// This function will panic if the waited process ID does not match the target
/// process ID.
pub fn wait_pid(pid: ProcId) -> Result<ExitStatus, Ov6Error> {
    let (wpid, status) = syscall::wait(WaitTarget::Process(pid))?;
    assert_eq!(
        pid, wpid,
        "The waited process ID does not match the target process ID"
    );
    Ok(status)
}
