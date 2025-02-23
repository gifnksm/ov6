use core::fmt::{self, Write as _};

use crate::{error::Error, os};

pub const STDIN_FD: i32 = 0;
pub const STDOUT_FD: i32 = 1;
pub const STDERR_FD: i32 = 2;

pub trait Read {
    fn read(&mut self, buf: &mut [u8]) -> Result<usize, Error>;
}

pub trait Write {
    fn write(&mut self, buf: &[u8]) -> Result<usize, Error>;
}

pub fn stdout() -> Stdout {
    Stdout {}
}

pub fn stderr() -> Stderr {
    Stderr {}
}

pub fn stdin() -> Stdin {
    Stdin {}
}

pub struct Stdout {}

impl Write for Stdout {
    fn write(&mut self, buf: &[u8]) -> Result<usize, Error> {
        os::fd_write(STDOUT_FD, buf)
    }
}

impl Write for &'_ Stdout {
    fn write(&mut self, buf: &[u8]) -> Result<usize, Error> {
        os::fd_write(STDOUT_FD, buf)
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
        os::fd_write(STDERR_FD, buf)
    }
}

impl Write for &'_ Stderr {
    fn write(&mut self, buf: &[u8]) -> Result<usize, Error> {
        os::fd_write(STDERR_FD, buf)
    }
}

impl fmt::Write for Stderr {
    fn write_str(&mut self, s: &str) -> fmt::Result {
        Write::write(self, s.as_bytes()).map_err(|_| fmt::Error)?;
        Ok(())
    }
}

pub struct Stdin {}

impl Read for Stdin {
    fn read(&mut self, buf: &mut [u8]) -> Result<usize, Error> {
        os::fd_read(STDIN_FD, buf)
    }
}

pub fn _print(args: fmt::Arguments) {
    stdout().write_fmt(args).unwrap();
}

pub fn _eprint(args: fmt::Arguments) {
    stderr().write_fmt(args).unwrap();
}

#[macro_export]
macro_rules! print {
    ($($arg:tt)*) => {
        $crate::io::_print(format_args!($($arg)*))
    };
}

#[macro_export]
macro_rules! println {
    () => {
        $crate::print!("\n")
    };
    ($($arg:tt)*) => {
        $crate::print!("{}\n", format_args!($($arg)*))
    };
}

#[macro_export]
macro_rules! eprint {
    ($($arg:tt)*) => {
        $crate::io::_eprint(format_args!($($arg)*))
    };
}

#[macro_export]
macro_rules! eprintln {
    () => {
        $crate::eprint!("\n")
    };
    ($($arg:tt)*) => {
        $crate::eprint!("{}\n", format_args!($($arg)*))
    };
}
