use core::convert::Infallible;

use alloc_crate::vec::Vec;
use ov6_syscall::UserSlice;
pub use ov6_types::process::ProcId;
use ov6_types::{os_str::OsStr, path::Path};

pub use crate::os::ov6::syscall::{exit, fork, kill, wait};
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
    Parent { child: ProcId },
    Child,
}

impl ForkResult {
    #[must_use]
    pub fn as_parent(&self) -> Option<ProcId> {
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
pub fn id() -> ProcId {
    syscall::getpid()
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
/// This function is unsafe because it may invalidate the region of memory that
/// was previously allocated by the kernel.
pub unsafe fn shrink_break(size: usize) -> Result<*mut u8, Ov6Error> {
    unsafe { syscall::sbrk(-isize::try_from(size).unwrap()) }
}

pub struct ForkFnHandle {
    pid: ProcId,
}

impl ForkFnHandle {
    #[must_use]
    pub fn pid(&self) -> ProcId {
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
