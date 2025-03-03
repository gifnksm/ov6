use core::{
    cell::UnsafeCell,
    ops::{Deref, DerefMut},
    ptr,
};

use mutex_api::Mutex;

use crate::{
    cpu::Cpu,
    error::Error,
    proc::{self, ProcId},
};

use super::SpinLock;

struct RawSleepLock {
    locked: SpinLock<(bool, ProcId)>,
}

impl Default for RawSleepLock {
    fn default() -> Self {
        Self::new()
    }
}

impl RawSleepLock {
    const fn new() -> Self {
        Self {
            locked: SpinLock::new((false, ProcId::new(0))),
        }
    }

    fn try_acquire(&self) -> Result<(), Error> {
        let mut locked = self.locked.try_lock()?;
        if locked.0 {
            return Err(Error::Unknown);
        }

        locked.0 = true;
        locked.1 = Cpu::current().pid();
        Ok(())
    }

    fn acquire(&self) {
        let mut locked = self.locked.lock();
        while locked.0 {
            locked = proc::sleep(ptr::from_ref(self).cast(), locked);
        }
        locked.0 = true;
        locked.1 = Cpu::current().pid();
    }

    fn release(&self) {
        let mut locked = self.locked.lock();
        locked.0 = false;
        locked.1 = ProcId::new(0);
        proc::wakeup(ptr::from_ref(self).cast());
        drop(locked);
    }

    // fn holding(&self) -> bool {
    //     let mut locked = self.locked.lock();
    //     let holding = locked.0 && locked.1 == unsafe { Cpu::current().pid() };
    //     holding
    // }
}

#[derive(Default)]
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

    pub fn try_lock(&self) -> Result<SleepLockGuard<T>, Error> {
        self.lock.try_acquire()?;
        Ok(SleepLockGuard { lock: self })
    }

    /// Acquires the lock.
    ///
    /// Sleeps (spins) until the lock is acquired.
    pub fn lock(&self) -> SleepLockGuard<T> {
        self.lock.acquire();
        SleepLockGuard { lock: self }
    }
}

impl<T> Mutex for SleepLock<T> {
    type Data = T;
    type Guard<'a>
        = SleepLockGuard<'a, T>
    where
        T: 'a;

    fn new(data: Self::Data) -> Self {
        Self::new(data)
    }

    fn lock(&self) -> Self::Guard<'_> {
        self.lock()
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
