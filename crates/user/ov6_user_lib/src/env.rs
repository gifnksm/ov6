use core::{
    ffi::{CStr, c_char},
    slice,
    sync::atomic::{AtomicPtr, AtomicUsize, Ordering},
};

use ov6_types::{os_str::OsStr, path::Path};

use crate::{error::Ov6Error, os::ov6::syscall};

pub(crate) static ARGC: AtomicUsize = AtomicUsize::new(0);
pub(crate) static ARGV: AtomicPtr<*const c_char> = AtomicPtr::new(core::ptr::null_mut());

#[cfg(all(feature = "lang_items", not(feature = "test")))]
pub(crate) fn set_args(argc: usize, argv: *const *const c_char) {
    ARGC.store(argc, Ordering::Relaxed);
    ARGV.store(argv.cast_mut(), Ordering::Relaxed);
}

fn argv() -> &'static [*const c_char] {
    let argc = ARGC.load(Ordering::Relaxed);
    let argv = ARGV.load(Ordering::Relaxed);
    if argv.is_null() {
        return &[];
    }

    unsafe { slice::from_raw_parts(argv, argc) }
}

/// Returns the first argument passed to the program (usually the program name).
///
/// # Panics
///
/// Panics if `argc` is less than 1, which should not happen as the program name
/// is always the first argument.
#[must_use]
pub fn arg0() -> &'static OsStr {
    let arg0 = argv().first().expect("argc should be greater than 1");
    let cstr = unsafe { CStr::from_ptr(*arg0) };
    OsStr::from_bytes(cstr.to_bytes())
}

#[must_use]
pub fn args() -> Args {
    let args = argv();
    let iter = args.iter();
    Args { iter }
}

#[must_use]
pub fn args_os() -> ArgsOs {
    let args = argv();
    let iter = args.iter();
    ArgsOs { iter }
}

pub struct Args {
    iter: slice::Iter<'static, *const c_char>,
}

impl Iterator for Args {
    type Item = &'static str;

    fn next(&mut self) -> Option<Self::Item> {
        self.iter
            .next()
            .map(|&arg| unsafe { CStr::from_ptr(arg).to_str().unwrap() })
    }
}

impl DoubleEndedIterator for Args {
    fn next_back(&mut self) -> Option<Self::Item> {
        self.iter
            .next_back()
            .map(|&arg| unsafe { CStr::from_ptr(arg).to_str().unwrap() })
    }
}

impl ExactSizeIterator for Args {
    fn len(&self) -> usize {
        self.iter.len()
    }
}

pub struct ArgsOs {
    iter: slice::Iter<'static, *const c_char>,
}

impl Iterator for ArgsOs {
    type Item = &'static OsStr;

    fn next(&mut self) -> Option<Self::Item> {
        self.iter.next().map(|&arg| {
            let cstr = unsafe { CStr::from_ptr(arg) };
            OsStr::from_bytes(cstr.to_bytes())
        })
    }
}

impl DoubleEndedIterator for ArgsOs {
    fn next_back(&mut self) -> Option<Self::Item> {
        self.iter.next_back().map(|&arg| {
            let cstr = unsafe { CStr::from_ptr(arg) };
            OsStr::from_bytes(cstr.to_bytes())
        })
    }
}

impl ExactSizeIterator for ArgsOs {
    fn len(&self) -> usize {
        self.iter.len()
    }
}

pub fn set_current_directory<P>(path: P) -> Result<(), Ov6Error>
where
    P: AsRef<Path>,
{
    syscall::chdir(path.as_ref())
}
