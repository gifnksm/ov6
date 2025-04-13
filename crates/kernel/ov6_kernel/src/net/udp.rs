use alloc::{boxed::Box, sync::Arc};
use core::{
    alloc::{AllocError, Allocator, Layout},
    mem::MaybeUninit,
    net::SocketAddrV4,
    ptr::NonNull,
};

use arraydeque::{ArrayDeque, Saturating};
use dataview::{DataView, Pod};
use once_init::OnceInit;
use slab_allocator::{ArcInnerLayout, SlabAllocator};

use super::{Eth, EthType, HOST_MAC, Ipv4, LOCAL_IP, LOCAL_MAC, ipv4::IpProtocol};
use crate::{
    device::e1000,
    error::KernelError,
    memory::{
        PAGE_SIZE,
        addr::{GenericMutSlice, GenericSlice},
        page::PageFrameAllocator,
        vm_user::UserPageTable,
    },
    sync::{SpinLock, SpinLockCondVar},
};

pub(super) fn handle_receive(_eth: &Eth, ipv4: &Ipv4, ipv4_body: &[u8]) {
    let Some((udp, udp_body)) = ipv4_body.split_at_checked(size_of::<Udp>()) else {
        return;
    };
    let udp = DataView::from(udp).get::<Udp>(0);
    let src_port = udp.src_port();
    let dst_port = udp.dst_port();
    let len = usize::from(udp.len()) - size_of::<Udp>();
    let Some((payload, _)) = udp_body.split_at_checked(len) else {
        return;
    };

    let ports = PORTS.lock();
    let Some(port) = ports.iter().flatten().find(|p| p.0 == dst_port) else {
        return;
    };
    let port = Arc::clone(&port.1);
    drop(ports);

    let Ok(data) = Box::try_new_zeroed_in(PageFrameAllocator) else {
        return;
    };
    let mut data: Box<[u8; PAGE_SIZE], PageFrameAllocator> = unsafe { data.assume_init() };
    data[..len].copy_from_slice(payload);

    let src = SocketAddrV4::new(ipv4.src(), src_port);
    let _ = port
        .queue
        .lock()
        .datagrams
        .push_back(Datagram { src, data, len });
    port.receive.notify();
}

pub fn bind(port: u16) -> Result<(), KernelError> {
    let mut ports = PORTS.lock();
    if ports.iter().flatten().any(|p| p.0 == port) {
        return Err(KernelError::PortAlreadyBound);
    }
    let Some(p) = ports.iter_mut().find(|p| p.is_none()) else {
        return Err(KernelError::NoFreePort);
    };

    *p = Some((port, PortRef::new_in(Port::new(true), PortAllocator)));

    Ok(())
}

pub fn unbind(port: u16) -> Result<(), KernelError> {
    let mut ports = PORTS.lock();
    let Some(p) = ports.iter_mut().find_map(|p| p.take_if(|p| p.0 == port)) else {
        return Err(KernelError::PortNotBound);
    };
    p.1.unbind();
    Ok(())
}

pub fn send(
    src_port: u16,
    dst: SocketAddrV4,
    bytes: &GenericSlice<u8>,
) -> Result<usize, KernelError> {
    let total_len = bytes.len() + size_of::<Eth>() + size_of::<Ipv4>() + size_of::<Udp>();
    if total_len > PAGE_SIZE {
        return Err(KernelError::TooLargeUdpPacket);
    }

    let mut tx = e1000::transmitter().ok_or(KernelError::NoSendBuffer)?;
    let buf = tx.buffer();
    let (eth, eth_body) = buf.split_at_mut(size_of::<Eth>());
    let eth = DataView::from_mut(eth).get_mut::<Eth>(0);
    eth.set_dhost(HOST_MAC);
    eth.set_shost(LOCAL_MAC);
    eth.set_ty(EthType::Ipv4);

    let (ipv4, ipv4_body) = eth_body.split_at_mut(size_of::<Ipv4>());
    let ipv4 = DataView::from_mut(ipv4).get_mut::<Ipv4>(0);
    ipv4.set_vhl(4, 5);
    ipv4.set_tos(0);
    ipv4.set_len(u16::try_from(total_len - size_of::<Eth>()).unwrap());
    ipv4.set_id(0);
    ipv4.set_off(0);
    ipv4.set_ttl(100);
    ipv4.set_protocol(IpProtocol::Udp);
    ipv4.set_src(LOCAL_IP);
    ipv4.set_dst(*dst.ip());
    ipv4.compute_sum();

    let (udp, udp_payload) = ipv4_body.split_at_mut(size_of::<Udp>());
    let udp = DataView::from_mut(udp).get_mut::<Udp>(0);
    udp.set_src_port(src_port);
    udp.set_dst_port(dst.port());
    udp.set_len(u16::try_from(bytes.len() + size_of::<Udp>()).unwrap());

    UserPageTable::copy_x2k_bytes(&mut udp_payload[..bytes.len()], bytes);

    tx.set_len(total_len);
    tx.send();

    Ok(bytes.len())
}

pub fn recv_from(
    port: u16,
    bytes: &mut GenericMutSlice<u8>,
) -> Result<(usize, SocketAddrV4), KernelError> {
    let ports = PORTS.lock();
    let Some(port) = ports.iter().flatten().find(|p| p.0 == port) else {
        return Err(KernelError::PortNotBound);
    };
    let port = Arc::clone(&port.1);
    drop(ports);

    let data = port.wait_receive()?;
    let copy_size = usize::min(data.len, bytes.len());
    UserPageTable::copy_k2x_bytes(&mut bytes.take_mut(copy_size), &data.data[..copy_size]);

    Ok((copy_size, data.src))
}

#[repr(C)]
#[derive(Debug, Pod)]
struct Udp {
    src_port: [u8; 2],
    dst_port: [u8; 2],
    len: [u8; 2],
    sum: [u8; 2],
}

impl Udp {
    fn src_port(&self) -> u16 {
        u16::from_be_bytes(self.src_port)
    }

    fn set_src_port(&mut self, port: u16) {
        self.src_port = port.to_be_bytes();
    }

    fn dst_port(&self) -> u16 {
        u16::from_be_bytes(self.dst_port)
    }

    fn set_dst_port(&mut self, port: u16) {
        self.dst_port = port.to_be_bytes();
    }

    fn len(&self) -> u16 {
        u16::from_be_bytes(self.len)
    }

    fn set_len(&mut self, len: u16) {
        self.len = len.to_be_bytes();
    }
}

const MAX_BIND_PORT: usize = 16;
const MAX_PORT_MSGS: usize = 16;

struct Port {
    receive: SpinLockCondVar,
    queue: SpinLock<PortQueue>,
}

impl Port {
    fn new(bound: bool) -> Self {
        Self {
            receive: SpinLockCondVar::new(),
            queue: SpinLock::new(PortQueue::new(bound)),
        }
    }

    fn unbind(&self) {
        let mut queue = self.queue.lock();
        queue.bound = false;
        self.receive.notify();
    }

    fn wait_receive(&self) -> Result<Datagram, KernelError> {
        let mut queue = self.queue.lock();
        while queue.bound {
            if let Some(data) = queue.datagrams.pop_front() {
                return Ok(data);
            }
            queue = self.receive.wait(queue).map_err(|(_, e)| e)?;
        }
        Err(KernelError::PortNotBound)
    }
}

struct PortQueue {
    bound: bool,
    datagrams: ArrayDeque<Datagram, MAX_PORT_MSGS, Saturating>,
}

impl PortQueue {
    fn new(bound: bool) -> Self {
        Self {
            bound,
            datagrams: ArrayDeque::new(),
        }
    }
}

struct Datagram {
    src: SocketAddrV4,
    data: Box<[u8; PAGE_SIZE], PageFrameAllocator>,
    len: usize,
}

type PortRef = Arc<Port, PortAllocator>;
type PortRefLayout = ArcInnerLayout<Port>;
static PORT_ALLOCATOR: OnceInit<SpinLock<SlabAllocator<PortRefLayout>>> = OnceInit::new();

pub(super) fn init() {
    static mut PORT_REF_MEMORY: [MaybeUninit<PortRefLayout>; MAX_BIND_PORT] =
        [const { MaybeUninit::uninit() }; MAX_BIND_PORT];

    unsafe {
        let start = (&raw mut PORT_REF_MEMORY[0]).cast::<PortRefLayout>();
        let end = start.add(MAX_BIND_PORT);
        let alloc = SlabAllocator::new(start..end);
        PORT_ALLOCATOR.init(SpinLock::new(alloc));
    }
}

#[derive(Clone)]
struct PortAllocator;

unsafe impl Allocator for PortAllocator {
    fn allocate(&self, layout: Layout) -> Result<NonNull<[u8]>, AllocError> {
        assert_eq!(layout, Layout::new::<PortRefLayout>());
        let Some(ptr) = PORT_ALLOCATOR.get().lock().allocate() else {
            return Err(AllocError);
        };
        Ok(NonNull::slice_from_raw_parts(ptr.cast(), layout.size()))
    }

    unsafe fn deallocate(&self, ptr: NonNull<u8>, _layout: Layout) {
        unsafe { PORT_ALLOCATOR.get().lock().deallocate(ptr.cast()) }
    }
}

static PORTS: SpinLock<[Option<(u16, PortRef)>; 16]> = SpinLock::new([const { None }; 16]);
