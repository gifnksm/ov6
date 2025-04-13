use core::net::SocketAddrV4;

use crate::{error::Ov6Error, os::ov6::syscall};

pub struct UdpSocket {
    local_port: u16,
}

impl UdpSocket {
    pub fn bind(port: u16) -> Result<Self, Ov6Error> {
        syscall::bind(port)?;
        Ok(Self { local_port: port })
    }

    pub fn recv_from(&self, buf: &mut [u8]) -> Result<(usize, SocketAddrV4), Ov6Error> {
        syscall::recv(self.local_port, buf)
    }

    pub fn send_to(&self, buf: &[u8], addr: SocketAddrV4) -> Result<usize, Ov6Error> {
        syscall::send(self.local_port, addr, buf)
    }
}

impl Drop for UdpSocket {
    fn drop(&mut self) {
        syscall::unbind(self.local_port).unwrap();
    }
}
