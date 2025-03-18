use std::{fs, num::Wrapping, path::PathBuf};

use anyhow::Context as _;
use tokio::{
    io::AsyncWriteExt as _,
    net::{UnixSocket, UnixStream},
    time::{self, Duration},
};

pub struct Gdb {
    stream: UnixStream,
}

impl Gdb {
    pub async fn connect(sock: PathBuf) -> Result<Self, anyhow::Error> {
        let stream = connect(sock).await;
        Ok(Self { stream })
    }

    pub async fn cont(&mut self) -> Result<(), anyhow::Error> {
        self.send("c").await
    }

    async fn send(&mut self, cmd: &str) -> Result<(), anyhow::Error> {
        let sum: Wrapping<u8> = cmd.as_bytes().iter().copied().map(Wrapping).sum();
        let packet = format!("${cmd}#{sum:02x}");
        self.stream
            .write_all(packet.as_bytes())
            .await
            .context("failed to write GDB socket")?;
        Ok(())
    }
}

async fn connect(sock_path: PathBuf) -> UnixStream {
    loop {
        let sock = UnixSocket::new_stream().unwrap();
        let Ok(stream) = sock.connect(&sock_path).await else {
            time::sleep(Duration::from_millis(10)).await;
            continue;
        };
        fs::remove_file(sock_path).unwrap();
        return stream;
    }
}
