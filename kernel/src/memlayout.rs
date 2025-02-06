//! Physical memory layout
//!
//! qemu -machine virt is set up like this,
//! based on qemu's hw/riscv/virt.c:
//!
//! ```text
//! 00001000 -- boot ROM, provided by qemu
//! 02000000 -- CLINT
//! 0C000000 -- PLIC
//! 10000000 -- uart0
//! 10001000 -- virtio disk
//! 80000000 -- boot ROM jumps here in machine mode
//!             -kernel loads the kernel here
//! unused RAM after 80000000.
//! ```
//!
//! the kernel uses physical memory thus:
//!
//! ```text
//! 80000000 -- entry.S, then kernel text and data
//! end -- start of kernel page allocation area
//! PHYSTOP -- end RAM used by the kernel
//! ```

use crate::vm::{PAGE_SIZE, VirtAddr};

// qemu puts UART registers here in physical memory.
pub const UART0: usize = 0x1000_0000;
// pub const UART0_IRQ: usize = 10;

// virtio mmio interface
pub const VIRTIO0: usize = 0x1000_1000;
// pub const VIRTIO0_IRQ: usize = 1;

// qemu puts platform-level interrupt controller (PLIC) here.
pub const PLIC: usize = 0x0c00_0000;
// pub const PLIC_PRIORITY: usize = PLIC + 0x0;
// pub const PLIC_PENDING: usize = PLIC + 0x1000;
// pub const fn plic_senable(hart: usize) -> usize {
//     PLIC + 0x2080 + hart * 0x100
// }
// pub const fn plic_spriority(hart: usize) -> usize {
//     PLIC + 0x20_1000 + hart * 0x2000
// }
// pub const fn plic_sclain(hart: usize) -> usize {
//     PLIC + 0x20_1004 + hart * 0x2000
// }

// the kernel expects there to be RAM
// for use by the kernel and user pages
// from physical address 0x80000000 to PHYSTOP.
pub const KERN_BASE: usize = 0x8000_0000;
pub const PHYS_TOP: usize = KERN_BASE + 128 * 1024 * 1024;

// User memory layout.
// ```text
// Address zero first:
//  text
//  original data and bss
//  fixed-size stack
//  expandable heap
//  ...
//  TRAPFRAME (p.trapframe, used by the trampoline)
//  TRAMPOLINE
// ```

pub const TRAMPOLINE: VirtAddr = VirtAddr::MAX.byte_sub(PAGE_SIZE);
