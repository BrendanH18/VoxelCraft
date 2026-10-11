//! Device-free LAN exercise server. Reuses real world streaming, player physics and actions.
//! The desktop host additionally supplies puppet actions, GUI containers and entity replication.
use super::{Connection, Input, Listener, Packet, handshake};
use crate::agent::{Agent, Command};
use crate::entity::{Entities, PlayerId};
use crate::rules::{GameMode, GameRules};
use crate::simulation::{TICK_SECONDS, difficulty::Difficulty};
use crate::world::{
    World,
    chunk::{ChunkData, chunk_of},
    terrain::Generator,
};
use glam::{DVec3, IVec3};
use serde_json::json;
use std::collections::BTreeMap;
use std::net::SocketAddr;
use std::sync::Arc;

struct Session {
    connection: Connection,
    profile: Option<String>,
    input: Input,
    dead: bool,
    connected: std::time::Instant,
    sent: BTreeMap<(i32, i32, i32), Arc<ChunkData>>,
}
pub struct HeadlessHost {
    listener: Listener,
    pub world: World,
    pub entities: Entities,
    pub players: BTreeMap<String, Agent>,
    sessions: Vec<Session>,
    spawn: DVec3,
    mode: GameMode,
    tick: u64,
}
impl HeadlessHost {
    pub fn new(address: SocketAddr, world: World, spawn: DVec3, mode: GameMode) -> std::io::Result<Self> {
        let entities = Entities::new(world.generator.seed);
        Ok(Self {
            listener: Listener::bind(address)?,
            world,
            entities,
            players: BTreeMap::new(),
            sessions: Vec::new(),
            spawn,
            mode,
            tick: 0,
        })
    }
    pub fn address(&self) -> SocketAddr {
        self.listener.address
    }
    /// One authority tick, always 50 ms of simulation regardless of I/O readiness.
    pub fn step(&mut self) -> std::io::Result<()> {
        for _ in 0..8 {
            let Some(mut connection) = self.listener.accept()? else { break };
            if self.sessions.len() >= 8 {
                connection.close("Server full");
                continue;
            }
            self.sessions.push(Session {
                connection,
                profile: None,
                input: Input::default(),
                dead: false,
                connected: std::time::Instant::now(),
                sent: BTreeMap::new(),
            });
        }
        let mut sessions = std::mem::take(&mut self.sessions);
        sessions.retain_mut(|s| {
            if s.profile.is_none() && s.connected.elapsed() > std::time::Duration::from_secs(5) {
                s.connection.close("Handshake timed out");
                return false;
            }
            let Ok(packets) = s.connection.poll() else { return false };
            for packet in packets {
                if s.profile.is_none() {
                    let name = match handshake(&packet) {
                        Ok(n) => n.to_owned(),
                        Err(e) => {
                            s.connection.close(&e);
                            return false;
                        }
                    };
                    if sessions_profile_taken(&self.players, &name) {
                        s.connection.close("Profile already connected");
                        return false;
                    }
                    let mut agent = Agent::new(self.spawn);
                    agent.set_mode(self.mode);
                    agent.id = PlayerId(self.players.values().map(|a| a.id.0).max().unwrap_or(0) + 1);
                    if self.mode.is_creative() {
                        agent.inventory.add(crate::item::Item::from(crate::world::block::Block::DIRT), 64);
                    }
                    let id = agent.id.0;
                    self.players.insert(name.clone(), agent);
                    s.profile = Some(name);
                    if s.connection
                        .send(Packet::Welcome {
                            seed: self.world.generator.seed,
                            dimension: self.world.generator.dimension.name().into(),
                            id,
                        })
                        .is_err()
                    {
                        return false;
                    }
                    continue;
                }
                let name = s.profile.as_deref().unwrap();
                match packet {
                    Packet::Input(input) => s.input = input,
                    Packet::Command(text) => {
                        let result = Command::parse(&text).and_then(|command| {
                            if command.cheat() {
                                return Err("Headless harness cheats are disabled".into());
                            }
                            self.players.get_mut(name).unwrap().execute(
                                command,
                                &mut self.world,
                                &mut self.entities,
                                &[],
                            )
                        });
                        if let Err(e) = result {
                            let _ = s.connection.send(Packet::Chat(format!("Error: {e}")));
                        }
                    }
                    Packet::Disconnect(_) => {
                        s.connection.close("Player left");
                        return false;
                    }
                    _ => {
                        s.connection.close("Invalid harness packet");
                        return false;
                    }
                }
            }
            true
        });
        self.players.retain(|name, _| sessions.iter().any(|s| s.profile.as_ref() == Some(name)));
        let positions: Vec<_> = self.players.values().map(|a| a.player.pos).collect();
        self.world.update_players(self.spawn, &positions);
        for s in &mut sessions {
            let Some(name) = &s.profile else { continue };
            let agent = self.players.get_mut(name).unwrap();
            agent.player.yaw = s.input.yaw;
            agent.player.pitch = s.input.pitch.clamp(-1.55, 1.55);
            if s.input.sequence > 0 {
                agent.selected = s.input.selected as usize;
                agent.hold(s.input.movement(), s.input.buttons & 8 != 0, s.input.buttons & 16 != 0);
            }
            agent.tick_rules(&mut self.world, &mut self.entities, Difficulty::Normal, &GameRules::default());
            let center = chunk_of(agent.player.pos.floor().as_ivec3());
            s.sent.retain(|p, _| (IVec3::new(p.0, p.1, p.2) - center).with_y(0).length_squared() <= 16);
            let chunks = self.world.network_chunks(center, 4);
            for (pos, data) in chunks {
                let key = (pos.x, pos.y, pos.z);
                if s.sent.get(&key).is_some_and(|old| Arc::ptr_eq(old, &data)) {
                    continue;
                }
                if s.connection.queued_bytes() > 512 * 1024 {
                    break;
                }
                if s.connection.send(Packet::Chunk { pos, data: data.clone() }).is_err() {
                    s.dead = true;
                    break;
                }
                s.sent.insert(key, data);
            }
            if s.dead || s.connection.send(Packet::State(json!({"tick":self.tick,"sequence":s.input.sequence,"harness":true,"state":agent.observe(&self.world,0)}))).and_then(|()|s.connection.flush()).is_err() {s.dead=true;s.connection.close("Client is too slow or disconnected");}
        }
        crate::simulation::tick_world_rules(&mut self.world, self.spawn, true, 3);
        sessions.retain(|s| !s.dead);
        self.players.retain(|name, _| sessions.iter().any(|s| s.profile.as_ref() == Some(name)));
        self.sessions = sessions;
        self.tick += 1;
        Ok(())
    }
    pub fn close(&mut self) {
        for s in &mut self.sessions {
            s.connection.close("Server closed");
        }
        self.sessions.clear();
        self.players.clear();
    }
}
fn sessions_profile_taken(players: &BTreeMap<String, Agent>, name: &str) -> bool {
    players.contains_key(name)
}
impl Drop for HeadlessHost {
    fn drop(&mut self) {
        self.close();
    }
}

pub fn generated_host(address: SocketAddr, seed: u64) -> std::io::Result<HeadlessHost> {
    let generator = Arc::new(Generator::new(seed));
    let y = generator.column(0, 0).height as f64 + 1.0;
    let world = World::new_headless(generator, Default::default(), 2);
    HeadlessHost::new(address, world, DVec3::new(0.5, y, 0.5), GameMode::Survival)
}
/// No clocks/devices are involved in simulation; callers pace `step` at this interval.
pub const STEP: std::time::Duration = std::time::Duration::from_millis((TICK_SECONDS * 1000.0) as u64);
