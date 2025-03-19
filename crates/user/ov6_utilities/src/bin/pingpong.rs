#![no_std]

use ov6_user_lib::{
    eprintln,
    io::{self, Read as _, Write as _},
    process::{self, ProcessBuilder, Stdio},
};
use ov6_utilities::{OrExit as _, exit_err};

fn main() {
    let mut child = ProcessBuilder::new()
        .stdin(Stdio::Pipe)
        .stdout(Stdio::Pipe)
        .spawn_fn(|| {
            let pid = process::id();
            io::stdin()
                .read_exact(&mut [0])
                .or_exit(|e| exit_err!(e, "failed to read"));
            eprintln!("{pid}: received ping");
            io::stdout()
                .write_all(&[0])
                .or_exit(|e| exit_err!(e, "failed to write"));
            process::exit(0);
        })
        .or_exit(|e| exit_err!(e, "failed to spawn child"));

    let pid = process::id();
    let mut child_stdin = child.stdin.take().unwrap();
    let mut child_stdout = child.stdout.take().unwrap();
    child_stdin
        .write_all(&[0])
        .or_exit(|e| exit_err!(e, "failed to write"));
    child_stdout
        .read_exact(&mut [0])
        .or_exit(|e| exit_err!(e, "failed to read"));
    eprintln!("{pid}: received pong");
    process::exit(0);
}
