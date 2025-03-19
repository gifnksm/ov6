use core::convert::Infallible;

use ov6_user_lib::{
    os::{
        fd::{AsRawFd as _, RawFd},
        ov6::syscall,
    },
    process::{ChildWithIo, ExitStatus, ProcessBuilder},
};
use ov6_utilities::{message, try_or_exit};

pub(super) trait SpawnFnOrExit {
    fn spawn_fn_or_exit<F>(&mut self, f: F) -> ChildWithIo
    where
        F: FnOnce() -> Infallible;
}

impl SpawnFnOrExit for ProcessBuilder {
    fn spawn_fn_or_exit<F>(&mut self, f: F) -> ChildWithIo
    where
        F: FnOnce() -> Infallible,
    {
        try_or_exit!(
            self.spawn_fn(f),
            e => "fork child process failed: {e}",
        )
    }
}

pub(super) trait WaitOrExit {
    fn wait_or_exit(&mut self) -> ExitStatus;
}

impl WaitOrExit for ChildWithIo {
    fn wait_or_exit(&mut self) -> ExitStatus {
        let status = try_or_exit!(
            self.wait(),
            e => "wait child process failed: {e}",
        );
        if !status.success() {
            message!("command failed with status {}", status.code());
        }
        status
    }
}

pub(super) unsafe fn close_or_exit(fd: RawFd, fd_name: &str) {
    try_or_exit!(
        unsafe { syscall::close(fd.as_raw_fd()) },
        e => "close {fd_name} failed: {e}",
    );
}
