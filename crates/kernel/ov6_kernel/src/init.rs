use core::arch::{asm, naked_asm};

use riscv::register::{
    medeleg::{self, Medeleg},
    mepc, mhartid,
    mideleg::{self, Mideleg},
    mstatus, pmpaddr0, pmpcfg0,
    satp::{self, Satp},
    sie,
};

use crate::{cpu, interrupt::timer, param::NCPU};

// entry.s needs one stack per CPU.
const KERNEL_STACK_SIZE: usize = 4096;

#[repr(align(4096))]
struct KernelStack {
    _data: [u8; KERNEL_STACK_SIZE],
}

static mut KERNEL_STACK: [KernelStack; NCPU] = [const {
    KernelStack {
        _data: [0; KERNEL_STACK_SIZE],
    }
}; NCPU];

#[naked]
#[unsafe(link_section = ".text.init")]
#[unsafe(export_name = "boot")]
extern "C" fn boot() {
    unsafe {
        naked_asm!(
            // Workaround for spurious LLVM error
            // See also:
            //  - <https://github.com/rust-embedded/riscv/issues/175>
            //  - <https://github.com/rust-embedded/riscv/pull/176>
            r#".attribute arch, "rv64imac""#,

            // set up a stack for kernel.
            // sp = kernel_stack + ((hartid + 1) * stack_size)
            "la sp, {kernel_stack}",
            "li a0, {stack_size}",
            "csrr a1, mhartid",
            "addi a1, a1, 1",
            "mul a0, a0, a1",
            "add sp, sp, a0",

            // jump to init
            "call {init}",
            kernel_stack = sym KERNEL_STACK,
            stack_size = const size_of::<KernelStack>(),
            init = sym init,
        );
    }
}

extern "C" fn init() -> ! {
    // set M Previous Privilege mode to Supervisor, for mret.
    unsafe {
        mstatus::set_mpp(mstatus::MPP::Supervisor);
    }

    // set M Exception Program Counter to main, for mret.
    // requires gcc -mcmodel=medany
    unsafe {
        mepc::write(entry as usize);
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
    timer::init();

    // keep each CPU's hartid in its tp register, for `cpu::id()`.
    let id = mhartid::read();
    unsafe {
        cpu::set_id(id);
    }

    unsafe {
        asm!("mret", options(noreturn));
    }
}

#[naked]
extern "C" fn entry() -> ! {
    unsafe {
        naked_asm!(
            // set up stack for kernel
            // sp = kernel_stack + ((tp + 1) * stack_size)
            // where tp = mhartid
            "la sp, {kernel_stack}",
            "li a0, {stack_size}",
            "addi a1, tp, 1",
            "mul a0, a0, a1",
            "add sp, sp, a0",

            // set up stack frame
            "mv fp, sp",
            "addi sp, sp, -16",
            "sd zero, 0(sp)",
            "sd zero, 8(sp)",

            // jump to main
            "call {main}",
            kernel_stack = sym KERNEL_STACK,
            stack_size = const size_of::<KernelStack>(),
            main = sym crate::main,
        );
    }
}
