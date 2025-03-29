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

pub mod alloc;
pub mod env;
pub mod error;
pub mod fs;
pub mod io;
pub mod os;
pub mod pipe;
pub mod process;
mod rt;
pub mod sync;
pub mod thread;
pub mod time;

// The Rust entry point `lang_start` defines the `main` function, but the linker
// expects the entry point to be named `_start`. Therefore, assembly code is
// used to define `_start` as an alias for `main`.
#[cfg(all(feature = "lang_items", not(feature = "test")))]
core::arch::global_asm!(".global _start", ".global main", ".equiv _start, main");

#[cfg(all(feature = "lang_items", not(feature = "test")))]
#[lang = "start"]
fn lang_start<T>(main: fn() -> T, argc: isize, argv: *const *const u8, _: u8) -> isize {
    assert!(argc >= 0, "argc should be greater than or equal to 0");
    env::set_args(argc.cast_unsigned(), argv.cast());
    main();
    process::exit(0);
}

#[cfg(all(feature = "lang_items", not(feature = "test")))]
#[panic_handler]
fn panic(info: &core::panic::PanicInfo) -> ! {
    eprintln!("panic: {info}");
    process::exit(1);
}
