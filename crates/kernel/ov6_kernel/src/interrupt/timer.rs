use core::{arch::asm, time::Duration};

use riscv::register::{mcounteren, mie, scounteren};

use crate::{
    cpu,
    sync::{SpinLock, SpinLockCondVar},
};

const NANOS_PER_CLOCK: u64 = 100;
const NANOS_PER_SEC: u64 = 1_000_000_000;
const TICKS_PER_SEC: u64 = 10;
const NANOS_PER_TICK: u64 = NANOS_PER_SEC / TICKS_PER_SEC;
const CLOCKS_PER_TICK: u64 = NANOS_PER_TICK / NANOS_PER_CLOCK;
pub const NANOS_PER_TICKS: u64 = NANOS_PER_SEC / TICKS_PER_SEC;

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
    let time: u64;
    unsafe {
        asm!("csrr {}, time", out(reg) time);
        asm!("csrw stimecmp, {}", in(reg) time + CLOCKS_PER_TICK );
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) struct Uptime {
    time: u64,
}

impl Uptime {
    pub(crate) const MAX: Self = Self { time: u64::MAX };

    pub(crate) fn now() -> Self {
        let time: u64;
        unsafe { asm!("csrr {}, time", out(reg) time) }
        Self { time }
    }

    pub(crate) fn checked_add(self, dur: Duration) -> Option<Self> {
        let nanos = self.time.checked_add(
            (dur.as_nanos() / u128::from(NANOS_PER_CLOCK))
                .try_into()
                .ok()?,
        )?;
        Some(Self { time: nanos })
    }

    pub(crate) fn saturating_add(self, dur: Duration) -> Self {
        self.checked_add(dur).unwrap_or(Self::MAX)
    }
}
