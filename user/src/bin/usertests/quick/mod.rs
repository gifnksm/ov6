use crate::{TestFn, test_entry};

mod copy_in;
mod copy_in_str;
mod copy_out;
mod inode_put;
mod misc;
mod more_fork;
mod more_fs;
mod rw_sbrk;
mod simple_fork;
mod simple_fs;
mod truncate;

pub const TESTS: &[(&str, TestFn)] = &[
    test_entry!(copy_in::test),
    test_entry!(copy_out::test),
    test_entry!(copy_in_str::test1),
    test_entry!(copy_in_str::test2),
    test_entry!(copy_in_str::test3),
    test_entry!(rw_sbrk::test),
    test_entry!(truncate::test1),
    test_entry!(truncate::test2),
    test_entry!(truncate::test3),
    test_entry!(inode_put::open_test),
    test_entry!(inode_put::exit_test),
    test_entry!(inode_put::chdir_test),
    test_entry!(simple_fs::open_test),
    test_entry!(simple_fs::write_test),
    test_entry!(simple_fs::write_big_test),
    test_entry!(simple_fs::create_test),
    test_entry!(simple_fs::dir_test),
    test_entry!(simple_fs::exec_test),
    test_entry!(simple_fs::bad_fd),
    test_entry!(simple_fork::pipe),
    test_entry!(simple_fork::broken_pipe),
    test_entry!(simple_fork::kill_status),
    test_entry!(simple_fork::kill_error),
    test_entry!(simple_fork::preempt),
    test_entry!(simple_fork::exit_wait),
    test_entry!(simple_fork::reparent1),
    test_entry!(simple_fork::two_children),
    test_entry!(simple_fork::fork_fork),
    test_entry!(simple_fork::fork_fork_fork),
    test_entry!(simple_fork::reparent2),
    test_entry!(simple_fork::mem),
    test_entry!(more_fs::shared_fd),
    test_entry!(more_fs::four_files),
    test_entry!(more_fs::create_delete),
    test_entry!(more_fs::unlink_read),
    test_entry!(more_fs::link),
    test_entry!(more_fs::concreate),
    test_entry!(more_fs::link_unlink),
    test_entry!(more_fs::subdir),
    test_entry!(more_fs::big_write),
    test_entry!(more_fs::big_file),
    test_entry!(more_fs::fourteen),
    test_entry!(more_fs::rm_dot),
    test_entry!(more_fs::dir_file),
    test_entry!(more_fs::iref),
    test_entry!(more_fork::fork),
    test_entry!(more_fork::sbrk_basic),
    test_entry!(more_fork::sbrk_much),
    test_entry!(more_fork::kern_mem),
    test_entry!(more_fork::max_va_plus),
    test_entry!(more_fork::sbrk_fail),
    test_entry!(more_fork::sbrk_arg),
    test_entry!(misc::validate),
    test_entry!(misc::bss),
    test_entry!(misc::big_arg),
    test_entry!(misc::fs_full),
    test_entry!(misc::argp),
    test_entry!(misc::stack),
    test_entry!(misc::no_write),
    test_entry!(misc::pg_bug),
    test_entry!(misc::sbrk_bugs),
    test_entry!(misc::sbrk_last),
    test_entry!(misc::sbrk8000),
    test_entry!(misc::bad_arg),
];
