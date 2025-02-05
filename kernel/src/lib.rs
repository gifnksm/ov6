#![feature(c_variadic)]
#![feature(extern_types)]
#![no_std]

use core::{
    hint,
    sync::atomic::{AtomicBool, Ordering},
};

mod console;
mod entry;
mod file;
mod kalloc;
mod memlayout;
mod print;
mod proc;
mod spinlock;
mod uart;

unsafe extern "C" {
    fn kvminit();
    fn kvminithart();
    fn trapinit();
    fn trapinithart();
    fn plicinit();
    fn plicinithart();
    fn binit();
    fn iinit();
    fn fileinit();
    fn virtio_disk_init();
    fn userinit();
}

static STARTED: AtomicBool = AtomicBool::new(false);

// start() jumps here in supervisor mode on all CPUs.
#[unsafe(no_mangle)]
extern "C" fn main() -> ! {
    if proc::cpuid() == 0 {
        console::init();
        println!();
        println!("xv6 kernel is booting");
        println!();
        kalloc::init(); // physical page allocator
        unsafe {
            kvminit(); // create kernel page table
            kvminithart(); // turn on paging
            proc::init(); // process table
            trapinit(); // trap vectors
            trapinithart(); // install kernel trap vector
            plicinit(); // set up interrupt controller
            plicinithart(); // ask PLIC for device interrupts
            binit(); // buffer cache
            iinit(); // inode table
            fileinit(); // file table
            virtio_disk_init(); // emulated hard disk
            userinit(); // first user process
            STARTED.store(true, Ordering::Release);
        }
    } else {
        while !STARTED.load(Ordering::Acquire) {
            hint::spin_loop();
        }
        println!("hart {} starting", proc::cpuid());
        unsafe {
            kvminithart(); // turn on paging
            trapinithart(); // install kernel trap vector
            plicinithart(); // ask PLIC for device interrupts
        }
    }

    proc::scheduler();
}
