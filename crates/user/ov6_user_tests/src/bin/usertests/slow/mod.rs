use crate::{TestFn, test_entry};

mod slow_fs;
mod slow_proc;

pub const TESTS: &[(&str, TestFn)] = &[
    test_entry!(slow_fs::big_dir),
    test_entry!(slow_fs::many_writes),
    test_entry!(slow_fs::bad_write),
    test_entry!(slow_proc::execout),
    test_entry!(slow_fs::disk_full),
    test_entry!(slow_fs::out_of_inodes),
];
