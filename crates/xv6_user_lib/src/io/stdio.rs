use core::fmt::{self, Write as _};

use alloc_crate::string::String;
use once_init::OnceInit;

use crate::{
    error::Error,
    io::DEFAULT_BUF_SIZE,
    os::{fd::RawFd, xv6::syscall},
    sync::spin::{Mutex, MutexGuard},
};

use super::{BufRead, BufReader, Read, Write};

pub fn _print(args: fmt::Arguments) {
    stdout().write_fmt(args).unwrap();
}

pub fn _eprint(args: fmt::Arguments) {
    stderr().write_fmt(args).unwrap();
}

pub const STDIN_FD: RawFd = 0;
pub const STDOUT_FD: RawFd = 1;
pub const STDERR_FD: RawFd = 2;

struct StdinRaw {}

impl Read for StdinRaw {
    fn read(&mut self, buf: &mut [u8]) -> Result<usize, Error> {
        syscall::read(STDIN_FD, buf)
    }
}

pub fn stdout() -> Stdout {
    Stdout {}
}

pub fn stderr() -> Stderr {
    Stderr {}
}

pub fn stdin() -> Stdin {
    static INSTANCE: OnceInit<Mutex<BufReader<StdinRaw>>> = OnceInit::new();
    let _ = INSTANCE
        .try_init_with(|| Mutex::new(BufReader::with_capacity(DEFAULT_BUF_SIZE, StdinRaw {})));
    let instance = loop {
        if let Ok(instance) = INSTANCE.try_get() {
            break instance;
        }
    };
    Stdin { inner: instance }
}

pub struct Stdout {}

impl Write for Stdout {
    fn write(&mut self, buf: &[u8]) -> Result<usize, Error> {
        syscall::write(STDOUT_FD, buf)
    }
}

impl Write for &'_ Stdout {
    fn write(&mut self, buf: &[u8]) -> Result<usize, Error> {
        syscall::write(STDOUT_FD, buf)
    }
}

impl fmt::Write for Stdout {
    fn write_str(&mut self, s: &str) -> fmt::Result {
        Write::write(self, s.as_bytes()).map_err(|_| fmt::Error)?;
        Ok(())
    }
}

pub struct Stderr {}

impl Write for Stderr {
    fn write(&mut self, buf: &[u8]) -> Result<usize, Error> {
        syscall::write(STDERR_FD, buf)
    }
}

impl Write for &'_ Stderr {
    fn write(&mut self, buf: &[u8]) -> Result<usize, Error> {
        syscall::write(STDERR_FD, buf)
    }
}

impl fmt::Write for Stderr {
    fn write_str(&mut self, s: &str) -> fmt::Result {
        Write::write(self, s.as_bytes()).map_err(|_| fmt::Error)?;
        Ok(())
    }
}

pub struct Stdin {
    inner: &'static Mutex<BufReader<StdinRaw>>,
}

pub struct StdinLock<'lock> {
    inner: MutexGuard<'lock, BufReader<StdinRaw>>,
}

impl Stdin {
    pub fn lock(&self) -> StdinLock<'_> {
        StdinLock {
            inner: self.inner.lock(),
        }
    }

    pub fn read_line(&mut self, buf: &mut String) -> Result<usize, Error> {
        let mut locked = self.lock();
        locked.read_line(buf)
    }
}

impl Read for Stdin {
    fn read(&mut self, buf: &mut [u8]) -> Result<usize, Error> {
        self.inner.lock().read(buf)
    }
}

impl Read for StdinLock<'_> {
    fn read(&mut self, buf: &mut [u8]) -> Result<usize, Error> {
        self.inner.read(buf)
    }
}

impl BufRead for StdinLock<'_> {
    fn fill_buf(&mut self) -> Result<&[u8], Error> {
        self.inner.fill_buf()
    }

    fn consume(&mut self, amt: usize) {
        self.inner.consume(amt)
    }
}
