use user::{ensure_or_exit, message, try_or_exit};
use xv6_user_lib::{
    os::{fd::AsRawFd, xv6::syscall},
    process::{self, ExitStatus, ForkResult},
};

pub(super) fn fork_or_exit() -> ForkResult {
    try_or_exit!(
        process::fork(),
        e => "fork child process failed: {e}",
    )
}

pub(super) fn wait_or_exit(expected_pids: &[u32]) -> (u32, ExitStatus) {
    let (pid, status) = try_or_exit!(
        process::wait(),
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

pub(super) unsafe fn close_or_exit(fd: impl AsRawFd, fd_name: &str) {
    try_or_exit!(
        unsafe { syscall::close(fd) },
        e => "close {fd_name} failed: {e}",
    )
}
