#![no_std]
#![no_main]

use xv6_user_lib::println;

#[unsafe(no_mangle)]
fn main() {
    println!("Hello, world!");
}
