use core::{net::Ipv4Addr, num::Wrapping};

use dataview::{DataView, Pod, PodMethods as _};
use strum::FromRepr;

use super::{Eth, udp};

pub(super) fn handle_receive(eth: &Eth, eth_body: &[u8]) {
    let Some((ipv4, ipv4_body)) = eth_body.split_at_checked(size_of::<Ipv4>()) else {
        return;
    };

    let ipv4 = DataView::from(ipv4).get::<Ipv4>(0);
    let Some(protocol) = ipv4.protocol() else {
        return;
    };
    match protocol {
        IpProtocol::Icmp | IpProtocol::Tcp => {}
        IpProtocol::Udp => udp::handle_receive(eth, ipv4, ipv4_body),
    }
}

#[repr(C)]
#[derive(Debug, Pod)]
pub(super) struct Ipv4 {
    vhl: u8,
    tos: u8,
    len: [u8; 2],
    id: [u8; 2],
    off: [u8; 2],
    ttl: u8,
    protocol: u8,
    sum: [u8; 2],
    src: [u8; 4],
    dst: [u8; 4],
}

impl Ipv4 {
    pub(super) fn set_vhl(&mut self, ver: u8, hlen: u8) {
        assert!(ver <= 0b1111);
        assert!(hlen <= 0b1111);
        self.vhl = (ver << 4) | hlen;
    }

    pub(super) fn set_tos(&mut self, tos: u8) {
        self.tos = tos;
    }

    pub(super) fn set_len(&mut self, len: u16) {
        self.len = len.to_be_bytes();
    }

    pub(super) fn set_id(&mut self, id: u16) {
        self.id = id.to_be_bytes();
    }

    pub(super) fn set_off(&mut self, off: u16) {
        self.off = off.to_be_bytes();
    }

    pub(super) fn set_ttl(&mut self, ttl: u8) {
        self.ttl = ttl;
    }

    fn protocol(&self) -> Option<IpProtocol> {
        IpProtocol::from_repr(self.protocol)
    }

    pub(super) fn set_protocol(&mut self, protocol: IpProtocol) {
        self.protocol = protocol as u8;
    }

    pub(super) fn src(&self) -> Ipv4Addr {
        Ipv4Addr::from_bits(u32::from_be_bytes(self.src))
    }

    pub(super) fn set_src(&mut self, src: Ipv4Addr) {
        self.src = src.to_bits().to_be_bytes();
    }

    pub(super) fn set_dst(&mut self, dst: Ipv4Addr) {
        self.dst = dst.to_bits().to_be_bytes();
    }

    pub(super) fn compute_sum(&mut self) {
        self.sum = checksum(self.as_bytes()).to_be_bytes();
    }
}

#[repr(u8)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, FromRepr)]
pub(super) enum IpProtocol {
    Icmp = 1,
    Tcp = 6,
    Udp = 17,
}

fn checksum(bytes: &[u8]) -> u16 {
    let mut sum = bytes
        .chunks(2)
        .map(|chunk| match chunk {
            [a, b] => u16::from_be_bytes([*a, *b]),
            [a] => u16::from_be_bytes([*a, 0]),
            _ => unreachable!(),
        })
        .map(|n| Wrapping(u32::from(n)))
        .sum::<Wrapping<u32>>()
        .0;

    sum = (sum & 0xffff) + (sum >> 16);
    sum += sum >> 16;

    !u16::try_from(sum).unwrap()
}
