//! VoxelCraft LAN protocol. Nonblocking, bounded TCP; all simulation stays on the authority.
pub mod headless;

use std::collections::VecDeque;
use std::io::{self, Read, Write};
use std::net::{SocketAddr, TcpListener, TcpStream, UdpSocket};
use std::sync::Arc;
use std::time::{Duration, Instant};

use crate::inventory::components::{WRITABLE_MAX_PAGES, WRITABLE_PAGE_LENGTH, WRITTEN_TITLE_MAX_LENGTH};
use crate::world::{
    block::Block,
    chunk::{CHUNK_VOLUME, ChunkData},
};
use glam::IVec3;

/// 2: chunk states may be 4096 and above (`STATE_CAPACITY` 16,384).
/// 3: chunk states may be 16,384 and above (`STATE_CAPACITY` 32,768).
/// 4: `Welcome` carries the world's generation (v0.13).
/// 5: v0.16's mobs, voices and sounds shift the dense `MobKind` and sound
/// numbers, so a build still versioned 0.15.0 refuses v0.15 peers.
/// It also adds the Book packet (issue #97), before any release used 5.
/// 6: desktop hosts send inventory changes separately from tick state.
pub const PROTOCOL: u16 = 6;
pub const GAME_VERSION: &str = env!("CARGO_PKG_VERSION");
pub const MAX_FRAME: usize = 256 * 1024;
/// One inventory part per host tick leaves half the I/O budget for state and terrain.
pub const INVENTORY_PART_BYTES: usize = MAX_FRAME / 2;
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
            ..Default::default()
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
        /// The world's [`Generation`](crate::world::terrain::Generation) number.
        generation: u8,
        dimension: String,
        id: u32,
    },
    Input(Input),
    /// Immediate action on the same validated authority as CLI players.
    Command(String),
    /// Slot click with the inventory revision shown to the client.
    Click {
        slot: u16,
        right: bool,
        shift: bool,
        revision: u32,
        /// The displayed crafting grid, or zero while its contents are pending.
        grid_size: u8,
    },
    Chat(String),
    State(serde_json::Value),
    Chunk {
        pos: IVec3,
        data: Arc<ChunkData>,
    },
    /// Changed cells of a chunk the client already has: (linear index, new state).
    Edits {
        pos: IVec3,
        cells: Vec<(u16, Block)>,
    },
    Disconnect(String),
    /// A guest's book editor closed (`ServerboundEditBookPacket`): the pages
    /// of its book and quill, signed when `title` is set. Its own packet
    /// because a full book is far over a `Command`'s 1024 bytes (issue #97).
    Book {
        pages: Vec<String>,
        title: Option<String>,
        offhand: bool,
    },
    /// An ordered part of the host's inventory update for this client.
    InventoryPart {
        last: bool,
        bytes: Vec<u8>,
    },
}

/// Most cells an `Edits` packet carries; more changes send the whole chunk.
pub const MAX_EDITS: usize = 1024;
/// Book text goes as UTF-16, the unit Java's page and title limits count, so
/// the largest book (100 pages of 1024 units, about 205 KB) fits a frame.
fn utf16(out: &mut Vec<u8>, s: &str, max: usize) -> io::Result<()> {
    let units: Vec<u16> = s.encode_utf16().collect();
    if units.len() > max {
        return Err(bad("book text too long"));
    }
    out.extend_from_slice(&(units.len() as u16).to_le_bytes());
    out.extend(units.iter().flat_map(|u| u.to_le_bytes()));
    Ok(())
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
            Self::Welcome { seed, generation, dimension, id } => {
                out.push(1);
                out.extend_from_slice(&seed.to_le_bytes());
                out.push(*generation);
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
            Self::Edits { pos, cells } => {
                if cells.len() > MAX_EDITS {
                    return Err(bad("too many edits"));
                }
                out.push(9);
                for p in pos.to_array() {
                    out.extend_from_slice(&p.to_le_bytes());
                }
                for (i, b) in cells {
                    out.extend_from_slice(&i.to_le_bytes());
                    out.extend_from_slice(&b.0.to_le_bytes());
                }
            }
            Self::Click { slot, right, shift, revision, grid_size } => {
                out.push(8);
                out.extend_from_slice(&slot.to_le_bytes());
                out.extend_from_slice(&[u8::from(*right), u8::from(*shift)]);
                out.extend_from_slice(&revision.to_le_bytes());
                out.push(*grid_size);
            }
            Self::Book { pages, title, offhand } => {
                if pages.len() > WRITABLE_MAX_PAGES {
                    return Err(bad("too many book pages"));
                }
                out.push(10);
                out.push(u8::from(*offhand) | u8::from(title.is_some()) << 1);
                if let Some(title) = title {
                    utf16(&mut out, title, WRITTEN_TITLE_MAX_LENGTH)?;
                }
                out.push(pages.len() as u8);
                for page in pages {
                    utf16(&mut out, page, WRITABLE_PAGE_LENGTH)?;
                }
            }
            Self::InventoryPart { last, bytes } => {
                if bytes.is_empty() || bytes.len() > INVENTORY_PART_BYTES {
                    return Err(bad("invalid inventory part size"));
                }
                out.extend_from_slice(&[11, u8::from(*last)]);
                out.extend_from_slice(bytes);
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
            1 => Self::Welcome { seed: r.u64()?, generation: r.u8()?, dimension: r.text(16)?, id: r.u32()? },
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
            9 => {
                let pos = IVec3::new(r.i32()?, r.i32()?, r.i32()?);
                if pos.x.abs_diff(0) > 937_500 || pos.z.abs_diff(0) > 937_500 || !(-2..10).contains(&pos.y) {
                    return Err(bad("invalid chunk position"));
                }
                if r.0.len() % 4 != 0 || r.0.len() / 4 > MAX_EDITS {
                    return Err(bad("invalid edits"));
                }
                let mut cells = Vec::with_capacity(r.0.len() / 4);
                while !r.0.is_empty() {
                    let i = r.u16()?;
                    let b = Block(r.u16()?);
                    if i as usize >= CHUNK_VOLUME || b.0 as usize >= crate::world::block::STATE_CAPACITY {
                        return Err(bad("invalid edit"));
                    }
                    cells.push((i, b));
                }
                Self::Edits { pos, cells }
            }
            8 => {
                let slot = r.u16()?;
                let right = r.u8()?;
                let shift = r.u8()?;
                if right > 1 || shift > 1 {
                    return Err(bad("invalid click"));
                }
                let revision = r.u32()?;
                let grid_size = r.u8()?;
                if !matches!(grid_size, 0 | 2 | 3) {
                    return Err(bad("invalid crafting grid size"));
                }
                Self::Click { slot, right: right != 0, shift: shift != 0, revision, grid_size }
            }
            10 => {
                let flags = r.u8()?;
                if flags > 3 {
                    return Err(bad("invalid book flags"));
                }
                let title = if flags & 2 != 0 { Some(r.utf16(WRITTEN_TITLE_MAX_LENGTH)?) } else { None };
                let count = r.u8()? as usize;
                if count > WRITABLE_MAX_PAGES {
                    return Err(bad("too many book pages"));
                }
                let pages = (0..count).map(|_| r.utf16(WRITABLE_PAGE_LENGTH)).collect::<io::Result<_>>()?;
                Self::Book { pages, title, offhand: flags & 1 != 0 }
            }
            11 => {
                let last = r.u8()?;
                if last > 1 || r.0.is_empty() || r.0.len() > INVENTORY_PART_BYTES {
                    return Err(bad("invalid inventory part"));
                }
                let bytes = std::mem::take(&mut r.0).to_vec();
                Self::InventoryPart { last: last == 1, bytes }
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
    fn utf16(&mut self, max: usize) -> io::Result<String> {
        let n = self.u16()? as usize;
        if n > max || self.0.len() < n * 2 {
            return Err(bad("invalid book text length"));
        }
        let (a, b) = self.0.split_at(n * 2);
        self.0 = b;
        let units: Vec<u16> = a.as_chunks::<2>().0.iter().map(|&u| u16::from_le_bytes(u)).collect();
        String::from_utf16(&units).map_err(|_| bad("invalid book text"))
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
    /// Multicasts the world's name, port, player count (host included) and game version.
    pub fn announce(&mut self, name: &str, players: usize) {
        if self.last_announce.elapsed() < Duration::from_millis(1500) {
            return;
        }
        self.last_announce = Instant::now();
        let text = format!(
            "[VX1][MOTD]{}[/MOTD][AD]{}[/AD][P]{}/{}[/P][V]{}[/V]",
            name.chars().filter(|c| !c.is_control() && *c != '[' && *c != ']').take(64).collect::<String>(),
            self.address.port(),
            players.min(MAX_PLAYERS),
            MAX_PLAYERS,
            GAME_VERSION,
        );
        let _ = self.announce.send_to(text.as_bytes(), (std::net::Ipv4Addr::new(224, 0, 2, 60), DISCOVERY_PORT));
    }
}
/// The host plus eight joined clients.
pub const MAX_PLAYERS: usize = 9;

#[derive(Clone)]
pub struct Advertisement {
    pub address: SocketAddr,
    pub name: String,
    /// Players in the world (host included), when the host reports it.
    pub players: Option<(usize, usize)>,
    /// The host's game version; joining needs an exact match.
    pub version: Option<String>,
    pub seen: Instant,
}

/// Parses `[VX1][MOTD]name[/MOTD][AD]port[/AD]` plus optional `[P]n/max[/P][V]version[/V]`.
/// A parsed announcement: world name, port, players/max and game version.
pub type Announcement = (String, u16, Option<(usize, usize)>, Option<String>);

pub fn parse_advertisement(text: &str) -> Option<Announcement> {
    let (name, rest) = text.strip_prefix("[VX1][MOTD]")?.split_once("[/MOTD][AD]")?;
    let (port, rest) = rest.split_once("[/AD]")?;
    let port = port.parse::<u16>().ok().filter(|p| *p != 0)?;
    let tag = |open: &str, close: &str| rest.split_once(open).and_then(|(_, r)| r.split_once(close)).map(|(v, _)| v);
    let players = tag("[P]", "[/P]").and_then(|p| {
        let (n, max) = p.split_once('/')?;
        Some((n.parse().ok()?, max.parse().ok()?))
    });
    let version = tag("[V]", "[/V]").map(|v| v.chars().filter(|c| !c.is_control()).take(32).collect());
    Some((name.chars().filter(|c| !c.is_control()).take(64).collect(), port, players, version))
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
            let Some((name, port, players, version)) = parse_advertisement(text) else {
                continue;
            };
            let address = SocketAddr::new(from.ip(), port);
            let ad = Advertisement { address, name, players, version, seen: Instant::now() };
            // Keep each server's place in the list, so a focused row stays on it.
            if let Some(known) = self.servers.iter_mut().find(|s| s.address == address) {
                *known = ad;
            } else if self.servers.len() < 32 {
                self.servers.push(ad);
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
        chunk.extend_from_slice(&(crate::world::block::STATE_CAPACITY as u16).to_le_bytes());
        assert!(Packet::decode(&chunk).is_err(), "out-of-registry states must be rejected");
        chunk.truncate(13);
        chunk.extend_from_slice(&1u16.to_le_bytes());
        chunk.extend_from_slice(&0u16.to_le_bytes());
        assert!(Packet::decode(&chunk).is_err(), "partial chunks must not render");
    }
    /// Issue #97: the largest book the editor makes fits one frame, and
    /// anything over Java's limits is refused both ways.
    #[test]
    fn books_round_trip_within_their_limits() {
        let full = vec!["語".repeat(WRITABLE_PAGE_LENGTH); WRITABLE_MAX_PAGES];
        let title = Some("𝄞".repeat(WRITTEN_TITLE_MAX_LENGTH / 2));
        for (title, offhand) in [(None, false), (title, true)] {
            let book = Packet::Book { pages: full.clone(), title: title.clone(), offhand };
            let encoded = book.encode().unwrap();
            assert!(encoded.len() - 4 <= MAX_FRAME);
            let Packet::Book { pages, title: t, offhand: o } = Packet::decode(&encoded[4..]).unwrap() else {
                panic!("wrong packet")
            };
            assert_eq!((pages, t, o), (full.clone(), title, offhand));
        }
        let page = |n| "a".repeat(n);
        for (pages, title) in [
            (vec![page(1); WRITABLE_MAX_PAGES + 1], None),
            (vec![page(WRITABLE_PAGE_LENGTH + 1)], None),
            (vec![page(1)], Some(page(WRITTEN_TITLE_MAX_LENGTH + 1))),
        ] {
            assert!(Packet::Book { pages, title, offhand: false }.encode().is_err());
        }
        // Bad flags, a page over the limit, an unpaired surrogate, too many pages.
        assert!(Packet::decode(&[10, 4, 0]).is_err());
        let mut long = vec![10, 0, 1];
        long.extend_from_slice(&(WRITABLE_PAGE_LENGTH as u16 + 1).to_le_bytes());
        long.resize(long.len() + 2 * (WRITABLE_PAGE_LENGTH + 1), b'a');
        assert!(Packet::decode(&long).is_err());
        assert!(Packet::decode(&[10, 0, 1, 1, 0, 0x00, 0xd8]).is_err());
        assert!(Packet::decode(&[10, 0, WRITABLE_MAX_PAGES as u8 + 1]).is_err());
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
    fn edits_round_trip_and_reject_bad_cells() {
        let packet = Packet::Edits { pos: IVec3::new(3, 4, -5), cells: vec![(0, Block::STONE), (32767, Block::DIRT)] };
        let frame = packet.encode().unwrap();
        let Packet::Edits { pos, cells } = Packet::decode(&frame[4..]).unwrap() else { panic!() };
        assert_eq!((pos, cells), (IVec3::new(3, 4, -5), vec![(0, Block::STONE), (32767, Block::DIRT)]));
        let mut bad = frame[4..].to_vec();
        bad.extend_from_slice(&[0xff, 0xff, 1, 0]); // index 65535
        assert!(Packet::decode(&bad).is_err());
        let many = Packet::Edits { pos: IVec3::ZERO, cells: vec![(0, Block::AIR); MAX_EDITS + 1] };
        assert!(many.encode().is_err());
    }
    #[test]
    fn advertisements_carry_player_count_and_version_and_accept_the_old_form() {
        let (name, port, players, version) =
            parse_advertisement("[VX1][MOTD]My World[/MOTD][AD]25565[/AD][P]3/9[/P][V]0.7.0[/V]").unwrap();
        assert_eq!(
            (name.as_str(), port, players, version.as_deref()),
            ("My World", 25565, Some((3, 9)), Some("0.7.0"))
        );
        let (_, port, players, version) = parse_advertisement("[VX1][MOTD]Old[/MOTD][AD]4000[/AD]").unwrap();
        assert_eq!((port, players, version), (4000, None, None));
        assert!(parse_advertisement("[VX1][MOTD]Bad[/MOTD][AD]0[/AD]").is_none());
        assert!(parse_advertisement("hello").is_none());
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
