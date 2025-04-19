//! Monitoring utilities for QEMU and GDB in integration tests.
//!
//! This module provides functions to monitor the boot process, interact with
//! the QEMU instance, and run integration test commands.

use std::process::ExitStatus;

use anyhow::Context as _;
use tokio::time::{self, Duration};

use crate::{Gdb, Qemu, Runner};

/// A constant message indicating the kernel boot process.
pub const BOOT_MSG: &str = "ov6 kernel is booting";

/// Waits for the QEMU instance to output the boot message.
///
/// This function blocks until the specified boot message is detected in the
/// QEMU instance's stdout.
///
/// # Errors
///
/// Returns an error if the boot message is not detected.
pub async fn wait_boot(qemu: &Qemu, output_start: usize) -> Result<(), anyhow::Error> {
    qemu.wait_output(output_start, |s| s.contains(BOOT_MSG))
        .await
}

/// Waits for the QEMU instance to output a shell prompt.
///
/// This function blocks until a shell prompt (`$ `) is detected in the QEMU
/// instance's stdout.
///
/// # Errors
///
/// Returns an error if the shell prompt is not detected.
pub async fn wait_prompt(qemu: &Qemu, output_start: usize) -> Result<(), anyhow::Error> {
    qemu.wait_output(output_start, |s| s.contains("$ ")).await
}

/// Runs a series of commands on the QEMU instance.
///
/// This function waits for a shell prompt before sending each command and
/// updates the output position after each command.
///
/// # Errors
///
/// Returns an error if any command fails to execute or if the shell prompt
/// is not detected.
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
        qemu.stdin_tx()
            .ok_or_else(|| anyhow::anyhow!("QEMU stdin channel is closed"))?
            .send(msg)
            .await?;
    }

    Ok(output_start)
}

/// Runs an integration test with the specified QEMU and GDB setup.
///
/// This function launches the QEMU and GDB instances, executes the provided
/// test function, and waits for the QEMU instance to terminate.
///
/// # Errors
///
/// Returns an error if the test times out or if any part of the test fails.
pub async fn run_test<F, T>(
    r: Runner,
    timeout: Duration,
    f: F,
) -> Result<(ExitStatus, String, T), anyhow::Error>
where
    F: AsyncFnOnce(&Qemu, &Gdb) -> Result<T, anyhow::Error>,
{
    time::timeout(timeout, async {
        let (qemu, gdb) = r.launch().await?;
        let ret = f(&qemu, &gdb).await?;
        let (exit_status, stdout) = qemu.wait_terminate().await?;
        Ok((exit_status, stdout, ret))
    })
    .await
    .context("test timeout")?
}
