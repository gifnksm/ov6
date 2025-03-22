#![expect(internal_features)]
#![feature(slice_split_once)]
#![feature(str_internals)]
#![cfg_attr(not(test), no_std)]

#[cfg(feature = "alloc")]
extern crate alloc;

pub mod fs;
pub mod os_str;
pub mod path;
pub mod process;
