//! Low-level code to handle traps from user space into
//! the kernel, and returns from kernel to user.
//!
//! the kernel maps the page holding this code
//! at the same virtual address (TRAMPOLINE)
//! in user and kernel space so that it continues
//! to work when it switches page tables.
//!
//! `kernel.ld` causes  this code to start at
//! a page boundary.
use core::{arch::naked_asm, mem::offset_of};

use crate::{
    memlayout::{TRAMPOLINE, TRAPFRAME},
    proc::TrapFrame,
    vm::VirtAddr,
};

pub fn user_vec_addr() -> VirtAddr {
    TRAMPOLINE.byte_add(user_vec as usize - trampoline as usize)
}

pub fn user_ret_addr() -> VirtAddr {
    TRAMPOLINE.byte_add(user_ret as usize - trampoline as usize)
}

#[naked]
#[unsafe(link_section = "trampsec")]
pub extern "C" fn trampoline() {
    unsafe { naked_asm!("") }
}

/// Traps from user space start here,
/// in supervisor mode, but with a
/// user page table.
#[naked]
#[repr(align(4))]
#[unsafe(link_section = "trampsec")]
pub extern "C" fn user_vec() {
    unsafe {
        naked_asm!(
            // save user a0 in sscratch so
            // a0 can be used to get at TRAMPOLINE
            "csrw sscratch, a0",
            // each process has a separate p->trapframe memory area,
            // but it's mapped to the same virtual address
            // (TRAPFRAME) in every process's user page table.
            "li a0, {trapframe}",
            // save the user registers in TRAPFRAME
            "sd ra, {tf_ra}(a0)",
            "sd sp, {tf_sp}(a0)",
            "sd gp, {tf_gp}(a0)",
            "sd tp, {tf_tp}(a0)",
            "sd t0, {tf_t0}(a0)",
            "sd t1, {tf_t1}(a0)",
            "sd t2, {tf_t2}(a0)",
            "sd s0, {tf_s0}(a0)",
            "sd s1, {tf_s1}(a0)",
            // don't store a0 here
            "sd a1, {tf_a1}(a0)",
            "sd a2, {tf_a2}(a0)",
            "sd a3, {tf_a3}(a0)",
            "sd a4, {tf_a4}(a0)",
            "sd a5, {tf_a5}(a0)",
            "sd a6, {tf_a6}(a0)",
            "sd a7, {tf_a7}(a0)",
            "sd s2, {tf_s2}(a0)",
            "sd s3, {tf_s3}(a0)",
            "sd s4, {tf_s4}(a0)",
            "sd s5, {tf_s5}(a0)",
            "sd s6, {tf_s6}(a0)",
            "sd s7, {tf_s7}(a0)",
            "sd s8, {tf_s8}(a0)",
            "sd s9, {tf_s9}(a0)",
            "sd s10, {tf_s10}(a0)",
            "sd s11, {tf_s11}(a0)",
            "sd t3, {tf_t3}(a0)",
            "sd t4, {tf_t4}(a0)",
            "sd t5, {tf_t5}(a0)",
            "sd t6, {tf_t6}(a0)",
            // save the user a0 in p->trapframe->a0
            "csrr t0, sscratch",
            "sd t0, {tf_a0}(a0)",
            // initialize kernel stack pointer, from p->trapframe->kernel_sp
            "ld sp, {tf_kernel_sp}(a0)",
            // make tp hold the current hartid, from p->trapframe->kernel_hartid
            "ld tp, {tf_kernel_hartid}(a0)",
            // load the address of usertrap(), from p->trapframe->kernel_trap
            "ld t0, {tf_kernel_trap}(a0)",
            // fetch the kernel page table address, from p->trapframe->kernel_satp.
            "ld t1, {tf_kernel_satp}(a0)",
            // wait for any previous memory operations to complete, so that
            // they use the user page table.
            "sfence.vma zero, zero",
            // install the kernel page table.
            "csrw satp, t1",
            // flush now-stale user entries from the TLB.
            "sfence.vma zero, zero",
            // jump to usertrap(), which does not return
            "jr t0",
            trapframe = const TRAPFRAME.addr(),
            tf_kernel_satp = const offset_of!(TrapFrame, kernel_satp),
            tf_kernel_sp = const offset_of!(TrapFrame, kernel_sp),
            tf_kernel_trap = const offset_of!(TrapFrame, kernel_trap),
            tf_kernel_hartid = const offset_of!(TrapFrame, kernel_hartid),
            tf_ra = const offset_of!(TrapFrame, ra),
            tf_sp = const offset_of!(TrapFrame, sp),
            tf_gp = const offset_of!(TrapFrame, gp),
            tf_tp = const offset_of!(TrapFrame, tp),
            tf_t0 = const offset_of!(TrapFrame, t0),
            tf_t1 = const offset_of!(TrapFrame, t1),
            tf_t2 = const offset_of!(TrapFrame, t2),
            tf_s0 = const offset_of!(TrapFrame, s0),
            tf_s1 = const offset_of!(TrapFrame, s1),
            tf_a0 = const offset_of!(TrapFrame, a0),
            tf_a1 = const offset_of!(TrapFrame, a1),
            tf_a2 = const offset_of!(TrapFrame, a2),
            tf_a3 = const offset_of!(TrapFrame, a3),
            tf_a4 = const offset_of!(TrapFrame, a4),
            tf_a5 = const offset_of!(TrapFrame, a5),
            tf_a6 = const offset_of!(TrapFrame, a6),
            tf_a7 = const offset_of!(TrapFrame, a7),
            tf_s2 = const offset_of!(TrapFrame, s2),
            tf_s3 = const offset_of!(TrapFrame, s3),
            tf_s4 = const offset_of!(TrapFrame, s4),
            tf_s5 = const offset_of!(TrapFrame, s5),
            tf_s6 = const offset_of!(TrapFrame, s6),
            tf_s7 = const offset_of!(TrapFrame, s7),
            tf_s8 = const offset_of!(TrapFrame, s8),
            tf_s9 = const offset_of!(TrapFrame, s9),
            tf_s10 = const offset_of!(TrapFrame, s10),
            tf_s11 = const offset_of!(TrapFrame, s11),
            tf_t3 = const offset_of!(TrapFrame, t3),
            tf_t4 = const offset_of!(TrapFrame, t4),
            tf_t5 = const offset_of!(TrapFrame, t5),
            tf_t6 = const offset_of!(TrapFrame, t6),
        )
    }
}

/// Switches from kernel to user.
#[naked]
#[unsafe(link_section = "trampsec")]
pub extern "C" fn user_ret(satp: usize) {
    unsafe {
        naked_asm!(
            // a0: user page table, for satp.

            // switch to the user page table.
            "sfence.vma zero, zero",
            "csrw satp, a0",
            "sfence.vma zero, zero",
            "li a0, {trapframe}",
            // restore all but a0 from TRAPFRAME
            "ld ra, {tf_ra}(a0)",
            "ld sp, {tf_sp}(a0)",
            "ld gp, {tf_gp}(a0)",
            "ld tp, {tf_tp}(a0)",
            "ld t0, {tf_t0}(a0)",
            "ld t1, {tf_t1}(a0)",
            "ld t2, {tf_t2}(a0)",
            "ld s0, {tf_s0}(a0)",
            "ld s1, {tf_s1}(a0)",
            // don't restore a0 here
            "ld a1, {tf_a1}(a0)",
            "ld a2, {tf_a2}(a0)",
            "ld a3, {tf_a3}(a0)",
            "ld a4, {tf_a4}(a0)",
            "ld a5, {tf_a5}(a0)",
            "ld a6, {tf_a6}(a0)",
            "ld a7, {tf_a7}(a0)",
            "ld s2, {tf_s2}(a0)",
            "ld s3, {tf_s3}(a0)",
            "ld s4, {tf_s4}(a0)",
            "ld s5, {tf_s5}(a0)",
            "ld s6, {tf_s6}(a0)",
            "ld s7, {tf_s7}(a0)",
            "ld s8, {tf_s8}(a0)",
            "ld s9, {tf_s9}(a0)",
            "ld s10, {tf_s10}(a0)",
            "ld s11, {tf_s11}(a0)",
            "ld t3, {tf_t3}(a0)",
            "ld t4, {tf_t4}(a0)",
            "ld t5, {tf_t5}(a0)",
            "ld t6, {tf_t6}(a0)",
            // restore user a0
            "ld a0, {tf_a0}(a0)",
            // return to user mode and user pc.
            // usertrapret() set up sstatus and sepc.
            "sret",
            trapframe = const TRAPFRAME.addr(),
            tf_ra = const offset_of!(TrapFrame, ra),
            tf_sp = const offset_of!(TrapFrame, sp),
            tf_gp = const offset_of!(TrapFrame, gp),
            tf_tp = const offset_of!(TrapFrame, tp),
            tf_t0 = const offset_of!(TrapFrame, t0),
            tf_t1 = const offset_of!(TrapFrame, t1),
            tf_t2 = const offset_of!(TrapFrame, t2),
            tf_s0 = const offset_of!(TrapFrame, s0),
            tf_s1 = const offset_of!(TrapFrame, s1),
            tf_a0 = const offset_of!(TrapFrame, a0),
            tf_a1 = const offset_of!(TrapFrame, a1),
            tf_a2 = const offset_of!(TrapFrame, a2),
            tf_a3 = const offset_of!(TrapFrame, a3),
            tf_a4 = const offset_of!(TrapFrame, a4),
            tf_a5 = const offset_of!(TrapFrame, a5),
            tf_a6 = const offset_of!(TrapFrame, a6),
            tf_a7 = const offset_of!(TrapFrame, a7),
            tf_s2 = const offset_of!(TrapFrame, s2),
            tf_s3 = const offset_of!(TrapFrame, s3),
            tf_s4 = const offset_of!(TrapFrame, s4),
            tf_s5 = const offset_of!(TrapFrame, s5),
            tf_s6 = const offset_of!(TrapFrame, s6),
            tf_s7 = const offset_of!(TrapFrame, s7),
            tf_s8 = const offset_of!(TrapFrame, s8),
            tf_s9 = const offset_of!(TrapFrame, s9),
            tf_s10 = const offset_of!(TrapFrame, s10),
            tf_s11 = const offset_of!(TrapFrame, s11),
            tf_t3 = const offset_of!(TrapFrame, t3),
            tf_t4 = const offset_of!(TrapFrame, t4),
            tf_t5 = const offset_of!(TrapFrame, t5),
            tf_t6 = const offset_of!(TrapFrame, t6),
        )
    }
}
