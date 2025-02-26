use crate::{error::Error, os::xv6::syscall};

pub fn sleep(dur: i32) -> Result<(), Error> {
    syscall::sleep(dur)
}
