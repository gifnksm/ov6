#![feature(extern_types)]
#![no_std]

mod proc;
mod spinlock;

#[panic_handler]
fn panic(_info: &core::panic::PanicInfo) -> ! {
    loop {}
}
