mod sleep_lock;
mod spin_lock;

pub use self::{
    sleep_lock::SleepLock,
    spin_lock::{RawSpinLock, SpinLock, SpinLockGuard},
};
