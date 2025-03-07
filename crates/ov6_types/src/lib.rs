#![feature(slice_split_once)]
#![cfg_attr(not(test), no_std)]

#[cfg(feature = "alloc")]
extern crate alloc;

pub mod os_str;
pub mod path;
