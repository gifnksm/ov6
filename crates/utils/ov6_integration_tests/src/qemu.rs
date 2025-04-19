//! QEMU Integration APIs.
//!
//! This module defines the `Qemu` struct, which manages the execution of QEMU
//! instances for integration testing. It provides utilities for interacting
//! with QEMU's standard input, output, and lifecycle.

use std::{
    path::{Path, PathBuf},
    process::ExitStatus,
    sync::{Arc, Mutex},
};

use anyhow::Context as _;
use tokio::sync::{mpsc, watch};

use crate::logged_command::LoggedCommand;

/// Represents a QEMU instance used for integration testing.
///
/// This struct wraps a `LoggedCommand` to manage the QEMU process and provides
/// methods for interacting with its input/output streams and monitoring its
/// execution state.
pub struct Qemu {
    /// The underlying logged command managing the QEMU process.
    command: LoggedCommand,
    /// The path to the QEMU monitor socket.
    monitor_sock: PathBuf,
}

impl Qemu {
    /// Creates a new `Qemu` instance.
    ///
    /// This method initializes and starts a QEMU process with the specified
    /// configuration. It uses environment variables and arguments to set up
    /// the QEMU instance.
    ///
    /// # Errors
    ///
    /// Returns an error if the QEMU process fails to start or if any required
    /// resources cannot be initialized.
    pub fn new(
        runner_id: usize,
        project_root: &Path,
        workspace_dir: &Path,
        qemu_kernel: &Path,
        qemu_fs: &Path,
        gdb_sock: &Path,
        monitor_sock: PathBuf,
    ) -> Result<Self, anyhow::Error> {
        let mut command = crate::make_command(project_root);
        command.args([
            "qemu-gdb-noinit",
            &format!("QEMU_KERNEL={}", qemu_kernel.display()),
            &format!("QEMU_FS={}", qemu_fs.display()),
            &format!("GDB_SOCK={}", gdb_sock.display()),
            "QEMU_MONITOR_FWD=1",
            &format!("QEMU_MONITOR_SOCK={}", monitor_sock.display()),
            "FWD_PORT1=0",
            "FWD_PORT2=0",
        ]);

        let command = LoggedCommand::new(command, runner_id, "qemu", workspace_dir)
            .context("spawn qemu failed")?;

        Ok(Self {
            command,
            monitor_sock,
        })
    }

    /// Returns the path to the QEMU monitor socket.
    ///
    /// This socket is used for communicating with the QEMU monitor.
    #[must_use]
    pub fn monitor_sock(&self) -> &Path {
        &self.monitor_sock
    }

    /// Returns a sender for writing to the QEMU process's standard input.
    ///
    /// This allows sending data to the QEMU process's stdin asynchronously.
    #[must_use]
    pub fn stdin_tx(&self) -> Option<&mpsc::Sender<Vec<u8>>> {
        self.command.stdin_tx()
    }

    /// Closes the standard input of the QEMU process.
    ///
    /// This prevents further input from being sent to the QEMU process.
    pub fn close_stdin(&mut self) {
        self.command.close_stdin();
    }

    /// Returns a reference to the QEMU process's standard output.
    ///
    /// This provides access to the buffered output of the QEMU process.
    #[must_use]
    pub fn stdout(&self) -> &Arc<Mutex<String>> {
        self.command.stdout()
    }

    /// Returns a watch receiver for monitoring changes to the standard output.
    ///
    /// This allows observing updates to the QEMU process's stdout content.
    #[must_use]
    pub fn stdout_watch(&self) -> watch::Receiver<usize> {
        self.command.stdout_watch()
    }

    /// Returns the current position in the standard output.
    ///
    /// This indicates the length of the stdout content buffered so far.
    #[must_use]
    pub fn stdout_pos(&self) -> usize {
        self.command.stdout_pos()
    }

    /// Waits for specific output from the QEMU process.
    ///
    /// This method blocks until the specified condition is met in the QEMU
    /// process's stdout content.
    ///
    /// # Errors
    ///
    /// Returns an error if the condition is not met or if the watcher fails.
    pub async fn wait_output<F>(&self, start: usize, cond: F) -> Result<(), anyhow::Error>
    where
        F: FnMut(&str) -> bool,
    {
        self.command.wait_output(start, cond).await
    }

    /// Waits for the QEMU process to terminate.
    ///
    /// This method blocks until the QEMU process exits and collects its
    /// stdout content and exit status.
    ///
    /// # Errors
    ///
    /// Returns an error if the process fails to terminate cleanly or if any
    /// subprocess tasks fail.
    pub async fn wait_terminate(self) -> Result<(ExitStatus, String), anyhow::Error> {
        self.command.wait_terminate().await
    }
}
