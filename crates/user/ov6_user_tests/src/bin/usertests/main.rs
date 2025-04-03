#![feature(allocator_api)]
#![cfg_attr(not(test), no_std)]

extern crate alloc;

use ov6_fs_types::FS_BLOCK_SIZE;
use ov6_kernel_params::MAX_OP_BLOCKS;
use ov6_user_tests::test_runner::TestParam;

mod tests;

const PAGE_SIZE: usize = 4096;
const KERN_BASE: usize = 0x8000_0000;
const MAX_VA: usize = 1 << (9 + 9 + 9 + 12 - 1);
const README_PATH: &str = "README";
const ECHO_PATH: &str = "echo";
const ROOT_DIR_PATH: &str = "/";

const BUF_SIZE: usize = (MAX_OP_BLOCKS + 2) * FS_BLOCK_SIZE;
static mut BUF: [u8; BUF_SIZE] = [0; BUF_SIZE];

fn main() {
    TestParam::parse().run(tests::TESTS);
}
