//! the RISC-V Platform Level Interrupt Controller (PLIC).

use core::ptr;

use crate::{
    cpu,
    memory::layout::{PLIC, UART0_IRQ, VIRTIO0_IRQ, plic_sclaim, plic_senable, plic_spriority},
};

pub fn init() {
    // set desired IRQ priorities non-zero (otherwise disabled).
    unsafe {
        ptr::with_exposed_provenance_mut::<u32>(PLIC + UART0_IRQ * 4).write_volatile(1);
        ptr::with_exposed_provenance_mut::<u32>(PLIC + VIRTIO0_IRQ * 4).write_volatile(1);

        // PCIe IRQs are 32 to 35
        for irq in 1..0x35 {
            ptr::with_exposed_provenance_mut::<u32>(PLIC + irq * 4).write_volatile(1);
        }
    }
}

pub fn init_hart() {
    let hart = cpu::id();

    // set enable bits for this hart's S-mode
    // for the uart and virtio disk.
    unsafe {
        ptr::with_exposed_provenance_mut::<u32>(plic_senable(hart))
            .write_volatile((1 << UART0_IRQ) | (1 << VIRTIO0_IRQ));
    }

    // hack to get at next 32 IRQs for E1000
    unsafe {
        ptr::with_exposed_provenance_mut::<u32>(plic_senable(hart) + 4).write_volatile(0xffff_ffff);
    }

    // set this hart's S-mode priority threshold to 0
    unsafe {
        ptr::with_exposed_provenance_mut::<u32>(plic_spriority(hart)).write_volatile(0);
    }
}

/// Asks the PLIC what interrupt we should serve.
pub fn claim() -> u32 {
    let hart = cpu::id();
    unsafe { ptr::with_exposed_provenance_mut::<u32>(plic_sclaim(hart)).read_volatile() }
}

/// Tells the PLIC we've served this IRQ.
pub fn complete(irq: u32) {
    let hart = cpu::id();
    unsafe {
        ptr::with_exposed_provenance_mut::<u32>(plic_sclaim(hart)).write_volatile(irq);
    }
}
