#![feature(core_io_borrowed_buf)]
#![feature(lang_items)]
#![feature(naked_functions)]
#![feature(maybe_uninit_slice)]
#![allow(internal_features)]
#![no_std]

extern crate alloc as alloc_crate;

pub use ov6_types::{os_str, path};

#[macro_use]
mod macros;

#[cfg(all(not(feature = "std"), not(test)))]
pub mod alloc;
pub mod env;
pub mod error;
pub mod fs;
pub mod io;
pub mod os;
pub mod pipe;
pub mod process;
pub mod sync;
pub mod thread;
pub mod time;

#[cfg(all(not(feature = "std"), not(test)))]
#[lang = "start"]
fn lang_start<T>(main: fn() -> T, argc: isize, argv: *const *const u8, _: u8) -> isize {
    assert!(argc >= 0, "argc should be greater than or equal to 0");
    env::set_args(argc.cast_unsigned(), argv.cast());
    main();
    process::exit(0);
}

#[cfg(all(not(feature = "std"), not(test)))]
#[panic_handler]
fn panic(info: &core::panic::PanicInfo) -> ! {
    eprintln!("panic: {info}");
    process::exit(1);
}
