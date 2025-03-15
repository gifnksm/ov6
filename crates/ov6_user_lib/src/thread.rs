use crate::{
    os::ov6::syscall,
    time::{Duration, DurationExt as _},
};

pub fn sleep(dur: Duration) {
    syscall::sleep(dur.as_ticks());
}
