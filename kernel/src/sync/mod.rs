mod sleep_lock;
mod spin_lock;

pub use self::{
    sleep_lock::{RawSleepLock, SleepLock, SleepLockGuard},
    spin_lock::{RawSpinLock, SpinLock, SpinLockCondVar, SpinLockGuard},
};
