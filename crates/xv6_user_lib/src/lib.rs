#![feature(naked_functions)]
#![no_std]

pub const STDIN_FD: i32 = 0;
pub const STDOUT_FD: i32 = 1;
pub const STDERR_FD: i32 = 2;

unsafe extern "Rust" {
    fn main(argc: i32, argv: *mut *mut u8);
}

pub mod error;
pub mod fs;
pub mod io;
pub mod os;
pub mod process;
pub mod syscall;

#[unsafe(export_name = "_start")]
extern "C" fn start(argc: i32, argv: *mut *mut u8) {
    unsafe {
        main(argc, argv);
    }
    process::exit(0);
}

#[panic_handler]
fn panic(info: &core::panic::PanicInfo) -> ! {
    println!("panic: {info}");
    process::exit(1);
}
