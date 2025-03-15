use core::arch::asm;

use riscv::register::{mcounteren, mie, scounteren};

use crate::{
    cpu,
    sync::{SpinLock, SpinLockCondVar},
};

pub static TICKS: SpinLock<u64> = SpinLock::new(0);
pub static TICKS_UPDATED: SpinLockCondVar = SpinLockCondVar::new();

/// Ask each hart to generate timer interrupts.
pub fn init() {
    // enable supervisor-mode timer interrupts.
    unsafe {
        mie::set_stimer();
    }

    // enable the sstc extension (i.e. stimecmp).
    unsafe {
        asm!("csrs menvcfg, {}", in(reg) 1_u64 << 63);
    }

    // allow supervisor to use stimecmp and time.
    unsafe {
        mcounteren::set_tm();
    }
    // allow user to use time.
    unsafe {
        scounteren::set_tm();
    }

    // ask for the very first timer interrupt.
    unsafe {
        let time: u64;
        asm!("csrr {}, time", out(reg) time);
        asm!("csrw stimecmp, {}", in(reg) time);
    }
}

pub(super) fn handle_interrupt() {
    if cpu::id() == 0 {
        let mut ticks = TICKS.lock();
        *ticks += 1;
        TICKS_UPDATED.notify();
        drop(ticks);
    }

    // ask for the next timer interrupt. this also clears
    // the interrupt request. 1_000_000 is about a tenth
    // of a second.
    let time: usize;
    unsafe {
        asm!("csrr {}, time", out(reg) time);
        asm!("csrw stimecmp, {}", in(reg) time + 1_000_000);
    }
}
