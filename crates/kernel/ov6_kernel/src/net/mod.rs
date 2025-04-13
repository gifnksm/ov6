use core::net::Ipv4Addr;

use self::{
    ethernet::{Eth, EthType},
    ipv4::Ipv4,
};

mod arp;
mod ethernet;
mod ipv4;
pub mod udp;

const LOCAL_MAC: [u8; ethernet::ADDR_LEN] = [0x52, 0x54, 0x00, 0x12, 0x34, 0x56];
const LOCAL_IP: Ipv4Addr = Ipv4Addr::new(10, 0, 2, 15);

const HOST_MAC: [u8; ethernet::ADDR_LEN] = [0x52, 0x55, 0x0a, 0x00, 0x02, 0x02];

pub fn handle_receive(bytes: &[u8]) {
    ethernet::handle_receive(bytes);
}

pub fn init() {
    udp::init();
}
