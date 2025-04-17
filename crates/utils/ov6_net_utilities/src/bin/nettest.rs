use std::{env, net::UdpSocket, process, thread, time::Duration};

fn usage() -> ! {
    let arg0 = env::args().next().unwrap();
    eprintln!(
        "Usage: {arg0} [--fwd-port1 <port>] [--fwd-port2 <port>] [--server-port <port>] <command>"
    );
    process::exit(1);
}

fn bind_server(command: &str, server_port: u16) -> UdpSocket {
    let sock = UdpSocket::bind(("127.0.0.1", server_port)).unwrap();
    let port = sock.local_addr().unwrap().port();
    println!("{command}: server UDP port is {port}");
    sock
}

struct Args {
    server_port: u16,
    fwd_port1: u16,
    fwd_port2: u16,
    command: String,
}

impl Args {
    fn parse() -> Self {
        let mut args = env::args();
        let _ = args.next(); // skip the program name

        let uid = u16::try_from(nix::unistd::getuid().as_raw() % 5000).unwrap();
        let mut server_port = uid + 25099;
        let mut fwd_port1 = uid + 25999;
        let mut fwd_port2 = uid + 30999;

        let mut command = None;

        while let Some(arg) = args.next() {
            match arg.as_str() {
                "--server-port" => {
                    let Some(arg) = args.next().and_then(|s| s.parse().ok()) else {
                        usage();
                    };
                    server_port = arg;
                }
                "--fwd-port1" => {
                    let Some(arg) = args.next().and_then(|s| s.parse().ok()) else {
                        usage();
                    };
                    fwd_port1 = arg;
                }
                "--fwd-port2" => {
                    let Some(arg) = args.next().and_then(|s| s.parse().ok()) else {
                        usage();
                    };
                    fwd_port2 = arg;
                }
                s if s.starts_with('-') => usage(),
                _ => command = Some(arg),
            }
        }

        let Some(command) = command else {
            usage();
        };

        Self {
            server_port,
            fwd_port1,
            fwd_port2,
            command,
        }
    }
}

fn main() {
    let Args {
        server_port,
        fwd_port1,
        fwd_port2,
        command,
    } = Args::parse();

    match command.as_str() {
        "txone" => {
            // Listen for a single UDP packet sent by ov6's nettest txone.
            let sock = bind_server(&command, server_port);
            println!("{command}: listening on a UDP packet");
            let mut buf = [0; 4096];
            let (len, _addr) = sock.recv_from(&mut buf).unwrap();
            let received = &buf[..len];
            assert_eq!(received, b"txone", "{command}: unexpected payload");
            println!("{command}: OK");
        }
        "rxone" => {
            // sending a single UDP packet to ov6
            println!("{command}: sending one UDP packet");
            let sock = bind_server(&command, server_port);
            sock.send_to(b"xyz", ("127.0.0.1", fwd_port1)).unwrap();
        }
        "rx" => {
            // sending a slow stream of UDP packets, which should appear on port 2000
            let sock = bind_server(&command, server_port);
            for i in 0.. {
                let txt = format!("packet {i}");
                println!("{txt}");
                sock.send_to(txt.as_bytes(), ("127.0.0.1", fwd_port1))
                    .unwrap();
                thread::sleep(Duration::from_secs(1));
            }
        }
        "rx2" => {
            // sending to two different UDP ports
            let sock = bind_server(&command, server_port);
            for i in 0.. {
                let txt = format!("one {i}");
                println!("{txt}");
                sock.send_to(txt.as_bytes(), ("127.0.0.1", fwd_port1))
                    .unwrap();

                let txt = format!("two {i}");
                println!("{txt}");
                sock.send_to(txt.as_bytes(), ("127.0.0.1", fwd_port2))
                    .unwrap();

                thread::sleep(Duration::from_secs(1));
            }
        }
        "rxburst" => {
            // sending a big burst of packets to 2001, then a packet to 2000.
            let sock = bind_server(&command, server_port);
            for i in 0.. {
                for _ in 0..32 {
                    let txt = format!("packet {i}");
                    println!("{txt}");
                    sock.send_to(txt.as_bytes(), ("127.0.0.1", fwd_port2))
                        .unwrap();
                }

                let txt = format!("packet {i}");
                println!("{txt}");
                sock.send_to(txt.as_bytes(), ("127.0.0.1", fwd_port1))
                    .unwrap();

                thread::sleep(Duration::from_secs(1));
            }
        }
        "tx" => {
            let sock = bind_server(&command, server_port);
            println!("{command} Listening for UDP packets");

            let mut buf0 = [0; 4096];
            let (len0, _addr0) = sock.recv_from(&mut buf0).unwrap();
            let received0 = &buf0[..len0];
            assert_eq!(received0, b"t 0");

            let mut buf1 = [0; 4096];
            let (len1, _addr1) = sock.recv_from(&mut buf1).unwrap();
            let received1 = &buf1[..len1];
            assert_eq!(received1, b"t 1");
            println!("{command}: OK");
        }
        "ping" => {
            let sock = bind_server(&command, server_port);
            println!("{command}: listening for UDP packets");
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
