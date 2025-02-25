#![feature(lang_items)]
#![feature(naked_functions)]
#![allow(internal_features)]
#![no_std]

pub mod env;
pub mod error;
pub mod fs;
pub mod io;
pub mod os;
pub mod process;

#[lang = "start"]
fn lang_start<T>(main: fn() -> T, argc: isize, argv: *const *const u8, _: u8) -> isize {
    if argc < 0 {
        panic!("argc should be greater than 0");
    }
    env::set_args(argc.cast_unsigned(), argv);
    main();
    process::exit(0);
}

#[panic_handler]
fn panic(info: &core::panic::PanicInfo) -> ! {
    println!("panic: {info}");
    process::exit(1);
}
