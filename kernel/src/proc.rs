use core::{
    ffi::{c_int, c_void},
    ptr,
};

mod ffi {
    use crate::spinlock::SpinLock;

    use super::*;

    unsafe extern "C" {
        pub type Proc;
        pub fn cpuid() -> c_int;
        pub fn mycpu() -> *mut Cpu;
        pub fn either_copyin(dst: *mut c_void, user_src: c_int, src: u64, len: u64) -> c_int;
        pub fn either_copyout(user_dst: c_int, dst: u64, src: *const c_void, len: u64) -> c_int;
        pub fn myproc() -> *mut Proc;
        pub fn sleep(chan: *const c_void, lk: *mut SpinLock);
        pub fn wakeup(chan: *const c_void);
        pub fn killed(p: *mut Proc) -> c_int;
        pub fn procdump();
        pub fn procinit();
        pub fn scheduler() -> !;
    }
}

pub use ffi::Proc;

use crate::spinlock::MutexGuard;

impl Proc {
    pub fn myproc() -> *mut Self {
        unsafe { ffi::myproc() }
    }

    pub fn killed(&self) -> bool {
        unsafe { ffi::killed(self as *const _ as *mut _) != 0 }
    }
}

/// Saved registers for kernel context switches.
#[repr(C)]
struct Context {
    ra: u64,
    sp: u64,

    // callee-saved
    s0: u64,
    s1: u64,
    s2: u64,
    s3: u64,
    s4: u64,
    s5: u64,
    s6: u64,
    s7: u64,
    s8: u64,
    s9: u64,
    s10: u64,
    s11: u64,
}

/// Per-CPU state.
#[repr(C)]
pub struct Cpu {
    /// The process running on thie Cpu, or null.
    proc: *mut ffi::Proc,
    /// swtch() here to enter scheduler()
    context: Context,
    /// Depth of `push_off()` nesting.
    pub noff: c_int,
    /// Were interrupts enabled before `push_off()`?
    pub intena: c_int,
}

impl Cpu {
    #[inline]
    pub fn mycpu() -> *mut Self {
        unsafe { ffi::mycpu() }
    }
}

pub fn cpuid() -> i32 {
    unsafe { ffi::cpuid() }
}

pub unsafe fn either_copyin(
    dst: *mut u8,
    user_src: bool,
    src: usize,
    len: usize,
) -> Result<(), ()> {
    if unsafe { ffi::either_copyin(dst.cast(), user_src as c_int, src as u64, len as u64) } < 0 {
        return Err(());
    }
    Ok(())
}

pub fn either_copyout(user_dst: bool, dst: usize, src: *const u8, len: usize) -> Result<(), ()> {
    if unsafe { ffi::either_copyout(user_dst.into(), dst as u64, src.cast(), len as u64) } < 0 {
        return Err(());
    }
    Ok(())
}

pub fn sleep<T>(chan: *const c_void, lock: &mut MutexGuard<T>) {
    unsafe { ffi::sleep(chan, ptr::from_ref(lock.spinlock()).cast_mut()) }
}

pub fn wakeup(chan: *const c_void) {
    unsafe { ffi::wakeup(chan) }
}

pub fn dump() {
    unsafe { ffi::procdump() }
}

pub fn init() {
    unsafe { ffi::procinit() }
}

pub fn scheduler() -> ! {
    unsafe { ffi::scheduler() }
}
