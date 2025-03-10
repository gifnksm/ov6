use crate::os::ov6::syscall;

pub fn sleep(dur: u64) {
    syscall::sleep(dur)
}
