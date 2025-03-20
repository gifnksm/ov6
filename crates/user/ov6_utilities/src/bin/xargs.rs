#![no_std]

extern crate alloc;

use alloc::{borrow::Cow, vec, vec::Vec};
use core::iter::Peekable;

use ov6_user_lib::{
    env::{self, ArgsOs},
    error::Ov6Error,
    io::{self, BufRead as _},
    os_str::OsStr,
    process::{self, ProcessBuilder},
};
use ov6_utilities::{OrExit as _, exit, exit_err, message_err, usage_and_exit};

fn arg_stack_size(arg: &OsStr) -> usize {
    (arg.len() + 1) + size_of::<usize>()
}

fn argv_stack_size(argv: &[Cow<'_, OsStr>]) -> usize {
    let mut size = size_of::<usize>();
    for arg in argv {
        size += arg_stack_size(arg);
    }
    size
}

const USER_STACK_SIZE: usize = 4096;
const STACK_SIZE_LIMIT: usize = USER_STACK_SIZE / 8;

fn exec(argv: &[Cow<'_, OsStr>]) {
    loop {
        let res = ProcessBuilder::new().spawn_fn(|| {
            let Err(e) = process::exec(&argv[0], argv);
            exit_err!(e, "failed to execute '{}'", argv[0].display());
        });
        match res {
            Ok(_) => break,
            Err(Ov6Error::OutOfMemory | Ov6Error::ResourceTempolaryUnavailable) => {
                match process::wait_any() {
                    Ok(_) | Err(Ov6Error::NoChildProcess) => {}
                    Err(e) => message_err!(e, "cannot wait child process"),
                }
            }
            Err(e) => {
                message_err!(e, "cannot spawn child process");
                break;
            }
        }
    }
}

fn wait() {
    loop {
        match process::wait_any() {
            Ok(_) => {}
            Err(Ov6Error::NoChildProcess) => break,
            Err(e) => message_err!(e, "cannot wait child process"),
        }
    }
}

fn usage() -> ! {
    usage_and_exit!("[-n count] commands...")
}

struct Params {
    n: usize,
}

fn parse_arg(args: &mut Peekable<ArgsOs>) -> Params {
    let mut params = Params { n: usize::MAX };

    while let Some(s) = args.next_if(|s| s.as_bytes().starts_with(b"-")) {
        match s.as_bytes() {
            b"-n" => {
                let Ok(n) = args.next().ok_or_else(|| usage());
                let n = str::from_utf8(n.as_bytes())
                    .or_exit(|e| exit_err!(e, "invalid argument '{}'", n.display()));
                let n = n
                    .parse()
                    .or_exit(|e| exit_err!(e, "invalid argument for '{n}'"));
                params.n = n;
            }
            _ => usage(),
        }
    }
    params
}

fn main() {
    let mut args = env::args_os().peekable();
    let params = parse_arg(&mut args);

    let mut base_cmd = args.map(Cow::Borrowed).collect::<Vec<_>>();
    if base_cmd.is_empty() {
        base_cmd = vec![OsStr::new("echo").into()];
    }

    if argv_stack_size(&base_cmd) > STACK_SIZE_LIMIT {
        exit!("argument list too long");
    }

    let stdin = io::stdin();
    let mut stdin = stdin.lock();
    let mut arg = vec![];
    let mut exec_cmd_opt = None;
    loop {
        arg.clear();
        let n = stdin
            .read_until(b'\n', &mut arg)
            .or_exit(|e| exit_err!(e, "cannot read stgin"));
        if n == 0 {
            break;
        }

        loop {
            let exec_cmd = exec_cmd_opt.get_or_insert_with(|| base_cmd.clone());
            let arg = arg.strip_suffix(b"\n").unwrap_or(&arg);
            let arg = OsStr::from_bytes(arg);
            let added_len = exec_cmd.len() - base_cmd.len();
            if added_len >= params.n
                || argv_stack_size(exec_cmd) + arg_stack_size(arg) > STACK_SIZE_LIMIT
            {
                if added_len == 0 {
                    exit!("argument list too long");
                }
                exec(exec_cmd);
                exec_cmd_opt = None;
                continue;
            }

            exec_cmd.push(arg.to_os_string().into());
            break;
        }
    }

    if let Some(exec_args) = exec_cmd_opt.take() {
        exec(&exec_args);
    }
    wait();
}
