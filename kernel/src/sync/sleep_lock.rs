use core::{
    cell::UnsafeCell,
    ffi::{CStr, c_char},
    ptr,
    sync::atomic::{AtomicU32, Ordering},
};

use crate::{
    proc::{self, Proc, ProcId},
    sync::RawSpinLock,
};

#[repr(C)]
pub struct SleepLock {
    /// Is the lock held?
    locked: AtomicU32,
    /// Spinlock protecting this sleep lock
    lk: RawSpinLock,

    // For debugging:
    name: *const c_char,
    pid: UnsafeCell<ProcId>,
}

impl SleepLock {
    pub const fn new(name: &'static CStr) -> Self {
        Self {
            locked: AtomicU32::new(0),
            lk: RawSpinLock::new(),
            name: name.as_ptr(),
            pid: UnsafeCell::new(ProcId::new(0)),
        }
    }

    pub fn acquire(&self) {
        self.lk.acquire();
        while self.locked.load(Ordering::Acquire) != 0 {
            proc::sleep_raw(ptr::from_ref(self).cast(), &self.lk);
        }
        self.locked.store(1, Ordering::Relaxed);
        unsafe {
            *self.pid.get() = Proc::current().pid();
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

    pub fn holding(&self) -> bool {
        self.lk.acquire();
        let holding = self.locked.load(Ordering::Relaxed) != 0
            && unsafe { *self.pid.get() } == Proc::current().pid();
        self.lk.release();
        holding
    }
}
