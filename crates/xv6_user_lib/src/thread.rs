use crate::os::xv6::syscall;

pub fn sleep(dur: i32) {
    syscall::sleep(dur).unwrap()
}
