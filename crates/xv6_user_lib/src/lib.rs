#![feature(naked_functions)]
#![no_std]

pub mod env;
pub mod error;
pub mod fs;
pub mod io;
pub mod os;
pub mod process;
pub mod syscall;

unsafe extern "Rust" {
    fn main();
}

#[unsafe(export_name = "_start")]
extern "C" fn start(argc: usize, argv: *const *const u8) {
    env::set_args(argc, argv);
    unsafe {
        main();
    }
    process::exit(0);
}

#[panic_handler]
fn panic(info: &core::panic::PanicInfo) -> ! {
    println!("panic: {info}");
    process::exit(1);
}
