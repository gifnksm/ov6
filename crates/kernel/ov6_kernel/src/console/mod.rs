//! Console input and output, to the UART.
//!
//! Reads are line at a time.
//! Implements special input characters:
//!
//! * `newline` -- end of line
//! * `control-h` -- backspace
//! * `control-u` -- kill line
//! * `control-d` -- end of file
//! * `control-p` -- print process list

use crate::{
    error::KernelError,
    file::{self, Device},
    fs::DeviceNo,
    memory::{
        addr::{GenericMutSlice, GenericSlice},
        vm_user::UserPageTable,
    },
    proc,
    sync::{SpinLock, SpinLockCondVar, WaitError},
};

pub mod print;
pub mod uart;

const fn ctrl(x: u8) -> u8 {
    x - b'@'
}

const CTRL_H: u8 = ctrl(b'H');
const CTRL_U: u8 = ctrl(b'U');
const CTRL_D: u8 = ctrl(b'D');
const CTRL_P: u8 = ctrl(b'P');

/// Send one character to the UART.
///
/// Called by `println!()`, and to echo input characters,
/// but not from `write()`.
pub fn put_char(c: char) {
    uart::putc_sync(c);
}

/// Sends backspace character to the UART.
fn put_backspace() {
    uart::putc_sync('\x08');
    uart::putc_sync(' ');
    uart::putc_sync('\x08');
}

struct Cons {
    /// Input
    buf: [u8; 128],
    /// Read index
    r: usize,
    /// Write index
    w: usize,
    /// Edit index
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
/// User write()s to the console go here.
fn write(src: &GenericSlice<u8>) -> usize {
    for i in 0..src.len() {
        let mut c: [u8; 1] = [0];
        UserPageTable::copy_x2k_bytes(&mut c, &src.skip(i).take(1));
        uart::putc(c[0]);
    }
    src.len()
}

/// Reads the bytes from the console.
///
/// User read()s to the console go here.
/// Copy (up to) a whole input line to `dst`.
/// `user_dst` indicates whether `dst` is a user
/// or kernel address.
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
/// `uart::handle_interrupts()` calls this for input character.
/// Do erase/kill processing, append to `cons.buf`,
/// wake up `read()` if a whole line has arrived.
pub fn handle_interrupt(c: u8) {
    let mut cons = CONSOLE_BUFFER.lock();

    match c {
        // Print process list.
        CTRL_P => proc::ops::dump(),
        // Kill line.
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

#[expect(clippy::unnecessary_wraps)]
fn console_write(src: &GenericSlice<u8>) -> Result<usize, KernelError> {
    Ok(write(src))
}

fn console_read(dst: &mut GenericMutSlice<u8>) -> Result<usize, KernelError> {
    read(dst)
}

pub fn init() {
    uart::init();

    file::register_device(
        DeviceNo::CONSOLE,
        Device {
            read: console_read,
            write: console_write,
        },
    );
}
