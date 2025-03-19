use core::convert::Infallible;

use ov6_user_lib::{
    os::{
        fd::{AsRawFd as _, RawFd},
        ov6::syscall,
    },
    process::{self, ExitStatus, ForkFnHandle, ForkResult, ProcId},
};
use ov6_utilities::{ensure_or_exit, message, try_or_exit};

pub(super) fn fork_or_exit() -> ForkResult {
    try_or_exit!(
        process::fork(),
        e => "fork child process failed: {e}",
    )
}

pub(super) fn fork_fn_or_exit<F>(child_fn: F) -> ForkFnHandle
where
    F: FnOnce() -> Infallible,
{
    try_or_exit!(
        process::fork_fn(child_fn),
        e => "fork child process failed: {e}",
    )
}

pub(super) fn wait_or_exit(expected_pids: &[ProcId]) -> (ProcId, ExitStatus) {
    let (pid, status) = try_or_exit!(
        process::wait_any(),
        e => "wait child process failed: {e}"
    );
    ensure_or_exit!(
        expected_pids.contains(&pid),
        "unexpected process caught by wait"
    );
    if !status.success() {
        message!("command failed with status {}", status.code());
    }
    (pid, status)
}

pub(super) unsafe fn close_or_exit(fd: RawFd, fd_name: &str) {
    try_or_exit!(
        unsafe { syscall::close(fd.as_raw_fd()) },
        e => "close {fd_name} failed: {e}",
    );
}
