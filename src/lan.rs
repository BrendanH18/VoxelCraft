//! VoxelCraft LAN protocol v1. Nonblocking, bounded TCP; all simulation stays on the authority.
pub mod headless;

use std::collections::VecDeque;
use std::io::{self, Read, Write};
use std::net::{SocketAddr, TcpListener, TcpStream, UdpSocket};
use std::sync::Arc;
use std::time::{Duration, Instant};

use crate::world::{
    block::Block,
    chunk::{CHUNK_VOLUME, ChunkData},
};
use glam::IVec3;

pub const PROTOCOL: u16 = 1;
pub const GAME_VERSION: &str = env!("CARGO_PKG_VERSION");
pub const MAX_FRAME: usize = 256 * 1024;
const MAX_QUEUE: usize = 2 * 1024 * 1024;
pub const DISCOVERY_PORT: u16 = 4445;

#[derive(Clone, Copy, Debug, Default)]
pub struct Input {
    pub sequence: u32,
    pub yaw: f32,
    pub pitch: f32,
    pub forward: f32,
    pub right: f32,
    /// jump, descend, sprint, attack, use, flying.
    pub buttons: u8,
    pub selected: u8,
}
impl Input {
    pub fn movement(self) -> crate::player::MoveInput {
        crate::player::MoveInput {
            forward: self.forward as f64,
            right: self.right as f64,
            jump: self.buttons & 1 != 0,
            descend: self.buttons & 2 != 0,
            sprint: self.buttons & 4 != 0,
        }
    }
}

#[derive(Clone)]
pub enum Packet {
    Hello {
        protocol: u16,
        version: String,
        profile: String,
    },
    Welcome {
        seed: u64,
        dimension: String,
        id: u32,
    },
    Input(Input),
    /// Immediate action on the same validated authority as CLI players.
    Command(String),
    /// Slot click: encoded slot, right button, shift click.
    Click {
        slot: u16,
        right: bool,
        shift: bool,
    },
    Chat(String),
    State(serde_json::Value),
    Chunk {
        pos: IVec3,
        data: Arc<ChunkData>,
    },
    Disconnect(String),
}
fn bad(message: &str) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, message)
}
fn string(out: &mut Vec<u8>, s: &str) {
    out.extend_from_slice(&(s.len() as u16).to_le_bytes());
    out.extend_from_slice(s.as_bytes());
}

impl Packet {
    pub fn encode(&self) -> io::Result<Vec<u8>> {
        let mut out = Vec::new();
        match self {
            Self::Hello { protocol, version, profile } => {
                if version.len() > 32 || profile.len() > 24 {
                    return Err(bad("invalid handshake text"));
                }
                out.push(0);
                out.extend_from_slice(&protocol.to_le_bytes());
                string(&mut out, version);
                string(&mut out, profile);
            }
            Self::Welcome { seed, dimension, id } => {
                out.push(1);
                out.extend_from_slice(&seed.to_le_bytes());
                string(&mut out, dimension);
                out.extend_from_slice(&id.to_le_bytes());
            }
            Self::Input(i) => {
                out.push(2);
                out.extend_from_slice(&i.sequence.to_le_bytes());
                for f in [i.yaw, i.pitch, i.forward, i.right] {
                    out.extend_from_slice(&f.to_le_bytes());
                }
                out.extend_from_slice(&[i.buttons, i.selected]);
            }
            Self::Command(s) | Self::Chat(s) | Self::Disconnect(s) => {
                if s.len() > 1024 {
                    return Err(bad("text exceeds 1024 bytes"));
                }
                out.push(match self {
                    Self::Command(_) => 3,
                    Self::Chat(_) => 4,
                    _ => 7,
                });
                string(&mut out, s);
            }
            Self::State(v) => {
                out.push(5);
                out.extend_from_slice(&serde_json::to_vec(v)?);
            }
            Self::Chunk { pos, data } => {
                out.push(6);
                for p in pos.to_array() {
                    out.extend_from_slice(&p.to_le_bytes());
                }
                let mut row = [Block::AIR; 32];
                let mut previous = Block::AIR;
                let mut run = 0u16;
                for start in (0..CHUNK_VOLUME).step_by(32) {
                    data.copy_row(start, &mut row);
                    for b in row {
                        if run > 0 && b != previous {
                            out.extend_from_slice(&run.to_le_bytes());
                            out.extend_from_slice(&previous.0.to_le_bytes());
                            run = 0;
                        }
                        previous = b;
                        run += 1;
                    }
                }
                out.extend_from_slice(&run.to_le_bytes());
                out.extend_from_slice(&previous.0.to_le_bytes());
            }
            Self::Click { slot, right, shift } => {
                out.push(8);
                out.extend_from_slice(&slot.to_le_bytes());
                out.extend_from_slice(&[u8::from(*right), u8::from(*shift)]);
            }
        }
        if out.len() > MAX_FRAME {
            return Err(bad("frame too large"));
        }
        let mut frame = (out.len() as u32).to_le_bytes().to_vec();
        frame.extend(out);
        Ok(frame)
    }
    pub fn decode(bytes: &[u8]) -> io::Result<Self> {
        let mut r = Reader(bytes);
        let packet = match r.u8()? {
            0 => Self::Hello { protocol: r.u16()?, version: r.text(32)?, profile: r.text(24)? },
            1 => Self::Welcome { seed: r.u64()?, dimension: r.text(16)?, id: r.u32()? },
            2 => {
                let i = Input {
                    sequence: r.u32()?,
                    yaw: r.f32()?,
                    pitch: r.f32()?,
                    forward: r.f32()?,
                    right: r.f32()?,
                    buttons: r.u8()?,
                    selected: r.u8()?,
                };
                if !i.yaw.is_finite()
                    || !i.pitch.is_finite()
                    || !i.forward.is_finite()
                    || !i.right.is_finite()
                    || i.forward.abs() > 1.0
                    || i.right.abs() > 1.0
                    || i.selected >= 9
                    || i.buttons & !63 != 0
                {
                    return Err(bad("invalid player input"));
                }
                Self::Input(i)
            }
            tag @ (3 | 4 | 7) => {
                let s = r.text(1024)?;
                match tag {
                    3 => Self::Command(s),
                    4 => Self::Chat(s),
                    _ => Self::Disconnect(s),
                }
            }
            5 => {
                let value = serde_json::from_slice(r.0)?;
                r.0 = &[];
                Self::State(value)
            }
            6 => {
                let pos = IVec3::new(r.i32()?, r.i32()?, r.i32()?);
                if pos.x.abs_diff(0) > 937_500 || pos.z.abs_diff(0) > 937_500 || !(-2..10).contains(&pos.y) {
                    return Err(bad("invalid chunk position"));
                }
                let mut blocks = ChunkData::new_dense(Block::AIR);
                let mut cursor = 0;
                while !r.0.is_empty() {
                    let run = r.u16()? as usize;
                    let b = Block(r.u16()?);
                    if run == 0 || cursor + run > CHUNK_VOLUME || b.0 as usize >= crate::world::block::STATE_CAPACITY {
                        return Err(bad("invalid chunk run"));
                    }
                    blocks[cursor..cursor + run].fill(b);
                    cursor += run;
                }
                if cursor != CHUNK_VOLUME {
                    return Err(bad("incomplete chunk"));
                }
                Self::Chunk { pos, data: Arc::new(ChunkData::from_dense(blocks)) }
            }
            8 => {
                let slot = r.u16()?;
                let right = r.u8()?;
                let shift = r.u8()?;
                if right > 1 || shift > 1 {
                    return Err(bad("invalid click"));
                }
                Self::Click { slot, right: right != 0, shift: shift != 0 }
            }
            _ => return Err(bad("unknown packet")),
        };
        if !r.0.is_empty() {
            return Err(bad("trailing packet bytes"));
        }
        Ok(packet)
    }
}
struct Reader<'a>(&'a [u8]);
impl Reader<'_> {
    fn take<const N: usize>(&mut self) -> io::Result<[u8; N]> {
        if self.0.len() < N {
            return Err(bad("truncated packet"));
        }
        let (a, b) = self.0.split_at(N);
        self.0 = b;
        Ok(a.try_into().unwrap())
    }
    fn u8(&mut self) -> io::Result<u8> {
        Ok(self.take::<1>()?[0])
    }
    fn u16(&mut self) -> io::Result<u16> {
        Ok(u16::from_le_bytes(self.take()?))
    }
    fn u32(&mut self) -> io::Result<u32> {
        Ok(u32::from_le_bytes(self.take()?))
    }
    fn i32(&mut self) -> io::Result<i32> {
        Ok(i32::from_le_bytes(self.take()?))
    }
    fn u64(&mut self) -> io::Result<u64> {
        Ok(u64::from_le_bytes(self.take()?))
    }
    fn f32(&mut self) -> io::Result<f32> {
        Ok(f32::from_le_bytes(self.take()?))
    }
    fn text(&mut self, max: usize) -> io::Result<String> {
        let n = self.u16()? as usize;
        if n > max || self.0.len() < n {
            return Err(bad("invalid text length"));
        }
        let (a, b) = self.0.split_at(n);
        self.0 = b;
        Ok(std::str::from_utf8(a).map_err(io::Error::other)?.to_owned())
    }
}

pub struct Connection {
    stream: TcpStream,
    read: Vec<u8>,
    write: VecDeque<Vec<u8>>,
    offset: usize,
    queued: usize,
    last_read: Instant,
    pub sent_bytes: u64,
    pub received_bytes: u64,
}
impl Connection {
    pub fn new(stream: TcpStream) -> io::Result<Self> {
        stream.set_nonblocking(true)?;
        stream.set_nodelay(true)?;
        Ok(Self {
            stream,
            read: Vec::new(),
            write: VecDeque::new(),
            offset: 0,
            queued: 0,
            last_read: Instant::now(),
            sent_bytes: 0,
            received_bytes: 0,
        })
    }
    pub fn connect(address: SocketAddr, profile: &str) -> io::Result<Self> {
        if !crate::control::valid_name(profile) {
            return Err(bad("profile must contain 1..24 letters, digits or underscores"));
        }
        let stream = TcpStream::connect_timeout(&address, Duration::from_secs(3))?;
        let mut c = Self::new(stream)?;
        c.send(Packet::Hello { protocol: PROTOCOL, version: GAME_VERSION.into(), profile: profile.into() })?;
        c.flush()?;
        Ok(c)
    }
    pub fn queued_bytes(&self) -> usize {
        self.queued
    }
    pub fn send(&mut self, packet: Packet) -> io::Result<()> {
        let frame = packet.encode()?;
        if self.queued + frame.len() > MAX_QUEUE {
            return Err(bad("client is too slow"));
        }
        self.queued += frame.len();
        self.write.push_back(frame);
        Ok(())
    }
    pub fn flush(&mut self) -> io::Result<()> {
        let mut budget = MAX_FRAME;
        while budget > 0 {
            let Some(frame) = self.write.front() else {
                break;
            };
            match self.stream.write(&frame[self.offset..frame.len().min(self.offset + budget)]) {
                Ok(0) => return Err(io::Error::new(io::ErrorKind::BrokenPipe, "Connection lost")),
                Ok(n) => {
                    self.offset += n;
                    self.queued -= n;
                    budget -= n;
                    self.sent_bytes += n as u64;
                    if self.offset == frame.len() {
                        self.write.pop_front();
                        self.offset = 0;
                    }
                }
                Err(e) if e.kind() == io::ErrorKind::WouldBlock => break,
                Err(e) => return Err(e),
            }
        }
        Ok(())
    }
    pub fn poll(&mut self) -> io::Result<Vec<Packet>> {
        self.flush()?;
        let mut packets = Vec::new();
        let mut buf = [0; 8192];
        let mut budget = MAX_FRAME;
        // Parse incrementally, retaining incomplete frames across WouldBlock.
        loop {
            while self.read.len() >= 4 {
                let n = u32::from_le_bytes(self.read[..4].try_into().unwrap()) as usize;
                if n == 0 || n > MAX_FRAME {
                    return Err(bad("frame length outside bounds"));
                }
                if self.read.len() < n + 4 {
                    break;
                }
                packets.push(Packet::decode(&self.read[4..n + 4])?);
                self.read.drain(..n + 4);
                if packets.len() >= 64 {
                    return Ok(packets);
                }
            }
            if budget == 0 {
                break;
            }
            let take = buf.len().min(budget);
            match self.stream.read(&mut buf[..take]) {
                // Deliver a final disconnect before observing EOF next poll.
                Ok(0) if !packets.is_empty() => return Ok(packets),
                Ok(0) => return Err(io::Error::new(io::ErrorKind::UnexpectedEof, "Connection lost")),
                Ok(n) => {
                    self.read.extend_from_slice(&buf[..n]);
                    budget -= n;
                    self.received_bytes += n as u64;
                    self.last_read = Instant::now();
                }
                Err(e) if e.kind() == io::ErrorKind::WouldBlock => break,
                Err(e) => return Err(e),
            }
        }
        if self.last_read.elapsed() > Duration::from_secs(15) {
            return Err(io::Error::new(io::ErrorKind::TimedOut, "Connection timed out"));
        }
        Ok(packets)
    }
    pub fn close(&mut self, reason: &str) {
        let _ = self.send(Packet::Disconnect(reason.into()));
        // Shutdown is the only blocking write. Cap the entire drain at 100 ms,
        // including a final reason after any partially transmitted frame.
        let deadline = Instant::now() + Duration::from_millis(100);
        let _ = self.stream.set_nonblocking(false);
        while let Some(frame) = self.write.front() {
            let remaining = deadline.saturating_duration_since(Instant::now());
            if remaining.is_zero() {
                break;
            }
            let _ = self.stream.set_write_timeout(Some(remaining));
            match self.stream.write(&frame[self.offset..]) {
                Ok(0) | Err(_) => break,
                Ok(n) => {
                    self.offset += n;
                    self.queued -= n;
                    self.sent_bytes += n as u64;
                    if self.offset == frame.len() {
                        self.write.pop_front();
                        self.offset = 0;
                    }
                }
            }
        }
        let _ = self.stream.shutdown(std::net::Shutdown::Both);
    }
}
pub struct Listener {
    listener: TcpListener,
    pub address: SocketAddr,
    announce: UdpSocket,
    last_announce: Instant,
}
impl Listener {
    pub fn bind(address: SocketAddr) -> io::Result<Self> {
        let listener = TcpListener::bind(address)?;
        listener.set_nonblocking(true)?;
        let address = listener.local_addr()?;
        let announce = UdpSocket::bind("0.0.0.0:0")?;
        announce.set_nonblocking(true)?;
        Ok(Self { listener, address, announce, last_announce: Instant::now() - Duration::from_secs(2) })
    }
    pub fn accept(&self) -> io::Result<Option<Connection>> {
        match self.listener.accept() {
            Ok((s, _)) => Connection::new(s).map(Some),
            Err(e) if e.kind() == io::ErrorKind::WouldBlock => Ok(None),
            Err(e) => Err(e),
        }
    }
    pub fn announce(&mut self, name: &str) {
        if self.last_announce.elapsed() < Duration::from_millis(1500) {
            return;
        }
        self.last_announce = Instant::now();
        let text = format!(
            "[VX1][MOTD]{}[/MOTD][AD]{}[/AD]",
            name.chars().filter(|c| !c.is_control() && *c != '[' && *c != ']').take(64).collect::<String>(),
            self.address.port()
        );
        let _ = self.announce.send_to(text.as_bytes(), (std::net::Ipv4Addr::new(224, 0, 2, 60), DISCOVERY_PORT));
    }
}
#[derive(Clone)]
pub struct Advertisement {
    pub address: SocketAddr,
    pub name: String,
    pub seen: Instant,
}
pub struct Discovery {
    socket: UdpSocket,
    pub servers: Vec<Advertisement>,
}
impl Discovery {
    pub fn new() -> io::Result<Self> {
        let socket = UdpSocket::bind((std::net::Ipv4Addr::UNSPECIFIED, DISCOVERY_PORT))?;
        socket.join_multicast_v4(&std::net::Ipv4Addr::new(224, 0, 2, 60), &std::net::Ipv4Addr::UNSPECIFIED)?;
        socket.set_nonblocking(true)?;
        Ok(Self { socket, servers: Vec::new() })
    }
    pub fn poll(&mut self) {
        let mut buf = [0; 512];
        for _ in 0..32 {
            let Ok((n, from)) = self.socket.recv_from(&mut buf) else {
                break;
            };
            let Ok(text) = std::str::from_utf8(&buf[..n]) else {
                continue;
            };
            let Some((name, port)) = text.strip_prefix("[VX1][MOTD]").and_then(|s| s.split_once("[/MOTD][AD]")) else {
                continue;
            };
            let Some(port) = port.strip_suffix("[/AD]").and_then(|s| s.parse::<u16>().ok()).filter(|p| *p != 0) else {
                continue;
            };
            let address = SocketAddr::new(from.ip(), port);
            self.servers.retain(|s| s.address != address);
            if self.servers.len() < 32 {
                self.servers.push(Advertisement {
                    address,
                    name: name.chars().filter(|c| !c.is_control()).take(64).collect(),
                    seen: Instant::now(),
                });
            }
        }
        self.servers.retain(|s| s.seen.elapsed() < Duration::from_secs(5));
    }
}

pub fn handshake(packet: &Packet) -> Result<&str, String> {
    let Packet::Hello { protocol, version, profile } = packet else {
        return Err("Expected LAN handshake".into());
    };
    if *protocol != PROTOCOL || version != GAME_VERSION {
        return Err(format!(
            "Version mismatch: host {GAME_VERSION} / protocol {PROTOCOL}, client {version} / protocol {protocol}"
        ));
    }
    if !crate::control::valid_name(profile) {
        return Err("Invalid player profile".into());
    }
    Ok(profile)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn binary_codec_preserves_extended_states_and_input() {
        let mut data = ChunkData::Uniform(Block::AIR);
        data.set(1, 2, 3, Block(1000));
        let packet = Packet::Chunk { pos: IVec3::new(-1, 4, 2), data: Arc::new(data) };
        let encoded = packet.encode().unwrap();
        assert!(encoded.len() < 100, "uniform regions should compress well");
        let Packet::Chunk { pos, data } = Packet::decode(&encoded[4..]).unwrap() else { panic!("wrong packet") };
        assert_eq!(pos, IVec3::new(-1, 4, 2));
        assert_eq!(data.get(1, 2, 3), Block(1000));
        let input = Input { sequence: 42, yaw: 1.5, pitch: -0.5, forward: 1.0, right: -1.0, buttons: 63, selected: 8 };
        let encoded = Packet::Input(input).encode().unwrap();
        let Packet::Input(decoded) = Packet::decode(&encoded[4..]).unwrap() else { panic!("wrong packet") };
        assert_eq!(
            (
                decoded.sequence,
                decoded.yaw,
                decoded.pitch,
                decoded.forward,
                decoded.right,
                decoded.buttons,
                decoded.selected
            ),
            (42, 1.5, -0.5, 1.0, -1.0, 63, 8)
        );
    }
    #[test]
    fn malformed_packets_and_oversized_text_are_rejected() {
        for input in [
            Input { yaw: f32::NAN, ..Default::default() },
            Input { forward: 1.1, ..Default::default() },
            Input { selected: 9, ..Default::default() },
            Input { buttons: 128, ..Default::default() },
        ] {
            assert!(Packet::decode(&Packet::Input(input).encode().unwrap()[4..]).is_err());
        }
        assert!(Packet::Chat("x".repeat(1025)).encode().is_err());
        assert!(Packet::decode(&[8, 0, 0, 2, 0]).is_err());
        assert!(Packet::decode(&[9]).is_err());
        let mut chunk = vec![6];
        chunk.extend_from_slice(&[0; 12]);
        chunk.extend_from_slice(&[0, 0, 0, 0]);
        assert!(Packet::decode(&chunk).is_err(), "zero runs cannot allocate or loop forever");
        chunk.truncate(13);
        chunk.extend_from_slice(&32768u16.to_le_bytes());
        chunk.extend_from_slice(&4096u16.to_le_bytes());
        assert!(Packet::decode(&chunk).is_err(), "out-of-registry states must be rejected");
        chunk.truncate(13);
        chunk.extend_from_slice(&1u16.to_le_bytes());
        chunk.extend_from_slice(&0u16.to_le_bytes());
        assert!(Packet::decode(&chunk).is_err(), "partial chunks must not render");
    }
    #[test]
    fn fragmented_frames_and_invalid_length() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let mut stream = TcpStream::connect(listener.local_addr().unwrap()).unwrap();
        let (accepted, _) = listener.accept().unwrap();
        let mut connection = Connection::new(accepted).unwrap();
        let frame = Packet::Chat("fragmented".into()).encode().unwrap();
        for byte in &frame[..frame.len() - 1] {
            stream.write_all(&[*byte]).unwrap();
            assert!(connection.poll().unwrap().is_empty());
        }
        stream.write_all(&frame[frame.len() - 1..]).unwrap();
        let deadline = Instant::now() + Duration::from_secs(1);
        loop {
            if !connection.poll().unwrap().is_empty() {
                break;
            }
            assert!(Instant::now() < deadline);
            std::thread::yield_now();
        }
        stream.write_all(&((MAX_FRAME + 1) as u32).to_le_bytes()).unwrap();
        let deadline = Instant::now() + Duration::from_secs(1);
        loop {
            if connection.poll().is_err() {
                break;
            }
            assert!(Instant::now() < deadline);
            std::thread::yield_now();
        }
    }
    #[test]
    fn output_backpressure_is_bounded() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let stream = TcpStream::connect(listener.local_addr().unwrap()).unwrap();
        let _ = listener.accept().unwrap();
        let mut connection = Connection::new(stream).unwrap();
        let mut count = 0;
        while connection.send(Packet::State(serde_json::json!({"data":"x".repeat(200_000)}))).is_ok() {
            count += 1;
        }
        assert!(count > 0 && count <= 11);
        assert!(connection.queued_bytes() <= MAX_QUEUE);
    }
    #[test]
    fn handshake_checks_game_and_protocol_versions_and_identity() {
        let hello = |protocol, version: &str, profile: &str| Packet::Hello {
            protocol,
            version: version.into(),
            profile: profile.into(),
        };
        assert!(handshake(&hello(PROTOCOL, GAME_VERSION, "Player_1")).is_ok());
        assert!(handshake(&hello(PROTOCOL, "old", "Player_1")).unwrap_err().contains("Version mismatch"));
        assert!(handshake(&hello(PROTOCOL + 1, GAME_VERSION, "Player_1")).is_err());
        assert!(handshake(&hello(PROTOCOL, GAME_VERSION, "../../save")).is_err());
    }
}
