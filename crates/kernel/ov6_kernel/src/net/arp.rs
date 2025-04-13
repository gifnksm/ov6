use core::{
    net::Ipv4Addr,
    sync::atomic::{AtomicBool, Ordering},
};

use dataview::{DataView, Pod};
use safe_cast::to_u8;
use strum::FromRepr;

use super::ethernet;
use crate::{
    device::e1000,
    net::{
        LOCAL_IP, LOCAL_MAC,
        ethernet::{Eth, EthType},
    },
};

pub(super) fn handle_receive(eth: &Eth, eth_body: &[u8]) {
    static ARP_SEEN: AtomicBool = AtomicBool::new(false);

    let Some((arp, _arp_body)) = eth_body.split_at_checked(size_of::<Arp>()) else {
        return;
    };

    if ARP_SEEN.swap(true, Ordering::Acquire) {
        return;
    }

    let arp = DataView::from(arp).get::<Arp>(0);

    let mut tx = e1000::transmitter().unwrap();
    let out = tx.buffer();
    let out_len = size_of::<Eth>() + size_of::<Arp>();

    let (out_eth, out_eth_body) = out.split_at_mut(size_of::<Eth>());
    let out_eth = DataView::from_mut(out_eth).get_mut::<Eth>(0);
    out_eth.set_dhost(eth.shost());
    out_eth.set_shost(LOCAL_MAC);
    out_eth.set_ty(EthType::Arp);

    let (out_arp, _out_arp_body) = out_eth_body.split_at_mut(size_of::<Arp>());
    let out_arp = DataView::from_mut(out_arp).get_mut::<Arp>(0);
    out_arp.set_htype(ArpHardware::Ethernet);
    out_arp.set_ptype(EthType::Ipv4);
    out_arp.set_hlen(to_u8!(ethernet::ADDR_LEN));
    out_arp.set_plen(to_u8!(size_of::<u32>()));
    out_arp.set_opcode(ArpOp::Reply);

    out_arp.set_sender_haddr(LOCAL_MAC);
    out_arp.set_sender_paddr(LOCAL_IP);
    out_arp.set_target_haddr(eth.shost());
    out_arp.set_target_paddr(arp.sender_paddr());

    tx.set_len(out_len);
    tx.send();
}

#[repr(C, packed)]
#[derive(Debug, Pod)]
pub(super) struct Arp {
    /// Hardware type
    htype: [u8; 2],
    /// Protocol type
    ptype: [u8; 2],
    /// Hardware address length
    hlen: u8,
    /// Protocol address length
    plen: u8,
    /// Operation code
    opcode: [u8; 2],
    /// Sender hardware address
    sender_haddr: [u8; ethernet::ADDR_LEN],
    /// Sender protocol address
    sender_paddr: [u8; 4],
    /// Target hardware address
    target_haddr: [u8; ethernet::ADDR_LEN],
    /// Target protocol address
    target_paddr: [u8; 4],
}

#[repr(u16)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, FromRepr)]
enum ArpHardware {
    Ethernet = 1,
}

#[repr(u16)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, FromRepr)]
#[expect(dead_code)]
enum ArpOp {
    Request = 1,
    Reply = 2,
}

impl Arp {
    fn set_htype(&mut self, ty: ArpHardware) {
        self.htype = (ty as u16).to_be_bytes();
    }

    fn set_ptype(&mut self, ty: EthType) {
        self.ptype = (ty as u16).to_be_bytes();
    }

    fn set_hlen(&mut self, len: u8) {
        self.hlen = len;
    }

    fn set_plen(&mut self, len: u8) {
        self.plen = len;
    }

    fn set_opcode(&mut self, op: ArpOp) {
        self.opcode = (op as u16).to_be_bytes();
    }

    fn set_sender_haddr(&mut self, addr: [u8; ethernet::ADDR_LEN]) {
        self.sender_haddr = addr;
    }

    fn sender_paddr(&self) -> u32 {
        u32::from_be_bytes(self.sender_paddr)
    }

    fn set_sender_paddr(&mut self, addr: Ipv4Addr) {
        self.sender_paddr = addr.to_bits().to_be_bytes();
    }

    fn set_target_haddr(&mut self, addr: [u8; ethernet::ADDR_LEN]) {
        self.target_haddr = addr;
    }

    fn set_target_paddr(&mut self, addr: u32) {
        self.target_paddr = addr.to_be_bytes();
    }
}
