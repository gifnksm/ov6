use core::{
    ffi::{CStr, c_char},
    slice,
    sync::atomic::{AtomicPtr, AtomicUsize, Ordering},
};

use ov6_types::path::Path;

use crate::{error::Ov6Error, os::ov6::syscall};

pub(crate) static ARGC: AtomicUsize = AtomicUsize::new(0);
pub(crate) static ARGV: AtomicPtr<*const c_char> = AtomicPtr::new(core::ptr::null_mut());

#[cfg(not(feature = "std"))]
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

#[must_use]
pub fn arg0() -> &'static str {
    let arg0 = argv().first().expect("argc should be greater than 1");
    unsafe { CStr::from_ptr(*arg0).to_str().unwrap() }
}

#[must_use]
pub fn args() -> Args {
    let args = argv();
    let mut iter = args.iter();
    iter.next(); // Skip the program name
    Args { iter }
}

#[must_use]
pub fn args_cstr() -> ArgsCStr {
    let args = argv();
    let mut iter = args.iter();
    iter.next(); // Skip the program name
    ArgsCStr { iter }
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

impl ExactSizeIterator for Args {
    fn len(&self) -> usize {
        self.iter.len()
    }
}

pub struct ArgsCStr {
    iter: slice::Iter<'static, *const c_char>,
}

impl Iterator for ArgsCStr {
    type Item = &'static CStr;

    fn next(&mut self) -> Option<Self::Item> {
        self.iter.next().map(|&arg| unsafe { CStr::from_ptr(arg) })
    }
}

impl ExactSizeIterator for ArgsCStr {
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
