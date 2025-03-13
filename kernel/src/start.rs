use core::arch::asm;

use riscv::register::{
    mcounteren,
    medeleg::{self, Medeleg},
    mepc, mhartid,
    mideleg::{self, Mideleg},
    mie, mstatus, pmpaddr0, pmpcfg0,
    satp::{self, Satp},
    sie,
};

use crate::{cpu, main, param::NCPU};

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
    unsafe {
        mepc::write(main as usize);
    }

    // disable paging for now.
    let satp = Satp::from_bits(0);
    unsafe {
        satp::write(satp);
    }

    // delegate all interrupts and exceptions to supervisor mode.
    unsafe {
        medeleg::write(Medeleg::from_bits(0xffff));
        mideleg::write(Mideleg::from_bits(0xffff));
        let mut sie = sie::read();
        sie.set_sext(true);
        sie.set_stimer(true);
        sie.set_ssoft(true);
        sie::write(sie);
    }

    // configure Physical Memory Protection to give supervisor mode
    // access to all of physical memory.
    unsafe {
        pmpaddr0::write(0x3f_ffff_ffff_ffff);
    }
    unsafe {
        pmpcfg0::write(0xf);
    }

    // ask for clock interrupts;
    timerinit();

    // keep each CPU's hartid in its tp register, for `cpu::id()`.
    let id = mhartid::read();
    unsafe {
        cpu::set_id(id);
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
        asm!("csrs menvcfg, {}", in(reg) 1_u64 << 63);
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
