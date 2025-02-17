use core::{
    cell::UnsafeCell,
    ops::{Deref, DerefMut},
    ptr,
    sync::atomic::{AtomicBool, Ordering},
};

use crate::{
    proc::{self, Proc, ProcId},
    sync::RawSpinLock,
};

pub struct RawSleepLock {
    /// Is the lock held?
    locked: AtomicBool,
    /// Spinlock protecting this sleep lock
    lk: RawSpinLock,

    pid: UnsafeCell<ProcId>,
}

impl RawSleepLock {
    pub const fn new() -> Self {
        Self {
            locked: AtomicBool::new(false),
            lk: RawSpinLock::new(),
            pid: UnsafeCell::new(ProcId::new(0)),
        }
    }

    pub fn acquire(&self) {
        self.lk.acquire();
        while self.locked.load(Ordering::Acquire) {
            proc::sleep_raw(ptr::from_ref(self).cast(), &self.lk);
        }
        self.locked.store(true, Ordering::Relaxed);
        unsafe {
            *self.pid.get() = Proc::current().pid();
        }
        self.lk.release();
    }

    pub fn release(&self) {
        self.lk.acquire();
        self.locked.store(false, Ordering::Release);
        unsafe {
            *self.pid.get() = ProcId::new(0);
        }
        proc::wakeup(ptr::from_ref(self).cast());
        self.lk.release();
    }

    pub fn holding(&self) -> bool {
        self.lk.acquire();
        let holding = self.locked.load(Ordering::Relaxed)
            && unsafe { *self.pid.get() } == Proc::current().pid();
        self.lk.release();
        holding
    }
}

pub struct SleepLock<T> {
    lock: RawSleepLock,
    value: UnsafeCell<T>,
}

unsafe impl<T> Sync for SleepLock<T> where T: Send {}

impl<T> SleepLock<T> {
    pub const fn new(value: T) -> Self {
        Self {
            lock: RawSleepLock::new(),
            value: UnsafeCell::new(value),
        }
    }

    /// Acquires the lock.
    ///
    /// Sleeps (spins) until the lock is acquired.
    pub fn lock(&self) -> SleepLockGuard<T> {
        self.lock.acquire();
        SleepLockGuard { lock: self }
    }
}

pub struct SleepLockGuard<'a, T> {
    lock: &'a SleepLock<T>,
}

unsafe impl<T> Send for SleepLockGuard<'_, T> where T: Send {}
unsafe impl<T> Sync for SleepLockGuard<'_, T> where T: Sync {}

impl<T> Drop for SleepLockGuard<'_, T> {
    fn drop(&mut self) {
        self.lock.lock.release();
    }
}

impl<T> Deref for SleepLockGuard<'_, T> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        unsafe { &*self.lock.value.get() }
    }
}

impl<T> DerefMut for SleepLockGuard<'_, T> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        unsafe { &mut *self.lock.value.get() }
    }
}
