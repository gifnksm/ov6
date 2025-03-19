use core::convert::Infallible;

use alloc_crate::vec::Vec;
use ov6_syscall::{UserSlice, WaitTarget};
pub use ov6_types::process::ProcId;
use ov6_types::{os_str::OsStr, path::Path};

pub use self::builder::{ChildWithIo, ProcessBuilder, Stdio};
use crate::{error::Ov6Error, os::ov6::syscall};

mod builder;

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

#[derive(Debug)]
pub struct Child {
    pid: ProcId,
}

impl Child {
    #[must_use]
    pub fn id(&self) -> ProcId {
        self.pid
    }

    pub fn kill(&mut self) -> Result<(), Ov6Error> {
        kill(self.pid)
    }

    pub fn wait(&mut self) -> Result<ExitStatus, Ov6Error> {
        wait_pid(self.pid)
    }
}

#[derive(Debug)]
pub enum ForkResult {
    Parent { child: Child },
    Child,
}

impl ForkResult {
    #[must_use]
    pub fn into_parent(self) -> Option<Child> {
        match self {
            Self::Parent { child } => Some(child),
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

pub fn fork() -> Result<ForkResult, Ov6Error> {
    let pid = syscall::fork()?;
    Ok(pid.map_or(ForkResult::Child, |pid| ForkResult::Parent {
        child: Child { pid },
    }))
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

pub fn kill(pid: ProcId) -> Result<(), Ov6Error> {
    syscall::kill(pid)
}

pub fn exit(status: i32) -> ! {
    syscall::exit(status)
}

pub fn wait_any() -> Result<(ProcId, ExitStatus), Ov6Error> {
    syscall::wait(WaitTarget::AnyProcess)
}

pub fn wait_pid(pid: ProcId) -> Result<ExitStatus, Ov6Error> {
    let (wpid, status) = syscall::wait(WaitTarget::Process(pid))?;
    assert_eq!(
        pid, wpid,
        "The waited process ID does not match the target process ID"
    );
    Ok(status)
}
