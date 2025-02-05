use core::arch::global_asm;

use crate::start::{STACK_SIZE, STACK0, start};

global_asm!(
    include_str!("entry.s"),
    STACK0 = sym STACK0,
    STACK_SIZE = const STACK_SIZE,
    start = sym start,
);
