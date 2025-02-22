#![no_std]

use core::fmt::{self, Write as _};

pub const STDOUT_FD: i32 = 0;
pub const STDERR_FD: i32 = 1;
pub const STDIN_FD: i32 = 2;

unsafe extern "Rust" {
    fn main(argc: i32, argv: *mut *mut u8);
}

#[unsafe(export_name = "_start")]
extern "C" fn start(argc: i32, argv: *mut *mut u8) {
    unsafe {
        main(argc, argv);
    }
    exit(0);
}

pub fn exit(code: i32) -> ! {
    xv6_user_syscall::exit(code);
}

pub fn write(fd: i32, buf: &[u8]) -> Result<usize, ()> {
    let ret = unsafe { xv6_user_syscall::write(fd, buf.as_ptr(), buf.len()) };
    if ret < 0 {
        return Err(());
    }
    Ok(ret as usize)
}

#[panic_handler]
fn panic(info: &core::panic::PanicInfo) -> ! {
    println!("panic: {info}");
    exit(1);
}

pub struct Stdout;
pub struct Stderr;

impl fmt::Write for Stdout {
    fn write_str(&mut self, s: &str) -> fmt::Result {
        if write(STDOUT_FD, s.as_bytes()).is_err() {
            return Err(fmt::Error);
        }
        Ok(())
    }
}

impl fmt::Write for Stderr {
    fn write_str(&mut self, s: &str) -> fmt::Result {
        if write(STDERR_FD, s.as_bytes()).is_err() {
            return Err(fmt::Error);
        }
        Ok(())
    }
}

pub fn _print(args: fmt::Arguments) {
    Stdout.write_fmt(args).unwrap();
}

pub fn _eprint(args: fmt::Arguments) {
    Stdout.write_fmt(args).unwrap();
}

#[macro_export]
macro_rules! print {
    ($($arg:tt)*) => {
        $crate::_print(format_args!($($arg)*))
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
        $crate::_eprint(format_args!($($arg)*))
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
