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
//! 0x3000_0000 -- PCIe ECAM
//! 0x4000_0000 -- PCIe MMIO
//! 0x8000_0000 -- boot ROM jumps here in machine mode
//!                kernel loads the kernel here
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
//! [hw/riscv/virt.c]: https://github.com/qemu/qemu/blob/v9.2.2/hw/riscv/virt.c

use core::arch::global_asm;

use ov6_kernel_params::{NPROC, USER_STACK_PAGES};
use ov6_syscall::{USYSCALL_ADDR, USyscallData};

use crate::memory::{PAGE_SIZE, VirtAddr};

/// Test MMIO Device
pub const VIRT_TEST: usize = 0x10_0000;

// qemu puts UART registers here in physical memory.
pub const UART0: usize = 0x1000_0000;
pub const UART0_IRQ: usize = 10;

// virtio mmio interface
pub const VIRTIO0: usize = 0x1000_1000;
pub const VIRTIO0_IRQ: usize = 1;

pub const PCIE_ECAM: usize = 0x3000_0000;
pub const PCIE_ECAM_SIZE: usize = 0x1000_0000; // 256MB

pub const PCIE_MMIO: usize = 0x4000_0000;
pub const PCIE_MMIO_SIZE: usize = 0x4000_0000; // 1GB

pub const E1000_IRQ: usize = 33;

// SiFive CLINT (Core Local Interruptor)
pub const CLINT: usize = 0x0200_0000;
pub const CLINT_SIZE: usize = 0x1_0000; // 64kB
/// Machine-mode Software Interrupt Pending
pub const fn clint_msip(hart: usize) -> usize {
    CLINT + 4 * hart
}

// qemu puts platform-level interrupt controller (PLIC) here.
pub const PLIC: usize = 0x0c00_0000;
pub const PLIC_SIZE: usize = 0x400_0000; // 4MB
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
//
// ```text
// 0x0000_0000_0100 -- text
// ...                 data, bss
// ...                 expandable heap
// ...
// 0x000f_ffff_e000 -- user stack bottom
// 0x0010_0000_0000 -- user stack top
// ...
// 0x0020_0000_0000 -- usyscall
// ...
// 0x003f_ffff_e000 -- TRAPFRAME
// 0x003f_ffff_f000 -- TRAMPOLINE
// 0x0040_0000_0000 -- VirtAddr::MAX

pub const USER_STACK_BOTTOM_ADDR: usize = USER_STACK_TOP_ADDR - USER_STACK_SIZE;
pub const USER_STACK_TOP_ADDR: usize = 0x0020_0000_0000;
pub const USER_STACK_BOTTOM: VirtAddr = match VirtAddr::new(USER_STACK_BOTTOM_ADDR) {
    Ok(va) => va,
    Err(_) => unreachable!(),
};

pub const USER_STACK_SIZE: usize = USER_STACK_PAGES * PAGE_SIZE;

pub const USYSCALL: VirtAddr = match VirtAddr::new(USYSCALL_ADDR) {
    Ok(va) => va,
    Err(_) => unreachable!(),
};
pub const USYSCALL_SIZE: usize = PAGE_SIZE;

const _: () = assert!(USYSCALL_SIZE >= size_of::<USyscallData>());

pub const TRAMPOLINE_SIZE: usize = PAGE_SIZE;
pub const TRAMPOLINE: VirtAddr = match VirtAddr::MAX.byte_sub(TRAMPOLINE_SIZE) {
    Ok(va) => va,
    Err(_) => unreachable!(),
};

pub const TRAPFRAME_SIZE: usize = PAGE_SIZE;
pub const TRAPFRAME: VirtAddr = match TRAMPOLINE.byte_sub(TRAPFRAME_SIZE) {
    Ok(va) => va,
    Err(_) => unreachable!(),
};

pub const fn kstack(p: usize) -> VirtAddr {
    assert!(p < NPROC);
    match TRAPFRAME.byte_sub((1 + (p + 1) * (KSTACK_GUARD_PAGES + KSTACK_PAGES)) * PAGE_SIZE) {
        Ok(va) => va,
        Err(_) => unreachable!(),
    }
}

pub const KSTACK_PAGES: usize = 2;
pub const KSTACK_GUARD_PAGES: usize = 1;
