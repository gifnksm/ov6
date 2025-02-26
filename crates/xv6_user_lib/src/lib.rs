#![feature(core_io_borrowed_buf)]
#![feature(lang_items)]
#![feature(naked_functions)]
#![feature(maybe_uninit_slice)]
#![allow(internal_features)]
#![no_std]

extern crate alloc as alloc_crate;

#[macro_use]
mod macros;

#[cfg(not(feature = "std"))]
pub mod alloc;
pub mod env;
pub mod error;
pub mod fs;
pub mod io;
pub mod os;
pub mod pipe;
pub mod process;
pub mod sync;

#[cfg(not(feature = "std"))]
#[lang = "start"]
fn lang_start<T>(main: fn() -> T, argc: isize, argv: *const *const u8, _: u8) -> isize {
    if argc < 0 {
        panic!("argc should be greater than 0");
    }
    env::set_args(argc.cast_unsigned(), argv.cast());
    main();
    process::exit(0);
}

#[cfg(not(feature = "std"))]
#[panic_handler]
fn panic(info: &core::panic::PanicInfo) -> ! {
    println!("panic: {info}");
    process::exit(1);
}
