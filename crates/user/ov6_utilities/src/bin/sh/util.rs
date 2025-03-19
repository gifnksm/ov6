use core::convert::Infallible;

use ov6_user_lib::{
    os::{
        fd::{AsRawFd as _, RawFd},
        ov6::syscall,
    },
    process::{ChildWithIo, ExitStatus, ProcessBuilder},
};
use ov6_utilities::{OrExit as _, exit_err, message};

pub(super) trait SpawnFnOrExit {
    fn spawn_fn_or_exit<F>(&mut self, f: F) -> ChildWithIo
    where
        F: FnOnce() -> Infallible;
}

impl SpawnFnOrExit for ProcessBuilder {
    #[track_caller]
    fn spawn_fn_or_exit<F>(&mut self, f: F) -> ChildWithIo
    where
        F: FnOnce() -> Infallible,
    {
        self.spawn_fn(f)
            .or_exit(|e| exit_err!(e, "fork child process failed"))
    }
}

pub(super) trait WaitOrExit {
    fn wait_or_exit(&mut self) -> ExitStatus;
}

impl WaitOrExit for ChildWithIo {
    fn wait_or_exit(&mut self) -> ExitStatus {
        let status = self
            .wait()
            .or_exit(|e| exit_err!(e, "wait child process failed"));
        if !status.success() {
            message!("command failed with status {}", status.code());
        }
        status
    }
}

pub(super) unsafe fn close_or_exit(fd: RawFd, fd_name: &str) {
    unsafe { syscall::close(fd.as_raw_fd()) }.or_exit(|e| exit_err!(e, "close {fd_name} failed"));
}
