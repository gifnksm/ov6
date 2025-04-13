use core::ptr;

use super::e1000;
use crate::memory::layout::{PCIE_ECAM, PCIE_MMIO};

pub fn init() {
    let e1000_regs = ptr::with_exposed_provenance_mut::<u32>(PCIE_MMIO);
    let ecam = ptr::with_exposed_provenance_mut::<u32>(PCIE_ECAM);

    for dev in 0..32 {
        let bus = 0;
        let func = 0;
        let offset = 0;
        let off = (bus << 16) | (dev << 11) | (func << 8) | offset;
        let base = ecam.wrapping_add(off);
        let id = unsafe { base.read_volatile() };

        // 100e:8086 is an Intel 82540EM Gigabit Ethernet Controller
        if id == 0x100e_8086 {
            // command and status register
            // bit 0 : I/O access enable
            // bit 1 : memory access enable
            // bit 2 : bus master enable
            unsafe {
                base.wrapping_add(1).write_volatile(7);
            }

            for i in 0..6 {
                let bar = base.wrapping_add(4 + i);
                let old = unsafe { bar.read_volatile() };

                // writing all 1't to the BAR causes it to be replaced with its size.
                unsafe {
                    bar.write_volatile(u32::MAX);
                }

                unsafe {
                    bar.write_volatile(old);
                }
            }

            // tell the e1000 to reveal its registers at physical address 0x4000_0000
            unsafe {
                base.wrapping_add(4)
                    .write_volatile(u32::try_from(e1000_regs.addr()).unwrap());
            }

            unsafe {
                e1000::init(e1000_regs);
            }
        }
    }
}
