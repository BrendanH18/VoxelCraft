//! Desktop host/client adapter. LAN hands run on each peer's own session like
//! controller hands (`hands::Actor`); screens still borrow host container code.
use super::{Container, Game, GameMode, gamepad::Body, hud::SlotRef};
use crate::crafting::Grid;
use crate::inventory::{stack_from_str, stack_to_string};
use crate::knockback;
use crate::world::{
    World,
    block::Block,
    chunk::{ChunkData, chunk_of},
    terrain::{Dimension, Generator},
};
use glam::{DVec3, IVec3};
use serde_json::{Value, json};
use std::collections::{BTreeMap, VecDeque};
use std::net::SocketAddr;
use std::path::Path;
use std::sync::Arc;
use std::time::{Duration, Instant};
use voxelcraft::agent::{Agent, Command};
use voxelcraft::lan::{Connection, Input, Listener, Packet};

/// Largest view distance (in 32-block chunks) a LAN host streams to clients.
pub(super) const LAN_VIEW_MAX: i32 = 8;
pub(super) struct Peer {
    connection: Connection,
    profile: Option<String>,
    connected: Instant,
    input: Input,
    previous_buttons: u8,
    edges: Vec<(u8, bool)>,
    applied: u32,
    last_input: Instant,
    body: Body,
    grid: Grid,
    open: bool,
    dimension: Dimension,
    dead: bool,
    last_portal_message: Instant,
    used_at: Option<IVec3>,
    sent: BTreeMap<(i32, i32, i32), Arc<ChunkData>>,
    /// View distance the client asked for (chunks); the host streams at most its own.
    view: i32,
    /// Mobs whose look this client has; others get it on their next snapshot.
    looked: std::collections::BTreeSet<u32>,
    /// The respawn angle, held against the client's stale look input until
    /// its input reports that angle (it adopts it from the snapshot that
    /// shows the new respawn count) or a short timeout passes.
    look_override: Option<(f32, f32, Instant)>,
    /// The player's respawn count when last driven, to notice respawns.
    respawns: Option<u32>,
}
impl Peer {
    fn new(connection: Connection) -> Self {
        Self {
            connection,
            profile: None,
            connected: Instant::now(),
            input: Input::default(),
            previous_buttons: 0,
            edges: Vec::new(),
            applied: 0,
            last_input: Instant::now(),
            body: Body::default(),
            grid: Grid::new(2),
            open: false,
            dimension: Dimension::Overworld,
            dead: false,
            used_at: None,
            last_portal_message: Instant::now() - Duration::from_secs(10),
            sent: BTreeMap::new(),
            view: 2,
            looked: Default::default(),
            look_override: None,
            respawns: None,
        }
    }
}
#[derive(Default)]
pub(super) struct Lan {
    /// Host ticks with peers, to refresh mob looks once a second.
    look_ticks: u32,
    /// Particle effects since the last state, for clients to replay (JSON requests).
    pub effects: Vec<(DVec3, Value)>,
    /// A client's view of every player's block breaking, from the host.
    pub remote_cracks: Vec<(IVec3, f32)>,
    /// Host tick timing and traffic since the last `LAN stats` log line.
    pub stats_tick_secs: f64,
    pub stats_tick_max: f64,
    stats_ticks: u32,
    stats_sent: u64,
    pub host: Option<Listener>,
    peers: Vec<Peer>,
    pub client: Option<Connection>,
    pub lost: Option<String>,
    pub profile: String,
    pub offline_render_distance: Option<i32>,
    pub mode: GameMode,
    pub cheats: bool,
    pub pvp: bool,
    id: u32,
    sequence: u32,
    predictions: VecDeque<Input>,
    /// The hotbar slot whose bow or crossbow this client predicts is being
    /// drawn: started when use could start a draw, kept while use stays held
    /// on it (the host's `tick_draw` doesn't recheck what is targeted).
    predicted_draw: Option<usize>,
    /// Whether the use button was held on the last client tick.
    use_was_held: bool,
    /// Use was pressed since the last client tick (it may also have been
    /// released and pressed again in between).
    use_pressed: bool,
    /// The input sequence of the latest use press or release: a snapshot
    /// acknowledging it settles the predicted draw.
    use_edge_sequence: u32,
    ready: bool,
    messages: VecDeque<String>,
    /// The host's respawn count for this client's player, from the last snapshot.
    respawns: Option<u64>,
}
impl Lan {
    /// The world's remembered session rules, `cheats,pvp` as `0` or `1`.
    pub(super) fn rules_text(&self) -> String {
        format!("{},{}", u8::from(self.cheats), u8::from(self.pvp))
    }

    /// Restores [`Lan::rules_text`] as the next session's defaults; command
    /// line flags can still switch rules on. Malformed text keeps both off.
    pub(super) fn restore_rules(&mut self, text: Option<&str>) {
        let mut parts = text.unwrap_or("").split(',').map(|v| v == "1");
        self.cheats |= parts.next().unwrap_or(false);
        self.pvp |= parts.next().unwrap_or(false);
    }

    pub(super) fn has_peers(&self) -> bool {
        !self.peers.is_empty()
    }
    /// Each LAN player's profile with what it is breaking and its eating progress.
    pub(super) fn bodies(&self) -> impl Iterator<Item = (&str, Option<(IVec3, f32)>, f32)> {
        self.peers.iter().filter_map(|p| Some((p.profile.as_deref()?, p.body.breaking(), p.body.eating())))
    }
}

impl Game {
    /// Every player's block breaking: the host's, controller and LAN players' (their bodies)
    /// and CLI agents'. On a LAN client, the host's list.
    pub(super) fn all_breaking(&self) -> Vec<(IVec3, f32)> {
        if self.lan.client.is_some() {
            return self.lan.remote_cracks.clone();
        }
        let mut out: Vec<_> = self.body.actions.breaking.into_iter().collect();
        for (name, bot) in self.agents.players.iter().filter(|(_, b)| b.active) {
            let body = self
                .pad_view(name)
                .map(|p| p.breaking)
                .or_else(|| self.lan.bodies().find(|b| b.0 == name).map(|b| b.1));
            out.extend(body.unwrap_or_else(|| bot.agent.breaking(&self.world)));
        }
        out.retain(|&(_, f)| f > 0.0);
        out
    }
    /// Crack decals within 32 blocks of `eye`, nearest first.
    pub(super) fn cracks_seen_from(&self, eye: DVec3) -> Vec<(IVec3, u16)> {
        let mut cracks: Vec<_> = self
            .all_breaking()
            .into_iter()
            .filter(|(p, _)| (p.as_dvec3() + 0.5).distance_squared(eye) < 32.0 * 32.0)
            .collect();
        cracks.sort_by(|a, b| {
            (a.0.as_dvec3() + 0.5).distance_squared(eye).total_cmp(&(b.0.as_dvec3() + 0.5).distance_squared(eye))
        });
        cracks.dedup_by_key(|c| c.0);
        cracks
            .into_iter()
            .take(crate::render::MAX_CRACKS)
            .map(|(p, f)| (p, crate::world::block::tex::CRACK_0 + (f * 10.0).min(9.0) as u16))
            .collect()
    }
}
impl Drop for Lan {
    fn drop(&mut self) {
        for peer in &mut self.peers {
            peer.connection.close("Server closed");
        }
        if let Some(client) = &mut self.client {
            client.close("Player left");
        }
    }
}
pub(super) fn local_profile(saves: &Path) -> String {
    let path = saves.parent().unwrap_or(saves).join("lan-profile.txt");
    if let Ok(name) = std::fs::read_to_string(&path)
        && voxelcraft::control::valid_name(name.trim())
    {
        return name.trim().into();
    }
    let nonce = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap_or_default().as_nanos();
    let name = format!("Player_{:08x}", nonce as u32);
    let _ = std::fs::create_dir_all(path.parent().unwrap());
    let _ = std::fs::write(path, &name);
    name
}
fn vector(v: &Value) -> Option<DVec3> {
    let a = v.as_array()?;
    if a.len() != 3 {
        return None;
    }
    let p = DVec3::new(a[0].as_f64()?, a[1].as_f64()?, a[2].as_f64()?);
    (p.is_finite() && p.abs().max_element() <= 30_000_000.0).then_some(p)
}
/// Keeps a state snapshot inside one frame: in a crowded scene, sheds (in order) the
/// extra entities, dropped items, mob looks and finally all mobs, rather than disconnecting.
/// Returns the snapshot (if anything fits) and whether mob looks were shed.
fn fit_state(mut v: Value) -> (Option<Value>, bool) {
    let fits = |v: &Value| serde_json::to_vec(v).is_ok_and(|b| b.len() < voxelcraft::lan::MAX_FRAME - 16);
    let mut shed_looks = false;
    for shed in ["extras", "items", "looks", "mobs"] {
        if fits(&v) {
            return (Some(v), shed_looks);
        }
        match shed {
            "looks" => {
                shed_looks = true;
                if let Some(mobs) = v["mobs"].as_array_mut() {
                    for m in mobs {
                        if let Some(m) = m.as_object_mut() {
                            m.remove("look");
                        }
                    }
                }
            }
            key => v[key] = Value::Null,
        }
    }
    (fits(&v).then_some(v), true)
}

/// A LAN peer's look: its input, unless a respawn angle is still held
/// because the client hasn't reported adopting it (see `Peer::look_override`).
fn peer_look(look_override: &mut Option<(f32, f32, Instant)>, input: &Input) -> (f32, f32) {
    if let Some((yaw, pitch, since)) = *look_override {
        let turn = (input.yaw - yaw + std::f32::consts::PI).rem_euclid(std::f32::consts::TAU) - std::f32::consts::PI;
        if (turn.abs() < 0.01 && (input.pitch - pitch).abs() < 0.01) || since.elapsed() > Duration::from_secs(2) {
            *look_override = None;
        } else {
            return (yaw, pitch);
        }
    }
    (input.yaw.rem_euclid(std::f32::consts::TAU), input.pitch.clamp(-1.55, 1.55))
}

fn state(agent: &Agent, name: &str) -> Value {
    json!({"id":agent.session.id.0,"name":name,"pos":agent.session.player.pos.to_array(),"vel":agent.session.player.vel.to_array(),
        "yaw":agent.session.player.yaw,"pitch":agent.session.player.pitch,"ground":agent.session.player.on_ground,"flying":agent.session.player.flying,
        "sprint":agent.session.player.sprinting,"motion":agent.session.player.carried_motion(),"stuck":agent.session.player.carried_stuck().to_array(),"mode":agent.session.mode.name(),"selected":agent.selected,"inventory":agent.session.inventory.serialize(),
        "cursor":stack_to_string(agent.session.inventory.cursor),"ticks_frozen":agent.session.vitals.ticks_frozen,"health":agent.session.vitals.health,"air":agent.session.vitals.air,
        "food":agent.session.vitals.hunger.food,"saturation":agent.session.vitals.hunger.saturation,"exhaustion":agent.session.vitals.hunger.exhaustion,
        "death":agent.session.vitals.death,"xp":agent.session.vitals.xp.serialize(),"effects":agent.session.vitals.effects.serialize(),"sleep":agent.session.sleeping,"vehicle":agent.session.player.vehicle,
        "respawns":agent.session.respawns,"drawing":agent.session.drawing_slot()})
}
/// What clients need of *other* players: pose, held item, armor and health, not their inventory.
/// Rounds to a millimetre (or a thousandth of a radian): plenty for drawing, far fewer bytes.
fn r3(x: f64) -> f64 {
    (x * 1000.0).round() / 1000.0
}
fn state_lite(agent: &Agent, name: &str, swing: Option<f32>, eating: f32) -> Value {
    let p = agent.session.player.pos;
    json!({"swing":swing.map(|t| r3(t as f64)),"eat":r3(eating as f64),"id":agent.session.id.0,"name":name,"pos":[r3(p.x), r3(p.y), r3(p.z)],"vel":[0.0, 0.0, 0.0],
        "yaw":r3(agent.session.player.yaw as f64),"pitch":r3(agent.session.player.pitch as f64),"ground":agent.session.player.on_ground,
        "flying":agent.session.player.flying,"sneak":agent.session.player.sneaking,"mode":agent.session.mode.name(),"selected":agent.selected,
        "held":stack_to_string(agent.session.inventory.get(agent.selected)),
        "armor":agent.session.inventory.armor.iter().map(|s|stack_to_string(*s)).collect::<Vec<_>>(),
        "ticks_frozen":agent.session.vitals.ticks_frozen,"health":agent.session.vitals.health,"death":agent.session.vitals.death,"sleep":agent.session.sleeping,"vehicle":agent.session.player.vehicle})
}
fn apply(agent: &mut Agent, v: &Value) {
    if let Some(pos) = vector(&v["pos"]) {
        agent.previous_pos = agent.session.player.pos;
        agent.session.player.pos = pos;
    }
    if let Some(vel) = vector(&v["vel"]) {
        agent.session.player.vel = vel;
    }
    if let Some(mode) = v["mode"].as_str().and_then(GameMode::from_name) {
        agent.set_mode(mode);
    }
    agent.session.player.yaw = v["yaw"].as_f64().unwrap_or(0.0) as f32;
    agent.session.player.pitch = v["pitch"].as_f64().unwrap_or(0.0) as f32;
    agent.session.player.on_ground = v["ground"] == true;
    agent.session.player.flying = v["flying"] == true;
    // Java's latched sprint, so a client's replayed inputs accelerate as the host's did.
    agent.session.player.sprinting = v["sprint"] == true;
    if let Some(m) = v["motion"].as_array().filter(|m| m.len() == 4) {
        let byte = |i: usize| m[i].as_u64().unwrap_or(0).min(255) as u8;
        agent.session.player.restore_carried_motion([byte(0), byte(1), byte(2), byte(3)]);
    }
    if let Some(stuck) = vector(&v["stuck"]) {
        agent.session.player.restore_carried_stuck(stuck);
    }
    agent.selected = v["selected"].as_u64().unwrap_or(0).min(8) as usize;
    if let Some(inv) = v["inventory"].as_str().and_then(crate::inventory::Inventory::deserialize) {
        agent.session.inventory = inv;
    }
    if let Some(held) = v["held"].as_str() {
        // Another player's lite state: just what is drawn.
        agent.session.inventory = crate::inventory::Inventory::default();
        agent.session.inventory.slots[agent.selected] = stack_from_str(held).flatten();
        for (slot, s) in agent.session.inventory.armor.iter_mut().zip(v["armor"].as_array().into_iter().flatten()) {
            *slot = s.as_str().and_then(stack_from_str).flatten();
        }
    }
    agent.session.player.sneaking = v["sneak"] == true;
    agent.session.inventory.cursor = v["cursor"].as_str().and_then(stack_from_str).flatten();
    agent.session.vitals = crate::simulation::survival::Vitals::restore(
        v["health"].as_f64().unwrap_or(20.0) as f32,
        v["air"].as_f64().unwrap_or(15.0) as f32,
        v["death"].as_str().map(str::to_owned),
    );
    agent.session.vitals.hunger = crate::simulation::survival::Hunger::restore(
        v["food"].as_f64().unwrap_or(20.0) as f32,
        v["saturation"].as_f64().unwrap_or(5.0) as f32,
        v["exhaustion"].as_f64().unwrap_or(0.0) as f32,
    );
    if let Some(xp) = v["xp"].as_str().and_then(crate::simulation::experience::Experience::parse) {
        agent.session.vitals.xp = xp;
    }
    agent.session.vitals.ticks_frozen = v["ticks_frozen"].as_u64().unwrap_or(0).min(140) as u16;
    if let Some(effects) = v["effects"].as_str() {
        agent.session.vitals.effects = crate::simulation::effects::Effects::deserialize(effects);
    }
    agent.session.sleeping = v["sleep"].as_f64().map(|f| f as f32);
    agent.session.player.vehicle = v["vehicle"].as_u64().and_then(|id| u32::try_from(id).ok());
}

impl Game {
    pub(super) fn open_lan(&mut self, address: SocketAddr) -> Result<(), String> {
        if self.lan.client.is_some() {
            return Err("Only the host can open a world to LAN".into());
        }
        if self.lan.host.is_some() {
            return Err("World is already open to LAN".into());
        }
        self.lan.host = Some(Listener::bind(address).map_err(|e| format!("Could not host LAN: {e}"))?);
        let port = self.lan.host.as_ref().unwrap().address.port();
        let message = format!("Local game hosted on port {port}");
        eprintln!("{message}");
        self.chat_line(&message);
        self.show_popup(&message);
        Ok(())
    }
    pub(super) fn close_lan(&mut self, reason: &str) {
        let mut lan = std::mem::take(&mut self.lan);
        if let Some(distance) = lan.offline_render_distance {
            self.settings.render_distance = distance;
        }
        for peer in &mut lan.peers {
            peer.connection.close(reason);
        }
        if let Some(client) = &mut lan.client {
            client.close("Player left");
        }
    }
    pub(super) fn lan_send(&mut self, packet: Packet) {
        if let Some(c) = &mut self.lan.client
            && let Err(e) = c.send(packet).and_then(|()| c.flush())
        {
            self.lan.lost = Some(format!("Connection lost: {e}"));
        }
    }
    pub(super) fn lan_command(&mut self, command: &str) {
        self.lan_send(Packet::Command(command.into()));
    }
    pub(super) fn chat_line(&mut self, message: &str) {
        self.console.lines.push_back(message.into());
        while self.console.lines.len() > 64 {
            self.console.lines.pop_front();
        }
    }
    pub(super) fn broadcast_chat(&mut self, message: &str) {
        self.chat_line(message);
        if self.lan.messages.len() >= 64 {
            self.lan.messages.pop_front();
        }
        self.lan.messages.push_back(message.into());
    }
    pub(super) fn serialize_lan_profiles(&self) -> String {
        let text = self.agents.serialize(self.dimension.name());
        let Ok(mut profiles) = serde_json::from_str::<Value>(&text) else {
            return text;
        };
        if let Some(profiles) = profiles.as_array_mut() {
            for peer in &self.lan.peers {
                if let Some(p) = profiles.iter_mut().find(|p| p["name"].as_str() == peer.profile.as_deref())
                    && let Some(mut inventory) =
                        p["inventory"].as_str().and_then(crate::inventory::Inventory::deserialize)
                {
                    inventory.return_stacks(peer.grid.cells.into_iter().flatten());
                    p["inventory"] = json!(inventory.serialize());
                }
            }
        }
        profiles.to_string()
    }
    pub(super) fn lan_console(&mut self, input: &str) -> Option<Result<String, String>> {
        if self.lan.client.is_some() {
            self.lan_send(if input.starts_with('/') {
                Packet::Command(input.into())
            } else {
                Packet::Chat(input.into())
            });
            return Some(Ok(String::new()));
        }
        if !input.starts_with('/') && self.lan.host.is_some() {
            self.broadcast_chat(&format!("<{}> {input}", self.lan.profile));
            return Some(Ok(String::new()));
        }
        if let Some(name) = input.strip_prefix("/kick ") {
            let index = self.lan.peers.iter().position(|p| p.profile.as_deref() == Some(name.trim()));
            return Some(if let Some(i) = index {
                let mut p = self.lan.peers.remove(i);
                p.connection.close("Kicked by host");
                self.leave_lan(&mut p);
                Ok(format!("Kicked {}", name.trim()))
            } else {
                Err("No such LAN player".into())
            });
        }
        if let Some(value) = input.strip_prefix("/pvp ") {
            return Some(match value {
                "on" | "off" => {
                    self.lan.pvp = value == "on";
                    Ok(format!("PvP {value}"))
                }
                _ => Err("usage: /pvp on|off".into()),
            });
        }
        None
    }
    fn leave_lan(&mut self, peer: &mut Peer) {
        if let Some(name) = &peer.profile {
            if let Some(bot) = self.agents.players.get_mut(name) {
                bot.active = false;
                bot.agent.session.cancel_mining();
                bot.agent.hold(Default::default(), false, false);
                bot.agent.session.sleeping = None;
                bot.agent.session.inventory.return_stacks(peer.grid.take_all());
                let cursor = bot.agent.session.inventory.cursor.take();
                bot.agent.session.inventory.return_stacks(cursor);
                bot.agent
                    .session
                    .inventory
                    .return_stacks(bot.agent.session.work.iter_mut().filter_map(Option::take).collect::<Vec<_>>());
            }
            self.broadcast_chat(&format!("{name} left the game"));
        }
    }
    pub(super) fn lan_owns(&self, profile: &str) -> bool {
        self.lan.peers.iter().any(|p| p.profile.as_deref() == Some(profile))
    }
    pub(super) fn poll_lan(&mut self) {
        if let Some(client) = &mut self.lan.client {
            let packets = match client.poll() {
                Ok(p) => p,
                Err(e) => {
                    self.lan.lost = Some(format!("Connection lost: {e}"));
                    return;
                }
            };
            for packet in packets {
                match packet {
                    Packet::Welcome { seed, dimension, id } => {
                        let Some(dimension) = Dimension::from_name(&dimension) else {
                            self.lan.lost = Some("Invalid server dimension".into());
                            break;
                        };
                        self.lan.id = id;
                        self.reset_lan_world(seed, dimension);
                        let view = self.lan.offline_render_distance.unwrap_or(LAN_VIEW_MAX).clamp(2, LAN_VIEW_MAX);
                        self.lan_command(&format!("view {view}"));
                    }
                    Packet::Chunk { pos, data } => self.world.receive_chunk(pos, data),
                    Packet::Edits { pos, cells } => self.world.receive_edits(pos, &cells),
                    Packet::State(state) => self.receive_lan_state(&state),
                    Packet::Chat(message) => self.chat_line(&message),
                    Packet::Disconnect(reason) => {
                        self.lan.lost = Some(reason);
                        break;
                    }
                    _ => {
                        self.lan.lost = Some("Invalid server packet".into());
                        break;
                    }
                }
            }
            return;
        }
        if let Some(host) = &mut self.lan.host {
            let players = 1 + self.lan.peers.iter().filter(|p| p.profile.is_some()).count();
            host.announce(&self.world_name, players);
            for _ in 0..8 {
                match host.accept() {
                    Ok(Some(mut c)) if self.lan.peers.len() >= 8 => c.close("LAN server is full (8 clients)"),
                    Ok(Some(c)) => self.lan.peers.push(Peer::new(c)),
                    _ => break,
                }
            }
        }
        let mut peers = std::mem::take(&mut self.lan.peers);
        for peer in &mut peers {
            let packets = match peer.connection.poll() {
                Ok(p) => p,
                Err(_) => {
                    self.leave_lan(peer);
                    peer.connection.close("Connection lost");
                    peer.dead = true;
                    continue;
                }
            };
            if peer.profile.is_none() && peer.connected.elapsed() > Duration::from_secs(5) {
                peer.connection.close("Handshake timed out");
                peer.dead = true;
                continue;
            }
            for packet in packets {
                if peer.profile.is_none() {
                    let result = voxelcraft::lan::handshake(&packet).map(str::to_owned).and_then(|name| {
                        if name == self.lan.profile
                            || self.pads.seated(&name)
                            || self.agents.players.get(&name).is_some_and(|b| b.active)
                        {
                            return Err("Profile is already playing; choose a different local profile".into());
                        }
                        if !self.agents.players.contains_key(&name) {
                            if self.agents.players.len() >= 32 {
                                return Err("Player profile limit reached".into());
                            }
                            let mut a = Agent::new(self.session.player.pos + DVec3::new(2.0, 0.0, 0.0));
                            a.set_mode(self.lan.mode);
                            if a.creative {
                                a.inventory = crate::inventory::Inventory::with_hotbar(&super::CREATIVE_HOTBAR);
                            }
                            self.agents.insert(name.clone(), a);
                        }
                        let bot = self.agents.players.get_mut(&name).unwrap();
                        bot.active = true;
                        peer.connection
                            .send(Packet::Welcome {
                                seed: self.world.generator.seed,
                                dimension: self.dimension.name().into(),
                                id: bot.id.0,
                            })
                            .map_err(|e| e.to_string())?;
                        Ok(name)
                    });
                    match result {
                        Ok(name) => {
                            peer.profile = Some(name.clone());
                            peer.dimension = self.dimension;
                            self.broadcast_chat(&format!("{name} joined the game"));
                        }
                        Err(reason) => {
                            peer.connection.close(&reason);
                            peer.dead = true;
                            break;
                        }
                    }
                    continue;
                }
                match packet {
                    Packet::Input(input) => {
                        if input.sequence > peer.input.sequence {
                            peer.input = input;
                            peer.last_input = Instant::now();
                        }
                    }
                    Packet::Chat(message) => {
                        let clean: String = message.chars().filter(|c| !c.is_control()).take(256).collect();
                        let line = format!("<{}> {clean}", peer.profile.as_deref().unwrap());
                        self.broadcast_chat(&line);
                    }
                    Packet::Command(command) => {
                        if let Err(e) = self.remote_command(peer, &command) {
                            let _ = peer.connection.send(Packet::Chat(format!("Error: {e}")));
                        }
                    }
                    Packet::Click { slot, right, shift } => self.remote_click(peer, slot, right, shift),
                    Packet::Disconnect(_) => {
                        self.leave_lan(peer);
                        peer.dead = true;
                        break;
                    }
                    _ => {
                        self.leave_lan(peer);
                        peer.connection.close("Invalid client packet");
                        peer.dead = true;
                        break;
                    }
                }
            }
            let _ = peer.connection.flush();
        }
        peers.retain(|p| !p.dead);
        self.lan.peers = peers;
        while let Some(message) = self.lan.messages.pop_front() {
            for peer in &mut self.lan.peers {
                if peer.profile.is_some() {
                    let _ = peer.connection.send(Packet::Chat(message.clone()));
                }
            }
        }
    }
    /// Opens screen `container` for a LAN player, as their click would.
    fn peer_open(&mut self, peer: &mut Peer, container: Container) {
        let Some(name) = peer.profile.clone() else { return };
        if peer.open {
            return;
        }
        let Some(bot) = self.agents.players.get_mut(&name) else { return };
        let session = &mut bot.agent.session;
        if !session.mode.can_interact() {
            return;
        }
        let present = match container {
            Container::Chest(p) => self.world.chest(p).is_some(),
            Container::Furnace(p) => self.world.furnace(p).is_some(),
            Container::Brewing(p) => self.world.brewing_stand(p).is_some(),
            _ => true,
        };
        if !present {
            return;
        }
        let container = match (container, session.player.vehicle) {
            (Container::Inventory, Some(id)) if self.mobs.entities.vehicle_slots(id).is_some_and(|s| !s.is_empty()) => {
                Container::Minecart(id)
            }
            _ => container,
        };
        if container == Container::CraftingTable {
            peer.grid = Grid::new(3);
        }
        session.cancel_mining();
        session.cancel_consumption();
        let (id, at) = (session.id, session.player.pos);
        peer.body.let_go();
        peer.body.container = container;
        peer.open = true;
        if let Container::Chest(pos) = container {
            self.chest_sound(pos, 0.65);
            if self.world.get_block(pos).is_some_and(crate::entity::nether::guarded_by_piglins) {
                self.mobs.entities.piglins_notice(id, at, true, &self.world);
            }
        }
    }

    /// Closes a LAN player's screen, returning its grid and work slots.
    fn peer_close(&mut self, peer: &mut Peer) {
        let Some(name) = peer.profile.clone() else { return };
        if !peer.open {
            return;
        }
        if let Container::Chest(pos) = peer.body.container {
            self.chest_sound(pos, 0.8);
        }
        if let Some(bot) = self.agents.players.get_mut(&name) {
            let session = &mut bot.agent.session;
            let work: Vec<_> = session.work.iter_mut().filter_map(Option::take).collect();
            session.inventory.return_stacks(peer.grid.take_all().into_iter().chain(work));
        }
        peer.grid = Grid::new(2);
        peer.body.container = Container::Inventory;
        peer.open = false;
    }
    fn remote_command(&mut self, peer: &mut Peer, command: &str) -> Result<(), String> {
        match command {
            "attack_down" | "attack_up" | "use_down" | "use_up" => {
                if peer.edges.len() >= 64 {
                    return Err("Too many queued button edges".into());
                }
                peer.edges.push((if command.starts_with("attack") { 8 } else { 16 }, command.ends_with("down")));
            }
            c if c.starts_with("view ") => {
                let view = c[5..].trim().parse::<i32>().map_err(|_| "Bad view distance".to_string())?;
                peer.view = view.clamp(2, LAN_VIEW_MAX);
            }
            "inventory" => self.peer_open(peer, Container::Inventory),
            "close" => self.peer_close(peer),
            "pick" => {
                let name = peer.profile.clone().unwrap_or_default();
                if let Some(bot) = self.agents.players.get_mut(&name) {
                    use voxelcraft::session::interact::Pick;
                    match bot.agent.session.pick_block(bot.agent.selected, &self.world) {
                        Some(Pick::Select(i)) => {
                            bot.agent.selected = i;
                            if peer.body.actions.select(i) {
                                bot.agent.session.cancel_consumption();
                            }
                        }
                        Some(Pick::Replaced) => peer.body.actions.reset(),
                        None => {}
                    }
                }
            }
            "toss" | "toss_one" if peer.open => {
                let name = peer.profile.clone().unwrap_or_default();
                if let Some(bot) = self.agents.players.get_mut(&name)
                    && let Some(stack) = bot.agent.session.inventory.cursor
                {
                    let s = &mut bot.agent.session;
                    let count = if command == "toss_one" { 1 } else { stack.count };
                    s.inventory.cursor = (stack.count > count)
                        .then_some(crate::inventory::Stack { count: stack.count - count, ..stack });
                    self.mobs.entities.throw(
                        crate::inventory::Stack { count, ..stack },
                        s.player.eye(),
                        s.player.forward().as_dvec3(),
                    );
                }
            }
            "drop" | "drop_stack" => {
                let name = peer.profile.clone().unwrap_or_default();
                self.drop_agent_selected(&name, &mut peer.body, command == "drop_stack");
            }
            "respawn" => {
                let name = peer.profile.as_deref().unwrap().to_owned();
                if self.agents.players[&name].agent.session.vitals.is_dead() {
                    // Find the spawn before the respawn command does: its own lookup
                    // forgets a missing bed, so a second lookup would never report it.
                    let (at, yaw) = if self.dimension == Dimension::Overworld {
                        let (at, yaw, missing) = self.respawn_point_of(&PlayerRef::Agent(name.clone()));
                        if missing {
                            let _ = peer.connection.send(Packet::Chat("Your home bed was missing".into()));
                        }
                        (at, yaw)
                    } else {
                        (self.beside_host(), voxelcraft::session::DEFAULT_SPAWN_YAW)
                    };
                    self.agents.players.get_mut(&name).unwrap().agent.execute(
                        Command::Respawn,
                        &mut self.world,
                        &mut self.mobs.entities,
                        &[],
                    )?;
                    let agent = &mut self.agents.players.get_mut(&name).unwrap().agent;
                    agent.session.player.pos = at;
                    agent.session.player.yaw = yaw;
                    agent.previous_pos = at;
                    peer.body = Body::default();
                    peer.open = false;
                }
            }
            "/save-all" if self.lan.cheats => {
                self.save();
                let _ = peer.connection.send(Packet::Chat("Saved the game".into()));
            }
            _ => {
                if !command.starts_with('/') {
                    return Err("Unknown LAN action".into());
                }
                let parsed = Command::parse(command)?;
                if !parsed.cheat() {
                    return Err("Only cheat commands are accepted here".into());
                }
                if parsed.cheat() && !self.lan.cheats {
                    return Err("LAN cheats are disabled".into());
                }
                if matches!(
                    parsed,
                    Command::Dimension(_)
                        | Command::GameRule { .. }
                        | Command::Difficulty(_)
                        | Command::SetWorldSpawn(_)
                        | Command::Seed
                        | Command::LocateStructure(_)
                        | Command::LocateBiome(_)
                        | Command::LocateNetherBiome(_)
                ) {
                    return Err("This command is host-only".into());
                }
                let name = peer.profile.as_deref().unwrap();
                self.agents.players.get_mut(name).unwrap().agent.execute(
                    parsed,
                    &mut self.world,
                    &mut self.mobs.entities,
                    &[self.session.player.pos],
                )?;
            }
        }
        Ok(())
    }

    /// Whether `container` is still there and within `session`'s screen range.
    pub(super) fn container_still_valid(
        &self,
        session: &voxelcraft::session::PlayerSession,
        container: Container,
    ) -> bool {
        let eye = session.player.eye();
        let near = |p: IVec3, ok: fn(Block) -> bool| {
            session.can_keep_using_block(p) && self.world.get_block(p).is_some_and(ok)
        };
        match container {
            Container::Inventory | Container::CraftingTable => true,
            Container::Chest(p) => near(p, crate::world::chest::is_chest),
            Container::Furnace(p) => near(p, crate::world::furnace::is_furnace),
            Container::Brewing(p) => near(p, |b| b == Block::BREWING_STAND),
            Container::Enchanting(p) => near(p, |b| b == Block::ENCHANTING_TABLE),
            Container::Anvil(p) => near(p, Block::is_anvil),
            Container::Grindstone(p) => near(p, |b| b.base() == Block::GRINDSTONE),
            Container::Smithing(p) => near(p, |b| b == Block::SMITHING_TABLE),
            Container::Trading(id) => self.mobs.entities.merchant_in_reach(id, eye, session.entity_screen_range()),
            Container::Minecart(id) => self.mobs.entities.vehicle_in_reach(id, eye, session.entity_screen_range()),
        }
    }
    fn remote_click(&mut self, peer: &mut Peer, encoded: u16, right: bool, shift: bool) {
        if !peer.open {
            return;
        }
        if let Some(p) = peer.used_at {
            let Some(bot) = self.agents.players.get(peer.profile.as_deref().unwrap()) else { return };
            if !bot.agent.session.can_keep_using_block(p) {
                return;
            }
        }
        let Some(name) = peer.profile.clone() else { return };
        let Some(session) = self.agents.players.get(&name).map(|b| &b.agent.session) else { return };
        if !self.container_still_valid(session, peer.body.container) || session.vitals.is_dead() {
            return;
        }
        let Some(slot) = decode_slot(encoded, peer.body.container, session.mode, peer.grid.size) else { return };
        let quick = shift && session.inventory.cursor.is_none();
        let mut container = peer.body.container;
        let done = self.agent_screen(&name, &mut container, &mut peer.grid, |s| {
            if quick { s.quick_move(slot) } else { s.click_slot(slot, right) }
        });
        peer.body.container = container;
        for super::screen::Request::Popup(text) in done.map(|(_, r)| r).unwrap_or_default() {
            let _ = peer.connection.send(Packet::Chat(text));
        }
    }
    pub(super) fn drive_lan(&mut self, dt: f64) {
        self.mobs.entities.pvp = self.lan.host.is_some() && self.lan.pvp;
        if self.arrival.is_some() {
            return;
        }
        let mut peers = std::mem::take(&mut self.lan.peers);
        for peer in &mut peers {
            let Some(name) = peer.profile.clone() else {
                continue;
            };
            let Some(bot) = self.agents.players.get_mut(&name).filter(|b| b.active) else {
                continue;
            };
            let input = if peer.last_input.elapsed() < Duration::from_millis(500) {
                peer.input
            } else {
                Input { sequence: peer.input.sequence, ..Default::default() }
            };
            // Every respawn path (the respawn command, `doImmediateRespawn`)
            // counts on the session; hold the angle it chose.
            let respawns = bot.agent.session.respawns;
            if peer.respawns.is_some_and(|seen| seen != respawns) {
                let player = &bot.agent.session.player;
                peer.look_override = Some((player.yaw, player.pitch, Instant::now()));
            }
            peer.respawns = Some(respawns);
            let (yaw, pitch) = peer_look(&mut peer.look_override, &input);
            bot.agent.session.player.yaw = yaw;
            bot.agent.session.player.pitch = pitch;
            bot.agent.selected = input.selected as usize;
            bot.agent.session.player.flying = bot.agent.session.player.can_fly && input.buttons & 32 != 0;
            if bot.agent.session.sleeping.is_some() && input.buttons & 1 != 0 {
                bot.agent.session.sleeping = None;
            }
            if bot.agent.session.vitals.is_dead() || peer.open || bot.agent.session.sleeping.is_some() {
                bot.agent.session.cancel_mining();
                bot.agent.hold(Default::default(), false, false);
            } else {
                bot.agent.hold(input.movement(), false, false);
            }
            if peer.open
                && peer.grid.size == 3
                && peer.used_at.is_some_and(|p| {
                    self.world.get_block(p) != Some(Block::CRAFTING_TABLE)
                        || !self.agents.players[&name].agent.session.can_keep_using_block(p)
                })
            {
                self.peer_close(peer);
            }
            if self.agents.players[&name].agent.session.vitals.is_dead() {
                let mut stacks = peer.grid.take_all();
                let bot = self.agents.players.get_mut(&name).unwrap();
                stacks.extend(bot.agent.session.work.iter_mut().filter_map(Option::take));
                if self.gamerules.bool("keep_inventory") {
                    bot.agent.session.inventory.return_stacks(stacks);
                } else {
                    for stack in
                        stacks.into_iter().filter(|s| !s.enchants.has(crate::enchant::Enchantment::VanishingCurse))
                    {
                        self.mobs.entities.throw(stack, bot.agent.session.player.eye(), DVec3::ZERO);
                    }
                }
            }
            // Close destroyed or out-of-reach containers before accepting any more clicks.
            if peer.open {
                let session = &self.agents.players[&name].agent.session;
                if session.vitals.is_dead() || !self.container_still_valid(session, peer.body.container) {
                    self.peer_close(peer);
                }
            }
            if !peer.open
                && !self.agents.players[&name].agent.session.vitals.is_dead()
                && self.agents.players[&name].agent.session.sleeping.is_none()
            {
                let transitions = button_transitions(peer.previous_buttons, &peer.edges, input.buttons);
                let fighter = PlayerRef::Agent(name.clone());
                let mut requests = Vec::new();
                for (bit, pressed) in transitions {
                    let done = if bit == 8 {
                        let (slot, sprint, interact) = self.agent_attack_context(&name);
                        let hit = pressed && interact && self.attack_lan_player(&fighter, slot, sprint);
                        self.agent_hands(&name, &mut peer.body, |h| h.attack_button(pressed, hit))
                    } else {
                        self.agent_hands(&name, &mut peer.body, |h| h.use_button(pressed))
                    };
                    requests.extend(done.map(|(_, r)| r).unwrap_or_default());
                }
                let aiming = self.agent_aiming_at_target(&name, &peer.body);
                if let Some((_, r)) = self.agent_hands(&name, &mut peer.body, |h| h.act(true, dt, aiming)) {
                    requests.extend(r);
                }
                let mut used = None;
                for request in requests {
                    use super::hands::Request;
                    use voxelcraft::session::interact::Used;
                    match request {
                        Request::Popup(text) => {
                            let _ = peer.connection.send(Packet::Chat(text));
                        }
                        Request::StopSprint => self.stop_sprint(&name),
                        Request::Open(Used::OpenMerchant(id)) => {
                            self.peer_open(peer, Container::Trading(id));
                        }
                        Request::Open(Used::OpenVehicle(id)) => {
                            self.peer_open(peer, Container::Minecart(id));
                        }
                        Request::Open(Used::Screen { cell, .. } | Used::Bed { cell }) => used = Some(cell),
                        Request::Open(_) => {}
                        Request::MagicDamage(amount) => {
                            if let Some(bot) = self.agents.players.get_mut(&name) {
                                bot.agent.hurt_magic(amount, &mut self.mobs.entities);
                            }
                        }
                    }
                }
                if let Some(p) = used {
                    peer.used_at = Some(p);
                    match self.world.get_block(p) {
                        Some(b) if b.is_bed() => {
                            let _ = self.agent_sleep(&name);
                        }
                        Some(Block::CRAFTING_TABLE) => {
                            self.peer_open(peer, Container::CraftingTable);
                        }
                        Some(b) if crate::world::chest::is_chest(b) => {
                            self.peer_open(peer, Container::Chest(p));
                        }
                        Some(b) if crate::world::furnace::is_furnace(b) => {
                            self.peer_open(peer, Container::Furnace(p));
                        }
                        Some(Block::BREWING_STAND) => {
                            self.peer_open(peer, Container::Brewing(p));
                        }
                        Some(Block::ENCHANTING_TABLE) => {
                            self.peer_open(peer, Container::Enchanting(p));
                        }
                        Some(b) if b.is_anvil() => {
                            self.peer_open(peer, Container::Anvil(p));
                        }
                        Some(b) if b.base() == Block::GRINDSTONE => {
                            self.peer_open(peer, Container::Grindstone(p));
                        }
                        Some(Block::SMITHING_TABLE) => {
                            self.peer_open(peer, Container::Smithing(p));
                        }
                        _ => {}
                    }
                }
            }
            if let Some(bot) = self.agents.players.get(&name) {
                let p = bot.agent.session.player.pos.floor().as_ivec3();
                if self
                    .world
                    .get_block(p)
                    .is_some_and(|b| b == Block::NETHER_PORTAL || b == Block::END_PORTAL || b == Block::END_GATEWAY)
                    && peer.last_portal_message.elapsed() > Duration::from_secs(5)
                {
                    peer.last_portal_message = Instant::now();
                    let _ = peer.connection.send(Packet::Chat(
                        "Portal travel is host-controlled; LAN players follow when the host travels".into(),
                    ));
                }
            }
            if peer.open
                || self.agents.players[&name].agent.session.vitals.is_dead()
                || self.agents.players[&name].agent.session.sleeping.is_some()
            {
                self.agent_hands(&name, &mut peer.body, |h| {
                    h.attack_button(false, false);
                    h.use_button(false);
                });
                peer.previous_buttons = input.buttons;
            } else {
                peer.previous_buttons = input.buttons;
            }
            peer.edges.clear();
            peer.applied = input.sequence;
        }
        self.lan.peers = peers;
    }
    pub(super) fn send_lan_state(&mut self) {
        if self.lan.peers.is_empty() {
            self.audio.tap = None;
            self.lan.effects.clear();
            return;
        }
        let mut host = Agent::new(self.session.player.pos);
        host.id = voxelcraft::entity::PlayerId::HOST;
        host.player.yaw = self.session.player.yaw;
        host.player.pitch = self.session.player.pitch;
        host.set_mode(self.session.mode);
        host.inventory = self.session.inventory.clone();
        host.vitals = self.session.vitals.clone();
        host.sleeping = self.session.sleeping;
        let host_eating = (self.body.actions.eat_timer / super::EAT_TIME) as f32;
        let players: Vec<_> =
            std::iter::once(state_lite(&host, &self.lan.profile, self.hand.swing_time(), host_eating))
                .chain(self.agents.players.iter().filter(|(_, b)| b.active).map(|(n, b)| {
                    let eating = self
                        .pad_view(n)
                        .map(|p| p.eating)
                        .or_else(|| self.lan.bodies().find(|x| x.0 == n).map(|x| x.2))
                        .unwrap_or_else(|| b.agent.eating());
                    state_lite(&b.agent, n, b.hand.swing_time(), eating)
                }))
                .collect();
        self.lan.look_ticks = self.lan.look_ticks.wrapping_add(1);
        self.lan.stats_ticks += 1;
        if self.lan.stats_ticks >= 200 {
            // Every ten seconds: tick cost and bandwidth, for docs/lan.md's measurements.
            let sent: u64 = self.lan.peers.iter().map(|p| p.connection.sent_bytes).sum();
            let rate = sent.saturating_sub(self.lan.stats_sent) as f64 / 10.0 / 1024.0;
            log::info!(
                "LAN stats: {} clients, tick {:.2} ms avg / {:.2} ms max, {:.1} KiB/s sent ({:.1} per client)",
                self.lan.peers.len(),
                self.lan.stats_tick_secs * 1000.0 / self.lan.stats_ticks as f64,
                self.lan.stats_tick_max * 1000.0,
                rate,
                rate / self.lan.peers.len().max(1) as f64,
            );
            self.lan.stats_ticks = 0;
            self.lan.stats_tick_secs = 0.0;
            self.lan.stats_tick_max = 0.0;
            self.lan.stats_sent = sent;
        }
        // Sounds and particles since the last state, replayed by clients near them.
        let sounds = self.audio.tap.replace(Vec::new()).unwrap_or_default();
        let effects = std::mem::take(&mut self.lan.effects);
        let host_pos = self.session.player.pos;
        let breaking = self.all_breaking();
        let host_view = self.settings.render_distance.clamp(2, LAN_VIEW_MAX);
        let widest = self.lan.peers.iter().map(|p| p.view.min(host_view)).max().unwrap_or(2);
        self.world.set_agent_radius(widest + 2);
        for peer in &mut self.lan.peers {
            let Some(name) = &peer.profile else {
                continue;
            };
            let Some(bot) = self.agents.players.get(name) else {
                continue;
            };
            if peer.dimension != self.dimension {
                peer.dimension = self.dimension;
                peer.sent.clear();
                peer.open = false;
                let _ = peer.connection.send(Packet::Welcome {
                    seed: self.world.generator.seed,
                    dimension: self.dimension.name().into(),
                    id: bot.id.0,
                });
            }
            let view = peer.view.min(host_view);
            let center = chunk_of(bot.agent.session.player.pos.floor().as_ivec3());
            // Two rings beyond the client's view, so every meshed chunk has all eight
            // horizontal neighbours (diagonals of edge chunks reach about r + 1.42).
            let radius = view + 2;
            let chunks = self.world.network_chunks(center, radius);
            peer.sent.retain(|p, _| {
                let d = (IVec3::new(p.0, p.1, p.2) - center).with_y(0);
                d.length_squared() <= radius * radius
            });
            // Changed chunks the client has go first as cell edits (a whole chunk only past
            // MAX_EDITS changes); then up to 32 new chunks, nearest first.
            let mut fresh = 0;
            for (pos, data) in chunks {
                let key = (pos.x, pos.y, pos.z);
                let old = peer.sent.get(&key);
                if old.is_some_and(|old| Arc::ptr_eq(old, &data)) {
                    continue;
                }
                if peer.connection.queued_bytes() > 512 * 1024 {
                    break;
                }
                let edits = old
                    .and_then(|old| voxelcraft::world::chunk::ChunkData::diff(old, &data, voxelcraft::lan::MAX_EDITS));
                let packet = match edits {
                    Some(cells) => Packet::Edits { pos, cells },
                    None if fresh < 32 => {
                        fresh += 1;
                        Packet::Chunk { pos, data: data.clone() }
                    }
                    None => continue,
                };
                if peer.connection.send(packet).is_ok() {
                    peer.sent.insert(key, data);
                }
            }
            let work: Vec<_> = bot.agent.session.work.iter().map(|s| stack_to_string(*s)).collect();
            let container = match peer.body.container() {
                Container::Chest(p) => {
                    json!({"kind":"chest","pos":p.to_array(),"state":self.world.chest(p).map(|c|c.serialize())})
                }
                Container::Furnace(p) => {
                    json!({"kind":"furnace","pos":p.to_array(),"state":self.world.furnace(p).map(|c|c.serialize())})
                }
                Container::Brewing(p) => {
                    json!({"kind":"brewing","pos":p.to_array(),"state":self.world.brewing_stand(p).map(|b|b.serialize())})
                }
                Container::Enchanting(p) => json!({"kind":"enchanting","pos":p.to_array(),"work":work}),
                Container::Anvil(p) => json!({"kind":"anvil","pos":p.to_array(),"work":work}),
                Container::Grindstone(p) => json!({"kind":"grindstone","pos":p.to_array(),"work":work}),
                Container::Smithing(p) => json!({"kind":"smithing","pos":p.to_array(),"work":work}),
                Container::Trading(id) => match self.mobs.entities.merchant(id) {
                    Some(m) => json!({"kind":"trading","uid":m.uid,"trade":m.trade_view()}),
                    None => json!({"kind":"inventory"}),
                },
                Container::Minecart(id) => {
                    let slots: Vec<_> = self
                        .mobs
                        .entities
                        .vehicle_slots(id)
                        .map(|s| s.iter().map(|s| stack_to_string(*s)).collect())
                        .unwrap_or_default();
                    let mount = self.mobs.entities.mount(id).and_then(|m| m.mount.as_ref());
                    json!({"kind":"vehicle","id":id,"slots":slots,"chest":mount.is_some_and(|s| s.chest),
                        "strength":mount.map_or(3, |s| s.strength)})
                }
                Container::CraftingTable => json!({"kind":"crafting"}),
                Container::Inventory => json!({"kind":"inventory"}),
            };
            let refresh = self.lan.look_ticks.is_multiple_of(20);
            let mut looked = std::collections::BTreeSet::new();
            let mobs: Vec<_> = self
                .mobs
                .entities
                .mobs
                .iter()
                .filter(|m| m.pos.distance_squared(bot.agent.session.player.pos) < 160.0 * 160.0)
                .take(256)
                .map(|m| {
                    // Compact pose: uid, kind, position, yaws/pitch, health, hurt, dying (-1 alive), size, baby.
                    let kind = crate::entity::MobKind::ALL.iter().position(|k| *k == m.kind).unwrap_or(0);
                    let mut v = json!({"m":[m.uid, kind, r3(m.pos.x), r3(m.pos.y), r3(m.pos.z), r3(m.yaw as f64),
                        r3(m.head_yaw as f64), r3(m.head_pitch as f64), r3(m.health as f64), r3(m.hurt as f64),
                        m.dying.map_or(-1.0, |d| r3(d as f64)), m.size, u8::from(m.baby)]});
                    if refresh || !peer.looked.contains(&m.uid) {
                        v["look"] = m.look();
                    }
                    // The charge bar changes every tick, so it rides with the pose.
                    if let Some(mount) = m.mount.as_ref().filter(|s| s.jump_charge > 0.0) {
                        v["jump"] = json!(mount.jump_charge);
                    }
                    looked.insert(m.uid);
                    v
                })
                .collect();
            peer.looked = looked;
            let me = bot.agent.session.player.pos;
            let sounds: Vec<_> = sounds
                .iter()
                .filter_map(|&(sound, pos, gain, (lo, hi))| {
                    use crate::audio::sounds::Sound;
                    let movement = matches!(sound, Sound::Step(_) | Sound::Land | Sound::Splash | Sound::Swim);
                    // Positionless sounds are the host's own: only body sounds others would hear.
                    let pos = match pos {
                        Some(p) => p,
                        None if movement || sound == Sound::Hurt => host_pos,
                        None => return None,
                    };
                    // A client hears its own steps locally.
                    if movement && pos.distance_squared(me) < 1.0 {
                        return None;
                    }
                    (pos.distance_squared(me) < 64.0 * 64.0)
                        .then(|| json!([sound.key(), pos.x, pos.y, pos.z, gain, lo, hi]))
                })
                .take(96)
                .collect();
            let effects: Vec<_> = effects
                .iter()
                .filter(|(p, _)| p.distance_squared(me) < 96.0 * 96.0)
                .take(128)
                .map(|(_, j)| j.clone())
                .collect();
            let cracks: Vec<_> = breaking
                .iter()
                .filter(|(p, _)| (p.as_dvec3() + 0.5).distance_squared(me) < 32.0 * 32.0)
                .take(crate::render::MAX_CRACKS)
                .map(|(p, f)| json!([p.x, p.y, p.z, r3(*f as f64)]))
                .collect();
            let extras = self.mobs.entities.replica_extras(bot.agent.session.player.pos, 160.0);
            let falling: Vec<_> = self
                .world
                .falling_blocks()
                .iter()
                .filter(|f| f.pos.distance_squared(bot.agent.session.player.pos) < 160.0 * 160.0)
                .take(256)
                .map(|f| {
                    json!([f.block.0, f.pos.x, f.pos.y, f.pos.z, f.previous_pos.x, f.previous_pos.y, f.previous_pos.z])
                })
                .collect();
            let carts: Vec<_> = self
                .mobs
                .entities
                .minecarts
                .iter()
                .filter(|c| c.pos.distance_squared(bot.agent.session.player.pos) < 160.0 * 160.0)
                .take(64)
                .map(|c| json!({"cart": c.serialize(), "paddle": c.paddle}))
                .collect();
            let items = self
                .mobs
                .entities
                .items
                .iter()
                .filter(|m| m.pos.distance_squared(bot.agent.session.player.pos) < 160.0 * 160.0)
                .take(256)
                .map(|m| m.serialize())
                .collect::<Vec<_>>()
                .join(";");
            let arrows:Vec<_>=self.mobs.entities.arrows.iter().filter(|a|a.pos.distance_squared(bot.agent.session.player.pos)<160.0*160.0).take(256).map(|a|json!({"pos":a.pos.to_array(),"previous":a.previous_pos.to_array(),"dir":a.dir.to_array(),"player":a.from_player,"critical":a.critical})).collect();
            // Everyone but this client itself (it has its own full state).
            let others: Vec<&Value> = players.iter().filter(|p| p["id"] != bot.id.0).collect();
            let snapshot = json!({"sequence":peer.applied,"self":state(&bot.agent,name),"players":others,"time":self.day_time,"rain":self.weather.raining,"rain_strength":self.weather.strength,"thunder":self.weather.thundering,"dimension":self.dimension.name(),"seed":self.world.generator.seed,"view":view,"open":peer.open,"container":container,"grid":peer.grid.cells.iter().map(|s|stack_to_string(*s)).collect::<Vec<_>>(),"grid_size":peer.grid.size,"mobs":mobs,"carts":carts,"items":items,"arrows":arrows,"extras":extras,"falling":falling,"sounds":sounds,"effects":effects,"cracks":cracks});
            let (snapshot, shed_looks) = fit_state(snapshot);
            if shed_looks {
                // Looks dropped to fit: resend them all next tick.
                peer.looked.clear();
            }
            let result = match snapshot {
                Some(v) => peer.connection.send(Packet::State(v)),
                None => Ok(()),
            };
            if let Err(e) = result.and_then(|()| peer.connection.flush()) {
                peer.connection.close(&e.to_string());
            }
        }
    }
    fn reset_lan_world(&mut self, seed: u64, dimension: Dimension) {
        self.renderer.clear_world();
        self.dimension = dimension;
        self.arrival = None;
        self.world =
            World::new_remote(Arc::new(Generator::for_dimension(seed, dimension)), self.settings.render_distance);
        self.lan.predictions.clear();
        self.lan.ready = false;
        self.mobs.entities = crate::entity::Entities::new(seed);
    }
    fn sample_lan_input(&mut self) -> Input {
        let movement = self.movement_input(false);
        let active = self.menu.is_none() && !self.console.open && !self.inventory_open && self.mouse_grabbed;
        self.lan.sequence = self.lan.sequence.wrapping_add(1);
        Input {
            sequence: self.lan.sequence,
            yaw: self.session.player.yaw,
            pitch: self.session.player.pitch,
            forward: movement.forward as f32,
            right: movement.right as f32,
            buttons: u8::from(movement.jump)
                | (u8::from(movement.descend) * 2)
                | (u8::from(movement.sprint) * 4)
                | (u8::from(active && self.body.left_held) * 8)
                | (u8::from(active && self.body.right_held) * 16)
                | (u8::from(self.session.player.flying) * 32),
            selected: self.body.actions.selected as u8,
        }
    }
    pub(super) fn lan_button_edge(&mut self, attack: bool, pressed: bool) {
        if !attack && pressed {
            // The host sees every press; remember one between client ticks.
            self.lan.use_pressed = true;
        }
        let input = self.sample_lan_input();
        self.lan_send(Packet::Input(input));
        self.lan_command(match (attack, pressed) {
            (true, true) => "attack_down",
            (true, false) => "attack_up",
            (false, true) => "use_down",
            (false, false) => "use_up",
        });
    }
    /// The movement a LAN client predicts for one of its inputs: the host
    /// blocks sprinting by hunger and blindness, and slows the player while
    /// the use button holds food it may eat, a drink, or a bow or crossbow it
    /// can draw (Java's `isUsingItem`; the client runs no consumption clock).
    fn lan_movement(&self, input: &Input) -> voxelcraft::player::MoveInput {
        let slot = usize::from(input.selected).min(8);
        let drawable = self.lan.predicted_draw == Some(slot);
        // As `Hands::eat`: food (not drink) aimed at a composter without
        // sneaking fills the composter instead of being eaten.
        let drink = self.session.inventory.get(slot).is_some_and(|s| s.item.is_drink());
        let composting = input.buttons & 2 == 0
            && self
                .session
                .block_target(&self.world)
                .and_then(|(pos, _)| self.world.get_block(pos))
                .is_some_and(|b| voxelcraft::world::composter::level(b).is_some());
        let eating = self.session.consumption_duration(slot).is_ok() && (drink || !composting);
        voxelcraft::player::MoveInput {
            sprint_blocked: !self.session.vitals.may_sprint(self.session.mode.invulnerable()),
            using_item: input.buttons & 16 != 0 && (eating || drawable),
            ..input.movement()
        }
    }
    pub(super) fn tick_lan_client(&mut self) {
        let input = self.sample_lan_input();
        let slot = usize::from(input.selected).min(8);
        let use_held = input.buttons & 16 != 0;
        let bow = self
            .session
            .inventory
            .get(slot)
            .is_some_and(|s| matches!(s.item, voxelcraft::item::Item::BOW | voxelcraft::item::Item::CROSSBOW));
        // The host starts a draw on the press of use (`Hands::use_button`),
        // then keeps it while use stays held on the same bow.
        // A press since the last tick means use was released first, which
        // ends any draw; the press then starts a new one only if it can.
        let repressed = self.lan.use_pressed;
        let pressed = use_held && (!self.lan.use_was_held || repressed);
        if use_held != self.lan.use_was_held || repressed {
            self.lan.use_edge_sequence = input.sequence;
        }
        self.lan.use_was_held = use_held;
        self.lan.use_pressed = false;
        self.lan.predicted_draw = match self.lan.predicted_draw {
            Some(drawn) if use_held && !repressed && drawn == slot && bow => Some(drawn),
            _ if pressed && self.session.can_draw(slot, &self.world) => Some(slot),
            _ => None,
        };
        let movement = self.lan_movement(&input);
        self.lan_send(Packet::Input(input));
        self.previous_eye = self.session.player.eye();
        // A seated client takes its position from the host; the vehicle moves it.
        if self.lan.ready && self.session.player.vehicle.is_some() {
            self.lan.predictions.push_back(input);
        } else if self.lan.ready {
            self.session.player.apply_effects(&self.session.vitals.effects);
            self.session.player.powder_snow_equipment(&self.session.inventory.armor, self.session.vitals.ticks_frozen);
            self.session.player.wear_boots(crate::enchant::armor_level(
                &self.session.inventory.armor,
                crate::enchant::Enchantment::DepthStrider,
            ));
            self.session.player.tick(movement, &self.world);
            self.lan.predictions.push_back(input);
        }
        if self.lan.ready {
            self.tick_particles();
            if self.lan.predictions.len() > 128 {
                self.lan.lost = Some("Server stopped acknowledging input".into());
            }
        }
        self.jump_pressed = false;
    }
    fn receive_lan_state(&mut self, snapshot: &Value) {
        if snapshot["harness"] == true {
            self.lan.lost = Some("Headless protocol harness does not serve desktop gameplay".into());
            return;
        }
        let was_dead = self.session.vitals.is_dead();
        if let Some(dimension) = snapshot["dimension"].as_str().and_then(Dimension::from_name)
            && dimension != self.dimension
        {
            self.reset_lan_world(snapshot["seed"].as_u64().unwrap_or(0), dimension);
            self.chat_line("Host changed dimension; all LAN players follow the host");
        }
        let sequence = snapshot["sequence"].as_u64().unwrap_or(0) as u32;
        while self.lan.predictions.front().is_some_and(|i| i.sequence <= sequence) {
            self.lan.predictions.pop_front();
        }
        let mut own = Agent::new(self.session.player.pos);
        apply(&mut own, &snapshot["self"]);
        // The host's draw is authoritative (a charged crossbow fired and
        // reloading, say) once it has seen this client's latest use press or
        // release; until then the local prediction stands.
        if sequence >= self.lan.use_edge_sequence {
            self.lan.predicted_draw = snapshot["self"]["drawing"].as_u64().map(|slot| slot as usize);
        }
        // The client owns its look, except that a respawn faces the host's
        // respawn angle (Java's respawn transition sets the rotation).
        let respawns = snapshot["self"]["respawns"].as_u64();
        let respawned = self.lan.respawns.zip(respawns).is_some_and(|(seen, now)| seen != now);
        self.lan.respawns = respawns;
        let (yaw, pitch) = if respawned {
            (own.session.player.yaw, own.session.player.pitch)
        } else {
            (self.session.player.yaw, self.session.player.pitch)
        };
        self.previous_eye = self.session.player.eye();
        self.session.player = own.session.player;
        self.session.player.yaw = yaw;
        self.session.player.pitch = pitch;
        self.session.mode = own.session.mode;
        self.session.inventory = own.session.inventory;
        self.session.vitals = own.session.vitals;
        self.session.sleeping = own.session.sleeping;
        if !self.lan.ready {
            self.body.actions.selected = own.selected;
            self.previous_eye = self.session.player.eye();
        }
        let seated = self.session.player.vehicle.is_some();
        for input in self.lan.predictions.iter().filter(|_| !seated) {
            self.session.player.yaw = input.yaw;
            self.session.player.pitch = input.pitch;
            self.session.player.apply_effects(&self.session.vitals.effects);
            self.session.player.powder_snow_equipment(&self.session.inventory.armor, self.session.vitals.ticks_frozen);
            let movement = self.lan_movement(input);
            self.session.player.tick(movement, &self.world);
        }
        self.session.player.yaw = yaw;
        self.session.player.pitch = pitch;
        self.lan.ready = true;
        self.world.release_unloads();
        // View distance: the client's own setting, capped by what the host streams.
        if let Some(view) = snapshot["view"].as_i64() {
            let want =
                (view as i32).min(self.lan.offline_render_distance.unwrap_or(LAN_VIEW_MAX)).clamp(2, LAN_VIEW_MAX);
            if want != self.settings.render_distance {
                self.settings.render_distance = want;
                self.world.set_render_distance(want);
            }
        }
        self.day_time = snapshot["time"].as_f64().unwrap_or(self.day_time);
        self.weather.raining = snapshot["rain"] == true;
        self.weather.strength = snapshot["rain_strength"].as_f64().unwrap_or(0.0) as f32;
        self.weather.thundering = snapshot["thunder"] == true;
        let was_open = self.inventory_open;
        self.inventory_open = snapshot["open"] == true;
        if self.inventory_open != was_open {
            self.set_grab(!self.inventory_open && self.menu.is_none());
            self.body.left_held = false;
            self.body.right_held = false;
            self.keys.clear();
        }
        let c = &snapshot["container"];
        let pos = vector(&c["pos"]).map(|p| p.as_ivec3());
        self.body.container = match (c["kind"].as_str(), pos) {
            (Some("chest"), Some(p)) => {
                if let Some(text) = c["state"].as_str() {
                    self.world.load_chests(&format!("{},{},{}={text}", p.x, p.y, p.z));
                }
                Container::Chest(p)
            }
            (Some("furnace"), Some(p)) => {
                if let Some(text) = c["state"].as_str() {
                    self.world.load_furnaces(&format!("{},{},{}={text}", p.x, p.y, p.z));
                }
                Container::Furnace(p)
            }
            (Some("brewing"), Some(p)) => {
                if let Some(text) = c["state"].as_str() {
                    self.world.load_brewing_stands(&format!("{},{},{}={text}", p.x, p.y, p.z));
                }
                Container::Brewing(p)
            }
            (Some("enchanting"), Some(p)) => Container::Enchanting(p),
            (Some("anvil"), Some(p)) => Container::Anvil(p),
            (Some("grindstone"), Some(p)) => Container::Grindstone(p),
            (Some("smithing"), Some(p)) => Container::Smithing(p),
            (Some("trading"), _) => match c["trade"]["id"].as_u64() {
                Some(id) => Container::Trading(id),
                None => Container::Inventory,
            },
            (Some("vehicle"), _) => match c["id"].as_u64().and_then(|id| u32::try_from(id).ok()) {
                Some(id) => Container::Minecart(id),
                None => Container::Inventory,
            },
            (Some("crafting"), _) => Container::CraftingTable,
            _ => Container::Inventory,
        };
        self.session.work = [None; 3];
        if let Some(work) = c["work"].as_array() {
            for (slot, v) in self.session.work.iter_mut().zip(work) {
                *slot = v.as_str().and_then(stack_from_str).flatten();
            }
        }
        self.craft = Grid::new(if snapshot["grid_size"] == 3 { 3 } else { 2 });
        if let Some(cells) = snapshot["grid"].as_array() {
            for (slot, v) in self.craft.cells.iter_mut().zip(cells) {
                *slot = v.as_str().and_then(stack_from_str).flatten();
            }
        }
        if let Some(players) = snapshot["players"].as_array() {
            self.agents.players.retain(|n, _| players.iter().any(|p| p["name"] == *n && p["id"] != self.lan.id));
            for p in players.iter().take(40).filter(|p| p["id"] != self.lan.id) {
                let Some(name) = p["name"].as_str().filter(|n| voxelcraft::control::valid_name(n)) else { continue };
                if !self.agents.players.contains_key(name) {
                    self.agents.insert(name.into(), Agent::new(vector(&p["pos"]).unwrap_or(self.session.player.pos)));
                }
                let bot = self.agents.players.get_mut(name).unwrap();
                bot.active = true;
                apply(&mut bot.agent, p);
                bot.hand.set_swing_time(p["swing"].as_f64().map(|t| t as f32));
                bot.agent.show_eating(p["eat"].as_f64().unwrap_or(0.0) as f32);
                bot.id = voxelcraft::entity::PlayerId(p["id"].as_u64().unwrap_or(0) as u32);
            }
        }
        if let Some(mobs) = snapshot["mobs"].as_array() {
            let mut previous: BTreeMap<_, _> =
                std::mem::take(&mut self.mobs.entities.mobs).into_iter().map(|m| (m.uid, m)).collect();
            for m in mobs.iter().take(256) {
                let f = |i: usize| m["m"].get(i).and_then(Value::as_f64);
                let kind =
                    m["m"].get(1).and_then(Value::as_u64).and_then(|k| crate::entity::MobKind::ALL.get(k as usize));
                let (Some(&kind), Some(x), Some(y), Some(z)) = (kind, f(2), f(3), f(4)) else { continue };
                let pos = DVec3::new(x, y, z);
                if !pos.is_finite() {
                    continue;
                }
                let uid = m["m"].get(0).and_then(Value::as_u64).unwrap_or(0) as u32;
                let mut mob = previous.remove(&uid).unwrap_or_else(|| crate::entity::Mob::new(kind, pos, 0.0));
                mob.previous_pos = mob.pos;
                mob.pos = pos;
                mob.uid = uid;
                mob.yaw = f(5).unwrap_or(0.0) as f32;
                mob.head_yaw = f(6).unwrap_or(0.0) as f32;
                mob.head_pitch = f(7).unwrap_or(0.0) as f32;
                mob.health = f(8).unwrap_or(20.0) as f32;
                mob.hurt = f(9).unwrap_or(0.0) as f32;
                mob.dying = f(10).filter(|d| *d >= 0.0).map(|d| d as f32);
                mob.size = f(11).unwrap_or(1.0).clamp(1.0, 4.0) as u8;
                mob.baby = f(12) == Some(1.0);
                if m["look"].is_object() {
                    mob.apply_look(&m["look"]);
                }
                if let Some(mount) = mob.mount.as_mut() {
                    mount.jump_charge = m["jump"].as_f64().map_or(0.0, |j| j.clamp(0.0, 1.0) as f32);
                }
                mob.animate_replica(voxelcraft::simulation::TICK_SECONDS as f32);
                self.mobs.entities.mobs.push(mob);
            }
        }
        if let Some(carts) = snapshot["carts"].as_array() {
            let previous: BTreeMap<u32, DVec3> = self.mobs.entities.minecarts.iter().map(|c| (c.id, c.pos)).collect();
            self.mobs.entities.minecarts = carts
                .iter()
                .take(64)
                .filter_map(|c| {
                    let mut cart = crate::entity::minecart::Minecart::deserialize(c["cart"].as_str()?)?;
                    cart.previous_pos = previous.get(&cart.id).copied().unwrap_or(cart.pos);
                    cart.paddle = c["paddle"].as_f64().unwrap_or(0.0) as f32;
                    Some(cart)
                })
                .collect();
        }
        // Screens that show mob state: the merchant's offers and a mount's storage.
        let c = &snapshot["container"];
        if c["kind"] == "trading"
            && let Some(uid) = c["uid"].as_u64()
            && let Some(m) = self.mobs.entities.mobs.iter_mut().find(|m| m.uid as u64 == uid)
        {
            m.apply_trade_view(&c["trade"]);
        }
        if c["kind"] == "vehicle"
            && let Some(id) = c["id"].as_u64().and_then(|id| u32::try_from(id).ok())
            && let Some(s) = self.mobs.entities.mount_mut(id).and_then(|m| m.mount.as_mut())
        {
            s.tame = true;
            s.chest = c["chest"] == true;
            s.strength = c["strength"].as_u64().unwrap_or(3).clamp(1, 5) as u8;
            s.slots = [None; 17];
            for (slot, v) in s.slots.iter_mut().zip(c["slots"].as_array().into_iter().flatten()) {
                *slot = v.as_str().and_then(stack_from_str).flatten();
            }
        }
        self.lan.remote_cracks = snapshot["cracks"]
            .as_array()
            .map(|c| {
                c.iter()
                    .take(crate::render::MAX_CRACKS)
                    .filter_map(|c| {
                        let i = |k: usize| c.get(k).and_then(Value::as_i64).and_then(|v| i32::try_from(v).ok());
                        Some((IVec3::new(i(0)?, i(1)?, i(2)?), c.get(3)?.as_f64()?.clamp(0.0, 1.0) as f32))
                    })
                    .collect()
            })
            .unwrap_or_default();
        if let Some(sounds) = snapshot["sounds"].as_array() {
            for s in sounds.iter().take(96) {
                let f = |i: usize| s.get(i).and_then(Value::as_f64);
                let (Some(sound), Some(x), Some(y), Some(z), Some(gain), Some(lo), Some(hi)) = (
                    s.get(0).and_then(Value::as_u64).and_then(|k| crate::audio::sounds::Sound::from_key(k as usize)),
                    f(1),
                    f(2),
                    f(3),
                    f(4),
                    f(5),
                    f(6),
                ) else {
                    continue;
                };
                let pos = DVec3::new(x, y, z);
                if pos.is_finite() {
                    let pitch = (lo.clamp(0.1, 4.0) as f32, hi.clamp(0.1, 4.0) as f32);
                    self.audio.play(sound, Some(pos), gain.clamp(0.0, 4.0) as f32, pitch);
                }
            }
        }
        if let Some(effects) = snapshot["effects"].as_array() {
            let setting = self.settings.particles;
            for e in effects.iter().take(128) {
                if let Some(request) = crate::particles::Request::from_json(e) {
                    if let crate::particles::Request::Honey { pos, sound: true, .. } = request {
                        self.audio.play(crate::audio::Sound::HoneySlide, Some(pos), 1.0, (1.0, 1.0));
                    }
                    self.particles.request(request, setting, &self.world);
                }
            }
        }
        if snapshot["extras"].is_object() {
            self.mobs.entities.apply_replica_extras(&snapshot["extras"]);
        }
        if let Some(falling) = snapshot["falling"].as_array() {
            self.world.set_falling_replica(falling.iter().filter_map(|f| {
                let n = |i: usize| f.get(i).and_then(Value::as_f64);
                let block = voxelcraft::world::block::Block(u16::try_from(f.get(0)?.as_u64()?).ok()?);
                if block.0 as usize >= voxelcraft::world::block::STATE_CAPACITY {
                    return None;
                }
                let pos = DVec3::new(n(1)?, n(2)?, n(3)?);
                let previous = DVec3::new(n(4)?, n(5)?, n(6)?);
                (pos.is_finite() && previous.is_finite()).then_some((block, pos, previous))
            }));
        }
        self.mobs.entities.arrows.clear();
        if let Some(arrows) = snapshot["arrows"].as_array() {
            for a in arrows.iter().take(256) {
                let (Some(pos), Some(dir)) = (vector(&a["pos"]), vector(&a["dir"])) else { continue };
                let mut arrow = crate::entity::Arrow::shot(pos - dir * 0.3, dir, 0.0, false);
                arrow.pos = pos;
                arrow.previous_pos = vector(&a["previous"]).unwrap_or(pos);
                arrow.from_player = a["player"] == true;
                arrow.critical = a["critical"] == true;
                self.mobs.entities.arrows.push(arrow);
            }
        }
        let old_items = std::mem::take(&mut self.mobs.entities.items);
        if let Some(items) = snapshot["items"].as_str() {
            self.mobs.entities.load_items(items);
            for item in &mut self.mobs.entities.items {
                if let Some(old) = old_items
                    .iter()
                    .filter(|old| old.stack == item.stack && old.pos.distance_squared(item.pos) < 4.0)
                    .min_by(|a, b| a.pos.distance_squared(item.pos).total_cmp(&b.pos.distance_squared(item.pos)))
                {
                    item.previous_pos = old.pos;
                    item.phase = old.phase;
                }
            }
        }
        if self.session.vitals.is_dead() {
            self.set_grab(false);
        } else if was_dead {
            self.set_grab(self.menu.is_none());
        }
    }
}

// Only bounded, supported slot kinds cross the network. Validate against the actual host screen.
pub(super) fn encode_slot(slot: SlotRef) -> Option<u16> {
    Some(match slot {
        SlotRef::Inventory(i) => i as u16,
        SlotRef::Craft(i) => 100 + i as u16,
        SlotRef::CraftResult => 109,
        SlotRef::Chest(i) => 200 + i as u16,
        SlotRef::FurnaceInput => 300,
        SlotRef::FurnaceFuel => 301,
        SlotRef::FurnaceOutput => 302,
        SlotRef::Armor(piece) => 400 + piece as u16,
        SlotRef::BrewBottle(i) if i < 3 => 500 + i as u16,
        SlotRef::BrewIngredient => 503,
        SlotRef::BrewFuel => 504,
        SlotRef::EnchantItem => 510,
        SlotRef::EnchantLapis => 511,
        SlotRef::EnchantOffer(i) if i < 3 => 512 + i as u16,
        SlotRef::AnvilLeft => 520,
        SlotRef::AnvilRight => 521,
        SlotRef::AnvilResult => 522,
        SlotRef::SmithTemplate => 530,
        SlotRef::SmithBase => 531,
        SlotRef::SmithAddition => 532,
        SlotRef::SmithResult => 533,
        SlotRef::Trade(i) if i < 10 => 540 + i as u16,
        SlotRef::Palette(item) => 1000 + crate::item::Item::creative_palette().position(|i| i == item)? as u16,
        _ => return None,
    })
}
fn decode_slot(slot: u16, container: Container, mode: GameMode, grid_size: usize) -> Option<SlotRef> {
    match slot {
        0..=35 => Some(SlotRef::Inventory(slot as usize)),
        100..=108 if (slot - 100) < (grid_size * grid_size) as u16 => Some(SlotRef::Craft((slot - 100) as usize)),
        109 => Some(SlotRef::CraftResult),
        200..=253 if matches!(container, Container::Chest(_) | Container::Minecart(_)) => {
            Some(SlotRef::Chest((slot - 200) as usize))
        }
        500..=504 if matches!(container, Container::Brewing(_)) => Some(match slot {
            503 => SlotRef::BrewIngredient,
            504 => SlotRef::BrewFuel,
            i => SlotRef::BrewBottle((i - 500) as usize),
        }),
        510..=514 if matches!(container, Container::Enchanting(_)) => Some(match slot {
            510 => SlotRef::EnchantItem,
            511 => SlotRef::EnchantLapis,
            i => SlotRef::EnchantOffer((i - 512) as usize),
        }),
        520..=522 if matches!(container, Container::Anvil(_) | Container::Grindstone(_)) => Some(match slot {
            520 => SlotRef::AnvilLeft,
            521 => SlotRef::AnvilRight,
            _ => SlotRef::AnvilResult,
        }),
        530..=533 if matches!(container, Container::Smithing(_)) => Some(match slot {
            530 => SlotRef::SmithTemplate,
            531 => SlotRef::SmithBase,
            532 => SlotRef::SmithAddition,
            _ => SlotRef::SmithResult,
        }),
        540..=549 if matches!(container, Container::Trading(_)) => Some(SlotRef::Trade((slot - 540) as usize)),
        300..=302 if matches!(container, Container::Furnace(_)) => Some(match slot {
            300 => SlotRef::FurnaceInput,
            301 => SlotRef::FurnaceFuel,
            _ => SlotRef::FurnaceOutput,
        }),
        400..=403 if container == Container::Inventory => Some(SlotRef::Armor(match slot {
            400 => crate::item::ArmorPiece::Helmet,
            401 => crate::item::ArmorPiece::Chestplate,
            402 => crate::item::ArmorPiece::Leggings,
            _ => crate::item::ArmorPiece::Boots,
        })),
        1000.. if mode.is_creative() && container == Container::Inventory => {
            crate::item::Item::creative_palette().nth((slot - 1000) as usize).map(SlotRef::Palette)
        }
        _ => None,
    }
}

impl Game {
    pub(super) fn lan_ui(&self, ui: &mut crate::render::ui::Ui) {
        if self.lan.host.is_none() && self.lan.client.is_none() {
            return;
        }
        let (sw, sh) = ui.size();
        if self.keys.contains(&winit::keyboard::KeyCode::Tab) && !self.console.open {
            let mut names = vec![self.lan.profile.as_str()];
            names.extend(self.agents.players.iter().filter(|(_, b)| b.active).map(|(n, _)| n.as_str()));
            let width = names.iter().map(|n| crate::render::ui::Ui::text_width(n)).fold(100.0, f32::max) + 16.0;
            ui.rect((sw - width) / 2.0, 12.0, width, 16.0 + names.len() as f32 * 12.0, [0.0, 0.0, 0.0, 0.75]);
            for (i, name) in names.iter().enumerate() {
                ui.text((sw - width) / 2.0 + 8.0, 20.0 + i as f32 * 12.0, name, crate::render::ui::WHITE);
            }
        }
        if !self.console.open && self.menu.is_none() && !self.inventory_open {
            for (i, line) in self.console.lines.iter().rev().filter(|s| !s.is_empty()).take(4).enumerate() {
                ui.rect(
                    4.0,
                    sh - 64.0 - i as f32 * 12.0,
                    crate::render::ui::Ui::text_width(line) + 6.0,
                    11.0,
                    [0.0, 0.0, 0.0, 0.45],
                );
                ui.text(7.0, sh - 62.0 - i as f32 * 12.0, line, crate::render::ui::WHITE);
            }
        }
    }
}

/// A player on the desktop host: the host itself, or a controller or LAN
/// player by agent profile name.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) enum PlayerRef {
    Host,
    /// A controller or LAN player, by agent profile name.
    Agent(String),
}

impl Game {
    fn player(&self, who: &PlayerRef) -> Option<&voxelcraft::session::PlayerSession> {
        match who {
            PlayerRef::Host => Some(&self.session),
            PlayerRef::Agent(name) => self.agents.players.get(name).map(|b| &b.agent.session),
        }
    }

    fn player_mut(&mut self, who: &PlayerRef) -> Option<&mut voxelcraft::session::PlayerSession> {
        match who {
            PlayerRef::Host => Some(&mut self.session),
            PlayerRef::Agent(name) => self.agents.players.get_mut(name).map(|b| &mut b.agent.session),
        }
    }

    /// The other player under `attacker`'s crosshair, in reach and in front
    /// of any block or mob, when the host allows PvP.
    pub(super) fn lan_player_target(&self, attacker: &PlayerRef) -> Option<PlayerRef> {
        if self.lan.host.is_none() || !self.lan.pvp {
            return None;
        }
        let me = self.player(attacker)?;
        let (eye, dir) = (me.player.eye(), me.player.forward().as_dvec3());
        let mut reach = me
            .block_target(&self.world)
            .and_then(|(p, _)| crate::physics::ray_aabb(eye, dir, p.as_dvec3(), p.as_dvec3() + DVec3::ONE))
            .unwrap_or(me.entity_reach())
            .min(me.entity_reach());
        for mob in &self.mobs.entities.mobs {
            let (min, max) = mob.shape().aabb(mob.pos);
            if let Some(distance) = crate::physics::ray_aabb(eye, dir, min, max) {
                reach = reach.min(distance);
            }
        }
        let host = (!self.session.vitals.is_dead()).then_some((PlayerRef::Host, &self.session));
        let agents = self
            .agents
            .players
            .iter()
            .filter(|(_, b)| b.active && !b.agent.session.vitals.is_dead())
            .map(|(name, b)| (PlayerRef::Agent(name.clone()), &b.agent.session));
        host.into_iter()
            .chain(agents)
            .filter(|(who, _)| who != attacker)
            .filter_map(|(who, s)| {
                let p = &s.player;
                let shape = p.collision_shape();
                crate::physics::ray_aabb(
                    eye,
                    dir,
                    p.pos - DVec3::new(shape.half_width, 0.0, shape.half_width),
                    p.pos + DVec3::new(shape.half_width, shape.height, shape.half_width),
                )
                .filter(|d| *d <= reach)
                .map(|d| (d, who))
            })
            .min_by(|a, b| a.0.total_cmp(&b.0))
            .map(|(_, who)| who)
    }

    /// Armored PvP damage with Java's knockback (`hurtServer`'s push away from
    /// `source` (the attacker's feet), then `extra` along Java yaw `yaw`) and fire;
    /// returns where the target stands, its armor's Thorns levels and whether
    /// the hit landed (Creative targets and hurt immunity refuse it) if it was found.
    fn hurt_player_ref(
        &mut self,
        target: &PlayerRef,
        amount: f32,
        (source, yaw, extra): (DVec3, f32, f32),
        fire: f32,
    ) -> Option<(DVec3, [u8; 4], bool)> {
        const CAUSE: &str = "was slain by a player";
        let hit = knockback::Hit::melee(source, self.player(target)?.player.pos, yaw, extra);
        let landed = match target {
            PlayerRef::Host => {
                let landed = self.damage_player_armored(amount, CAUSE) > 0.0;
                if landed {
                    let entities = &mut self.mobs.entities;
                    self.session.knocked_back(&hit, &mut || f64::from(entities.roll_range(0.0, 1.0)));
                }
                landed
            }
            PlayerRef::Agent(name) => {
                let bot = self.agents.players.get_mut(name)?;
                bot.agent.hurt_knocked(amount, CAUSE, &hit, &mut self.mobs.entities) > 0.0
            }
        };
        let s = self.player_mut(target)?;
        // Fire Aspect is a post-attack effect, so only a hit that lands sets fire.
        if landed && fire > 0.0 {
            s.vitals.ignite(fire);
        }
        Some((s.player.pos, crate::entity::Target::thorns_of(&s.inventory.armor), landed))
    }

    /// Java's player melee against another player: the attack-strength charge
    /// scales damage; strong hits allow criticals while falling, sprint
    /// knockback and sword sweeps. Enchantment bonuses, Knockback, Fire Aspect
    /// and Thorns apply. Returns whether a player was hit.
    pub(super) fn attack_lan_player(&mut self, attacker: &PlayerRef, slot: usize, sprint: bool) -> bool {
        let Some(target) = self.lan_player_target(attacker) else {
            return false;
        };
        let Some(me) = self.player_mut(attacker) else { return false };
        if me.cannot_attack_with(slot, 5) {
            return false;
        }
        let strength = me.attack_strength(slot, 0.5);
        me.reset_attack_strength();
        let strong = strength > 0.9;
        let held = me.inventory.get(slot);
        let enchants = held.map_or(Default::default(), |s| s.active_enchants());
        let p = &me.player;
        let critical = strong && me.can_critical_attack(true, sprint);
        let base = (crate::mining::attack_damage(held.map(|s| s.item)) + me.vitals.effects.attack_bonus()).max(0.0)
            * (0.2 + strength * strength * 0.8);
        let enchant_bonus = crate::enchant::damage_bonus(enchants, crate::enchant::Creature::Other);
        let damage = base * if critical { 1.5 } else { 1.0 } + enchant_bonus * strength;
        let (knock_level, fire) = crate::mining::weapon_extras(held);
        let sprint_knockback = sprint && strong;
        let extra = knockback::attack_strength(0.0, u32::from(knock_level), sprint_knockback);
        let sword = held.and_then(|s| s.item.as_tool()).is_some_and(|(k, _)| k == crate::item::ToolKind::Sword);
        let (speed_level, slowness_level) = me.vitals.effects.movement_speed_levels();
        let speed = knockback::movement_speed(speed_level, slowness_level, sprint);
        let moved_sqr = (p.vel.with_y(0.0) / knockback::TICK_RATE).length_squared();
        let sweep = knockback::sweep_allowed(strong, critical, sprint_knockback, p.on_ground, moved_sqr, speed, sword);
        let attacker_pos = p.pos;
        let yaw = knockback::java_yaw_degrees(p.yaw);
        let survival = me.mode.is_survival();
        let held_item = held.map(|s| s.item);
        let Some((at, thorns, landed)) = self.hurt_player_ref(&target, damage, (attacker_pos, yaw, extra), fire) else {
            return true;
        };
        // `Player.attack`: the extra knockback, sweep and post-attack effects
        // (Thorns) follow only a hit `hurtOrSimulate` accepted.
        let (extra, sweep, thorns) = if landed { (extra, sweep, thorns) } else { (0.0, false, [0; 4]) };
        if extra > 0.0 {
            // `causeExtraKnockback`: the push slows the attacker and stops its sprint.
            if let Some(me) = self.player_mut(attacker) {
                me.player.vel = knockback::slow_attacker(me.player.vel);
                me.player.sprinting = false;
            }
            if let PlayerRef::Agent(name) = attacker {
                self.stop_sprint(name);
            }
        }
        if sweep {
            let level = u32::from(enchants.level(crate::enchant::Enchantment::SweepingEdge));
            let swept = knockback::sweep_damage(base, level, enchant_bonus, strength);
            let target_box = self.player(&target).map(|s| s.player.collision_shape().aabb(s.player.pos));
            let near_host = (!self.session.vitals.is_dead()).then_some((PlayerRef::Host, &self.session));
            let near: Vec<PlayerRef> = near_host
                .into_iter()
                .chain(
                    self.agents
                        .players
                        .iter()
                        .filter(|(_, b)| b.active && !b.agent.session.vitals.is_dead())
                        .map(|(n, b)| (PlayerRef::Agent(n.clone()), &b.agent.session)),
                )
                .filter(|(who, s)| {
                    let p = &s.player;
                    *who != target
                        && who != attacker
                        && target_box.is_some_and(|t| knockback::in_sweep_box(t, p.collision_shape().aabb(p.pos)))
                        && p.pos.distance_squared(attacker_pos) < knockback::SWEEP_RANGE_SQR
                })
                .map(|(who, _)| who)
                .collect();
            for other in near {
                self.hurt_player_ref(&other, swept, (attacker_pos, yaw, knockback::SWEEP_STRENGTH as f32), 0.0);
            }
            let mut burst = crate::particles::Burst::new(crate::particles::Kind::Poof, at + DVec3::Y, 3);
            burst.spread = DVec3::splat(0.5);
            self.mobs.entities.particles.push(crate::particles::Request::Burst(burst));
        }
        if critical {
            let mut burst = crate::particles::Burst::new(crate::particles::Kind::Crit, at + DVec3::Y, 16);
            burst.spread = DVec3::splat(0.4);
            self.mobs.entities.particles.push(crate::particles::Request::Burst(burst));
        }
        self.audio.play(
            crate::audio::sounds::Sound::Hit,
            Some(at + DVec3::Y),
            0.8,
            if critical { (1.25, 1.4) } else { (0.9, 1.1) },
        );
        // Thorns: each piece 15% per level to hit back for 1-5.
        for level in thorns.into_iter().filter(|&l| l > 0) {
            if self.mobs.entities.roll_chance(0.15 * level as f32) {
                let back = self.mobs.entities.roll_range(1.0, 5.0);
                const THORNS: &str = "was killed trying to hurt a player";
                match attacker {
                    PlayerRef::Host => {
                        self.damage_player(back, THORNS);
                    }
                    PlayerRef::Agent(name) => {
                        if let Some(bot) = self.agents.players.get_mut(name) {
                            bot.agent.hurt(back, THORNS, DVec3::ZERO, &mut self.mobs.entities);
                        }
                    }
                }
            }
        }
        let broke = match (survival, held_item, self.player_mut(attacker)) {
            (true, Some(item), Some(me)) => {
                me.vitals.hunger.exhaust(super::survival::EXHAUST_ATTACK);
                me.inventory.wear(slot, crate::mining::wear(item, true)).then_some(item)
            }
            _ => None,
        };
        if let Some(item) = broke {
            match attacker {
                PlayerRef::Host => self.tool_broke(item),
                PlayerRef::Agent(_) => self.audio.play(
                    crate::audio::sounds::Sound::Break(crate::audio::sounds::Material::Wood),
                    Some(attacker_pos + DVec3::Y),
                    0.8,
                    (1.3, 1.5),
                ),
            }
        }
        true
    }
}

/// Preserve short mouse taps even when movement snapshots are coalesced.
fn button_transitions(mut previous: u8, edges: &[(u8, bool)], latest: u8) -> Vec<(u8, bool)> {
    let mut result = edges.to_vec();
    for &(bit, pressed) in edges {
        if pressed {
            previous |= bit;
        } else {
            previous &= !bit;
        }
    }
    for bit in [8, 16] {
        if previous & bit != latest & bit {
            result.push((bit, latest & bit != 0));
        }
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    /// A respawned peer faces the respawn angle while its client's queued
    /// inputs still carry the pre-death look, then follows the client again
    /// once its input reports the angle (or the hold times out).
    #[test]
    fn a_respawn_angle_holds_against_stale_peer_look_until_the_client_adopts_it() {
        let stale = Input { yaw: 3.0, pitch: 1.2, ..Default::default() };
        let mut held = Some((std::f32::consts::FRAC_PI_2, 0.0, Instant::now()));
        assert_eq!(peer_look(&mut held, &stale), (std::f32::consts::FRAC_PI_2, 0.0));
        assert!(held.is_some());
        let adopted = Input { yaw: std::f32::consts::FRAC_PI_2 + std::f32::consts::TAU, pitch: 0.0, ..stale };
        assert_eq!(peer_look(&mut held, &adopted).1, 0.0);
        assert!(held.is_none(), "the client reported the angle");
        let turned = Input { yaw: 1.0, pitch: 0.3, ..stale };
        assert_eq!(peer_look(&mut held, &turned), (1.0, 0.3), "the client owns its look again");
        let mut expired = Some((0.0, 0.0, Instant::now() - Duration::from_secs(3)));
        assert_eq!(peer_look(&mut expired, &stale), (3.0, 1.2));
        assert!(expired.is_none());
    }
    #[test]
    fn oversized_state_sheds_optional_parts_instead_of_disconnecting() {
        let big = "x".repeat(1000);
        let mobs: Vec<_> = (0..256).map(|i| json!({"uid": i, "look": {"name": big}})).collect();
        let v = json!({"self": {"health": 20}, "mobs": mobs, "items": big, "extras": {"pad": big}});
        let (fitted, shed) = fit_state(v);
        let fitted = fitted.expect("still sends the player's own state");
        assert!(shed, "looks were dropped");
        assert_eq!(fitted["self"]["health"], 20);
        assert!(fitted["extras"].is_null() && fitted["items"].is_null());
        assert!(serde_json::to_vec(&fitted).unwrap().len() < voxelcraft::lan::MAX_FRAME);
        let small = json!({"self": {}, "mobs": [{"uid": 1, "look": {}}]});
        assert_eq!(fit_state(small.clone()), (Some(small), false));
    }
    #[test]
    fn session_rules_are_remembered_for_the_next_time_the_world_opens_to_lan() {
        let mut lan = Lan::default();
        lan.cheats = true;
        let saved = lan.rules_text();
        assert_eq!(saved, "1,0");
        let mut next = Lan::default();
        next.restore_rules(Some(&saved));
        assert!(next.cheats && !next.pvp);
        lan.pvp = true;
        let mut again = Lan::default();
        again.restore_rules(Some(&lan.rules_text()));
        assert!(again.cheats && again.pvp);
        // A world saved before this was remembered, or with damaged text, keeps both off.
        for text in [None, Some(""), Some("x"), Some("0,0")] {
            let mut fresh = Lan::default();
            fresh.restore_rules(text);
            assert!(!fresh.cheats && !fresh.pvp, "{text:?}");
        }
        // Command line flags still switch a rule on.
        let mut flagged = Lan::default();
        flagged.pvp = true;
        flagged.restore_rules(Some("0,0"));
        assert!(flagged.pvp);
    }
    #[test]
    fn button_edges_preserve_taps_and_do_not_reopen_a_closed_container() {
        assert_eq!(button_transitions(0, &[(8, true), (8, false)], 0), vec![(8, true), (8, false)]);
        assert!(button_transitions(16, &[], 16).is_empty());
        assert_eq!(button_transitions(16, &[], 0), vec![(16, false)]);
        assert_eq!(button_transitions(0, &[(16, true)], 16), vec![(16, true)]);
    }
    #[test]
    fn other_players_get_a_lite_state_with_what_is_drawn() {
        let mut p = Agent::new(DVec3::new(1.0, 2.0, 3.0));
        p.selected = 2;
        p.inventory.slots[2] = Some(crate::inventory::Stack::new(crate::item::Item::STICK, 1));
        p.inventory.slots[5] = Some(crate::inventory::Stack::new(crate::item::Item::DIAMOND, 64));
        p.inventory.armor[0] = Some(crate::inventory::Stack::new(crate::item::Item::BONE, 1));
        p.player.sneaking = true;
        let lite = state_lite(&p, "Remote", Some(0.1), 0.5);
        assert!(lite.get("inventory").is_none(), "no inventory for others");
        let mut copy = Agent::new(DVec3::ZERO);
        apply(&mut copy, &lite);
        assert_eq!(copy.inventory.get(2).unwrap().item, crate::item::Item::STICK);
        assert!(copy.inventory.get(5).is_none());
        assert_eq!(copy.inventory.armor[0].unwrap().item, crate::item::Item::BONE);
        assert!(copy.player.sneaking);
        assert_eq!(copy.player.pos, p.player.pos);
        assert_eq!(lite["swing"], 0.1);
        assert_eq!(lite["eat"], 0.5);
    }
    #[test]
    fn player_snapshot_preserves_progression_inventory_and_cursor() {
        let mut original = Agent::new(DVec3::new(12.0, 151.0, -8.0));
        original.id = voxelcraft::entity::PlayerId(7);
        original.set_mode(GameMode::Creative);
        original.player.vel = DVec3::new(1.0, -2.0, 3.0);
        original.player.yaw = 1.25;
        original.player.pitch = -0.4;
        original.inventory.add(crate::item::Item::DIAMOND, 11);
        original.inventory.cursor = Some(crate::inventory::Stack::new(crate::item::Item::STICK, 5));
        original.vitals.health = 13.0;
        original.vitals.hunger.food = 7.0;
        original.vitals.xp.add_levels(9);
        original.sleeping = Some(0.75);
        original.selected = 3;
        // Java's latched sprint and carried movement state reach the client
        // so its replayed inputs move as the host's did.
        original.player.sprinting = true;
        original.player.restore_carried_motion([6, 3, 1, 1]);
        original.player.restore_carried_stuck(DVec3::new(0.25, 0.05, 0.25));
        let frame = Packet::State(state(&original, "Builder")).encode().unwrap();
        let Packet::State(snapshot) = Packet::decode(&frame[4..]).unwrap() else { panic!("wrong packet") };
        let mut restored = Agent::new(DVec3::ZERO);
        apply(&mut restored, &snapshot);
        assert_eq!(restored.player.pos, original.player.pos);
        assert_eq!(restored.player.vel, original.player.vel);
        assert!(restored.player.sprinting);
        assert_eq!(restored.player.carried_motion(), [6, 3, 1, 1]);
        assert_eq!(restored.player.carried_stuck(), DVec3::new(0.25, 0.05, 0.25));
        assert_eq!(restored.mode, original.mode);
        assert_eq!(restored.selected, 3);
        assert_eq!(restored.inventory.serialize(), original.inventory.serialize());
        assert_eq!(restored.inventory.cursor, original.inventory.cursor);
        assert_eq!(restored.vitals.health, 13.0);
        assert_eq!(restored.vitals.hunger.food, 7.0);
        assert_eq!(restored.vitals.xp.level, 9);
        assert_eq!(restored.sleeping, Some(0.75));
    }
    #[test]
    fn slots_are_bounded_and_validated_against_the_authoritative_screen() {
        let inventory = Container::Inventory;
        assert!(decode_slot(35, inventory, GameMode::Survival, 2).is_some());
        assert!(decode_slot(36, inventory, GameMode::Survival, 2).is_none());
        assert!(decode_slot(104, inventory, GameMode::Survival, 2).is_none());
        assert!(decode_slot(104, Container::CraftingTable, GameMode::Survival, 3).is_some());
        assert!(decode_slot(200, inventory, GameMode::Survival, 2).is_none());
        assert!(decode_slot(200, Container::Chest(IVec3::ZERO), GameMode::Survival, 2).is_some());
        assert!(decode_slot(300, inventory, GameMode::Survival, 2).is_none());
        assert!(decode_slot(1000, inventory, GameMode::Survival, 2).is_none());
        assert!(decode_slot(u16::MAX, inventory, GameMode::Creative, 2).is_none());
        let item = crate::item::Item::from(Block::DIRT);
        let slot = encode_slot(SlotRef::Palette(item)).unwrap();
        assert!(
            matches!(decode_slot(slot,inventory,GameMode::Creative,2),Some(SlotRef::Palette(decoded)) if decoded==item)
        );
    }
}
