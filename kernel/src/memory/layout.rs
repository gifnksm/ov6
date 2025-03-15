//! Physical memory layout
//!
//! qemu -machine virt is set up like this,
//! based on qemu's [hw/riscv/virt.c]:
//!
//! ```text
//! 0x0000_1000 -- boot ROM, provided by qemu
//! 0x0010_0000 -- VIRT_TEST
//! 0x0200_0000 -- CLINT
//! 0x0c00_0000 -- PLIC
//! 0x1000_0000 -- UART0
//! 0x1000_1000 -- virtio disk
//! 0x8000_0000 -- boot ROM jumps here in machine mode
//!               -kernel loads the kernel here
//! unused RAM after 0x8000_0000.
//! ```
//!
//! the kernel uses physical memory thus:
//!
//! ```text
//! 0x8000_0000 -- KERNEL_BASE. start of kernel text
//! TEXT_END    -- start of kernel data
//! KERNEL_END  -- start of kernel page allocation area
//! PHYS_TOP    -- end RAM used by the kernel
//! ```
//!
//! [hw/riscv/virt.c]: https://github.com/qemu/qemu/blob/9.2.0/hw/riscv/virt.c

use core::arch::global_asm;

use ov6_kernel_params::NPROC;

use crate::memory::{PAGE_SIZE, VirtAddr};

/// Test MMIO Device
pub const VIRT_TEST: usize = 0x10_0000;

// qemu puts UART registers here in physical memory.
pub const UART0: usize = 0x1000_0000;
pub const UART0_IRQ: usize = 10;

// virtio mmio interface
pub const VIRTIO0: usize = 0x1000_1000;
pub const VIRTIO0_IRQ: usize = 1;

// qemu puts platform-level interrupt controller (PLIC) here.
pub const PLIC: usize = 0x0c00_0000;
// pub const PLIC_PRIORITY: usize = PLIC + 0x0;
// pub const PLIC_PENDING: usize = PLIC + 0x1000;
pub const fn plic_senable(hart: usize) -> usize {
    PLIC + 0x2080 + hart * 0x100
}
pub const fn plic_spriority(hart: usize) -> usize {
    PLIC + 0x20_1000 + hart * 0x2000
}
pub const fn plic_sclaim(hart: usize) -> usize {
    PLIC + 0x20_1004 + hart * 0x2000
}

// get linker symbol addresses
global_asm!(
    "
        .global _ov6_kernel_base_addr
        _ov6_kernel_base_addr: .dword _ov6_kernel_base
        .global _ov6_text_end_addr
        _ov6_text_end_addr: .dword _ov6_text_end
        .global _ov6_kernel_end_addr
        _ov6_kernel_end_addr: .dword _ov6_kernel_end
        .global _ov6_phys_top_addr
        _ov6_phys_top_addr: .dword _ov6_phys_top
    "
);

unsafe extern "C" {
    // the kernel expects there to be RAM
    // for use by the kernel and user pages
    // from physical address 0x80000000 to PHYSTOP.
    #[link_name = "_ov6_kernel_base_addr"]
    pub(super) static KERNEL_BASE: usize;

    /// Address of the end of kernel code.
    #[link_name = "_ov6_text_end_addr"]
    pub(super) static TEXT_END: usize;

    /// Address of the end of kernel code.
    #[link_name = "_ov6_kernel_end_addr"]
    pub(super) static KERNEL_END: usize;

    #[link_name = "_ov6_phys_top_addr"]
    pub(super) static PHYS_TOP: usize;
}

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

pub const TRAPFRAME: VirtAddr = TRAMPOLINE.byte_sub(PAGE_SIZE);

pub const fn kstack(p: usize) -> VirtAddr {
    assert!(p < NPROC);
    TRAPFRAME.byte_sub((1 + (p + 1) * (KSTACK_GUARD_PAGES + KSTACK_PAGES)) * PAGE_SIZE)
}

pub const KSTACK_PAGES: usize = 2;
pub const KSTACK_GUARD_PAGES: usize = 1;
