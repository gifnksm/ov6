use crate::{TestFn, test_entry};

mod fs;
mod proc;

pub const TESTS: &[(&str, TestFn)] = &[
    test_entry!(fs::big_dir),
    test_entry!(fs::many_writes),
    test_entry!(fs::bad_write),
    test_entry!(proc::execout),
    test_entry!(fs::disk_full),
    test_entry!(fs::out_of_inodes),
];
