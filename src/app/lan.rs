//! Desktop host/client adapter. LAN hands use the same puppet actions as pads.
use super::{Container, Game, GameMode, gamepad::Body, hud::SlotRef};
use crate::crafting::Grid;
use crate::inventory::{stack_from_str, stack_to_string};
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
        }
    }
}
#[derive(Default)]
pub(super) struct Lan {
    pub host: Option<Listener>,
    peers: Vec<Peer>,
    pub client: Option<Connection>,
    pub lost: Option<String>,
    pub profile: String,
    pub offline_render_distance: Option<i32>,
    pub mode: GameMode,
    pub cheats: bool,
    pub pvp: bool,
    host_mode: GameMode,
    id: u32,
    sequence: u32,
    predictions: VecDeque<Input>,
    ready: bool,
    messages: VecDeque<String>,
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
fn state(agent: &Agent, name: &str) -> Value {
    json!({"id":agent.id.0,"name":name,"pos":agent.player.pos.to_array(),"vel":agent.player.vel.to_array(),
        "yaw":agent.player.yaw,"pitch":agent.player.pitch,"ground":agent.player.on_ground,"flying":agent.player.flying,
        "mode":agent.mode.name(),"selected":agent.selected,"inventory":agent.inventory.serialize(),
        "cursor":stack_to_string(agent.inventory.cursor),"health":agent.vitals.health,"air":agent.vitals.air,
        "food":agent.vitals.hunger.food,"saturation":agent.vitals.hunger.saturation,"exhaustion":agent.vitals.hunger.exhaustion,
        "death":agent.vitals.death,"xp":agent.vitals.xp.serialize(),"effects":agent.vitals.effects.serialize(),"sleep":agent.sleeping})
}
fn apply(agent: &mut Agent, v: &Value) {
    if let Some(pos) = vector(&v["pos"]) {
        agent.previous_pos = agent.player.pos;
        agent.player.pos = pos;
    }
    if let Some(vel) = vector(&v["vel"]) {
        agent.player.vel = vel;
    }
    if let Some(mode) = v["mode"].as_str().and_then(GameMode::from_name) {
        agent.set_mode(mode);
    }
    agent.player.yaw = v["yaw"].as_f64().unwrap_or(0.0) as f32;
    agent.player.pitch = v["pitch"].as_f64().unwrap_or(0.0) as f32;
    agent.player.on_ground = v["ground"] == true;
    agent.player.flying = v["flying"] == true;
    agent.selected = v["selected"].as_u64().unwrap_or(0).min(8) as usize;
    if let Some(inv) = v["inventory"].as_str().and_then(crate::inventory::Inventory::deserialize) {
        agent.inventory = inv;
    }
    agent.inventory.cursor = v["cursor"].as_str().and_then(stack_from_str).flatten();
    agent.vitals = crate::simulation::survival::Vitals::restore(
        v["health"].as_f64().unwrap_or(20.0) as f32,
        v["air"].as_f64().unwrap_or(15.0) as f32,
        v["death"].as_str().map(str::to_owned),
    );
    agent.vitals.hunger = crate::simulation::survival::Hunger::restore(
        v["food"].as_f64().unwrap_or(20.0) as f32,
        v["saturation"].as_f64().unwrap_or(5.0) as f32,
        v["exhaustion"].as_f64().unwrap_or(0.0) as f32,
    );
    if let Some(xp) = v["xp"].as_str().and_then(crate::simulation::experience::Experience::parse) {
        agent.vitals.xp = xp;
    }
    if let Some(effects) = v["effects"].as_str() {
        agent.vitals.effects = crate::simulation::effects::Effects::deserialize(effects);
    }
    agent.sleeping = v["sleep"].as_f64().map(|f| f as f32);
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
                bot.agent.hold(Default::default(), false, false);
                bot.agent.sleeping = None;
                bot.agent.inventory.return_stacks(peer.grid.take_all());
                let cursor = bot.agent.inventory.cursor.take();
                bot.agent.inventory.return_stacks(cursor);
                bot.agent
                    .inventory
                    .return_stacks(bot.agent.work.iter_mut().filter_map(Option::take).collect::<Vec<_>>());
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
            host.announce(&self.world_name);
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
                            let mut a = Agent::new(self.player.pos + DVec3::new(2.0, 0.0, 0.0));
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
    fn with_remote<R>(&mut self, peer: &mut Peer, f: impl FnOnce(&mut Self) -> R) -> Option<R> {
        let name = peer.profile.as_deref()?;
        self.lan.host_mode = self.mode;
        let mut grid = peer.grid;
        let mut open = peer.open;
        let result = self.puppet_body(name, &mut peer.body, |g| {
            std::mem::swap(&mut g.craft, &mut grid);
            std::mem::swap(&mut g.inventory_open, &mut open);
            let result = f(g);
            std::mem::swap(&mut g.craft, &mut grid);
            std::mem::swap(&mut g.inventory_open, &mut open);
            result
        });
        peer.grid = grid;
        peer.open = open;
        result
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
            "inventory" => {
                self.with_remote(peer, |g| {
                    if !g.inventory_open {
                        g.toggle_inventory();
                    }
                });
            }
            "close" => {
                self.with_remote(peer, |g| {
                    if g.inventory_open {
                        g.toggle_inventory();
                    }
                });
            }
            "pick" => {
                self.with_remote(peer, |g| g.pick_block());
            }
            "drop" | "drop_stack" => {
                self.with_remote(peer, |g| g.drop_selected(command == "drop_stack"));
            }
            "respawn" => {
                let name = peer.profile.as_deref().unwrap().to_owned();
                if self.agents.players[&name].agent.vitals.is_dead() {
                    let kept_xp =
                        self.gamerules.bool("keepInventory").then_some(self.agents.players[&name].agent.vitals.xp);
                    self.agents.players.get_mut(&name).unwrap().agent.execute(
                        Command::Respawn,
                        &mut self.world,
                        &mut self.mobs.entities,
                        &[],
                    )?;
                    let at = if self.dimension == Dimension::Overworld {
                        self.with_remote(peer, |g| g.respawn_point()).unwrap_or(self.player.pos)
                    } else {
                        self.beside_host()
                    };
                    let agent = &mut self.agents.players.get_mut(&name).unwrap().agent;
                    agent.player.pos = at;
                    agent.previous_pos = at;
                    if let Some(xp) = kept_xp {
                        agent.vitals.xp = xp;
                    }
                    peer.body = Body::default();
                    peer.open = false;
                }
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
                    &[self.player.pos],
                )?;
            }
        }
        Ok(())
    }
    fn remote_click(&mut self, peer: &mut Peer, encoded: u16, right: bool, shift: bool) {
        if !peer.open {
            return;
        }
        if let Some(p) = peer.used_at {
            let Some(bot) = self.agents.players.get(peer.profile.as_deref().unwrap()) else { return };
            if bot.agent.player.eye().distance(p.as_dvec3() + 0.5) > super::REACH + 1.0 {
                return;
            }
        }
        self.with_remote(peer, |g| {
            let reachable = match g.container {
                Container::Chest(p) => {
                    g.world.get_block(p).is_some_and(crate::world::chest::is_chest)
                        && g.player.eye().distance(p.as_dvec3() + 0.5) <= super::REACH + 1.0
                }
                Container::Furnace(p) => {
                    g.world.get_block(p).is_some_and(crate::world::furnace::is_furnace)
                        && g.player.eye().distance(p.as_dvec3() + 0.5) <= super::REACH + 1.0
                }
                _ => true,
            };
            if !reachable || g.vitals.is_dead() {
                return;
            }
            if let Some(slot) = decode_slot(encoded, g.container, g.mode, g.craft.size) {
                if shift && g.inventory.cursor.is_none() {
                    g.quick_move(slot);
                } else {
                    g.click_slot(slot, right);
                }
            }
        });
    }
    pub(super) fn drive_lan(&mut self, dt: f64) {
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
            bot.agent.player.yaw = input.yaw.rem_euclid(std::f32::consts::TAU);
            bot.agent.player.pitch = input.pitch.clamp(-1.55, 1.55);
            bot.agent.selected = input.selected as usize;
            bot.agent.player.flying = bot.agent.player.can_fly && input.buttons & 32 != 0;
            if bot.agent.sleeping.is_some() && input.buttons & 1 != 0 {
                bot.agent.sleeping = None;
            }
            if bot.agent.vitals.is_dead() || peer.open || bot.agent.sleeping.is_some() {
                bot.agent.hold(Default::default(), false, false);
            } else {
                bot.agent.hold(input.movement(), false, false);
            }
            if peer.open
                && peer.grid.size == 3
                && peer.used_at.is_some_and(|p| {
                    self.world.get_block(p) != Some(Block::CRAFTING_TABLE)
                        || self.agents.players[&name].agent.player.eye().distance(p.as_dvec3() + 0.5)
                            > super::REACH + 1.0
                })
            {
                self.with_remote(peer, |g| g.toggle_inventory());
            }
            if self.agents.players[&name].agent.vitals.is_dead() {
                let stacks = peer.grid.take_all();
                let bot = self.agents.players.get_mut(&name).unwrap();
                if self.gamerules.bool("keepInventory") {
                    bot.agent.inventory.return_stacks(stacks);
                } else {
                    for stack in
                        stacks.into_iter().filter(|s| !s.enchants.has(crate::enchant::Enchantment::VanishingCurse))
                    {
                        self.mobs.entities.throw(stack, bot.agent.player.eye(), DVec3::ZERO);
                    }
                }
            }
            // Close destroyed or out-of-reach containers before accepting any more clicks.
            self.with_remote(peer, |g| {
                let pos = match g.container {
                    Container::Chest(p) | Container::Furnace(p) => Some(p),
                    _ => None,
                };
                if g.inventory_open
                    && (g.vitals.is_dead()
                        || pos.is_some_and(|p| {
                            g.player.eye().distance(p.as_dvec3() + 0.5) > super::REACH + 1.0
                                || g.world.get_block(p).is_none_or(|b| {
                                    !crate::world::chest::is_chest(b) && !crate::world::furnace::is_furnace(b)
                                })
                        }))
                {
                    g.toggle_inventory();
                }
            });
            if !peer.open
                && !self.agents.players[&name].agent.vitals.is_dead()
                && self.agents.players[&name].agent.sleeping.is_none()
            {
                let transitions = button_transitions(peer.previous_buttons, &peer.edges, input.buttons);
                let used = self
                    .with_remote(peer, |g| {
                        for (bit, pressed) in transitions {
                            if bit == 8 {
                                g.attack_button(pressed);
                            } else {
                                g.use_button(pressed);
                            }
                        }
                        g.act(true, dt);
                        g.mobs.attack_cooldown -= dt;
                        g.puppet_used.take()
                    })
                    .flatten();
                if let Some(p) = used {
                    peer.used_at = Some(p);
                    match self.world.get_block(p) {
                        Some(b) if b.is_bed() => {
                            let _ = self.agent_sleep(&name);
                        }
                        Some(Block::CRAFTING_TABLE) => {
                            self.with_remote(peer, |g| g.open_crafting_table());
                        }
                        Some(b) if crate::world::chest::is_chest(b) => {
                            self.with_remote(peer, |g| g.open_chest(p));
                        }
                        Some(b) if crate::world::furnace::is_furnace(b) => {
                            self.with_remote(peer, |g| g.open_furnace(p));
                        }
                        _ => {
                            let _ = peer
                                .connection
                                .send(Packet::Chat("This workstation is not supported by LAN yet".into()));
                        }
                    }
                }
            }
            if let Some(bot) = self.agents.players.get(&name) {
                let p = bot.agent.player.pos.floor().as_ivec3();
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
                || self.agents.players[&name].agent.vitals.is_dead()
                || self.agents.players[&name].agent.sleeping.is_some()
            {
                self.with_remote(peer, |g| {
                    g.attack_button(false);
                    g.use_button(false);
                });
                peer.previous_buttons = input.buttons;
            } else {
                peer.previous_buttons = input.buttons;
            }
            peer.edges.clear();
            peer.applied = input.sequence;
            self.puppet_used = None;
            self.puppet_merchant = None;
            if let Some(message) = self.puppet_popup.take() {
                let _ = peer.connection.send(Packet::Chat(message));
            }
        }
        self.lan.peers = peers;
    }
    pub(super) fn send_lan_state(&mut self) {
        if self.lan.peers.is_empty() {
            return;
        }
        let mut host = Agent::new(self.player.pos);
        host.id = voxelcraft::entity::PlayerId::HOST;
        host.player.yaw = self.player.yaw;
        host.player.pitch = self.player.pitch;
        host.set_mode(self.mode);
        host.inventory = self.inventory.clone();
        host.vitals = self.vitals.clone();
        host.sleeping = self.sleeping;
        let players: Vec<_> = std::iter::once(state(&host, &self.lan.profile))
            .chain(self.agents.players.iter().filter(|(_, b)| b.active).map(|(n, b)| state(&b.agent, n)))
            .collect();
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
            let center = chunk_of(bot.agent.player.pos.floor().as_ivec3());
            // Two rings beyond the client's view, so every meshed chunk has all eight
            // horizontal neighbours (diagonals of edge chunks reach about r + 1.42).
            let radius = view + 2;
            let chunks = self.world.network_chunks(center, radius);
            peer.sent.retain(|p, _| {
                let d = (IVec3::new(p.0, p.1, p.2) - center).with_y(0);
                d.length_squared() <= radius * radius
            });
            for (pos, data) in chunks
                .into_iter()
                .filter(|(p, d)| peer.sent.get(&(p.x, p.y, p.z)).is_none_or(|old| !Arc::ptr_eq(old, d)))
                .take(32)
                .collect::<Vec<_>>()
            {
                if peer.connection.queued_bytes() > 512 * 1024 {
                    break;
                }
                if peer.connection.send(Packet::Chunk { pos, data: data.clone() }).is_ok() {
                    peer.sent.insert((pos.x, pos.y, pos.z), data);
                }
            }
            let container = match peer.body.container() {
                Container::Chest(p) => {
                    json!({"kind":"chest","pos":p.to_array(),"state":self.world.chest(p).map(|c|c.serialize())})
                }
                Container::Furnace(p) => {
                    json!({"kind":"furnace","pos":p.to_array(),"state":self.world.furnace(p).map(|c|c.serialize())})
                }
                Container::CraftingTable => json!({"kind":"crafting"}),
                _ => json!({"kind":"inventory"}),
            };
            let mobs: Vec<_> = self.mobs.entities.mobs.iter().filter(|m|m.pos.distance_squared(bot.agent.player.pos)<160.0*160.0).take(256).map(|m|json!({"uid":m.uid,"kind":m.kind.name(),"pos":m.pos.to_array(),"yaw":m.yaw,"head_yaw":m.head_yaw,"head_pitch":m.head_pitch,"health":m.health,"hurt":m.hurt,"dying":m.dying,"size":m.size,"baby":m.baby})).collect();
            let items = self
                .mobs
                .entities
                .items
                .iter()
                .filter(|m| m.pos.distance_squared(bot.agent.player.pos) < 160.0 * 160.0)
                .take(256)
                .map(|m| m.serialize())
                .collect::<Vec<_>>()
                .join(";");
            let arrows:Vec<_>=self.mobs.entities.arrows.iter().filter(|a|a.pos.distance_squared(bot.agent.player.pos)<160.0*160.0).take(256).map(|a|json!({"pos":a.pos.to_array(),"previous":a.previous_pos.to_array(),"dir":a.dir.to_array(),"player":a.from_player,"critical":a.critical})).collect();
            let snapshot = json!({"sequence":peer.applied,"self":state(&bot.agent,name),"players":players,"time":self.day_time,"rain":self.weather.raining,"rain_strength":self.weather.strength,"thunder":self.weather.thundering,"dimension":self.dimension.name(),"seed":self.world.generator.seed,"view":view,"open":peer.open,"container":container,"grid":peer.grid.cells.iter().map(|s|stack_to_string(*s)).collect::<Vec<_>>(),"grid_size":peer.grid.size,"mobs":mobs,"items":items,"arrows":arrows});
            if let Err(e) = peer.connection.send(Packet::State(snapshot)).and_then(|()| peer.connection.flush()) {
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
            yaw: self.player.yaw,
            pitch: self.player.pitch,
            forward: movement.forward as f32,
            right: movement.right as f32,
            buttons: u8::from(movement.jump)
                | (u8::from(movement.descend) * 2)
                | (u8::from(movement.sprint) * 4)
                | (u8::from(active && self.left_held) * 8)
                | (u8::from(active && self.right_held) * 16)
                | (u8::from(self.player.flying) * 32),
            selected: self.actions.selected as u8,
        }
    }
    pub(super) fn lan_button_edge(&mut self, attack: bool, pressed: bool) {
        let input = self.sample_lan_input();
        self.lan_send(Packet::Input(input));
        self.lan_command(match (attack, pressed) {
            (true, true) => "attack_down",
            (true, false) => "attack_up",
            (false, true) => "use_down",
            (false, false) => "use_up",
        });
    }
    pub(super) fn tick_lan_client(&mut self) {
        let input = self.sample_lan_input();
        let movement = input.movement();
        self.lan_send(Packet::Input(input));
        self.previous_eye = self.player.eye();
        if self.lan.ready {
            self.player.apply_effects(&self.vitals.effects);
            self.player.wear_boots(crate::enchant::armor_level(
                &self.inventory.armor,
                crate::enchant::Enchantment::DepthStrider,
            ));
            self.player.update(crate::simulation::TICK_SECONDS, movement, &self.world);
            self.lan.predictions.push_back(input);
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
        let was_dead = self.vitals.is_dead();
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
        let mut own = Agent::new(self.player.pos);
        apply(&mut own, &snapshot["self"]);
        let (yaw, pitch) = (self.player.yaw, self.player.pitch);
        self.previous_eye = self.player.eye();
        self.player = own.player;
        self.player.yaw = yaw;
        self.player.pitch = pitch;
        self.mode = own.mode;
        self.inventory = own.inventory;
        self.vitals = own.vitals;
        self.sleeping = own.sleeping;
        if !self.lan.ready {
            self.actions.selected = own.selected;
            self.previous_eye = self.player.eye();
        }
        for input in &self.lan.predictions {
            self.player.yaw = input.yaw;
            self.player.pitch = input.pitch;
            self.player.apply_effects(&self.vitals.effects);
            self.player.update(crate::simulation::TICK_SECONDS, input.movement(), &self.world);
        }
        self.player.yaw = yaw;
        self.player.pitch = pitch;
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
            self.left_held = false;
            self.right_held = false;
            self.keys.clear();
        }
        let c = &snapshot["container"];
        let pos = vector(&c["pos"]).map(|p| p.as_ivec3());
        self.container = match (c["kind"].as_str(), pos) {
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
            (Some("crafting"), _) => Container::CraftingTable,
            _ => Container::Inventory,
        };
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
                    self.agents.insert(name.into(), Agent::new(vector(&p["pos"]).unwrap_or(self.player.pos)));
                }
                let bot = self.agents.players.get_mut(name).unwrap();
                bot.active = true;
                apply(&mut bot.agent, p);
                bot.id = voxelcraft::entity::PlayerId(p["id"].as_u64().unwrap_or(0) as u32);
            }
        }
        if let Some(mobs) = snapshot["mobs"].as_array() {
            let mut previous: BTreeMap<_, _> =
                std::mem::take(&mut self.mobs.entities.mobs).into_iter().map(|m| (m.uid, m)).collect();
            for m in mobs.iter().take(256) {
                let (Some(kind), Some(pos)) =
                    (m["kind"].as_str().and_then(crate::entity::MobKind::from_name), vector(&m["pos"]))
                else {
                    continue;
                };
                let uid = m["uid"].as_u64().unwrap_or(0) as u32;
                let mut mob = previous.remove(&uid).unwrap_or_else(|| crate::entity::Mob::new(kind, pos, 0.0));
                mob.previous_pos = mob.pos;
                mob.pos = pos;
                mob.uid = uid;
                mob.yaw = m["yaw"].as_f64().unwrap_or(0.0) as f32;
                mob.head_yaw = m["head_yaw"].as_f64().unwrap_or(0.0) as f32;
                mob.head_pitch = m["head_pitch"].as_f64().unwrap_or(0.0) as f32;
                mob.health = m["health"].as_f64().unwrap_or(20.0) as f32;
                mob.hurt = m["hurt"].as_f64().unwrap_or(0.0) as f32;
                mob.dying = m["dying"].as_f64().map(|f| f as f32);
                mob.size = m["size"].as_u64().unwrap_or(1) as u8;
                mob.baby = m["baby"] == true;
                self.mobs.entities.mobs.push(mob);
            }
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
        if self.vitals.is_dead() {
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
        SlotRef::Palette(item) => 1000 + crate::item::Item::creative_palette().position(|i| i == item)? as u16,
        _ => return None,
    })
}
fn decode_slot(slot: u16, container: Container, mode: GameMode, grid_size: usize) -> Option<SlotRef> {
    match slot {
        0..=35 => Some(SlotRef::Inventory(slot as usize)),
        100..=108 if (slot - 100) < (grid_size * grid_size) as u16 => Some(SlotRef::Craft((slot - 100) as usize)),
        109 => Some(SlotRef::CraftResult),
        200..=253 if matches!(container, Container::Chest(_)) => Some(SlotRef::Chest((slot - 200) as usize)),
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

impl Game {
    pub(super) fn lan_player_target(&self) -> Option<String> {
        if self.lan.host.is_none() || !self.lan.pvp {
            return None;
        }
        let eye = self.player.eye();
        let dir = self.player.forward().as_dvec3();
        let mut reach = self
            .target()
            .and_then(|(p, _)| crate::physics::ray_aabb(eye, dir, p.as_dvec3(), p.as_dvec3() + DVec3::ONE))
            .unwrap_or(super::REACH)
            .min(super::REACH);
        for mob in &self.mobs.entities.mobs {
            let (min, max) = mob.shape().aabb(mob.pos);
            if let Some(distance) = crate::physics::ray_aabb(eye, dir, min, max) {
                reach = reach.min(distance);
            }
        }
        self.agents
            .players
            .iter()
            .filter(|(_, b)| b.active && !b.agent.vitals.is_dead())
            .filter_map(|(name, b)| {
                // While puppeting, this profile contains the original host's body.
                if b.id == self.actor && !self.puppet {
                    return None;
                }
                let p = &b.agent.player;
                let shape = p.collision_shape();
                crate::physics::ray_aabb(
                    eye,
                    dir,
                    p.pos - DVec3::new(shape.half_width, 0.0, shape.half_width),
                    p.pos + DVec3::new(shape.half_width, shape.height, shape.half_width),
                )
                .filter(|d| *d <= reach)
                .map(|d| (d, name))
            })
            .min_by(|a, b| a.0.total_cmp(&b.0))
            .map(|(_, n)| n.clone())
    }
    pub(super) fn attack_lan_player(&mut self) -> bool {
        let Some(name) = self.lan_player_target() else {
            return false;
        };
        self.mobs.attack_held = true;
        if self.mobs.attack_cooldown > 0.0 {
            return true;
        }
        self.mobs.attack_cooldown = crate::mining::attack_cooldown(self.held_item());
        let held = self.inventory.get(self.actions.selected);
        let enchants = held.map_or(Default::default(), |s| s.active_enchants());
        let damage = crate::mining::attack_damage(self.held_item())
            + self.vitals.effects.attack_bonus()
            + crate::enchant::damage_bonus(enchants, crate::enchant::Creature::Other);
        let dir = self.player.forward().as_dvec3().with_y(0.0).normalize_or_zero();
        let bot = self.agents.players.get_mut(&name).unwrap();
        let (mode, creative) = (bot.agent.mode, bot.agent.creative);
        if self.puppet && bot.id == self.actor {
            bot.agent.mode = self.lan.host_mode;
            bot.agent.creative = self.lan.host_mode.is_creative();
        }
        bot.agent.hurt(damage, "was slain by a player", dir * 0.4 + DVec3::Y * 0.2, &mut self.mobs.entities);
        bot.agent.mode = mode;
        bot.agent.creative = creative;
        self.wear_held(true);
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
    #[test]
    fn button_edges_preserve_taps_and_do_not_reopen_a_closed_container() {
        assert_eq!(button_transitions(0, &[(8, true), (8, false)], 0), vec![(8, true), (8, false)]);
        assert!(button_transitions(16, &[], 16).is_empty());
        assert_eq!(button_transitions(16, &[], 0), vec![(16, false)]);
        assert_eq!(button_transitions(0, &[(16, true)], 16), vec![(16, true)]);
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
        let frame = Packet::State(state(&original, "Builder")).encode().unwrap();
        let Packet::State(snapshot) = Packet::decode(&frame[4..]).unwrap() else { panic!("wrong packet") };
        let mut restored = Agent::new(DVec3::ZERO);
        apply(&mut restored, &snapshot);
        assert_eq!(restored.player.pos, original.player.pos);
        assert_eq!(restored.player.vel, original.player.vel);
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
