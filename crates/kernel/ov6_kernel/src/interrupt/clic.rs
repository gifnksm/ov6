use core::ptr;

use crate::{cpu, memory::layout};

pub fn send_software_interrupt(hart: usize) {
    let msip = ptr::with_exposed_provenance_mut::<u32>(layout::clint_msip(hart));
    unsafe { msip.write_volatile(1) }
}

pub fn is_software_interrupt_pending() -> bool {
    let hart = cpu::id();
    let msip = ptr::with_exposed_provenance_mut::<u32>(layout::clint_msip(hart));
    unsafe { msip.read_volatile() != 0 }
}

pub fn complete_software_interrupt() {
    let hart = cpu::id();
    let msip = ptr::with_exposed_provenance_mut::<u32>(layout::clint_msip(hart));
    unsafe { msip.write_volatile(0) }
}
