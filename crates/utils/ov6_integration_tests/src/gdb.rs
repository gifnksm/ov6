//! GDB Integration APIs.
//!
//! This module provides the `Gdb` struct, which facilitates communication
//! with a GDB server over a Unix socket. It includes methods for sending
//! commands and managing the connection.

use std::{fs, num::Wrapping, path::PathBuf};

use anyhow::Context as _;
use tokio::{
    io::AsyncWriteExt as _,
    net::{UnixSocket, UnixStream},
    time::{self, Duration},
};

/// Represents a GDB connection.
///
/// This struct wraps a `UnixStream` to communicate with a GDB server and
/// provides methods for sending commands.
pub struct Gdb {
    /// The Unix stream used for communication with the GDB server.
    stream: UnixStream,
}

impl Gdb {
    /// Connects to a GDB server using the specified Unix socket path.
    ///
    /// # Errors
    ///
    /// Returns an error if the connection fails.
    pub async fn connect(sock: PathBuf) -> Result<Self, anyhow::Error> {
        let stream = connect(sock).await;
        Ok(Self { stream })
    }

    /// Sends a "continue" command (`c`) to the GDB server.
    ///
    /// # Errors
    ///
    /// Returns an error if the command fails to send.
    pub async fn cont(&mut self) -> Result<(), anyhow::Error> {
        self.send("c").await
    }

    /// Sends a custom command to the GDB server.
    ///
    /// # Errors
    ///
    /// Returns an error if the command fails to send.
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

/// Establishes a connection to a GDB server.
///
/// This function repeatedly attempts to connect to the specified Unix socket
/// until successful. Once connected, it removes the socket file.
///
/// # Returns
///
/// A `UnixStream` representing the connection.
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
