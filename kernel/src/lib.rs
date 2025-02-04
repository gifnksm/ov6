#![feature(extern_types)]
#![no_std]

mod console;
mod file;
mod proc;
mod spinlock;
mod uart;

#[panic_handler]
fn panic(_info: &core::panic::PanicInfo) -> ! {
    loop {}
}
