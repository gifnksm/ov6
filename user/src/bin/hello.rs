#![no_std]
#![no_main]

use xv6_user_lib::println;

#[unsafe(no_mangle)]
pub fn main(_argc: i32, _argv: *const *const u8) {
    println!("Hello, world!");
}
