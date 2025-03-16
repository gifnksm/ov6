use core::{
    cell::UnsafeCell,
    fmt, hint,
    ops::{Deref, DerefMut},
    sync::atomic::{AtomicBool, Ordering},
};

pub struct Mutex<T> {
    locked: AtomicBool,
    data: UnsafeCell<T>,
}

impl<T> Mutex<T> {
    pub const fn new(data: T) -> Self {
        Self {
            locked: AtomicBool::new(false),
            data: UnsafeCell::new(data),
        }
    }

    pub fn try_lock(&self) -> Option<MutexGuard<'_, T>> {
        if self.locked.swap(true, Ordering::Acquire) {
            return None;
        }
        Some(MutexGuard { mutex: self })
    }

    pub fn lock(&self) -> MutexGuard<'_, T> {
        while self.locked.swap(true, Ordering::Acquire) {
            hint::spin_loop();
        }
        MutexGuard { mutex: self }
    }
}

unsafe impl<T> Sync for Mutex<T> {}

impl<T> fmt::Debug for Mutex<T>
where
    T: fmt::Debug,
{
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut d = f.debug_struct("Mutex");
        match self.try_lock() {
            Some(guard) => d.field("data", &&*guard),
            None => d.field("data", &format_args!("<locked>")),
        };
        d.finish()
    }
}

pub struct MutexGuard<'lock, T> {
    mutex: &'lock Mutex<T>,
}

impl<T> Drop for MutexGuard<'_, T> {
    fn drop(&mut self) {
        self.mutex.locked.store(false, Ordering::Release);
    }
}

impl<T> Deref for MutexGuard<'_, T> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        unsafe { self.mutex.data.get().as_ref().unwrap() }
    }
}

impl<T> DerefMut for MutexGuard<'_, T> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        unsafe { self.mutex.data.get().as_mut().unwrap() }
    }
}

unsafe impl<T> Send for MutexGuard<'_, T> where T: Send {}
unsafe impl<T> Sync for MutexGuard<'_, T> where T: Sync {}
