//! Formatted console output

use core::{
    ffi::{CStr, c_char},
    fmt::{self, Write as _},
    hint,
    sync::atomic::{AtomicBool, Ordering},
};

use crate::{
    console,
    spinlock::{Mutex, MutexGuard},
};

pub static PANICKED: AtomicBool = AtomicBool::new(false);

mod ffi {
    use super::*;

    #[unsafe(no_mangle)]
    unsafe extern "C" fn panic(msg: *const c_char) -> ! {
        let msg = unsafe {
            CStr::from_ptr(msg)
                .to_str()
                .unwrap_or("panic message is not a valid UTF-8 string")
        };
        panic!("{msg}");
    }
}

// lock to avoid interleaving concurrent print's.
struct Print {
    locking: AtomicBool,
    lock: Mutex<()>,
}

static PRINT: Print = Print {
    locking: AtomicBool::new(true),
    lock: Mutex::new(()),
};

impl Print {
    fn lock(&self) -> Writer {
        let guard = self
            .locking
            .load(Ordering::Relaxed)
            .then(|| self.lock.lock());
        Writer { _guard: guard }
    }
}

struct Writer<'a> {
    _guard: Option<MutexGuard<'a, ()>>,
}

impl fmt::Write for Writer<'_> {
    fn write_str(&mut self, s: &str) -> fmt::Result {
        for c in s.chars() {
            console::put_char(c);
        }
        Ok(())
    }
}

pub fn _print(args: fmt::Arguments) {
    let mut writer = PRINT.lock();
    writer.write_fmt(args).unwrap();
}

#[macro_export]
macro_rules! print {
    ($($arg:tt)*) => {
        $crate::print::_print(format_args!($($arg)*))
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

#[panic_handler]
fn panic(info: &core::panic::PanicInfo) -> ! {
    PRINT.locking.store(false, Ordering::Relaxed);
    println!("panic: {info}");
    PANICKED.store(true, Ordering::Relaxed); // freeze uart output from other CPUs
    loop {
        hint::spin_loop();
    }
}
