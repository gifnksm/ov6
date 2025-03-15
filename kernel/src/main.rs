#![feature(allocator_api)]
#![feature(box_as_ptr)]
#![feature(fn_align)]
#![feature(naked_functions)]
#![no_std]
#![no_main]

use core::{
    arch::naked_asm,
    hint,
    sync::atomic::{AtomicBool, Ordering},
};

use ov6_kernel_params as param;

extern crate alloc;

mod console;
mod cpu;
mod device;
mod error;
mod file;
mod fs;
mod interrupt;
mod memory;
mod proc;
mod start;
mod sync;
mod syscall;

#[naked]
#[unsafe(link_section = ".text.init")]
#[unsafe(export_name = "_entry")]
extern "C" fn entry() {
    unsafe {
        naked_asm!(
            // Workaround for spurious LLVM error
            // See also:
            //  - <https://github.com/rust-embedded/riscv/issues/175>
            //  - <https://github.com/rust-embedded/riscv/pull/176>
            r#".attribute arch, "rv64imac""#,

            // set up a stack for kernel.t
            // sp = STACK0 + (hartid * STACK_SIZE)
            "la sp, {stack0}",
            "li a0, {stack_size}",
            "csrr a1, mhartid",
            "addi a1, a1, 1",
            "mul a0, a0, a1",
            "add sp, sp, a0",

            // jump to start
            "call {start}",
            stack0 = sym self::start::STACK0,
            stack_size = const self::start::STACK_SIZE,
            start = sym self::start::start,
        );
    }
}

// start() jumps here in supervisor mode on all CPUs.
extern "C" fn main() -> ! {
    static STARTED: AtomicBool = AtomicBool::new(false);

    interrupt::disable();

    if cpu::id() == 0 {
        console::init();
        println!();
        println!("ov6 kernel is booting");
        println!();
        device::test::init(); // test device
        memory::page::init(); // physical page allocator
        memory::vm_kernel::init(); // create kernel page table
        memory::vm_kernel::init_hart(); // turn on paging
        interrupt::trap::init_hart(); // install kernel trap vectort
        interrupt::plic::init(); // set up interrupt controller
        interrupt::plic::init_hart(); // ask PLIC for device interrupts
        fs::init(); // file system (buffer cache and hard disk)
        file::init(); // file table
        proc::ops::spawn_init(); // first user process

        STARTED.store(true, Ordering::Release);
    } else {
        while !STARTED.load(Ordering::Acquire) {
            hint::spin_loop();
        }
        println!("hart {} starting", cpu::id());
        memory::vm_kernel::init_hart(); // turn on paging
        interrupt::trap::init_hart(); // install kernel trap vector
        interrupt::plic::init_hart(); // ask PLIC for device interrupts
    }

    proc::scheduler::schedule();
}
