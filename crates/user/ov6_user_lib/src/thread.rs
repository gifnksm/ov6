use crate::{os::ov6::syscall, time::Duration};

/// Puts the current thread to sleep for the specified duration.
///
/// # Panics
///
/// This function will panic if the underlying syscall fails.
pub fn sleep(dur: Duration) {
    syscall::sleep(dur).unwrap();
}
