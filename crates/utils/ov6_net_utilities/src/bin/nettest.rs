use std::{env, net::UdpSocket, process, thread, time::Duration};

fn usage() -> ! {
    let arg0 = env::args().next().unwrap();
    eprintln!("Usage: {arg0} txone");
    eprintln!("       {arg0} rxone");
    eprintln!("       {arg0} rx");
    eprintln!("       {arg0} rx2");
    eprintln!("       {arg0} rxburst");
    eprintln!("       {arg0} tx");
    eprintln!("       {arg0} ping");
    process::exit(1);
}

fn main() {
    let args = env::args().collect::<Vec<String>>();
    if args.len() != 2 {
        usage();
    }

    let uid = u16::try_from(nix::unistd::getuid().as_raw() % 5000).unwrap();
    let server_port = uid + 25099;
    let fwd_port1 = uid + 25999;
    let fwd_port2 = uid + 30999;

    let command = args[1].as_str();

    match command {
        "txone" => {
            // Listen for a single UDP packet sent by ov6's nettest txone.
            let sock = UdpSocket::bind(("127.0.0.1", server_port)).unwrap();
            eprintln!("{command}: listening on a UDP packet");
            let mut buf = [0; 4096];
            let (len, _addr) = sock.recv_from(&mut buf).unwrap();
            let received = &buf[..len];
            assert_eq!(received, b"txone", "{command}: unexpected payload");
            eprintln!("{command}: OK");
        }
        "rxone" => {
            // sending a single UDP packet to ov6
            eprintln!("{command}: sending one UDP packet");
            let sock = UdpSocket::bind(("127.0.0.1", server_port)).unwrap();
            sock.send_to(b"xyz", ("127.0.0.1", fwd_port1)).unwrap();
        }
        "rx" => {
            // sending a slow stream of UDP packets, which should appear on port 2000
            let sock = UdpSocket::bind(("127.0.0.1", server_port)).unwrap();
            for i in 0.. {
                let txt = format!("packet {i}");
                eprintln!("{txt}");
                sock.send_to(txt.as_bytes(), ("127.0.0.1", fwd_port1))
                    .unwrap();
                thread::sleep(Duration::from_secs(1));
            }
        }
        "rx2" => {
            // sending to two different UDP ports
            let sock = UdpSocket::bind(("127.0.0.1", server_port)).unwrap();
            for i in 0.. {
                let txt = format!("one {i}");
                eprintln!("{txt}");
                sock.send_to(txt.as_bytes(), ("127.0.0.1", fwd_port1))
                    .unwrap();

                let txt = format!("two {i}");
                eprintln!("{txt}");
                sock.send_to(txt.as_bytes(), ("127.0.0.1", fwd_port2))
                    .unwrap();

                thread::sleep(Duration::from_secs(1));
            }
        }
        "rxburst" => {
            // sending a big burst of packets to 2001, then a packet to 2000.
            let sock = UdpSocket::bind(("127.0.0.1", server_port)).unwrap();
            for i in 0.. {
                for _ in 0..32 {
                    let txt = format!("packet {i}");
                    eprintln!("{txt}");
                    sock.send_to(txt.as_bytes(), ("127.0.0.1", fwd_port2))
                        .unwrap();
                }

                let txt = format!("packet {i}");
                eprintln!("{txt}");
                sock.send_to(txt.as_bytes(), ("127.0.0.1", fwd_port1))
                    .unwrap();

                thread::sleep(Duration::from_secs(1));
            }
        }
        "tx" => {
            let sock = UdpSocket::bind(("127.0.0.1", server_port)).unwrap();
            eprintln!("{command} Listening for UDP packets");

            let mut buf0 = [0; 4096];
            let (len0, _addr0) = sock.recv_from(&mut buf0).unwrap();
            let received0 = &buf0[..len0];
            assert_eq!(received0, b"t 0");

            let mut buf1 = [0; 4096];
            let (len1, _addr1) = sock.recv_from(&mut buf1).unwrap();
            let received1 = &buf1[..len1];
            assert_eq!(received1, b"t 1");
        }
        "ping" => {
            let sock = UdpSocket::bind(("127.0.0.1", server_port)).unwrap();
            eprintln!("{command}: listening for UDP packets");
            loop {
                let mut buf = [0; 4096];
                let (len, raddr) = sock.recv_from(&mut buf).unwrap();
                let received = &buf[..len];
                sock.send_to(received, raddr).unwrap();
            }
        }
        _ => usage(),
    }
}
