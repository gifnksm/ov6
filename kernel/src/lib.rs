#![feature(c_variadic)]
#![feature(extern_types)]
#![feature(non_null_from_ref)]
#![no_std]

use core::{
    arch::global_asm,
    hint,
    sync::atomic::{AtomicBool, Ordering},
};

mod console;
mod file;
mod fs;
mod kalloc;
mod log;
mod memlayout;
mod param;
mod print;
mod proc;
mod spinlock;
mod start;
mod trap;
mod uart;
mod vm;

global_asm!(
    include_str!("entry.s"),
    STACK0 = sym self::start::STACK0,
    STACK_SIZE = const self::start::STACK_SIZE,
    start = sym self::start::start,
);

unsafe extern "C" {
    fn trapinit();
    fn trapinithart();
    fn plicinit();
    fn plicinithart();
    fn binit();
    fn iinit();
    fn fileinit();
    fn virtio_disk_init();
}

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
        unsafe {
            trapinit(); // trap vectors
            trapinithart(); // install kernel trap vector
            plicinit(); // set up interrupt controller
            plicinithart(); // ask PLIC for device interrupts
            binit(); // buffer cache
            iinit(); // inode table
            fileinit(); // file table
            virtio_disk_init(); // emulated hard disk
            proc::user_init(); // first user process
            STARTED.store(true, Ordering::Release);
        }
    } else {
        while !STARTED.load(Ordering::Acquire) {
            hint::spin_loop();
        }
        println!("hart {} starting", proc::cpuid());
        vm::kernel::init_hart(); // turn on paging
        unsafe {
            trapinithart(); // install kernel trap vector
            plicinithart(); // ask PLIC for device interrupts
        }
    }

    proc::scheduler();
}
