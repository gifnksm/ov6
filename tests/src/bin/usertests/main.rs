#![feature(allocator_api)]
#![no_std]

extern crate alloc;

use alloc::{borrow::ToOwned as _, string::String};

use ov6_fs_types::FS_BLOCK_SIZE;
use ov6_kernel_params::MAX_OP_BLOCKS;
use ov6_user_lib::{
    env, eprint, eprintln,
    io::{Read as _, Write as _},
    pipe,
    process::{self},
    time::Instant,
};
use tests::message;

mod macros;
mod quick;
mod slow;

const PAGE_SIZE: usize = 4096;
const KERN_BASE: usize = 0x8000_0000;
const MAX_VA: usize = 1 << (9 + 9 + 9 + 12 - 1);
const README_PATH: &str = "README";
const ECHO_PATH: &str = "echo";
const ROOT_DIR_PATH: &str = "/";

const BUF_SIZE: usize = (MAX_OP_BLOCKS + 2) * FS_BLOCK_SIZE;
static mut BUF: [u8; BUF_SIZE] = [0; BUF_SIZE];

#[derive(Debug)]
enum TestError {
    TestFailed,
}

type TestFn = fn();

fn run(name: &str, test: TestFn) -> Result<(), TestError> {
    eprint!("{name:-30} ");

    let start = Instant::now();

    let status = process::fork_fn(|| {
        test();
        process::exit(0);
    })
    .unwrap()
    .wait()
    .unwrap();

    let elapsed = start.elapsed();

    if !status.success() {
        eprintln!(
            "FAIL [{:3}.{:03}s]",
            elapsed.as_secs(),
            elapsed.subsec_millis()
        );
        return Err(TestError::TestFailed);
    }

    eprintln!(
        "PASS [{:3}.{:03}s]",
        elapsed.as_secs(),
        elapsed.subsec_millis()
    );
    Ok(())
}

fn run_tests(
    tests: &[(&str, TestFn)],
    run_just_one: Option<&str>,
    continuous: Continuous,
) -> Result<(), TestError> {
    for (name, test) in tests {
        if let Some(just_one) = run_just_one {
            if *name != just_one {
                continue;
            }
        }

        if let Err(e) = continuous.judge_result(run(name, *test)) {
            eprintln!("SOME TESTS FAILED");
            return Err(e);
        }
    }

    Ok(())
}

/// Counts that the kernel can allocate and deallocate memory.
///
/// This uses `sbrt()` to count how many free physical memory pages there are.
/// Touches the pages to force allocation.
/// Because out of memory with lazy allocation results in the process
/// taking a fault and being killed, fork and report back.
fn count_free_pages() -> usize {
    let (mut rx, mut tx) = pipe::pipe().unwrap();

    if process::fork().unwrap().is_child() {
        drop(rx);

        loop {
            unsafe {
                let Ok(a) = process::grow_break(4096) else {
                    break;
                };

                // modify the memory to make sure it's really allocated.
                a.add(4096 - 1).write(1);

                // report back one more page.
                tx.write_all(b"x").unwrap();
            }
        }

        process::exit(0);
    }

    drop(tx);

    let mut n = 0;
    loop {
        let mut buf = [0];
        if rx.read(&mut buf).unwrap() == 0 {
            break;
        }
        n += 1;
    }

    drop(rx);
    process::wait().unwrap();

    n
}

fn drive_tests(param: &Param) -> Result<(), TestError> {
    loop {
        eprint!("freepages: ");
        let start = Instant::now();
        let free0 = count_free_pages();
        let elapsed = start.elapsed();
        eprintln!(
            "{free0} [{:3}.{:03}s]",
            elapsed.as_secs(),
            elapsed.subsec_millis()
        );

        eprintln!("starting");

        param.continuous.judge_result(run_tests(
            quick::TESTS,
            param.run_just_one.as_deref(),
            param.continuous,
        ))?;

        if !param.run_quick_only {
            if param.run_just_one.is_none() {
                eprintln!("running slow tests");
            }

            param.continuous.judge_result(run_tests(
                slow::TESTS,
                param.run_just_one.as_deref(),
                param.continuous,
            ))?;
        }

        eprint!("freepages: ");
        let start = Instant::now();
        let free1 = count_free_pages();
        let elapsed = start.elapsed();
        eprintln!(
            "{free1} [{:3}.{:03}s]",
            elapsed.as_secs(),
            elapsed.subsec_millis()
        );

        if free0 != free1 {
            eprintln!("freepages: FAIL -- lost some free pages {free1} (out of {free0})");
            return Err(TestError::TestFailed);
        }

        eprintln!("freepages: PASS");

        if param.continuous == Continuous::Once {
            break;
        }
    }

    Ok(())
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Continuous {
    Once,
    UntilFailure,
    Forever,
}

impl Continuous {
    fn judge_result<E>(self, result: Result<(), E>) -> Result<(), E> {
        match self {
            Self::Once | Self::UntilFailure => result,
            Self::Forever => Ok(()),
        }
    }
}

struct Param {
    run_quick_only: bool,
    run_just_one: Option<String>,
    continuous: Continuous,
}

impl Param {
    fn usage_and_exit() -> ! {
        eprintln!("Usage: {} [-c] [-C] [-q] [testname]", env::arg0().display());
        process::exit(1);
    }

    fn parse() -> Self {
        let mut args = env::args();

        if args.len() > 1 {
            Self::usage_and_exit();
        }

        let mut param = Self {
            run_quick_only: false,
            run_just_one: None,
            continuous: Continuous::Once,
        };

        if let Some(arg) = args.next() {
            match arg {
                "-q" => param.run_quick_only = true,
                "-c" => param.continuous = Continuous::UntilFailure,
                "-C" => param.continuous = Continuous::Forever,
                _ if !arg.starts_with('-') => param.run_just_one = Some(arg.to_owned()),
                _ => Self::usage_and_exit(),
            }
        }
        param
    }
}

fn main() {
    let param = Param::parse();

    let start = Instant::now();
    let res = drive_tests(&param);
    let elapsed = start.elapsed();

    match res {
        Ok(()) => {
            message!(
                "ALL TESTS PASSED [{:3}.{:03}s]",
                elapsed.as_secs(),
                elapsed.subsec_millis(),
            );
            process::exit(0);
        }
        Err(TestError::TestFailed) => {
            message!(
                "TEST FAILED [{:3}.{:03}s]",
                elapsed.as_secs(),
                elapsed.subsec_millis()
            );
            process::exit(1);
        }
    }
}
