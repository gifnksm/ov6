use core::{
    cell::UnsafeCell,
    ffi::{CStr, c_char},
    ptr,
    sync::atomic::{AtomicU32, Ordering},
};

use crate::{
    proc::{self, Proc, ProcId},
    spinlock::SpinLock,
};

mod ffi {
    use super::*;

    #[unsafe(no_mangle)]
    extern "C" fn initsleeplock(lk: *mut SleepLock, name: *const c_char) {
        unsafe { *lk = SleepLock::new(CStr::from_ptr(name)) };
    }

    #[unsafe(no_mangle)]
    extern "C" fn acquiresleep(lk: *mut SleepLock) {
        unsafe { (*lk).acquire(Proc::myproc().unwrap()) }
    }

    #[unsafe(no_mangle)]
    extern "C" fn releasesleep(lk: *mut SleepLock) {
        unsafe { (*lk).release() }
    }

    #[unsafe(no_mangle)]
    extern "C" fn holdingsleep(lk: *mut SleepLock) -> bool {
        unsafe { (*lk).holding(Proc::myproc().unwrap()) }
    }
}

#[repr(C)]
pub struct SleepLock {
    /// Is the lock held?
    locked: AtomicU32,
    /// Spinlock protecting this sleep lock
    lk: SpinLock,

    // For debugging:
    name: UnsafeCell<*const c_char>,
    pid: UnsafeCell<ProcId>,
}

impl SleepLock {
    pub const fn new(name: &'static CStr) -> Self {
        Self {
            locked: AtomicU32::new(0),
            lk: SpinLock::new(c"sleep lock"),
            name: UnsafeCell::new(name.as_ptr()),
            pid: UnsafeCell::new(ProcId::new(0)),
        }
    }

    pub fn acquire(&self, p: &mut Proc) {
        self.lk.acquire();
        while self.locked.load(Ordering::Acquire) != 0 {
            proc::sleep_raw(p, ptr::from_ref(self).cast(), &self.lk);
        }
        self.locked.store(1, Ordering::Relaxed);
        unsafe {
            *self.pid.get() = p.pid();
        }
        self.lk.release();
    }

    pub fn release(&self) {
        self.lk.acquire();
        self.locked.store(0, Ordering::Release);
        unsafe {
            *self.pid.get() = ProcId::new(0);
        }
        proc::wakeup(ptr::from_ref(self).cast());
        self.lk.release();
    }

    pub fn holding(&self, p: &Proc) -> bool {
        self.lk.acquire();
        let holding =
            self.locked.load(Ordering::Relaxed) != 0 && unsafe { *self.pid.get() } == p.pid();
        self.lk.release();
        holding
    }
}
