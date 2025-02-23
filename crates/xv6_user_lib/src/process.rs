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

pub fn wait() -> Result<ExitStatus, Error> {
    let mut status = 0;
    let ret = unsafe { syscall::wait(&mut status) };
    if ret < 0 {
        return Err(Error::Unknown);
    }
    Ok(ExitStatus { status })
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
