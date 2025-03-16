use crate::{os::ov6::syscall, time::Duration};

pub fn sleep(dur: Duration) {
    syscall::sleep(dur).unwrap();
}
