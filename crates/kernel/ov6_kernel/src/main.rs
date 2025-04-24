#![feature(allocator_api)]
#![feature(box_as_ptr)]
#![feature(fn_align)]
#![feature(maybe_uninit_slice)]
#![no_std]
#![no_main]

use core::{
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
mod init;
mod interrupt;
mod memory;
mod net;
mod proc;
mod sync;
mod syscall;

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
        device::pci::init(); // PCI device driver
        net::init();

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
