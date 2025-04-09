#![cfg_attr(not(test), no_std)]

use ov6_user_lib::{
    os::ov6::syscall::{self, MemoryInfo, SystemInfo},
    println,
};
use ov6_user_tests::{OrExit as _, exit_err};

fn main() {
    let sysinfo = syscall::get_system_info().or_exit(|e| {
        exit_err!(e, "cannot get system info");
    });

    let SystemInfo { memory } = sysinfo;

    print_memory_info(&memory);
}

fn print_memory_info(info: &MemoryInfo) {
    let MemoryInfo {
        free_pages,
        total_pages,
        page_size,
    } = info;

    println!("# Memory Information");
    println!("{:<12} {total_pages}", "PageTotal");
    println!("{:<12} {free_pages}", "PageFree");
    println!("{:<12} {} kB", "MemTotal", total_pages * page_size / 1024);
    println!("{:<12} {} kB", "MemFree", free_pages * page_size / 1024);
}
