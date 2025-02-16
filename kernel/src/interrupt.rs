//! Utilities for controlling interrupt enability.

use core::mem;

use riscv::register::sstatus;

use crate::proc::Cpu;

/// Enables interrupts.
pub fn enable() {
    unsafe {
        sstatus::set_sie();
    }
}

/// Disables interrupts.
pub fn disable() {
    unsafe {
        sstatus::clear_sie();
    }
}

/// Returns `true` if interrupts are enabled.
pub fn is_enabled() -> bool {
    sstatus::read().sie()
}

/// Save current interrupt enable state and disable interrupts.
pub fn push_disabled() -> Guard {
    let current = is_enabled();
    disable();

    let cpu = Cpu::mycpu();
    unsafe {
        if *cpu.noff.get() == 0 {
            *Cpu::mycpu().intena.get() = current.into();
        }
        *Cpu::mycpu().noff.get() += 1;
    }
    Guard { cpu }
}

/// Restore interrupt enable state saved by [`push_disabled()`].
pub unsafe fn pop_disabled() {
    drop(Guard { cpu: Cpu::mycpu() })
}

/// Guard that restores interrupt enable state when dropped.
pub struct Guard {
    cpu: &'static Cpu,
}

impl Drop for Guard {
    fn drop(&mut self) {
        unsafe {
            assert!(!is_enabled());
            assert!(*self.cpu.noff.get() > 0);
            *self.cpu.noff.get() -= 1;
            if *self.cpu.noff.get() == 0 && *self.cpu.intena.get() != 0 {
                enable();
            }
        }
    }
}

impl Guard {
    pub fn forget(self) {
        mem::forget(self);
    }
}

pub fn with_push_disabled<T, F>(f: F) -> T
where
    F: FnOnce() -> T,
{
    let _guard = push_disabled();
    f()
}
