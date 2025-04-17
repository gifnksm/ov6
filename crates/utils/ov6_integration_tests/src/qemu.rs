use std::{
    path::Path,
    process::ExitStatus,
    sync::{Arc, Mutex},
};

use anyhow::Context as _;
use tokio::sync::{mpsc, watch};

use crate::logged_command::LoggedCommand;

pub struct Qemu {
    command: LoggedCommand,
}

impl Qemu {
    pub const BOOT_MSG: &str = "ov6 kernel is booting";

    #[expect(clippy::missing_panics_doc)]
    pub fn new(
        runner_id: usize,
        project_root: &Path,
        workspace_dir: &Path,
        qemu_kernel: &Path,
        qemu_fs: &Path,
        gdb_sock: &Path,
        qemu_monitor_sock: &Path,
    ) -> Result<Self, anyhow::Error> {
        let mut command = crate::make_command(project_root);
        command.args([
            "qemu-gdb-noinit",
            &format!("QEMU_KERNEL={}", qemu_kernel.display()),
            &format!("QEMU_FS={}", qemu_fs.display()),
            &format!("GDB_SOCK={}", gdb_sock.display()),
            "QEMU_MONITOR_FWD=1",
            &format!("QEMU_MONITOR_SOCK={}", qemu_monitor_sock.display()),
            "FWD_PORT1=0",
            "FWD_PORT2=0",
        ]);

        let command = LoggedCommand::new(command, runner_id, "qemu", workspace_dir)
            .context("spawn qemu failed")?;

        Ok(Self { command })
    }

    #[must_use]
    pub fn stdin_tx(&self) -> Option<&mpsc::Sender<Vec<u8>>> {
        self.command.stdin_tx()
    }

    #[must_use]
    pub fn close_stdin(&mut self) {
        self.command.close_stdin();
    }

    #[must_use]
    pub fn stdout(&self) -> &Arc<Mutex<String>> {
        &self.command.stdout()
    }

    #[must_use]
    pub fn stdout_watch(&self) -> watch::Receiver<usize> {
        self.command.stdout_watch()
    }

    #[must_use]
    pub fn stdout_pos(&self) -> usize {
        self.command.stdout_pos()
    }

    #[expect(clippy::missing_panics_doc)]
    pub async fn wait_output<F>(&self, start: usize, cond: F) -> Result<(), anyhow::Error>
    where
        F: FnMut(&str) -> bool,
    {
        self.command.wait_output(start, cond).await
    }

    #[expect(clippy::missing_panics_doc)]
    pub async fn wait_terminate(self) -> Result<(ExitStatus, String), anyhow::Error> {
        self.command.wait_terminate().await
    }
}
