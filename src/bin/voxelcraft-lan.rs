//! Headless LAN protocol exerciser, useful on CI machines without a GPU.
use std::net::SocketAddr;
use std::time::{Duration, Instant};
use voxelcraft::lan::{Connection, Input, Packet, headless};
fn run() -> Result<(), Box<dyn std::error::Error>> {
    let mut args = std::env::args().skip(1);
    let mut host = None;
    let mut join = None;
    let mut ticks = 200;
    let mut profile = "Loopback".to_string();
    let mut command = None;
    while let Some(arg) = args.next() {
        let value = args.next().ok_or("expected flag value")?;
        match arg.as_str() {
            "--host-lan" => host = Some(value.parse::<SocketAddr>()?),
            "--join" => join = Some(value.parse::<SocketAddr>()?),
            "--ticks" => ticks = value.parse::<u32>()?,
            "--profile" => profile = value,
            "--command" => command = Some(value),
            _ => return Err(format!("unknown flag {arg}").into()),
        }
    }
    if let Some(address) = host {
        let mut server = headless::generated_host(address, 12345)?;
        println!("LAN harness listening on {}", server.address());
        for _ in 0..ticks {
            let start = Instant::now();
            server.step()?;
            std::thread::sleep(headless::STEP.saturating_sub(start.elapsed()));
        }
        server.close();
        return Ok(());
    }
    if let Some(address) = join {
        let start = Instant::now();
        let mut client = Connection::connect(address, &profile)?;
        let mut chunks = 0;
        let mut connected = false;
        for tick in 1..=ticks {
            for packet in client.poll()? {
                match packet {
                    Packet::Welcome { .. } => {
                        connected = true;
                        println!("handshake in {:.2} ms", start.elapsed().as_secs_f64() * 1000.0);
                        if let Some(command) = command.take() {
                            client.send(Packet::Command(command))?;
                        }
                    }
                    Packet::Chunk { .. } => {
                        chunks += 1;
                        if chunks == 1 {
                            println!("first chunk in {:.2} ms", start.elapsed().as_secs_f64() * 1000.0);
                        }
                    }
                    Packet::Chat(message) => println!("{message}"),
                    Packet::Disconnect(reason) => {
                        println!("{reason}");
                        return Ok(());
                    }
                    _ => {}
                }
            }
            if connected {
                client.send(Packet::Input(Input { sequence: tick, ..Default::default() }))?;
            }
            std::thread::sleep(Duration::from_millis(50));
        }
        println!(
            "received {chunks} chunk snapshots, {} bytes; sent {} bytes",
            client.received_bytes, client.sent_bytes
        );
        client.close("Player left");
        return Ok(());
    }
    Err("usage: voxelcraft-lan --host-lan IP:PORT | --join IP:PORT [--ticks N] [--profile NAME] [--command TEXT]"
        .into())
}
fn main() {
    if let Err(e) = run() {
        eprintln!("LAN: {e}");
        std::process::exit(1);
    }
}
