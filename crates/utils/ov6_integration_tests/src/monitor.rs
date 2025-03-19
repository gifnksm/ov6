use std::process::ExitStatus;

use anyhow::Context as _;
use tokio::time::{self, Duration};

use crate::{Gdb, Qemu, Runner};

pub const BOOT_MSG: &str = "ov6 kernel is booting";

pub async fn wait_boot(qemu: &Qemu, output_start: usize) -> Result<(), anyhow::Error> {
    qemu.wait_output(output_start, |s| s.contains(BOOT_MSG))
        .await
}

pub async fn wait_prompt(qemu: &Qemu, output_start: usize) -> Result<(), anyhow::Error> {
    qemu.wait_output(output_start, |s| s.contains("$ ")).await
}

pub async fn run_commands<I, S>(
    qemu: &Qemu,
    mut output_start: usize,
    commands: I,
) -> Result<usize, anyhow::Error>
where
    I: IntoIterator<Item = S>,
    S: AsRef<str>,
{
    for cmd in commands {
        let cmd = cmd.as_ref();
        wait_prompt(qemu, output_start).await?;
        output_start = qemu.stdout_pos();

        let msg = format!("{cmd}\n").into_bytes();
        qemu.stdin_tx().send(msg).await?;
    }

    Ok(output_start)
}

pub async fn run_test<F>(
    r: Runner,
    timeout: Duration,
    f: F,
) -> Result<(ExitStatus, String), anyhow::Error>
where
    F: AsyncFnOnce(&Qemu, &Gdb) -> Result<(), anyhow::Error>,
{
    time::timeout(timeout, async {
        let (qemu, gdb) = r.launch().await?;
        f(&qemu, &gdb).await?;
        qemu.wait_terminate().await
    })
    .await
    .context("test timeout")?
}
