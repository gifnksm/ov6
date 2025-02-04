//! Formatted console output

use core::{
    ffi::{CStr, c_char, c_int, c_long, c_longlong, c_uint, c_ulong, c_ulonglong, c_void},
    fmt::{self, Write as _},
    sync::atomic::{AtomicBool, Ordering},
};

use crate::{
    console,
    spinlock::{Mutex, MutexGuard},
};

#[allow(non_upper_case_globals)]
#[unsafe(no_mangle)]
static mut panicked: i32 = 0;

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
    unsafe {
        panicked = 1; // freeze uart output from other CPUs
    }
    loop {}
}

/// Print to the console.
#[unsafe(no_mangle)]
unsafe extern "C" fn printf(fmt: *const c_char, mut args: ...) -> c_int {
    let mut fmt = unsafe {
        CStr::from_ptr(fmt)
            .to_str()
            .unwrap_or("printf format is not a valid UTF-8 string")
    }
    .chars();

    let mut writer = PRINT.lock();
    loop {
        let Some(c) = fmt.next() else {
            return 0;
        };

        if c != '%' {
            console::put_char(c);
            continue;
        }

        if let Some(rest) = fmt.as_str().strip_prefix("d") {
            let arg = unsafe { args.arg::<c_int>() };
            write!(&mut writer, "{arg}").unwrap();
            fmt = rest.chars();
            continue;
        }

        if let Some(rest) = fmt.as_str().strip_prefix("ld") {
            let arg = unsafe { args.arg::<c_long>() };
            write!(&mut writer, "{arg}").unwrap();
            fmt = rest.chars();
            continue;
        }

        if let Some(rest) = fmt.as_str().strip_prefix("lld") {
            let arg = unsafe { args.arg::<c_longlong>() };
            write!(&mut writer, "{arg}").unwrap();
            fmt = rest.chars();
            continue;
        }

        if let Some(rest) = fmt.as_str().strip_prefix("u") {
            let arg = unsafe { args.arg::<c_uint>() };
            write!(&mut writer, "{arg}").unwrap();
            fmt = rest.chars();
            continue;
        }

        if let Some(rest) = fmt.as_str().strip_prefix("lu") {
            let arg = unsafe { args.arg::<c_ulong>() };
            write!(&mut writer, "{arg}").unwrap();
            fmt = rest.chars();
            continue;
        }

        if let Some(rest) = fmt.as_str().strip_prefix("llu") {
            let arg = unsafe { args.arg::<c_ulonglong>() };
            write!(&mut writer, "{arg}").unwrap();
            fmt = rest.chars();
            continue;
        }

        if let Some(rest) = fmt.as_str().strip_prefix("x") {
            let arg = unsafe { args.arg::<c_uint>() };
            write!(&mut writer, "{arg:x}").unwrap();
            fmt = rest.chars();
            continue;
        }

        if let Some(rest) = fmt.as_str().strip_prefix("lx") {
            let arg = unsafe { args.arg::<c_ulong>() };
            write!(&mut writer, "{arg:x}").unwrap();
            fmt = rest.chars();
            continue;
        }

        if let Some(rest) = fmt.as_str().strip_prefix("llx") {
            let arg = unsafe { args.arg::<c_ulonglong>() };
            write!(&mut writer, "{arg:x}").unwrap();
            fmt = rest.chars();
            continue;
        }

        if let Some(rest) = fmt.as_str().strip_prefix("p") {
            let arg = unsafe { args.arg::<*const c_void>() };
            write!(&mut writer, "{arg:p}").unwrap();
            fmt = rest.chars();
            continue;
        }

        if let Some(rest) = fmt.as_str().strip_prefix("s") {
            let arg = unsafe { args.arg::<*const c_char>() };
            if arg.is_null() {
                write!(&mut writer, "(null)").unwrap();
            } else {
                let str = unsafe { CStr::from_ptr(arg).to_str().unwrap() };
                write!(&mut writer, "{str}").unwrap();
            }
            fmt = rest.chars();
            continue;
        }

        if let Some(rest) = fmt.as_str().strip_prefix('%') {
            write!(&mut writer, "%").unwrap();
            fmt = rest.chars();
            continue;
        }

        // Print unknown % sequence to draw attention.
        write!(&mut writer, "%").unwrap();
    }
}
