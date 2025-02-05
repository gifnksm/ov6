use core::arch::asm;

use riscv::register::{mcounteren, mepc, mhartid, mie, mstatus, pmpaddr0, pmpcfg0, satp, sie};

use crate::{main, param::NCPU};

// entry.s needs one stack per CPU.
pub const STACK_SIZE: usize = 4096;
pub static mut STACK0: [u8; STACK_SIZE * NCPU] = [0; STACK_SIZE * NCPU];

// entry.s jumps here in machine mode on STACK0.
pub extern "C" fn start() -> ! {
    // set M Previous Privilege mode to Supervisor, for mret.
    unsafe {
        mstatus::set_mpp(mstatus::MPP::Supervisor);
    }

    // set M Exception Program Counter to main, for mret.
    // requires gcc -mcmodel=medany
    mepc::write(main as usize);

    // disable paging for now.
    satp::write(0);

    // delegate all interrupts and exceptions to supervisor mode.
    unsafe {
        asm!("csrw medeleg, {}", in(reg) 0xffff);
        asm!("csrw mideleg, {}", in(reg) 0xffff);
        sie::set_sext();
        sie::set_stimer();
        sie::set_ssoft();
    }

    // configure Physical Memory Protection to give supervisor mode
    // access to all of physical memory.
    pmpaddr0::write(0x3fffffffffffff);
    pmpcfg0::write(0xf);

    // ask for clock interrupts;
    timerinit();

    // keep each CPU's hartid in its tp register, for cpuid().
    let id = mhartid::read();
    unsafe {
        asm!("mv tp, {}", in(reg) id);
    }

    unsafe {
        asm!("mret", options(noreturn));
    }
}

/// Ask each hart to generate timer interrupts.
fn timerinit() {
    // enable supervisor-mode timer interrupts.
    unsafe {
        mie::set_stimer();
    }

    // enable the sstc extension (i.e. stimecmp).
    unsafe {
        asm!("csrs menvcfg, {}", in(reg) 1u64 << 63);
    }

    // allow supervisor to use stimecmp and time.
    unsafe {
        mcounteren::set_tm();
    }

    // ask for the very first timer interrupt.
    unsafe {
        let time: u64;
        asm!("csrr {}, time", out(reg) time);
        asm!("csrw stimecmp, {}", in(reg) time);
    }
}
