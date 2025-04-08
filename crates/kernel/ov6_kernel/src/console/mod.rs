//! Console input and output, to the UART.
//!
//! This module provides functionality for console input and output through the
//! UART interface. It supports line-based input and special control characters
//! for editing and process management.
//!
//! Special input characters:
//!
//! * `newline` (`\n`) -- end of line
//! * `control-h` (`CTRL_H`) -- backspace
//! * `control-u` (`CTRL_U`) -- kill line
//! * `control-d` (`CTRL_D`) -- end of file
//! * `control-p` (`CTRL_P`) -- print process list

use crate::{
    error::KernelError,
    file::{self, Device},
    fs::DeviceNo,
    memory::{
        addr::{GenericMutSlice, GenericSlice},
        vm_user::UserPageTable,
    },
    proc,
    sync::{SleepLock, SpinLock, SpinLockCondVar, WaitError},
};

pub mod print;
pub mod uart;

/// Converts a character to its control character equivalent.
///
/// # Examples
///
/// ```
/// assert_eq!(ctrl(b'H'), 8); // CTRL-H
/// ```
const fn ctrl(x: u8) -> u8 {
    x - b'@'
}

const CTRL_H: u8 = ctrl(b'H');
const CTRL_U: u8 = ctrl(b'U');
const CTRL_D: u8 = ctrl(b'D');
const CTRL_P: u8 = ctrl(b'P');

/// Send one character to the UART.
///
/// This function is used by macros like `println!()` to send characters to the
/// UART. But not from `write()`, which is used by the user process.
///
/// This function is not intended for direct use in user code.
pub fn put_char(c: char) {
    uart::putc_sync(c);
}

/// Sends a backspace character to the UART.
///
/// This function sends a backspace character followed by a space and another
/// backspace to visually erase the last character on the console.
fn put_backspace() {
    uart::putc_sync('\x08');
    uart::putc_sync(' ');
    uart::putc_sync('\x08');
}

/// Represents the console buffer.
///
/// This structure holds the input buffer and indices for reading, writing, and
/// editing.
struct Cons {
    /// Input buffer.
    buf: [u8; 128],
    /// Read index.
    r: usize,
    /// Write index.
    w: usize,
    /// Edit index.
    e: usize,
}

static CONSOLE_BUFFER: SpinLock<Cons> = SpinLock::new(Cons {
    buf: [0; 128],
    r: 0,
    w: 0,
    e: 0,
});
static CONSOLE_BUFFER_WRITTEN: SpinLockCondVar = SpinLockCondVar::new();

/// Writes the bytes to the console.
///
/// This function handles user `write()` calls to the console. It ensures that
/// only one process can write to the console at a time, preventing interleaved
/// or corrupted output.
fn write(src: &GenericSlice<u8>) -> Result<usize, KernelError> {
    static CONSOLE_WRITE_LOCK: SleepLock<()> = SleepLock::new(());

    // ensure that only one process can write to the console at a time,
    // preventing output from being interleaved or corrupted
    let _guard = CONSOLE_WRITE_LOCK.wait_lock()?;

    for i in 0..src.len() {
        let mut c: [u8; 1] = [0];
        UserPageTable::copy_x2k_bytes(&mut c, &src.skip(i).take(1));
        if let Err(e) = uart::putc(c[0]) {
            if i > 0 {
                return Ok(i);
            }
            return Err(e);
        }
    }

    Ok(src.len())
}

/// Reads the bytes from the console.
///
/// This function handles user `read()` calls to the console. It copies up to a
/// whole input line to the provided buffer. The `user_dst` parameter indicates
/// whether the destination is a user or kernel address.
fn read(dst: &mut GenericMutSlice<u8>) -> Result<usize, KernelError> {
    let mut i = 0;
    let mut cons = CONSOLE_BUFFER.lock();
    while i < dst.len() {
        // wait until interrupt handler has put some
        // input into cons.buffer.
        while cons.r == cons.w {
            match CONSOLE_BUFFER_WRITTEN.wait(cons) {
                Ok(guard) => cons = guard,
                Err((_guard, WaitError::WaitingProcessAlreadyKilled)) => {
                    return Err(KernelError::CallerProcessAlreadyKilled);
                }
            }
        }

        let c = cons.buf[cons.r % cons.buf.len()];
        cons.r += 1;

        // end-of-file
        if c == CTRL_D {
            if i == 0 {
                break;
            }

            // Save ^D for next time, to make sure
            // caller gets a 0-byte result.
            if i < dst.len() {
                cons.r -= 1;
            }
            break;
        }

        // copy the input byte to the user-space buffer.
        let cbuf = &[c];
        UserPageTable::copy_k2x_bytes(&mut dst.skip_mut(i).take_mut(1), cbuf);

        i += 1;

        if c == b'\n' {
            // a whole line has arrived, return to
            // the user-level read().
            break;
        }
    }
    Ok(i)
}

/// Handles console input interrupts.
///
/// This function is called by `uart::handle_interrupts()` for input characters.
/// It processes special control characters for editing and process management,
/// appends input to the console buffer, and wakes up `read()` if a whole line
/// has been entered.
pub fn handle_interrupt(c: u8) {
    let mut cons = CONSOLE_BUFFER.lock();

    match c {
        // Prints process list.
        CTRL_P => proc::ops::dump(),
        // Kills line.
        CTRL_U => {
            while cons.e != cons.w && cons.buf[(cons.e - 1) % cons.buf.len()] != b'\n' {
                cons.e -= 1;
                put_backspace();
            }
        }
        // Backspace or Delete key
        CTRL_H | b'\x7f' => {
            if cons.e != cons.w {
                cons.e -= 1;
                put_backspace();
            }
        }
        _ => {
            if c != 0 && cons.e - cons.r < cons.buf.len() {
                let c = if c == b'\r' { b'\n' } else { c };

                // echo back to the user.
                put_char(c as char);

                // store for consumption by `read()`.
                let idx = cons.e % cons.buf.len();
                cons.buf[idx] = c;
                cons.e += 1;

                if c == b'\n' || c == CTRL_D || cons.e - cons.r == cons.buf.len() {
                    // wake up `read()` if a whole line (or end-of-file)
                    // has arrived.
                    cons.w = cons.e;
                    CONSOLE_BUFFER_WRITTEN.notify();
                }
            }
        }
    }
}

/// Initializes the console subsystem.
///
/// This function initializes the UART and registers the console as a device
/// for reading and writing.
pub fn init() {
    uart::init();

    file::register_device(DeviceNo::CONSOLE, Device { read, write });
}
