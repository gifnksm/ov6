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
    process::{Child, ChildStderr, ChildStdin, ChildStdout},
    sync::{mpsc, watch},
    task::JoinHandle,
};

pub struct Qemu {
    proc: Child,
    stdin_handle: JoinHandle<Result<(), anyhow::Error>>,
    stdout_handle: JoinHandle<Result<(), anyhow::Error>>,
    stderr_handle: JoinHandle<Result<(), anyhow::Error>>,

    stdin_tx: mpsc::Sender<Vec<u8>>,
    stdout_content: Arc<Mutex<String>>,
    stdout_rx: watch::Receiver<usize>,
}

impl Qemu {
    pub const BOOT_MSG: &str = "ov6 kernel is booting";

    pub fn new(
        runner_id: usize,
        project_root: &Path,
        workspace_dir: &Path,
        qemu_kernel: &Path,
        qemu_fs: &Path,
        gdb_sock: &Path,
    ) -> Result<Self, anyhow::Error> {
        let log_path = workspace_dir.join(format!("ov6.{}.{}.out", process::id(), runner_id));
        let log = Arc::new(Mutex::new(
            File::create(&log_path).context("open logfile failed")?,
        ));

        let mut proc = crate::make_command(project_root)
            .args([
                "qemu-gdb-noinit",
                &format!("QEMU_KERNEL={}", qemu_kernel.display()),
                &format!("QEMU_FS={}", qemu_fs.display()),
                &format!("GDB_SOCK={}", gdb_sock.display()),
            ])
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .kill_on_drop(true)
            .spawn()
            .context("spawn qemu failed")?;

        let stdin = proc.stdin.take().unwrap();
        let stdout = proc.stdout.take().unwrap();
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
            stdin_tx,
            stdout_content,
            stdout_rx,
        })
    }

    #[must_use]
    pub fn stdin_tx(&self) -> &mpsc::Sender<Vec<u8>> {
        &self.stdin_tx
    }

    #[must_use]
    pub fn stdout(&self) -> &Arc<Mutex<String>> {
        &self.stdout_content
    }

    #[must_use]
    pub fn stdout_watch(&self) -> watch::Receiver<usize> {
        self.stdout_rx.clone()
    }

    #[must_use]
    pub fn stdout_pos(&self) -> usize {
        *self.stdout_rx.borrow()
    }

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

    pub async fn wait_terminate(mut self) -> Result<ExitStatus, anyhow::Error> {
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
        Ok(status)
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
