use core::{convert::Infallible, ffi::CStr};

use crate::{error::Error, syscall};

pub fn exit(code: i32) -> ! {
    syscall::exit(code);
}

pub fn fork() -> Result<u32, Error> {
    let pid = syscall::fork();
    if pid < 0 {
        return Err(Error::Unknown);
    }
    Ok(pid.cast_unsigned())
}

pub fn exec(path: &CStr, argv: &[*const u8]) -> Result<Infallible, Error> {
    assert!(
        argv.last().unwrap().is_null(),
        "last element of argv must be null"
    );
    let res = unsafe { syscall::exec(path.as_ptr(), argv.as_ptr()) };
    assert!(res < 0);
    Err(Error::Unknown)
}

pub fn wait() -> Result<(u32, ExitStatus), Error> {
    let mut status = 0;
    let pid = unsafe { syscall::wait(&mut status) };
    if pid < 0 {
        return Err(Error::Unknown);
    }
    Ok((pid.cast_unsigned(), ExitStatus { status }))
}

pub fn kill(pid: u32) -> Result<(), Error> {
    let res = syscall::kill(pid.cast_signed());
    if res < 0 {
        return Err(Error::Unknown);
    }
    Ok(())
}

#[derive(Default, Debug, Clone, Copy, PartialEq, Eq)]
pub struct ExitStatus {
    status: i32,
}

impl ExitStatus {
    pub fn success(&self) -> bool {
        self.status == 0
    }

    pub fn code(&self) -> i32 {
        self.status
    }
}
