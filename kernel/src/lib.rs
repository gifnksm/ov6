#![feature(c_variadic)]
#![feature(extern_types)]
#![feature(fn_align)]
#![feature(naked_functions)]
#![feature(non_null_from_ref)]
#![no_std]

use core::{
    arch::global_asm,
    hint,
    sync::atomic::{AtomicBool, Ordering},
};

mod bio;
mod console;
mod elf;
mod exec;
mod fcntl;
mod file;
mod fs;
mod interrupt;
mod kalloc;
mod kernel_vec;
mod log;
mod memlayout;
mod param;
mod pipe;
mod plic;
mod print;
mod proc;
mod sleeplock;
mod spinlock;
mod start;
mod stat;
mod switch;
mod syscall;
mod syscall_file;
mod syscall_proc;
mod trampoline;
mod trap;
mod uart;
mod virtio;
mod virtio_disk;
mod vm;

global_asm!(
    include_str!("entry.s"),
    STACK0 = sym self::start::STACK0,
    STACK_SIZE = const self::start::STACK_SIZE,
    start = sym self::start::start,
);

static STARTED: AtomicBool = AtomicBool::new(false);

// start() jumps here in supervisor mode on all CPUs.
extern "C" fn main() -> ! {
    if proc::cpuid() == 0 {
        console::init();
        println!();
        println!("xv6 kernel is booting");
        println!();
        kalloc::init(); // physical page allocator
        vm::kernel::init(); // create kernel page table
        vm::kernel::init_hart(); // turn on paging
        proc::init(); // process table
        trap::init_hart(); // install kernel trap vectort
        plic::init(); // set up interrupt controller
        plic::init_hart(); // ask PLIC for device interrupts
        bio::init(); // buffer cache
        virtio_disk::init(); // emulated hard disk
        proc::user_init(); // first user process
        STARTED.store(true, Ordering::Release);
    } else {
        while !STARTED.load(Ordering::Acquire) {
            hint::spin_loop();
        }
        println!("hart {} starting", proc::cpuid());
        vm::kernel::init_hart(); // turn on paging
        trap::init_hart(); // install kernel trap vector
        plic::init_hart(); // ask PLIC for device interrupts
    }

    proc::scheduler();
}
