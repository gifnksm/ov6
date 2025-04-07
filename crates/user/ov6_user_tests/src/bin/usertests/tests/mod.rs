use ov6_user_tests::test_runner::TestEntry;

mod memory;
mod misc;
mod more_fork;
mod more_fs;
mod simple_fork;
mod simple_fs;
mod slow_fs;
mod slow_proc;

macro_rules! quick {
    ($mod:ident:: $name:ident) => {
        TestEntry {
            name: concat!(stringify!($mod), "::", stringify!($name)),
            test: $mod::$name,
            tags: &["quick", stringify!($mod), stringify!($name)],
        }
    };
}

macro_rules! slow {
    ($mod:ident:: $name:ident) => {
        TestEntry {
            name: concat!(stringify!($mod), "::", stringify!($name)),
            test: $mod::$name,
            tags: &["slow", stringify!($mod), stringify!($name)],
        }
    };
}

pub const TESTS: &[TestEntry] = &[
    quick!(memory::copy_u2k),
    quick!(memory::copy_k2u),
    quick!(memory::rw_sbrk),
    quick!(memory::count_free_pages),
    quick!(simple_fs::open_test),
    quick!(simple_fs::too_many_open_files),
    quick!(simple_fs::too_many_open_files_in_system),
    quick!(simple_fs::write_test),
    quick!(simple_fs::write_big_test),
    quick!(simple_fs::create_test),
    quick!(simple_fs::dir_test),
    quick!(simple_fs::exec_test),
    quick!(simple_fs::bad_fd),
    quick!(simple_fork::pipe),
    quick!(simple_fork::broken_pipe),
    quick!(simple_fork::pipe_bad_fd),
    quick!(simple_fork::kill_status),
    quick!(simple_fork::kill_error),
    quick!(simple_fork::preempt),
    quick!(simple_fork::exit_wait),
    quick!(simple_fork::reparent1),
    quick!(simple_fork::two_children),
    quick!(simple_fork::fork_fork),
    quick!(simple_fork::fork_fork_fork),
    quick!(simple_fork::reparent2),
    quick!(simple_fork::mem),
    quick!(more_fs::truncate1),
    quick!(more_fs::truncate2),
    quick!(more_fs::truncate3),
    quick!(more_fs::inode_put_open),
    quick!(more_fs::inode_put_exit),
    quick!(more_fs::inode_put_chdir),
    quick!(more_fs::shared_fd),
    quick!(more_fs::four_files),
    quick!(more_fs::create_delete),
    quick!(more_fs::unlink_read),
    quick!(more_fs::link),
    quick!(more_fs::concreate),
    quick!(more_fs::link_unlink),
    quick!(more_fs::subdir),
    quick!(more_fs::big_write),
    quick!(more_fs::big_file),
    quick!(more_fs::fourteen),
    quick!(more_fs::rm_dot),
    quick!(more_fs::dir_file),
    quick!(more_fs::iref),
    quick!(more_fork::fork),
    quick!(more_fork::sbrk_basic),
    quick!(more_fork::sbrk_much),
    quick!(more_fork::kern_mem),
    quick!(more_fork::max_va_plus),
    quick!(more_fork::sbrk_fail),
    quick!(more_fork::sbrk_arg),
    quick!(misc::validate),
    quick!(misc::bss),
    quick!(misc::big_arg),
    quick!(misc::fs_full),
    quick!(misc::argp),
    quick!(misc::stack),
    quick!(misc::no_write),
    quick!(misc::pg_bug),
    quick!(misc::sbrk_bugs),
    quick!(misc::sbrk_last),
    quick!(misc::sbrk8000),
    quick!(misc::bad_arg),
    slow!(slow_fs::big_dir),
    slow!(slow_fs::many_writes),
    slow!(slow_fs::bad_write),
    slow!(slow_proc::execout),
    slow!(slow_fs::disk_full),
    slow!(slow_fs::out_of_inodes),
];
