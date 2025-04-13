use dataview::{DataView, Pod};
use strum::FromRepr;

use crate::net::{arp, ipv4};

pub(super) fn handle_receive(bytes: &[u8]) {
    let Some((eth, eth_body)) = bytes.split_at_checked(size_of::<Eth>()) else {
        return;
    };

    let eth = DataView::from(eth).get::<Eth>(0);
    let Some(ty) = eth.ty() else {
        return;
    };

    match ty {
        EthType::Ipv4 => ipv4::handle_receive(eth, eth_body),
        EthType::Arp => arp::handle_receive(eth, eth_body),
    }
}

pub(super) const ADDR_LEN: usize = 6;

#[repr(C, packed)]
#[derive(Debug, Pod)]
pub(super) struct Eth {
    dhost: [u8; ADDR_LEN],
    shost: [u8; ADDR_LEN],
    ty: [u8; 2],
}

const _: () = {
    assert!(core::mem::size_of::<Eth>() == 14);
    assert!(core::mem::align_of::<Eth>() == 1);
};

#[repr(u16)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, FromRepr)]
pub(super) enum EthType {
    Ipv4 = 0x0800,
    Arp = 0x0806,
}

impl Eth {
    pub(super) fn set_dhost(&mut self, addr: [u8; ADDR_LEN]) {
        self.dhost = addr;
    }

    pub(super) fn shost(&self) -> [u8; ADDR_LEN] {
        self.shost
    }

    pub(super) fn set_shost(&mut self, addr: [u8; ADDR_LEN]) {
        self.shost = addr;
    }

    pub(super) fn ty(&self) -> Option<EthType> {
        let ty = u16::from_be_bytes(self.ty);
        EthType::from_repr(ty)
    }

    pub(super) fn set_ty(&mut self, ty: EthType) {
        self.ty = (ty as u16).to_be_bytes();
    }
}
