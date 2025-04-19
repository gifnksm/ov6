//! Utilities for managing and logging subprocess commands.
//!
//! This module provides the `LoggedCommand` struct, which wraps a subprocess
//! and handles its input/output streams, logging, and lifecycle management.

use std::{
    fs::File,
    io::Write as _,
    path::Path,
    process::{self, ExitStatus, Stdio},
    sync::{Arc, Mutex},
};

use anyhow::{Context as _, ensure};
use tokio::{
    io::{AsyncBufReadExt as _, AsyncReadExt as _, AsyncWriteExt as _, BufReader},
    process::{Child, ChildStderr, ChildStdin, ChildStdout, Command},
    sync::{mpsc, watch},
    task::JoinHandle,
};

/// A struct for managing and logging subprocess commands.
///
/// This struct wraps a subprocess and provides utilities for handling its
/// input/output streams, logging, and lifecycle management.
pub struct LoggedCommand {
    /// The child process being managed.
    proc: Child,
    /// Handle for managing the stdin task.
    stdin_handle: JoinHandle<Result<(), anyhow::Error>>,
    /// Handle for managing the stdout task.
    stdout_handle: JoinHandle<Result<(), anyhow::Error>>,
    /// Handle for managing the stderr task.
    stderr_handle: JoinHandle<Result<(), anyhow::Error>>,
    /// Sender for writing to the child process's stdin.
    stdin_tx: Option<mpsc::Sender<Vec<u8>>>,
    /// Buffer for storing the child process's stdout content.
    stdout_content: Arc<Mutex<String>>,
    /// Watcher for tracking changes in the stdout content length.
    stdout_rx: watch::Receiver<usize>,
}

impl LoggedCommand {
    /// Creates a new `LoggedCommand` instance.
    ///
    /// This spawns a subprocess using the provided `Command` and sets up
    /// logging and communication channels for its stdin, stdout, and stderr.
    ///
    /// # Errors
    ///
    /// Returns an error if the subprocess cannot be spawned or if any
    /// required resources cannot be initialized.
    pub fn new(
        mut command: Command,
        runner_id: usize,
        command_name: &str,
        workspace_dir: &Path,
    ) -> Result<Self, anyhow::Error> {
        let log_path = workspace_dir.join(format!(
            "ov6.{}.{}.{}.out",
            process::id(),
            runner_id,
            command_name
        ));
        let log = Arc::new(Mutex::new(
            File::create(&log_path).context("open logfile failed")?,
        ));

        let mut proc = command
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .kill_on_drop(true)
            .spawn()
            .context("spawn command failed")?;

        #[expect(clippy::missing_panics_doc, reason = "infallible")]
        let stdin = proc.stdin.take().unwrap();
        #[expect(clippy::missing_panics_doc, reason = "infallible")]
        let stdout = proc.stdout.take().unwrap();
        #[expect(clippy::missing_panics_doc, reason = "infallible")]
        let stderr = proc.stderr.take().unwrap();

        let (stdin_tx, stdin_rx) = mpsc::channel(1);
        let stdin_handle = tokio::spawn(handle_stdin(stdin, stdin_rx));

        let (stdout_tx, stdout_rx) = watch::channel(0);
        let stdout_content = Arc::new(Mutex::new(String::new()));

        let stdout_handle = tokio::spawn(handle_stdout(
            Arc::clone(&log),
            stdout,
            Arc::clone(&stdout_content),
            stdout_tx,
        ));

        let stderr_handle = tokio::spawn(handle_stderr(log, stderr));

        Ok(Self {
            proc,
            stdin_handle,
            stdout_handle,
            stderr_handle,
            stdin_tx: Some(stdin_tx),
            stdout_content,
            stdout_rx,
        })
    }

    /// Returns a reference to the stdin sender, if available.
    ///
    /// This allows sending data to the subprocess's stdin asynchronously.
    #[must_use]
    pub fn stdin_tx(&self) -> Option<&mpsc::Sender<Vec<u8>>> {
        self.stdin_tx.as_ref()
    }

    /// Closes the stdin channel, preventing further input to the subprocess.
    ///
    /// This method ensures that no more data can be sent to the subprocess's
    /// stdin.
    pub fn close_stdin(&mut self) {
        let _ = self.stdin_tx.take();
    }

    /// Returns a reference to the stdout content buffer.
    ///
    /// This provides access to the buffered output of the subprocess.
    #[must_use]
    pub fn stdout(&self) -> &Arc<Mutex<String>> {
        &self.stdout_content
    }

    /// Returns a clone of the stdout watcher.
    ///
    /// This allows monitoring changes to the length of the stdout content.
    #[must_use]
    pub fn stdout_watch(&self) -> watch::Receiver<usize> {
        self.stdout_rx.clone()
    }

    /// Returns the current position of the stdout content.
    ///
    /// This indicates the length of the stdout content buffered so far.
    #[must_use]
    pub fn stdout_pos(&self) -> usize {
        *self.stdout_rx.borrow()
    }

    /// Waits for the stdout content to satisfy a given condition.
    ///
    /// This method blocks until the specified condition is met in the stdout
    /// content of the subprocess.
    ///
    /// # Errors
    ///
    /// Returns an error if the watcher fails or if the condition cannot be
    /// evaluated.
    #[expect(clippy::missing_panics_doc)]
    pub async fn wait_output<F>(&self, start: usize, mut cond: F) -> Result<(), anyhow::Error>
    where
        F: FnMut(&str) -> bool,
    {
        let mut stdout_watch = self.stdout_watch();
        loop {
            let _len = *stdout_watch.borrow_and_update();
            if cond(&self.stdout().lock().unwrap()[start..]) {
                break;
            }
            stdout_watch.changed().await.unwrap();
        }
        Ok(())
    }

    /// Sends a kill signal to the subprocess.
    ///
    /// # Errors
    ///
    /// Returns an error if the kill signal cannot be sent.
    pub async fn kill(&mut self) -> Result<(), anyhow::Error> {
        self.proc.kill().await.context("kill command failed")?;
        Ok(())
    }

    /// Waits for the subprocess to terminate and collects its output.
    ///
    /// This method blocks until the subprocess exits and collects its
    /// stdout content and exit status.
    ///
    /// # Errors
    ///
    /// Returns an error if any of the subprocess tasks fail or if the process
    /// cannot be awaited.
    #[expect(clippy::missing_panics_doc)]
    pub async fn wait_terminate(mut self) -> Result<(ExitStatus, String), anyhow::Error> {
        drop(self.stdin_tx);
        self.stdin_handle
            .await
            .context("stdin handle join failed")??;
        self.stdout_handle
            .await
            .context("stdout handle join failed")??;
        self.stderr_handle
            .await
            .context("stderr handle join failed")??;
        let status = self.proc.wait().await?;
        let stdout = self.stdout_content.lock().unwrap().clone();
        Ok((status, stdout))
    }
}

const MAX_LEN_UTF8: usize = 4;

async fn handle_stdin(
    mut stdin: ChildStdin,
    mut rx: mpsc::Receiver<Vec<u8>>,
) -> Result<(), anyhow::Error> {
    while let Some(msg) = rx.recv().await {
        stdin.write_all(&msg).await?;
        stdin.flush().await?;
    }
    Ok(())
}

async fn handle_stdout(
    log: Arc<Mutex<File>>,
    stdout: ChildStdout,
    output: Arc<Mutex<String>>,
    output_tx: watch::Sender<usize>,
) -> Result<(), anyhow::Error> {
    let mut stdout = BufReader::new(stdout);

    loop {
        let buf = stdout
            .fill_buf()
            .await
            .context("fill qemu stdout buffer failed")?;

        let closed = buf.is_empty();
        let s = match str::from_utf8(buf) {
            Ok(s) => s,
            Err(e) => {
                ensure!(buf.len() - e.valid_up_to() < MAX_LEN_UTF8, "invalid utf-8");
                println!("{}", e.valid_up_to());
                str::from_utf8(&buf[..e.valid_up_to()]).unwrap()
            }
        };

        {
            let mut stdout = std::io::stdout().lock();
            stdout.write_all(s.as_bytes()).unwrap();
            stdout.flush().unwrap();
        }

        {
            let mut log = log.lock().unwrap();
            log.write_all(s.as_bytes()).unwrap();
            log.flush().unwrap();
        }

        {
            let mut output = output.lock().unwrap();
            output.push_str(s);
            output_tx
                .send(output.len())
                .context("send output notify failed")?;
        }

        let amt = s.len();
        stdout.consume(amt);

        if closed {
            break;
        }
    }
    Ok(())
}

async fn handle_stderr(
    log: Arc<Mutex<File>>,
    mut stderr: ChildStderr,
) -> Result<(), anyhow::Error> {
    let mut buf = vec![0; 4096];
    loop {
        let n = stderr
            .read(&mut buf)
            .await
            .context("read qemu stderr failed")?;

        let buf = &buf[..n];

        {
            let mut stderr = std::io::stderr().lock();
            stderr.write_all(buf).unwrap();
            stderr.flush().unwrap();
        }

        {
            let mut log = log.lock().unwrap();
            log.write_all(buf).unwrap();
            log.flush().unwrap();
        }

        if n == 0 {
            break;
        }
    }
    Ok(())
}
