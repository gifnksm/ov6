#![feature(allocator_api)]
#![feature(fn_align)]
#![feature(naked_functions)]
#![no_std]
#![no_main]

use core::{
    arch::global_asm,
    hint,
    sync::atomic::{AtomicBool, Ordering},
};

use xv6_kernel_params as param;

extern crate alloc;

mod console;
mod cpu;
mod error;
mod file;
mod fs;
mod interrupt;
mod memory;
mod proc;
mod start;
mod sync;
mod syscall;

global_asm!(
    include_str!("entry.s"),
    STACK0 = sym self::start::STACK0,
    STACK_SIZE = const self::start::STACK_SIZE,
    start = sym self::start::start,
);

static STARTED: AtomicBool = AtomicBool::new(false);

// start() jumps here in supervisor mode on all CPUs.
extern "C" fn main() -> ! {
    if cpu::id() == 0 {
        console::init();
        println!();
        println!("xv6 kernel is booting");
        println!();
        memory::page::init(); // physical page allocator
        memory::vm::kernel::init(); // create kernel page table
        memory::vm::kernel::init_hart(); // turn on paging
        proc::init(); // process table
        interrupt::trap::init_hart(); // install kernel trap vectort
        interrupt::plic::init(); // set up interrupt controller
        interrupt::plic::init_hart(); // ask PLIC for device interrupts
        fs::init(); // file system (buffer cache and hard disk)
        file::init(); // file table
        proc::user_init(); // first user process

        STARTED.store(true, Ordering::Release);
    } else {
        while !STARTED.load(Ordering::Acquire) {
            hint::spin_loop();
        }
        println!("hart {} starting", cpu::id());
        memory::vm::kernel::init_hart(); // turn on paging
        interrupt::trap::init_hart(); // install kernel trap vector
        interrupt::plic::init_hart(); // ask PLIC for device interrupts
    }

    proc::scheduler();
}
