//! Formatted console output

use core::{
    fmt::{self, Write as _},
    hint,
    sync::atomic::{AtomicBool, Ordering},
};

use crate::{
    console,
    sync::{SpinLock, SpinLockGuard},
};

pub static PANICKED: AtomicBool = AtomicBool::new(false);

// lock to avoid interleaving concurrent print's.
struct Print {
    locking: AtomicBool,
    lock: SpinLock<()>,
}

static PRINT: Print = Print {
    locking: AtomicBool::new(true),
    lock: SpinLock::new(()),
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
    _guard: Option<SpinLockGuard<'a, ()>>,
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
