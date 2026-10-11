use glam::{DVec3, IVec3};
use std::sync::Arc;
use std::time::{Duration, Instant};
use voxelcraft::lan::{Connection, GAME_VERSION, PROTOCOL, Packet, headless::HeadlessHost};
use voxelcraft::rules::GameMode;
use voxelcraft::world::{World, block::Block, chunk::ChunkData, terrain::Generator};

fn fixture() -> HeadlessHost {
    let mut chunk = ChunkData::Uniform(Block::AIR);
    for z in 0..32 {
        for x in 0..32 {
            chunk.set(x, 22, z, Block::STONE);
        }
    }
    chunk.set(3, 24, 0, Block::STONE);
    // Extended states must survive the same wire format as legacy byte states.
    chunk.set(8, 24, 8, Block((voxelcraft::world::block::STATE_CAPACITY - 1) as u16));
    let mut saved = rustc_hash::FxHashMap::default();
    saved.insert(IVec3::new(0, 4, 0), Arc::new(chunk));
    let world = World::new_headless(Arc::new(Generator::new(12345)), saved, 2);
    HeadlessHost::new("127.0.0.1:0".parse().unwrap(), world, DVec3::new(0.5, 151.0, 0.5), GameMode::Creative).unwrap()
}
fn until(host: &mut HeadlessHost, client: &mut Connection, mut check: impl FnMut(Packet) -> bool) {
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        host.step().unwrap();
        for packet in client.poll().unwrap() {
            if check(packet) {
                return;
            }
        }
        assert!(Instant::now() < deadline, "loopback operation timed out");
        std::thread::sleep(Duration::from_millis(1));
    }
}
#[test]
fn loopback_handshake_chunk_edit_and_clean_disconnect() {
    let mut host = fixture();
    let start = Instant::now();
    let mut client = Connection::connect(host.address(), "Builder").unwrap();
    let mut welcome = false;
    let mut replica = World::new_remote(Arc::new(Generator::new(12345)), 2);
    until(&mut host, &mut client, |packet| match packet {
        Packet::Welcome { seed, id, .. } => {
            assert_eq!(seed, 12345);
            assert_ne!(id, 0);
            welcome = true;
            false
        }
        Packet::Chunk { pos, data } => {
            replica.receive_chunk(pos, data);
            welcome && replica.get_block(IVec3::new(3, 152, 0)) == Some(Block::STONE)
        }
        _ => false,
    });
    let join = start.elapsed();
    let initial_bytes = client.received_bytes;
    assert_eq!(
        replica.get_block(IVec3::new(8, 152, 8)),
        Some(Block((voxelcraft::world::block::STATE_CAPACITY - 1) as u16))
    );
    let edit_start = Instant::now();
    client.send(Packet::Command("place".into())).unwrap();
    until(&mut host, &mut client, |packet| {
        if let Packet::Chunk { pos, data } = packet {
            replica.receive_chunk(pos, data);
        }
        replica.get_block(IVec3::new(2, 152, 0)) == Some(Block::DIRT)
    });
    assert_eq!(host.world.get_block(IVec3::new(2, 152, 0)), Some(Block::DIRT));
    let edit = edit_start.elapsed();
    // A failed action cannot grant items or alter remote blocks.
    client.send(Packet::Command("setblock 100 152 100 diamond_ore".into())).unwrap();
    until(&mut host, &mut client, |packet| matches!(packet,Packet::Chat(message) if message.contains("disabled")));
    assert_ne!(host.world.get_block(IVec3::new(100, 152, 100)), Some(Block::DIAMOND_ORE));
    let bytes = client.received_bytes;
    host.close();
    let mut closed = false;
    let deadline = Instant::now() + Duration::from_secs(2);
    while !closed && Instant::now() < deadline {
        for packet in client.poll().expect("shutdown reason must precede EOF") {
            if let Packet::Disconnect(reason) = packet {
                assert_eq!(reason, "Server closed");
                closed = true;
            }
        }
        if !closed {
            std::thread::sleep(Duration::from_millis(1));
        }
    }
    assert!(closed, "server must send the shutdown reason before EOF");
    assert!(client.poll().is_err());
    eprintln!(
        "LAN loopback: first chunk {:.2} ms / {} bytes, edit {:.2} ms, total {} bytes received / {} sent",
        join.as_secs_f64() * 1000.0,
        initial_bytes,
        edit.as_secs_f64() * 1000.0,
        bytes,
        client.sent_bytes
    );
}
#[test]
fn loopback_rejects_mismatched_version_without_joining() {
    let mut host = fixture();
    let stream = std::net::TcpStream::connect(host.address()).unwrap();
    let mut client = Connection::new(stream).unwrap();
    client
        .send(Packet::Hello { protocol: PROTOCOL + 1, version: GAME_VERSION.into(), profile: "OldClient".into() })
        .unwrap();
    until(
        &mut host,
        &mut client,
        |packet| matches!(packet,Packet::Disconnect(reason) if reason.contains("Version mismatch")),
    );
    assert!(host.players.is_empty());
}
#[test]
fn two_clients_share_ordered_authoritative_edits() {
    let mut host = fixture();
    let mut a = Connection::connect(host.address(), "A").unwrap();
    let mut b = Connection::connect(host.address(), "B").unwrap();
    until(&mut host, &mut a, |p| matches!(p, Packet::Welcome { .. }));
    until(&mut host, &mut b, |p| matches!(p, Packet::Welcome { .. }));
    a.send(Packet::Command("place".into())).unwrap();
    a.flush().unwrap();
    until(
        &mut host,
        &mut b,
        |p| matches!(p,Packet::Chunk{pos,data} if pos==IVec3::new(0,4,0)&&data.get(2,24,0)==Block::DIRT),
    );
    a.close("Player left");
    let deadline = Instant::now() + Duration::from_secs(2);
    while host.players.contains_key("A") && Instant::now() < deadline {
        host.step().unwrap();
        let _ = b.poll();
        std::thread::sleep(Duration::from_millis(1));
    }
    assert!(!host.players.contains_key("A"));
    assert!(host.players.contains_key("B"));
}
