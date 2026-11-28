//! Desktop host/client adapter. LAN hands run on each peer's own session like
//! controller hands (`hands::Actor`); screens still borrow host container code.
use super::{Container, Game, GameMode, gamepad::Body, hud::SlotRef};
use crate::crafting::Grid;
use crate::inventory::{Stack, components::Payload, stack_from_str, stack_to_string};
use crate::knockback;
use crate::world::{
    World,
    block::Block,
    chunk::{ChunkData, chunk_of},
    terrain::{Dimension, Generation, Generator},
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

mod inventory;

/// What a LAN client's use button was doing when it sampled an input: the
/// part of a movement prediction that must replay as it was.
#[derive(Clone, Copy, Debug, Default)]
struct LanUse {
    eating: bool,
    drawing: bool,
    /// v1.0 gap track 6: a bundle being emptied by holding use.
    bundle: bool,
}

/// Largest view distance (in 32-block chunks) a LAN host streams to clients.
pub(super) const LAN_VIEW_MAX: i32 = 8;
pub(super) const LAN_OUTSIDE_SLOT: u16 = u16::MAX;
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
    /// The sign and wood entity version, and where the player stood, when
    /// this client last got its signs, shelves and chiseled bookshelves.
    wood_sent: Option<(u64, DVec3)>,
    /// A sign side this client should open its editor for, numbered so the
    /// client opens each request once.
    sign_edit: Option<(IVec3, bool, u32)>,
    /// That editor is open: only it may write the sign (Java's
    /// `playerWhoMayEdit`), once.
    sign_editing: bool,
    /// v0.15 Track B: a lectern or held book this client should open its
    /// reader or editor for, numbered like `sign_edit`.
    book_open: Option<(Value, u32)>,
    /// v0.18 Track 5: the recipe book revision this client last got.
    recipe_book_sent: Option<u32>,
    inventory: inventory::Sender,
}
impl Peer {
    fn can_edit_book(&self, agent: &Agent, offhand: bool) -> bool {
        let slot = if offhand { voxelcraft::session::books::OFFHAND_SLOT } else { agent.selected };
        self.inventory.held_matches(slot, &agent.session.inventory)
    }

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
            wood_sent: None,
            sign_edit: None,
            sign_editing: false,
            book_open: None,
            recipe_book_sent: None,
            inventory: inventory::Sender::default(),
        }
    }
}
#[derive(Default)]
pub(super) struct Lan {
    /// Host ticks with peers, to refresh mob looks once a second.
    look_ticks: u32,
    /// Particle effects since the last state, for clients to replay (JSON requests).
    pub effects: Vec<(DVec3, Value)>,
    /// Raid horns for other players since the last state, each positioned
    /// for its player: Java's `Raid.playSound` sends every player its own
    /// (issue #98).
    pub horns: Vec<(voxelcraft::entity::PlayerId, DVec3)>,
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
    /// Inputs the host hasn't acknowledged, each with what the use button
    /// was doing when sampled (a replay must not re-predict item use from a
    /// draw that has since ended).
    predictions: VecDeque<(Input, LanUse)>,
    /// The hotbar slot whose bow or crossbow this client predicts is being
    /// drawn: started when use could start a draw, kept while use stays held
    /// on it (the host's `tick_draw` doesn't recheck what is targeted).
    predicted_draw: Option<usize>,
    /// Ongoing consumption predicted for the selected slot, reconciled with
    /// host snapshots even after every queued input has been acknowledged.
    predicted_eating: Option<usize>,
    /// v1.0 gap track 6: a bundle in use predicted for the selected slot,
    /// and the use ticks it has run (`BundleItem.getUseDuration` ends it).
    predicted_bundle: Option<(usize, u32)>,
    /// Whether the use button was held on the last client tick.
    use_was_held: bool,
    /// Use was pressed since the last client tick (it may also have been
    /// released and pressed again in between).
    use_pressed: bool,
    /// The input sequence of the latest use press or release: a snapshot
    /// acknowledging it settles predicted item use.
    use_edge_sequence: u32,
    ready: bool,
    messages: VecDeque<String>,
    /// The host's respawn count for this client's player, from the last snapshot.
    respawns: Option<u64>,
    /// The last sign editor request this client opened.
    sign_serial: u32,
    /// The last lectern or book request this client opened.
    book_serial: u32,
    inventory: inventory::Receiver,
    inventory_revision: u32,
}
impl Lan {
    /// What the use button was doing when `input` was sampled: eating or
    /// drinking (as `Hands::eat`), or drawing. Statue use blocks a new bite,
    /// while a bite already in progress continues until the host ends it.
    fn sample_use(
        &mut self,
        session: &voxelcraft::session::PlayerSession,
        world: &World,
        input: &Input,
        pressed: bool,
    ) -> LanUse {
        let slot = usize::from(input.selected).min(8);
        let drink = session.inventory.get(slot).is_some_and(|s| s.item.is_drink());
        let targeted = session.block_target(world).and_then(|(pos, _)| world.get_block(pos));
        let composting =
            input.buttons & 2 == 0 && targeted.is_some_and(|b| voxelcraft::world::composter::level(b).is_some());
        let continuing = !pressed && self.predicted_eating == Some(slot);
        let statue_use = !continuing
            && session.player.vehicle.is_none()
            && input.buttons & 2 == 0
            && targeted.and_then(voxelcraft::world::copper_golem_statue::decode).is_some();
        let eating = input.buttons & 16 != 0
            && session.consumption_duration(slot).is_ok()
            && !statue_use
            && (drink || !composting);
        self.predicted_eating = eating.then_some(slot);
        // v1.0 gap track 6: `BundleItem.use` in the main hand, or in the
        // off hand behind an empty one; it runs while use stays held.
        use voxelcraft::content::bundle::is_bundle;
        let main = session.inventory.get(slot);
        let holds_bundle = main.is_some_and(|s| is_bundle(s.item))
            || main.is_none() && session.inventory.offhand.is_some_and(|s| is_bundle(s.item));
        let sneaking = input.buttons & 2 != 0;
        self.predicted_bundle = match self.predicted_bundle {
            _ if input.buttons & 16 == 0 || !holds_bundle => None,
            Some((s, ticks)) if !pressed && s == slot => {
                (ticks + 1 < voxelcraft::session::bundles::USE_TICKS).then_some((slot, ticks + 1))
            }
            _ if pressed && !session.block_takes_use(slot, world, sneaking) => Some((slot, 0)),
            _ => None,
        };
        let bundle = self.predicted_bundle.is_some();
        LanUse { eating, drawing: self.predicted_draw == Some(slot), bundle }
    }

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
        // v1.0 gap Track 2: doors mobs are breaking (`destroyBlockProgress`).
        out.extend(self.mobs.entities.door_cracks());
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
/// extra entities, decorations, dropped items, mob looks and finally all mobs, rather
/// than disconnecting.
/// Returns the snapshot (if anything fits) and whether mob looks were shed.
/// Most bytes of sign, shelf and bookshelf contents in one snapshot, a
/// quarter of the frame; the nearest entries go first.
const WOOD_BYTES: usize = voxelcraft::lan::MAX_FRAME / 4;

fn fit_state(mut v: Value) -> (Option<Value>, bool) {
    let fits = |v: &Value| serde_json::to_vec(v).is_ok_and(|b| b.len() < voxelcraft::lan::MAX_FRAME - 16);
    let mut shed_looks = false;
    for shed in
        ["recipe_book", "extras", "workstations", "copper_golem_statues", "wood", "decor", "items", "looks", "mobs"]
    {
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
        "sprint":agent.session.player.sprinting,"motion":agent.session.player.carried_motion(),"stuck":agent.session.player.carried_stuck().to_array(),"mode":agent.session.mode.name(),"selected":agent.selected,
        "time_since_rest":agent.session.vitals.time_since_rest,"ticks_frozen":agent.session.vitals.ticks_frozen,"health":agent.session.vitals.health,"air":agent.session.vitals.air,
        "food":agent.session.vitals.hunger.food,"saturation":agent.session.vitals.hunger.saturation,"exhaustion":agent.session.vitals.hunger.exhaustion,
        "death":agent.session.vitals.death,"xp":agent.session.vitals.xp.serialize(),"effects":agent.session.vitals.effects.serialize(),"sleep":agent.session.sleeping,"vehicle":agent.session.player.vehicle,
        "respawns":agent.session.respawns,"drawing":agent.session.drawing_slot(),"consuming":agent.session.consuming(),
        "glide":agent.session.player.fall_flying,"glide_ticks":agent.session.player.fall_fly_ticks,
        "fire":agent.session.vitals.burning()})
}
/// What clients need of *other* players: pose, held item, armor and health, not their inventory.
/// Rounds to a millimetre (or a thousandth of a radian): plenty for drawing, far fewer bytes.
fn r3(x: f64) -> f64 {
    (x * 1000.0).round() / 1000.0
}
fn stack_appearance(stack: Option<Stack>) -> String {
    // Book and storage contents are invisible in other players' hands.
    stack_to_string(stack.map(|mut stack| {
        if stack.payload.is_some_and(|p| {
            matches!(
                p.get(),
                Payload::WritableBook(_) | Payload::WrittenBook(_) | Payload::Container(_) | Payload::Bundle(_)
            )
        }) {
            stack.payload = None;
        }
        stack
    }))
}
fn state_lite(agent: &Agent, name: &str, swing: Option<f32>, eating: f32) -> Value {
    let p = agent.session.player.pos;
    json!({"swing":swing.map(|t| r3(t as f64)),"eat":r3(eating as f64),"id":agent.session.id.0,"name":name,"pos":[r3(p.x), r3(p.y), r3(p.z)],"vel":[0.0, 0.0, 0.0],
        "yaw":r3(agent.session.player.yaw as f64),"pitch":r3(agent.session.player.pitch as f64),"ground":agent.session.player.on_ground,
        "flying":agent.session.player.flying,"sneak":agent.session.player.sneaking,"mode":agent.session.mode.name(),"selected":agent.selected,
        "held":stack_appearance(agent.session.inventory.get(agent.selected)),
        "armor":agent.session.inventory.armor.iter().copied().map(stack_appearance).collect::<Vec<_>>(),
        "offhand":stack_appearance(agent.session.inventory.offhand),
        "time_since_rest":agent.session.vitals.time_since_rest,"ticks_frozen":agent.session.vitals.ticks_frozen,"health":agent.session.vitals.health,"death":agent.session.vitals.death,"sleep":agent.session.sleeping,"vehicle":agent.session.player.vehicle,
        "glide":agent.session.player.fall_flying,"glide_ticks":agent.session.player.fall_fly_ticks,
        // v1.0 gap Track 8d: on fire, for Java's flame on other players.
        "fire":agent.session.vitals.burning()})
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
    // v0.16 Track B: gliding on elytra.
    agent.session.player.fall_flying = v["glide"] == true;
    agent.session.player.fall_fly_ticks = v["glide_ticks"].as_u64().unwrap_or(0).min(u64::from(u32::MAX)) as u32;
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
    if let Some(held) = v["held"].as_str() {
        // Another player's lite state: just what is drawn.
        agent.session.inventory = crate::inventory::Inventory::default();
        agent.session.inventory.slots[agent.selected] = stack_from_str(held).flatten();
        for (slot, s) in agent.session.inventory.armor.iter_mut().zip(v["armor"].as_array().into_iter().flatten()) {
            *slot = s.as_str().and_then(stack_from_str).flatten();
        }
        agent.session.inventory.offhand = v["offhand"].as_str().and_then(stack_from_str).flatten();
    }
    agent.session.player.sneaking = v["sneak"] == true;
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
    agent.session.vitals.time_since_rest = v["time_since_rest"].as_u64().unwrap_or(0).min(i32::MAX as u64) as u32;
    agent.session.vitals.ticks_frozen = v["ticks_frozen"].as_u64().unwrap_or(0).min(140) as u16;
    if let Some(effects) = v["effects"].as_str() {
        agent.session.vitals.effects = crate::simulation::effects::Effects::deserialize(effects);
        agent.session.vitals.restore_boosted_health(v["health"].as_f64().unwrap_or(20.0) as f32);
    }
    agent.session.sleeping = v["sleep"].as_f64().map(|f| f as f32);
    agent.session.player.vehicle = v["vehicle"].as_u64().and_then(|id| u32::try_from(id).ok());
    // v1.0 gap Track 8d: the host's word on whether this player burns (the
    // flame on them, the screen's fire); the next snapshot renews it.
    if v["fire"] == true {
        agent.session.vitals.ignite(1.0);
    }
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
    /// A click on the host's copy of this guest's screen; `selected` is the
    /// clicked bundle's selected stack.
    pub(super) fn lan_click(&mut self, slot: u16, right: bool, shift: bool, selected: Option<usize>) {
        let grid_size = self.lan.inventory.grid.map_or(0, |grid| grid.size as u8);
        if (100..=109).contains(&slot) && usize::from(grid_size) != self.craft.size {
            return;
        }
        if slot != LAN_OUTSIDE_SLOT {
            self.audio.ui_click();
        }
        let selected = selected.and_then(|i| u8::try_from(i).ok());
        let revision = self.lan.inventory_revision;
        self.lan_send(Packet::Click { slot, right, shift, revision, grid_size, selected });
    }
    pub(super) fn chat_line(&mut self, message: &str) {
        self.console.push(message.into());
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
            // v1.0 gap Track 8f: each profile's pearls with its player data.
            for p in profiles.iter_mut() {
                let id = p["name"].as_str().and_then(|n| self.agents.players.get(n)).map(|b| b.id);
                if let Some(pearls) = id.and_then(|id| self.owned_pearls.save(&self.mobs.entities, self.dimension, id))
                {
                    p[super::save::PEARLS_KEY] = pearls;
                }
            }
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
                    Packet::Welcome { seed, generation, dimension, id } => {
                        let Some(dimension) = Dimension::from_name(&dimension) else {
                            self.lan.lost = Some("Invalid server dimension".into());
                            break;
                        };
                        self.lan.id = id;
                        let generation = Generation::from_prop(Some(&generation.to_string()));
                        self.reset_lan_world(seed, generation, dimension);
                        let view = self.lan.offline_render_distance.unwrap_or(LAN_VIEW_MAX).clamp(2, LAN_VIEW_MAX);
                        self.lan_command(&format!("view {view}"));
                    }
                    Packet::Chunk { pos, data } => self.world.receive_chunk(pos, data),
                    Packet::Edits { pos, cells } => self.world.receive_edits(pos, &cells),
                    Packet::State(state) => self.receive_lan_state(&state),
                    Packet::InventoryPart { last, bytes } => {
                        if self.lan.inventory.push(last, &bytes).is_none() {
                            self.lan.lost = Some("Invalid server inventory".into());
                            break;
                        }
                    }
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
                                generation: self.world.generator.generation.number(),
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
                    Packet::Click { slot, right, shift, revision, grid_size, selected } => {
                        if !(100..=109).contains(&slot) || peer.grid.size == usize::from(grid_size) {
                            let selected = selected.map(usize::from);
                            self.remote_click(peer, slot, right, shift, revision, selected);
                        }
                    }
                    Packet::Book { pages, title, offhand } => {
                        let bot = self.agents.players.get_mut(peer.profile.as_deref().unwrap()).unwrap();
                        if !peer.can_edit_book(&bot.agent, offhand) {
                            let _ = peer.connection.send(Packet::Chat("Error: Book contents are unavailable".into()));
                            continue;
                        }
                        let book = Command::Book { pages, title, offhand };
                        if let Err(e) = bot.agent.execute(book, &mut self.world, &mut self.mobs.entities, &[]) {
                            let _ = peer.connection.send(Packet::Chat(format!("Error: {e}")));
                        }
                    }
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
    /// The chests LAN players have open.
    /// v1.0 Track 9: the vehicle stores LAN peers have open.
    pub(super) fn open_vehicles(&self) -> impl Iterator<Item = u32> + '_ {
        self.lan.peers.iter().filter(|p| p.open).filter_map(|p| match p.body.container() {
            Container::Minecart(id) => Some(id),
            _ => None,
        })
    }

    /// v1.0 gap Track 2d: the merchants LAN players trade with, by profile.
    pub(super) fn lan_trading(&self) -> impl Iterator<Item = (u64, &str)> + '_ {
        self.lan.peers.iter().filter(|p| p.open).filter_map(|p| match p.body.container() {
            Container::Trading(id) => Some((id, p.profile.as_deref()?)),
            _ => None,
        })
    }

    pub(super) fn open_chests(&self) -> impl Iterator<Item = IVec3> + '_ {
        self.lan.peers.iter().filter(|p| p.open).filter_map(|p| match p.body.container() {
            Container::Chest(pos) | Container::EnderChest(pos) => Some(pos),
            _ => None,
        })
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
        if let Container::Chest(pos) | Container::EnderChest(pos) = container
            && self.world.get_block(pos).is_some_and(crate::entity::nether::guarded_by_piglins)
        {
            self.mobs.entities.piglins_notice(id, at, true, &self.world);
        }
    }

    /// Closes a LAN player's screen, returning its grid and work slots.
    fn peer_close(&mut self, peer: &mut Peer) {
        let Some(name) = peer.profile.clone() else { return };
        if !peer.open {
            return;
        }
        if let Some(bot) = self.agents.players.get_mut(&name) {
            let session = &mut bot.agent.session;
            let work: Vec<_> = session.work.iter_mut().filter_map(Option::take).collect();
            session.inventory.return_stacks(peer.grid.take_all().into_iter().chain(work));
            session.clear_anvil_name();
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
            c if c.starts_with("recipe_") => self.remote_recipe_book(peer, c)?,
            // v0.18 Track 2: the guest typed in an anvil's name box
            // (`ServerboundRenameItemPacket`).
            c if let Some(name) = super::anvil::parse_rename(c) => {
                let Some(profile) = peer.profile.clone() else { return Ok(()) };
                if peer.open
                    && matches!(peer.body.container, Container::Anvil(_))
                    && let Some(bot) = self.agents.players.get_mut(&profile)
                    && !bot.agent.session.set_anvil_name(name)
                {
                    return Err("Invalid item name".into());
                }
            }
            // v1.0 final track F1: a guest's drags, double clicks and
            // number, swap-hands and drop keys over its screen's slots.
            c if c.starts_with("drag ")
                || c.starts_with("collect ")
                || c.starts_with("slot_swap ")
                || c.starts_with("slot_throw ") =>
            {
                self.remote_screen_click(peer, c)?
            }
            "swap" => {
                let name = peer.profile.clone().unwrap_or_default();
                self.agent_hands(&name, &mut peer.body, |h| {
                    h.swap_hands();
                });
            }
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
            // v0.15 Track B: the book editor and lectern buttons, through
            // the same CLI commands and session rules.
            c if c.starts_with("book ") || c.starts_with("lectern ") => {
                let parsed = Command::parse(c)?;
                let name = peer.profile.as_deref().unwrap();
                let bot = self.agents.players.get_mut(name).unwrap();
                if let Command::Book { offhand, .. } = &parsed
                    && !peer.can_edit_book(&bot.agent, *offhand)
                {
                    return Err("Book contents are unavailable".into());
                }
                bot.agent.execute(parsed, &mut self.world, &mut self.mobs.entities, &[]).map_err(|e| e.to_string())?;
            }
            c if c.starts_with("sign ") => {
                let Command::Sign(cell, front, lines) = Command::parse(c)? else { return Err("Bad sign text".into()) };
                if !peer.sign_editing || peer.sign_edit.is_none_or(|(p, f, _)| (p, f) != (cell, front)) {
                    return Err("That sign isn't open for editing".into());
                }
                peer.sign_editing = false;
                let name = peer.profile.as_deref().unwrap();
                let bot = self.agents.players.get_mut(name).unwrap();
                bot.agent.session.edit_sign(&mut self.world, cell, front, lines)?;
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
            Container::EnderChest(p) => near(p, |b| crate::world::utilities::ender_chest_of(b).is_some()),
            Container::Furnace(p) => near(p, crate::world::furnace::is_furnace),
            Container::Brewing(p) => near(p, |b| b == Block::BREWING_STAND),
            Container::Enchanting(p) => near(p, |b| b == Block::ENCHANTING_TABLE),
            Container::Anvil(p) => near(p, Block::is_anvil),
            Container::Grindstone(p) => near(p, |b| b.base() == Block::GRINDSTONE),
            Container::Smithing(p) => near(p, |b| b == Block::SMITHING_TABLE),
            Container::Stonecutter(p) => near(p, |b| b.base() == Block::STONECUTTER),
            Container::Loom(p) => near(p, |b| b.base() == Block::LOOM),
            Container::Cartography(p) => near(p, |b| b.base() == Block::CARTOGRAPHY_TABLE),
            Container::Beacon(p) => near(p, |b| b == crate::world::beacon::BEACON),
            Container::Trading(id) => self.mobs.entities.merchant_in_reach(id, eye, session.entity_screen_range()),
            Container::Minecart(id) => self.mobs.entities.vehicle_in_reach(id, eye, session.entity_screen_range()),
        }
    }
    /// v0.18 Track 5: a guest's recipe book (`ServerboundPlaceRecipePacket`,
    /// `ServerboundRecipeBookSeenRecipePacket` and
    /// `ServerboundRecipeBookChangeSettingsPacket`): `recipe_place <id> max|one`,
    /// `recipe_seen <id>` and `recipe_settings <book> <open> <filtering>`.
    fn remote_recipe_book(&mut self, peer: &mut Peer, command: &str) -> Result<(), String> {
        use voxelcraft::crafting::book::RecipeId;
        let Some(name) = peer.profile.clone() else { return Ok(()) };
        let words: Vec<&str> = command.split_whitespace().collect();
        let recipe =
            |w: Option<&&str>| w.and_then(|w| RecipeId::from_name(w)).ok_or_else(|| "Unknown recipe".to_string());
        match words.first().copied() {
            Some("recipe_place") => {
                let id = recipe(words.get(1))?;
                let max = words.get(2) == Some(&"max");
                let Some(session) = self.agents.players.get(&name).map(|b| &b.agent.session) else { return Ok(()) };
                if !peer.open || !self.container_still_valid(session, peer.body.container) || session.vitals.is_dead() {
                    return Ok(());
                }
                let selected = self.agents.players.get(&name).map_or(0, |b| b.agent.selected);
                let mut container = peer.body.container;
                let _ = self.agent_screen(&name, &mut container, &mut peer.grid, |s| s.place_recipe(id, max, selected));
                peer.body.container = container;
            }
            Some("recipe_seen") => {
                let id = recipe(words.get(1))?;
                if let Some(bot) = self.agents.players.get_mut(&name) {
                    bot.agent.session.recipe_book.mark_seen(id);
                }
            }
            Some("recipe_settings") => {
                let book =
                    words.get(1).and_then(|b| super::recipe_book::book_from_name(b)).ok_or("Unknown recipe book")?;
                let flag = |i: usize| words.get(i).map(|w| *w == "1").ok_or("Bad recipe book settings");
                let (open, filtering) = (flag(2)?, flag(3)?);
                if let Some(bot) = self.agents.players.get_mut(&name) {
                    bot.agent.session.recipe_book.set_open(book, open);
                    bot.agent.session.recipe_book.set_filtering(book, filtering);
                }
            }
            _ => return Err("Unknown recipe book command".into()),
        }
        Ok(())
    }

    /// v1.0 final track F1: the guest's `drag KIND SLOT...`, `collect SLOT
    /// RIGHT`, `slot_swap SLOT KEY` (0..8 or 40) and `slot_throw SLOT WHOLE`
    /// on the host's copy of its screen.
    fn remote_screen_click(&mut self, peer: &mut Peer, command: &str) -> Result<(), String> {
        let mut words = command.split_whitespace();
        let verb = words.next().unwrap_or_default();
        let kind = if verb == "drag" {
            Some(super::clicks::parse_drag_word(words.next().unwrap_or_default()).ok_or("Bad drag")?)
        } else {
            None
        };
        let numbers: Vec<u16> =
            words.map(|w| w.parse::<u16>().map_err(|_| "Bad slot".to_string())).collect::<Result<_, _>>()?;
        if numbers.is_empty() || numbers.len() > 64 {
            return Err("Bad slot".into());
        }
        if !peer.open {
            return Ok(());
        }
        let Some(name) = peer.profile.clone() else { return Ok(()) };
        let Some(session) = self.agents.players.get(&name).map(|b| &b.agent.session) else { return Ok(()) };
        if !self.container_still_valid(session, peer.body.container) || session.vitals.is_dead() {
            return Ok(());
        }
        let (container, mode, size) = (peer.body.container, session.mode, peer.grid.size);
        let decode = |n: u16| decode_slot(n, container, mode, size);
        let mut container = peer.body.container;
        let done = match (verb, kind) {
            ("drag", Some(kind)) => {
                let slots: Vec<SlotRef> = numbers.iter().filter_map(|n| decode(*n)).collect();
                self.agent_screen(&name, &mut container, &mut peer.grid, |s| s.drag(&slots, kind))
            }
            ("collect", _) => {
                let Some(slot) = decode(numbers[0]) else { return Ok(()) };
                let right = numbers.get(1) == Some(&1);
                self.agent_screen(&name, &mut container, &mut peer.grid, |s| s.pickup_all(slot, right))
            }
            ("slot_swap", _) => {
                let Some(slot) = decode(numbers[0]) else { return Ok(()) };
                let key = match numbers.get(1) {
                    Some(&k) if k < 9 => Some(k as usize),
                    Some(40) => None,
                    _ => return Err("Bad swap key".into()),
                };
                self.agent_screen(&name, &mut container, &mut peer.grid, |s| s.swap_slot(slot, key))
            }
            ("slot_throw", _) => {
                let Some(slot) = decode(numbers[0]) else { return Ok(()) };
                let whole = numbers.get(1) == Some(&1);
                self.agent_screen(&name, &mut container, &mut peer.grid, |s| s.throw_slot(slot, whole))
            }
            _ => return Err("Bad screen click".into()),
        };
        peer.body.container = container;
        for request in done.map(|(_, r)| r).unwrap_or_default() {
            match request {
                super::screen::Request::Popup(text) => {
                    let _ = peer.connection.send(Packet::Chat(text));
                }
                super::screen::Request::Close => self.peer_close(peer),
            }
        }
        Ok(())
    }

    fn remote_click(
        &mut self,
        peer: &mut Peer,
        encoded: u16,
        right: bool,
        shift: bool,
        revision: u32,
        selected: Option<usize>,
    ) {
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
        // During a multipart update the guest can still see slots from before
        // a substantial change. Small updates keep ordinary rapid clicks ordered.
        if peer.inventory.awaiting_large_update(revision) {
            return;
        }
        if encoded == LAN_OUTSIDE_SLOT {
            if !peer.inventory.cursor_matches(revision, session.inventory.cursor) {
                return;
            }
            let s = &mut self.agents.players.get_mut(&name).unwrap().agent.session;
            if let Some(stack) = s.inventory.cursor {
                let count = if right { 1 } else { stack.count };
                s.inventory.cursor = (stack.count > count).then_some(Stack { count: stack.count - count, ..stack });
                self.mobs.entities.throw(Stack { count, ..stack }, s.player.eye(), s.player.forward().as_dvec3());
            }
            return;
        }
        let Some(slot) = decode_slot(encoded, peer.body.container, session.mode, peer.grid.size) else { return };
        // v0.18 Track 4: creative cells take shift with a stack held too.
        let quick =
            shift && (session.inventory.cursor.is_none() || matches!(slot, SlotRef::Palette(_) | SlotRef::Destroy));
        let mut container = peer.body.container;
        let done = self.agent_screen(&name, &mut container, &mut peer.grid, |s| {
            if quick {
                s.quick_move(slot)
            } else {
                s.click_slot_selected(slot, right, selected);
            }
        });
        peer.body.container = container;
        for request in done.map(|(_, r)| r).unwrap_or_default() {
            match request {
                super::screen::Request::Popup(text) => {
                    let _ = peer.connection.send(Packet::Chat(text));
                }
                super::screen::Request::Close => self.peer_close(peer),
            }
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
                        // `openTextEdit`: the client opens its sign editor and
                        // answers with a `sign` command.
                        Request::Open(Used::EditSign { cell, front }) => {
                            let serial = peer.sign_edit.map_or(1, |(_, _, n)| n.wrapping_add(1));
                            peer.sign_edit = Some((cell, front, serial));
                            peer.sign_editing = true;
                        }
                        Request::Open(Used::Lectern { cell }) => {
                            let serial = peer.book_open.as_ref().map_or(1, |(_, n)| n.wrapping_add(1));
                            peer.book_open = Some((json!({ "lectern": cell.to_array() }), serial));
                        }
                        Request::Open(Used::OpenBook { slot }) => {
                            let serial = peer.book_open.as_ref().map_or(1, |(_, n)| n.wrapping_add(1));
                            peer.book_open = Some((json!({ "slot": slot }), serial));
                        }
                        Request::Open(Used::Utility {
                            effect: crate::world::utilities::interact::Effect::EnderChest,
                            cell,
                        }) => {
                            peer.used_at = Some(cell);
                            self.peer_open(peer, Container::EnderChest(cell));
                        }
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
                        Some(b) if b.is_bed() || super::bed::is_straw(Some(b)) => {
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
                        Some(b) if b.base() == Block::STONECUTTER => {
                            self.peer_open(peer, Container::Stonecutter(p));
                        }
                        Some(b) if b.base() == Block::LOOM => {
                            self.peer_open(peer, Container::Loom(p));
                        }
                        Some(b) if b.base() == Block::CARTOGRAPHY_TABLE => {
                            self.peer_open(peer, Container::Cartography(p));
                        }
                        Some(crate::world::beacon::BEACON) => {
                            self.peer_open(peer, Container::Beacon(p));
                            if let Some(bot) = self.agents.players.get_mut(&name) {
                                super::beacon::init_choice(&mut bot.agent.session, &self.world, p);
                            }
                        }
                        _ => {}
                    }
                }
            }
            // v1.0 gap Track 8: a spectator guest passes through portals as
            // Java's do, so there's nothing to explain.
            if let Some(bot) = self.agents.players.get(&name).filter(|b| b.agent.session.mode.uses_portals()) {
                let p = bot.agent.session.player.pos.floor().as_ivec3();
                if self
                    .world
                    .get_block(p)
                    .is_some_and(|b| b.is_nether_portal() || b == Block::END_PORTAL || b == Block::END_GATEWAY)
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
            self.lan.horns.clear();
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
        // v1.0 gap Track 8e: a host watching the credits has left the level
        // (`ServerPlayer.showEndCredits`), so guests don't see them.
        let host_here = self.credits.is_none();
        let players: Vec<_> =
            std::iter::once(state_lite(&host, &self.lan.profile, self.hand.swing_time(), host_eating))
                .filter(|_| host_here)
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
        let horns = std::mem::take(&mut self.lan.horns);
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
                peer.wood_sent = None;
                peer.open = false;
                let _ = peer.connection.send(Packet::Welcome {
                    seed: self.world.generator.seed,
                    generation: self.world.generator.generation.number(),
                    dimension: self.dimension.name().into(),
                    id: bot.id.0,
                });
            }
            let inventory = &bot.agent.session.inventory;
            if peer.connection.queued_bytes() <= 512 * 1024
                && let Some(part) = peer.inventory.next(inventory, peer.grid)
                && let Err(e) = peer.connection.send(part)
            {
                peer.connection.close(&e.to_string());
                continue;
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
                    // A double chest's other half (protocol 3 clients without it see one half).
                    let partner = self.world.chest_partner(p);
                    let partner_state = partner.and_then(|q| self.world.chest(q)).map(|c| c.serialize());
                    // v0.17 Track C1: a crafter's disabled slots, as a bit mask.
                    let disabled = self.world.crafter(p).map(|c| c.disabled);
                    json!({"kind":"chest","pos":p.to_array(),"state":self.world.chest(p).map(|c|c.serialize()),
                        "partner":partner.map(|q|q.to_array()),"partner_state":partner_state,
                        "crafter_disabled":disabled})
                }
                Container::EnderChest(p) => {
                    json!({"kind":"ender_chest","pos":p.to_array(),"state":bot.agent.session.ender.serialize()})
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
                Container::Stonecutter(p) | Container::Loom(p) | Container::Cartography(p) => {
                    let kind = match peer.body.container() {
                        Container::Stonecutter(_) => "stonecutter",
                        Container::Loom(_) => "loom",
                        _ => "cartography",
                    };
                    let s = &bot.agent.session;
                    let input = s.work_input.map(|i| i.0);
                    json!({"kind":kind,"pos":p.to_array(),"work":work,"choice":s.work_choice,"input":input})
                }
                // v0.16 Track C: the beacon's payment, chosen powers and levels.
                Container::Beacon(p) => {
                    let levels = self.world.beacon(p).map_or(0, |b| b.levels);
                    json!({"kind":"beacon","pos":p.to_array(),"work":work,"choice":bot.agent.session.work_choice,
                        "levels":levels})
                }
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
                    let kind = m.kind.lan_index();
                    let mut v = json!({"m":[m.uid, kind, r3(m.pos.x), r3(m.pos.y), r3(m.pos.z), r3(m.pose_values()[0] as f64),
                        r3(m.head_yaw as f64), r3(m.head_pitch as f64), r3(m.health as f64), r3(m.hurt as f64),
                        m.dying.map_or(-1.0, |d| r3(d as f64)), m.size, u8::from(m.baby)]});
                    if refresh || !peer.looked.contains(&m.uid) {
                        v["look"] = m.look();
                    }
                    // v0.17 Track D1: the vehicle a mob sits in, so a guest
                    // keeps the cart-passenger link.
                    if let Some(vehicle) = m.riding {
                        v["r"] = json!(vehicle);
                    }
                    // The charge bar changes every tick, so it rides with the pose.
                    if let Some(mount) = m.mount.as_ref().filter(|s| s.jump_charge > 0.0) {
                        v["jump"] = json!(mount.jump_charge);
                    }
                    // v1.0 gap Track 7c: the squid's tentacle cycle and the
                    // bee's `shouldRoll` drive their client animations
                    // every tick, so they ride with the pose too.
                    if let Some(t) = m.tentacle_cycle() {
                        v["tc"] = json!(r3(t as f64));
                    }
                    if m.bee.is_some_and(|b| b.rolling) {
                        v["roll"] = json!(true);
                    }
                    // v1.0 gap Track 8d: a burning mob carries Java's flame.
                    if m.burning {
                        v["fire"] = json!(true);
                    }
                    if let Some(t) = m.crossbow_draw() {
                        v["draw"] = json!(r3(t as f64));
                    }
                    // v1.0 gap Track 7f: the villager's `unhappyCounter`
                    // (synched data in Java) drives its head shake.
                    if let Some(c) = m.villager.as_ref().map(|v| v.unhappy).filter(|&c| c > 0) {
                        v["sad"] = json!(c);
                    }
                    looked.insert(m.uid);
                    v
                })
                .collect();
            peer.looked = looked;
            let me = bot.agent.session.player.pos;
            let sounds = peer_sounds(&sounds, &horns, bot.agent.session.id, me, host_pos);
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
            let arrows:Vec<_>=self.mobs.entities.arrows.iter().filter(|a|a.pos.distance_squared(bot.agent.session.player.pos)<160.0*160.0).take(256).map(|a|json!({"pos":a.pos.to_array(),"previous":a.previous_pos.to_array(),"dir":a.dir.to_array(),"player":a.from_player,"critical":a.critical,"tipped":a.tipped,"potion":a.potion.map(|p|p.0),"fire":a.burning()})).collect();
            let decor = self.mobs.entities.decor_near(bot.agent.session.player.pos, 160.0, 256);
            // Signs, shelves and chiseled bookshelves near the player, when
            // they changed or the player moved 16 blocks since the last send.
            let here = bot.agent.session.player.pos;
            let wood_version = self.world.wood_entities().version();
            let wood = match peer.wood_sent {
                Some((v, at)) if v == wood_version && at.distance_squared(here) < 16.0 * 16.0 => Value::Null,
                _ => json!(self.world.wood_near(here, 160.0, WOOD_BYTES)),
            };
            let sign_edit =
                peer.sign_edit.map(|(p, front, n)| json!({"pos": p.to_array(), "front": front, "serial": n}));
            // Everyone but this client itself (it has its own full state).
            let others: Vec<&Value> = players.iter().filter(|p| p["id"] != bot.id.0).collect();
            let snapshot = json!({"sequence":peer.applied,"self":state(&bot.agent,name),"players":others,"time":self.day_time,"rain":self.weather.raining,"rain_strength":self.weather.strength,"thunder":self.weather.thundering,"dimension":self.dimension.name(),"seed":self.world.generator.seed,"view":view,"open":peer.open,"container":container,"grid_size":peer.grid.size,"mobs":mobs,"carts":carts,"items":items,"decor":decor,"arrows":arrows,"extras":extras,"falling":falling,"sounds":sounds,"effects":effects,"cracks":cracks,"wood":wood,"sign_edit":sign_edit});
            let mut snapshot = snapshot;
            snapshot["copper_golem_statues"] = json!(self.world.copper_golem_statues_near(here));
            // v0.15 Track B: banner patterns and lectern books near the player.
            snapshot["workstations"] = json!(self.world.workstations_near(here, 160.0));
            // v0.16 Track B: shulker box lids that are open or moving.
            snapshot["shulker_lids"] = json!(self.world.shulker_lids_near(here, 160.0));
            // v0.18 Track 9: chest lids and barrel tops in use or moving.
            snapshot["lids"] = json!(self.world.lids_near(here, 160.0));
            if let Some((open, serial)) = &peer.book_open
                && (open["lectern"].is_array()
                    || open["slot"].as_u64().is_some_and(|slot| peer.inventory.held_matches(slot as usize, inventory)))
            {
                let mut open = open.clone();
                open["serial"] = json!(serial);
                snapshot["book_open"] = open;
            }
            // v0.18 Track 5: the guest's recipe book when it changed, and the
            // rule that hides the result of recipes it doesn't know.
            let book_revision = bot.agent.session.recipe_book.revision;
            if peer.recipe_book_sent != Some(book_revision) {
                snapshot["recipe_book"] = json!(bot.agent.session.recipe_book.serialize());
            }
            snapshot["limited_crafting"] = json!(self.gamerules.bool("limited_crafting"));
            let (snapshot, shed_looks) = fit_state(snapshot);
            if snapshot.as_ref().is_some_and(|v| v["wood"].is_string()) {
                peer.wood_sent = Some((wood_version, here));
            }
            if snapshot.as_ref().is_some_and(|v| v["recipe_book"].is_string()) {
                peer.recipe_book_sent = Some(book_revision);
            }
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
    fn reset_lan_world(&mut self, seed: u64, generation: Generation, dimension: Dimension) {
        self.renderer.clear_world();
        self.dimension = dimension;
        self.arrival = None;
        self.world = World::new_remote(
            Arc::new(Generator::with_generation(seed, dimension, generation)),
            self.settings.render_distance,
        );
        self.lan.predictions.clear();
        self.lan.predicted_eating = None;
        self.lan.predicted_bundle = None;
        self.lan.ready = false;
        self.mobs.entities = crate::entity::Entities::new(seed);
    }
    fn sample_lan_input(&mut self) -> Input {
        let movement = self.movement_input(false);
        let active = !self.menu.is_open() && !self.console.open && !self.inventory_open && self.mouse_grabbed;
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
    fn lan_movement(&self, input: &Input, use_state: LanUse) -> voxelcraft::player::MoveInput {
        voxelcraft::player::MoveInput {
            sprint_blocked: !self.session.vitals.may_sprint(self.session.mode.invulnerable()),
            using_item: input.buttons & 16 != 0 && (use_state.eating || use_state.drawing || use_state.bundle),
            ..input.movement()
        }
    }

    pub(super) fn tick_lan_client(&mut self) {
        let mut input = self.sample_lan_input();
        // v1.0 Track 9: the host simulates this player, so an auto-jump the
        // client's own move found goes to it as a jump press.
        self.session.player.auto_jump = self.settings.auto_jump;
        if self.session.player.take_auto_jump() {
            input.buttons |= 1;
        }
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
        let use_state = self.lan.sample_use(&self.session, &self.world, &input, pressed);
        let movement = self.lan_movement(&input, use_state);
        self.lan_send(Packet::Input(input));
        self.previous_eye = self.session.player.eye();
        // A seated client takes its position from the host; the vehicle moves it.
        if self.lan.ready && self.session.player.vehicle.is_some() {
            self.lan.predictions.push_back((input, use_state));
        } else if self.lan.ready {
            self.session.player.apply_effects(&self.session.vitals.effects);
            self.session.player.powder_snow_equipment(&self.session.inventory.armor, self.session.vitals.ticks_frozen);
            self.session.player.wear_boots(crate::enchant::armor_level(
                &self.session.inventory.armor,
                crate::enchant::Enchantment::DepthStrider,
            ));
            self.session.player.tick(movement, &self.world);
            self.lan.predictions.push_back((input, use_state));
        }
        if self.lan.ready {
            // v0.16 Track C: beacon beams from the replicated blocks.
            self.world.tick_beacon_replicas();
            // v0.16 Track F: conduits animate from the replicated blocks.
            self.world.tick_conduit_replicas();
            self.tick_particles();
            // v1.0 gap Track 1: Darkness's blend and pulse run on the client.
            self.session.vitals.effects.tick_client_view();
            // The client doesn't track portal time; nausea comes from the host.
            let nausea =
                self.session.vitals.effects.get(voxelcraft::simulation::effects::Effect::Nausea).map(|e| e.ticks);
            self.distortion.tick(nausea, false);
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
            let generation = self.world.generator.generation;
            self.reset_lan_world(snapshot["seed"].as_u64().unwrap_or(0), generation, dimension);
            self.chat_line("Host changed dimension; all LAN players follow the host");
        }
        let sequence = snapshot["sequence"].as_u64().unwrap_or(0) as u32;
        while self.lan.predictions.front().is_some_and(|(i, _)| i.sequence <= sequence) {
            self.lan.predictions.pop_front();
        }
        let mut own = Agent::new(self.session.player.pos);
        apply(&mut own, &snapshot["self"]);
        // v0.18 Track 5: the host's copy of this guest's recipe book; its
        // settings are taken only the first time (the guest changes them).
        if let Some(text) = snapshot["recipe_book"].as_str() {
            self.session.recipe_book.sync_from(text, !self.recipe_book.synced);
        }
        let limited = if snapshot["limited_crafting"] == true { "true" } else { "false" };
        let _ = self.gamerules.set("limited_crafting", limited);
        // The host's draw is authoritative (a charged crossbow fired and
        // reloading, say) once it has seen this client's latest use press or
        // release; until then the local prediction stands.
        // Once the host has seen the latest use press or release, every
        // queued input belongs to the hold it reports, so its draw state
        // (a crossbow finishing loading, say) also overrides theirs.
        let settled = sequence >= self.lan.use_edge_sequence;
        if settled {
            self.lan.predicted_draw = snapshot["self"]["drawing"].as_u64().map(|slot| slot as usize);
            self.lan.predicted_eating = (snapshot["self"]["consuming"] == true).then_some(own.selected);
        }
        // The client owns its look, except that a respawn faces the host's
        // respawn angle (Java's respawn transition sets the rotation).
        let respawns = snapshot["self"]["respawns"].as_u64();
        let respawned = self.lan.respawns.zip(respawns).is_some_and(|(seen, now)| seen != now);
        self.lan.respawns = respawns;
        if respawned {
            self.distortion.reset();
        }
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
        self.session.inventory = self.lan.inventory.current.clone();
        self.lan.inventory_revision = self.lan.inventory.revision;
        let previous = std::mem::take(&mut self.session.vitals.effects);
        self.session.vitals = own.session.vitals;
        // v1.0 gap Track 1: the client's own Darkness blend and tick count.
        self.session.vitals.effects.keep_client_view(&previous, !self.lan.ready);
        self.session.sleeping = own.session.sleeping;
        if !self.lan.ready {
            self.body.actions.selected = own.selected;
            self.previous_eye = self.session.player.eye();
        }
        let seated = self.session.player.vehicle.is_some();
        // Carry the host's draw through the queue: with every use press
        // acknowledged, a queued input can only end it (use released or
        // another slot selected), never start one.
        let mut running = self.lan.predicted_draw;
        let mut predictions = std::mem::take(&mut self.lan.predictions);
        for (input, sampled) in predictions.iter_mut().filter(|_| !seated) {
            let input = *input;
            self.session.player.yaw = input.yaw;
            self.session.player.pitch = input.pitch;
            self.session.player.apply_effects(&self.session.vitals.effects);
            self.session.player.powder_snow_equipment(&self.session.inventory.armor, self.session.vitals.ticks_frozen);
            // Sprint eligibility comes from the host's vitals just applied;
            // the use state is the sampled one, unless settled above.
            let slot = usize::from(input.selected).min(8);
            if running.is_some_and(|drawn| drawn != slot || input.buttons & 16 == 0) {
                running = None;
            }
            let drawing = if settled { running == Some(slot) } else { sampled.drawing };
            if settled {
                // Food can start again during the same hold after looking
                // away from a statue. Recheck each replayed view from the
                // host's consumption state, rather than copying one snapshot
                // over newer inputs or retaining a completed bite forever.
                sampled.eating = self.lan.sample_use(&self.session, &self.world, &input, false).eating;
            }
            let movement = self.lan_movement(&input, LanUse { drawing, ..*sampled });
            self.session.player.tick(movement, &self.world);
        }
        self.lan.predictions = predictions;
        if settled && !seated {
            self.lan.predicted_draw = running;
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
            self.set_grab(!self.inventory_open && !self.menu.is_open());
            self.body.left_held = false;
            self.body.right_held = false;
            self.keys.clear();
        }
        if let Some(text) = snapshot["wood"].as_str() {
            self.world.replace_wood(text);
        }
        if let Some(text) = snapshot["copper_golem_statues"].as_str() {
            self.world.replace_copper_golem_statues(text);
        }
        if let Some(text) = snapshot["workstations"].as_str() {
            self.world.replace_workstations(text);
        }
        self.world.apply_shulker_lids(&snapshot["shulker_lids"]);
        self.world.apply_lids(&snapshot["lids"]);
        let edit = &snapshot["sign_edit"];
        if let (Some(serial), Some(p)) = (edit["serial"].as_u64(), vector(&edit["pos"]))
            && serial as u32 != self.lan.sign_serial
            // Retried until the sign's contents have arrived to open it.
            && self.open_sign_editor(p.as_ivec3(), edit["front"] == true)
        {
            self.lan.sign_serial = serial as u32;
        }
        // v0.15 Track B: the host asked for a lectern reader or book screen.
        let open = &snapshot["book_open"];
        if let Some(serial) = open["serial"].as_u64()
            && serial as u32 != self.lan.book_serial
        {
            let opened = if let Some(p) = vector(&open["lectern"]) {
                let p = p.as_ivec3();
                // Retried until the lectern's book has arrived.
                self.world.lectern_at(p).is_some() && {
                    self.open_lectern(p);
                    true
                }
            } else {
                open["slot"].as_u64().is_some_and(|slot| self.open_held_book(slot as usize))
            };
            if opened {
                self.lan.book_serial = serial as u32;
            }
        }
        let c = &snapshot["container"];
        let pos = vector(&c["pos"]).map(|p| p.as_ivec3());
        // Any other screen takes back an earlier ender chest's loan, so a
        // chest later placed in that cell isn't hidden behind it.
        if c["kind"] != "ender_chest"
            && let Some(lent) = self.world.lent_chest()
        {
            self.world.take_lent_chest(lent);
        }
        self.body.container = match (c["kind"].as_str(), pos) {
            (Some("chest"), Some(p)) => {
                if let Some(text) = c["state"].as_str() {
                    self.world.load_chests(&format!("{},{},{}={text}", p.x, p.y, p.z));
                }
                if let (Some(q), Some(text)) =
                    (vector(&c["partner"]).map(|q| q.as_ivec3()), c["partner_state"].as_str())
                {
                    self.world.load_chests(&format!("{},{},{}={text}", q.x, q.y, q.z));
                }
                if let Some(mask) = c["crafter_disabled"].as_u64().and_then(|m| u16::try_from(m).ok()) {
                    self.world.set_crafter_disabled_mask(p, mask);
                }
                Container::Chest(p)
            }
            // The host keeps each player's ender inventory; this copy is lent
            // to the screen, like the keyboard player's own.
            (Some("ender_chest"), Some(p)) => {
                if let Some(chest) = c["state"].as_str().and_then(crate::world::chest::Chest::deserialize) {
                    self.world.lend_chest(p, chest);
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
            (Some(kind @ ("stonecutter" | "loom" | "cartography")), Some(p)) => {
                self.session.work_choice = c["choice"].as_u64().and_then(|v| u16::try_from(v).ok());
                self.session.work_input =
                    c["input"].as_u64().and_then(|v| u16::try_from(v).ok()).map(crate::item::Item);
                match kind {
                    "stonecutter" => Container::Stonecutter(p),
                    "loom" => Container::Loom(p),
                    _ => Container::Cartography(p),
                }
            }
            (Some("trading"), _) => match c["trade"]["id"].as_u64() {
                Some(id) => Container::Trading(id),
                None => Container::Inventory,
            },
            (Some("vehicle"), _) => match c["id"].as_u64().and_then(|id| u32::try_from(id).ok()) {
                Some(id) => Container::Minecart(id),
                None => Container::Inventory,
            },
            (Some("beacon"), Some(p)) => {
                self.session.work_choice = c["choice"].as_u64().and_then(|v| u16::try_from(v).ok());
                Container::Beacon(p)
            }
            (Some("crafting"), _) => Container::CraftingTable,
            _ => Container::Inventory,
        };
        self.session.work = [None; 3];
        if let Some(work) = c["work"].as_array() {
            for (slot, v) in self.session.work.iter_mut().zip(work) {
                *slot = v.as_str().and_then(stack_from_str).flatten();
            }
        }
        let grid_size = if snapshot["grid_size"] == 3 { 3 } else { 2 };
        self.craft =
            self.lan.inventory.grid.filter(|grid| grid.size == grid_size).unwrap_or_else(|| Grid::new(grid_size));
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
                let kind = m["m"]
                    .get(1)
                    .and_then(Value::as_u64)
                    .and_then(|k| crate::entity::MobKind::from_lan_index(k as usize));
                let (Some(kind), Some(x), Some(y), Some(z)) = (kind, f(2), f(3), f(4)) else { continue };
                let pos = DVec3::new(x, y, z);
                if !pos.is_finite() {
                    continue;
                }
                let uid = m["m"].get(0).and_then(Value::as_u64).unwrap_or(0) as u32;
                let known = previous.contains_key(&uid);
                let mut mob = previous.remove(&uid).unwrap_or_else(|| crate::entity::Mob::new(kind, pos, 0.0));
                mob.previous_pos = mob.pos;
                // v0.18 Track 6: turn and walk smoothly from the last snapshot's pose.
                let before = mob.pose_values();
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
                if let Some(t) = m["tc"].as_f64() {
                    mob.set_tentacle_cycle(t as f32);
                }
                if let Some(bee) = mob.bee.as_mut() {
                    bee.rolling = m["roll"] == true;
                }
                mob.burning = m["fire"] == true;
                mob.set_crossbow_draw(m["draw"].as_f64().map(|t| t as f32));
                if let Some(v) = mob.villager.as_mut() {
                    let c = m["sad"].as_u64().unwrap_or(0);
                    v.unhappy = c.min(u64::from(voxelcraft::entity::villager::UNHAPPY_TICKS)) as u8;
                }
                mob.riding = m["r"].as_u64().and_then(|r| u32::try_from(r).ok());
                mob.animate_replica(voxelcraft::simulation::TICK_SECONDS as f32);
                mob.previous_pose = if known { before } else { mob.pose_values() };
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
                    // The host's category for this play, else the sound's own.
                    let source = s
                        .get(7)
                        .and_then(Value::as_u64)
                        .and_then(|i| crate::audio::SoundSource::ALL.get(i as usize).copied())
                        .unwrap_or(sound.source());
                    self.audio.play_in(source, sound, Some(pos), gain.clamp(0.0, 4.0) as f32, pitch);
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
                arrow.tipped = a["tipped"] == true;
                arrow.set_burning(a["fire"] == true);
                arrow.potion = a["potion"]
                    .as_u64()
                    .filter(|i| *i < voxelcraft::potion::Potion::COUNT as u64)
                    .map(|i| voxelcraft::potion::Potion(i as u8));
                self.mobs.entities.arrows.push(arrow);
            }
        }
        // Item frames, paintings and armor stands; a shed list keeps the last copy.
        if let Some(decor) = snapshot["decor"].as_str() {
            self.mobs.entities.replace_decor(decor);
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
            self.set_grab(!self.menu.is_open());
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
        SlotRef::Offhand => 404,
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
        // v0.15 Track B workstations.
        SlotRef::CutInput => 550,
        SlotRef::CutResult => 551,
        SlotRef::LoomBanner => 552,
        SlotRef::LoomDye => 553,
        SlotRef::LoomPattern => 554,
        SlotRef::LoomResult => 555,
        SlotRef::MapInput => 556,
        SlotRef::MapExtra => 557,
        SlotRef::MapResult => 558,
        SlotRef::CutRecipe(i) if i < 100 => 600 + i as u16,
        SlotRef::LoomChoice(i) if i < 100 => 700 + i as u16,
        // v0.16 Track C: the beacon's payment slot and buttons.
        SlotRef::BeaconPayment => 560,
        SlotRef::BeaconPower(i) if i < 7 => 561 + u16::from(i),
        SlotRef::BeaconDone => 568,
        SlotRef::BeaconCancel => 569,
        // A plain creative stack keeps the code older hosts read: its place
        // in `Item::creative_palette`. v0.18 Track 4: other stacks (potions,
        // enchanted books, rockets) are 20000 + tab x 1000 + their place in
        // that tab of `creative::catalogue`.
        SlotRef::Palette(stack) => {
            match crate::item::Item::creative_palette().position(|i| Stack::new(i, 1) == stack) {
                Some(i) if i < 19_000 => 1000 + i as u16,
                _ => {
                    let c = voxelcraft::creative::catalogue();
                    let (t, i) = (0..c.tabs.len())
                        .find_map(|t| c.tabs[t].items.iter().position(|&s| s == stack).map(|i| (t, i)))?;
                    if i >= 1000 {
                        return None;
                    }
                    20_000 + t as u16 * 1000 + i as u16
                }
            }
        }
        SlotRef::Destroy => 405,
        SlotRef::PaletteEmpty => 406,
        _ => return None,
    })
}
fn decode_slot(slot: u16, container: Container, mode: GameMode, grid_size: usize) -> Option<SlotRef> {
    match slot {
        0..=35 => Some(SlotRef::Inventory(slot as usize)),
        100..=108 if (slot - 100) < (grid_size * grid_size) as u16 => Some(SlotRef::Craft((slot - 100) as usize)),
        109 => Some(SlotRef::CraftResult),
        200..=253 if matches!(container, Container::Chest(_) | Container::EnderChest(_) | Container::Minecart(_)) => {
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
        550 | 551 | 600..=699 if matches!(container, Container::Stonecutter(_)) => Some(match slot {
            550 => SlotRef::CutInput,
            551 => SlotRef::CutResult,
            i => SlotRef::CutRecipe((i - 600) as usize),
        }),
        552..=555 | 700..=799 if matches!(container, Container::Loom(_)) => Some(match slot {
            552 => SlotRef::LoomBanner,
            553 => SlotRef::LoomDye,
            554 => SlotRef::LoomPattern,
            555 => SlotRef::LoomResult,
            i => SlotRef::LoomChoice((i - 700) as usize),
        }),
        556..=558 if matches!(container, Container::Cartography(_)) => Some(match slot {
            556 => SlotRef::MapInput,
            557 => SlotRef::MapExtra,
            _ => SlotRef::MapResult,
        }),
        560..=569 if matches!(container, Container::Beacon(_)) => Some(match slot {
            560 => SlotRef::BeaconPayment,
            568 => SlotRef::BeaconDone,
            569 => SlotRef::BeaconCancel,
            i => SlotRef::BeaconPower((i - 561) as u8),
        }),
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
        404 if container == Container::Inventory => Some(SlotRef::Offhand),
        405 if mode.is_creative() && container == Container::Inventory => Some(SlotRef::Destroy),
        406 if mode.is_creative() && container == Container::Inventory => Some(SlotRef::PaletteEmpty),
        20_000.. if mode.is_creative() && container == Container::Inventory => {
            let (t, i) = ((slot - 20_000) / 1000, (slot - 20_000) % 1000);
            let tab = voxelcraft::creative::catalogue().tabs.get(t as usize)?;
            tab.items.get(i as usize).copied().map(SlotRef::Palette)
        }
        1000.. if mode.is_creative() && container == Container::Inventory => crate::item::Item::creative_palette()
            .nth((slot - 1000) as usize)
            .map(|item| SlotRef::Palette(Stack::new(item, 1))),
        _ => None,
    }
}

impl Game {
    pub(super) fn lan_ui(&self, ui: &mut crate::render::ui::Ui) {
        if self.lan.host.is_none() && self.lan.client.is_none() {
            self.chat_ui(ui);
            return;
        }
        let (sw, _) = ui.size();
        if self.held(super::keys::Action::PlayerList) && !self.console.open {
            let mut names = vec![self.lan.profile.as_str()];
            names.extend(self.agents.players.iter().filter(|(_, b)| b.active).map(|(n, _)| n.as_str()));
            let width = names.iter().map(|n| crate::render::ui::Ui::text_width(n)).fold(100.0, f32::max) + 16.0;
            ui.rect((sw - width) / 2.0, 12.0, width, 16.0 + names.len() as f32 * 12.0, [0.0, 0.0, 0.0, 0.75]);
            for (i, name) in names.iter().enumerate() {
                ui.text((sw - width) / 2.0 + 8.0, 20.0 + i as f32 * 12.0, name, crate::render::ui::WHITE);
            }
        }
        self.chat_ui(ui);
    }

    /// The chat over the hotbar, as Java's unfocused `ChatComponent` draws
    /// it: the last 10 lines (`chatHeightUnfocused`'s 90 pixels of 9-pixel
    /// lines) up from 40 pixels above the bottom, on a 332-pixel band of
    /// black at half opacity (`textBackgroundOpacity`), each line fading out
    /// 10 seconds after it came ([`super::console::chat_alpha`]), in a LAN
    /// game as in a single-player one (v1.0 gap track 6d; the debug
    /// feedback of v1.0 gap track 6c, Java's `showDebugChat`). Debug
    /// feedback starts with Java's yellow, bold `debug.prefix`.
    pub(super) fn chat_ui(&self, ui: &mut crate::render::ui::Ui) {
        if self.console.open || self.menu.is_open() || self.inventory_open {
            return;
        }
        let (_, sh) = ui.size();
        let bottom = sh - 40.0;
        let lines = self.console.lines.iter().zip(&self.console.added).rev().filter(|(s, _)| !s.is_empty());
        for (i, (line, added)) in lines.take(10).enumerate() {
            let alpha = super::console::chat_alpha(added.elapsed().as_secs_f32());
            if alpha <= 1e-5 {
                continue;
            }
            let entry_bottom = bottom - i as f32 * 9.0;
            ui.rect(0.0, entry_bottom - 9.0, 332.0, 9.0, crate::render::ui::shade(alpha * 0.5));
            // Java skips text whose alpha byte is 3 or less (`ARGB.alpha`).
            if alpha * 255.0 <= 3.0 {
                continue;
            }
            let y = entry_bottom - 8.0;
            let mut x = 4.0;
            let rest = match line.strip_prefix(DEBUG_PREFIX) {
                Some(rest) => {
                    // `ChatFormatting.YELLOW` (0xFFFF55), bold: each glyph
                    // drawn again a pixel right and advanced a pixel more.
                    let yellow = [1.0, 1.0, 85.0 / 255.0, alpha];
                    for c in DEBUG_PREFIX.chars() {
                        let s = c.to_string();
                        ui.text(x + 1.0, y, &s, yellow);
                        x += ui.text(x, y, &s, yellow) + 1.0;
                    }
                    rest
                }
                None => line.as_str(),
            };
            ui.text(x, y, rest, [1.0, 1.0, 1.0, alpha]);
        }
    }
}

/// Java's `debug.prefix`, which `KeyboardHandler.debugFeedbackComponent`
/// puts before debug feedback.
pub(super) const DEBUG_PREFIX: &str = "[Debug]:";

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
        by: crate::entity::PlayerId,
    ) -> Option<(DVec3, [u8; 4], bool)> {
        // v1.0 gap Track 8f: "was slain by" the attacker, by name.
        let owner = Some(crate::entity::trident::Owner::Player(by));
        let cause = voxelcraft::simulation::damage::Cause::PLAYER_ATTACK.by(owner);
        let hit = knockback::Hit::melee(source, self.player(target)?.player.pos, yaw, extra);
        let landed = match target {
            PlayerRef::Host => {
                let toward = source - self.session.player.pos;
                let landed = self.damage_player_from(amount, cause, Some(toward)) > 0.0;
                if landed {
                    let entities = &mut self.mobs.entities;
                    self.session.knocked_back(&hit, &mut || f64::from(entities.roll_range(0.0, 1.0)));
                }
                landed
            }
            PlayerRef::Agent(name) => {
                let bot = self.agents.players.get_mut(name)?;
                bot.agent.hurt_knocked(amount, cause, &hit, &mut self.mobs.entities) > 0.0
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
        // An axe hit that a raised shield blocks disables the shield.
        let axe = held_item.and_then(|i| i.as_tool()).is_some_and(|(kind, _)| kind == crate::item::ToolKind::Axe);
        if let Some(t) = self.player_mut(&target) {
            t.axe_incoming(axe);
        }
        let attacker_id = self.player(attacker).map_or(crate::entity::PlayerId::HOST, |s| s.id);
        let target_id = self.player(&target).map_or(crate::entity::PlayerId::HOST, |s| s.id);
        let hurt = self.hurt_player_ref(&target, damage, (attacker_pos, yaw, extra), fire, attacker_id);
        if let Some(t) = self.player_mut(&target) {
            t.axe_incoming(false);
        }
        let Some((at, thorns, landed)) = hurt else {
            return true;
        };
        // `Player.attack`: the extra knockback, sweep and post-attack effects
        // (Thorns) follow only a hit `hurtOrSimulate` accepted.
        let (extra, sweep, thorns) = if landed { (extra, sweep, thorns) } else { (0.0, false, [0; 4]) };
        // v1.0 gap Track 2g: the target's `lastHurtByMob`, which the foxes
        // that trust it defend against.
        if landed
            && let (Some(victim), Some(by)) = (self.player(&target).map(|s| s.id), self.player(attacker).map(|s| s.id))
        {
            self.mobs.entities.note_player_hurt_by_player(victim, by);
        }
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
                let push = (attacker_pos, yaw, knockback::SWEEP_STRENGTH as f32);
                self.hurt_player_ref(&other, swept, push, 0.0, attacker_id);
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
                // v1.0 gap Track 8f: "while trying to hurt" the wearer, by name.
                let owner = Some(crate::entity::trident::Owner::Player(target_id));
                let thorns = voxelcraft::simulation::damage::Cause::THORNS.by(owner);
                match attacker {
                    PlayerRef::Host => {
                        self.damage_player_armored(back, thorns);
                    }
                    PlayerRef::Agent(name) => {
                        if let Some(bot) = self.agents.players.get_mut(name) {
                            bot.agent.hurt(back, thorns, DVec3::ZERO, &mut self.mobs.entities);
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
                PlayerRef::Agent(_) => self.audio.play_in(
                    crate::audio::SoundSource::Players,
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

/// A guest's share of this tick's sounds, as `[key, x, y, z, gain, low pitch,
/// high pitch, source]`: the raid horns played for it, then the host's sounds
/// within 64 blocks. A horn is personal (`Raid.playSound` sends each player
/// its own, positioned for them), so the host's horn isn't forwarded and a
/// guest near both hears only its own (issue #98).
fn peer_sounds(
    tapped: &[crate::audio::Tapped],
    horns: &[(voxelcraft::entity::PlayerId, DVec3)],
    id: voxelcraft::entity::PlayerId,
    me: DVec3,
    host_pos: DVec3,
) -> Vec<Value> {
    use crate::audio::sounds::Sound;
    let horn = Sound::Raid(voxelcraft::entity::raid::Cue::Horn);
    let own = horns.iter().filter(|(player, _)| *player == id).map(|&(_, at)| {
        json!([horn.key(), at.x, at.y, at.z, 1.0, 1.0, 1.0, crate::audio::SoundSource::Neutral.index()])
    });
    let heard = tapped.iter().filter(|t| t.0 != horn).filter_map(|&(sound, pos, gain, (lo, hi), source)| {
        let movement = matches!(
            sound,
            Sound::Step(_)
                | Sound::Block(_, crate::audio::BlockCue::Step | crate::audio::BlockCue::Fall)
                | Sound::Land
                | Sound::Splash
                | Sound::Swim
        );
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
            .then(|| json!([sound.key(), pos.x, pos.y, pos.z, gain, lo, hi, source.index()]))
    });
    own.chain(heard).take(96).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Issue #98: a LAN guest hears the raid horn played for it, at its own
    /// position, once, whether or not the host heard one.
    #[test]
    fn raid_horns_reach_their_own_guest_once() {
        use crate::audio::{SoundSource, sounds::Sound};
        use voxelcraft::entity::PlayerId;
        let horn = Sound::Raid(voxelcraft::entity::raid::Cue::Horn);
        let heard = |sounds: Vec<Value>| -> Vec<DVec3> {
            sounds
                .iter()
                .filter(|s| s[0] == json!(horn.key()))
                .map(|s| DVec3::new(s[1].as_f64().unwrap(), s[2].as_f64().unwrap(), s[3].as_f64().unwrap()))
                .collect()
        };
        let (guest, other) = (PlayerId(2), PlayerId(3));
        let me = DVec3::new(0.5, 64.0, 0.5);
        let mine = DVec3::new(13.5, 64.0, 0.5);
        // The host is 150 blocks away and outside the raid: it played no horn.
        let far = DVec3::new(150.5, 64.0, 0.5);
        assert_eq!(heard(peer_sounds(&[], &[(guest, mine)], guest, me, far)), [mine]);
        // The host played its own horn beside the guest: the guest still
        // hears only its own, and another guest only theirs.
        let near = DVec3::new(4.5, 64.0, 0.5);
        let tapped = [(horn, Some(DVec3::new(17.5, 64.0, 0.5)), 1.0, (1.0, 1.0), SoundSource::Neutral)];
        let theirs = DVec3::new(0.5, 64.0, 13.5);
        let horns = [(guest, mine), (other, theirs)];
        assert_eq!(heard(peer_sounds(&tapped, &horns, guest, me, near)), [mine]);
        assert_eq!(heard(peer_sounds(&tapped, &horns, other, me, near)), [theirs]);
        // Other sounds still reach a guest in range.
        let hurt = [(Sound::Hurt, Some(near), 1.0, (1.0, 1.0), SoundSource::Neutral)];
        assert_eq!(peer_sounds(&hurt, &[], guest, me, near).len(), 1);
    }

    #[test]
    fn statue_prediction_matches_block_use_priority() {
        use voxelcraft::{
            entity::PlayerId,
            inventory::Stack,
            item::Item,
            player::EYE_HEIGHT,
            session::PlayerSession,
            world::{copper::Stage, copper_golem_statue as statue},
        };
        let p = IVec3::new(10, 140, 10);
        let mut world = World::new_remote(Arc::new(Generator::new(263)), 2);
        world.receive_chunk(IVec3::new(0, 4, 0), Arc::new(ChunkData::Uniform(Block::AIR)));
        world.set_block(p, statue::item_block(Stage::Unaffected, false));
        let eye = DVec3::new(10.5, 141., 13.);
        let direction = (p.as_dvec3() + DVec3::splat(0.5) - eye).normalize();
        let mut session = PlayerSession::new(PlayerId::HOST, eye - DVec3::Y * EYE_HEIGHT);
        session.player.yaw = direction.z.atan2(direction.x) as f32;
        session.player.pitch = direction.y.asin() as f32;
        session.vitals.hunger.food = 8.;
        let input = Input { buttons: 16, yaw: session.player.yaw, pitch: session.player.pitch, ..Input::default() };
        for item in [Item::APPLE, Item::HONEY_BOTTLE] {
            let mut lan = Lan::default();
            session.player.yaw = input.yaw;
            session.player.pitch = input.pitch;
            session.inventory.slots[0] = Some(Stack::new(item, 2));
            assert!(
                !lan.sample_use(&session, &world, &input, true).eating,
                "statue use must not predict eating slowdown"
            );
            assert!(lan.sample_use(&session, &world, &Input { buttons: 18, ..input }, true).eating);
            session.player.vehicle = Some(1);
            assert!(lan.sample_use(&session, &world, &input, true).eating, "riders skip statue block use");
            session.player.vehicle = None;
            world.set_block(p, Block::AIR);
            assert!(lan.sample_use(&session, &world, &input, true).eating);
            world.set_block(p, statue::item_block(Stage::Unaffected, false));
            assert!(lan.sample_use(&session, &world, &input, false).eating, "an existing bite continues");
            lan.predictions.push_back((input, LanUse { eating: true, drawing: false, bundle: false }));
            lan.predictions.clear();
            assert!(lan.sample_use(&session, &world, &input, false).eating, "acknowledging inputs preserves a bite");
            assert!(!lan.sample_use(&session, &world, &Input { buttons: 0, ..input }, false).eating);
            assert!(!lan.sample_use(&session, &world, &input, true).eating, "a new press uses the statue");
            lan.predicted_eating = Some(0);
            assert!(lan.sample_use(&session, &world, &input, false).eating);
            assert!(
                !lan.sample_use(&session, &world, &input, true).eating,
                "a release/repress between ticks starts anew"
            );
            lan.predicted_eating = Some(0);
            session.inventory.slots[1] = Some(Stack::new(item, 2));
            assert!(!lan.sample_use(&session, &world, &Input { selected: 1, ..input }, false).eating);
            lan.predicted_eating = None;
            assert!(
                !lan.sample_use(&session, &world, &input, false).eating,
                "host completion ends the bite at a statue"
            );
            let away = Input { yaw: 0., pitch: 0., ..input };
            session.player.yaw = away.yaw;
            session.player.pitch = away.pitch;
            assert!(lan.sample_use(&session, &world, &away, false).eating, "a newer look-away input can begin eating");
        }
        let mut agent = Agent::new(eye);
        agent.session.inventory.slots[0] = Some(Stack::new(Item::APPLE, 1));
        agent.session.vitals.hunger.food = 8.;
        agent.session.tick_consumption(0, true, &mut crate::entity::Entities::new(263)).unwrap();
        assert_eq!(state(&agent, "test")["consuming"], true);
        agent.session.cancel_consumption();
        assert_eq!(state(&agent, "test")["consuming"], false);
    }

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
    fn state_acknowledges_input_while_a_guest_carries_large_books() {
        use crate::inventory::Inventory;
        use crate::item::Item;
        for (page, count) in [("x".repeat(1024), 2), ("書".repeat(1024), 1)] {
            let mut original = Agent::new(DVec3::ZERO);
            let book = Stack::new(Item::WRITABLE_BOOK, 1).with_payload(Payload::WritableBook(vec![page; 100]));
            original.inventory.slots[..count].fill(Some(book));
            let mut old_snapshot = json!({"sequence": 1, "self": state(&original, "Guest")});
            old_snapshot["self"]["inventory"] = json!(original.inventory.serialize());
            assert!(fit_state(old_snapshot).0.is_none(), "inline inventory used to drop the snapshot");
            let mut sender = inventory::Sender::default();
            let mut receiver = inventory::Receiver::default();
            let mut grid = Grid::new(2);
            let mut sequence = 0;
            while let Some(part) = sender.next(&original.inventory, grid) {
                let frame = part.encode().unwrap();
                let Packet::InventoryPart { last, bytes } = Packet::decode(&frame[4..]).unwrap() else {
                    panic!("wrong packet");
                };
                receiver.push(last, &bytes).unwrap();
                if !last {
                    assert_eq!(receiver.current, Inventory::default(), "the update is atomic");
                }
                sequence += 1;
                let snapshot = json!({
                    "sequence": sequence,
                    "self": state(&original, "Guest"),
                    "players": [state_lite(&original, "Scribe", None, 0.0)],
                });
                let (snapshot, _) = fit_state(snapshot);
                let snapshot = snapshot.expect("tick state fits throughout inventory transfer");
                Packet::State(snapshot.clone()).encode().unwrap();
                assert_eq!(snapshot["sequence"], sequence);
                let rendered = snapshot["players"][0]["held"].as_str().and_then(stack_from_str).flatten().unwrap();
                assert_eq!(rendered.item, book.item);
                assert!(rendered.payload.is_none());
            }
            assert!(sequence > 1, "the inventory crossed multiple ticks");
            assert_eq!(receiver.current, original.inventory);
            original.inventory.slots[0] = None;
            original.inventory.cursor = Some(book);
            assert!(!sender.current(&original.inventory));
            let Some(Packet::InventoryPart { last: true, bytes }) = sender.next(&original.inventory, grid) else {
                panic!("moving a known book only needs one part");
            };
            assert!(bytes.len() < 512, "the book pages were not sent again");
            receiver.push(true, &bytes).unwrap();
            assert_eq!(receiver.current, original.inventory);
            assert_eq!(sender.revision, receiver.revision);
            grid.cells[0] = original.inventory.cursor.take();
            let Some(Packet::InventoryPart { last: true, bytes }) = sender.next(&original.inventory, grid) else {
                panic!("moving a known book into the crafting grid only needs one part");
            };
            receiver.push(true, &bytes).unwrap();
            assert_eq!(receiver.grid, Some(grid));
            assert_eq!(receiver.current, original.inventory);
        }
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
    /// v1.0 gap track 6d: another player's held stack keeps what its
    /// glint depends on (enchantments, a lodestone), so a LAN player's
    /// enchanted item glints in their hand on every screen.
    #[test]
    fn other_players_held_stacks_keep_their_glint() {
        let mut p = Agent::new(DVec3::ZERO);
        let item = crate::item::Item::from_name("diamond sword").expect("the diamond sword is registered");
        let mut sword = crate::inventory::Stack::new(item, 1);
        sword.enchants.set(crate::enchant::Enchantment::Sharpness, 3);
        assert!(sword.has_foil());
        p.inventory.slots[0] = Some(sword);
        p.inventory.offhand = Some(sword);
        let mut copy = Agent::new(DVec3::ZERO);
        apply(&mut copy, &state_lite(&p, "Remote", None, 0.0));
        assert_eq!(copy.inventory.get(0), Some(sword));
        assert!(copy.inventory.get(0).is_some_and(|s| s.has_foil()));
        assert!(copy.inventory.offhand.is_some_and(|s| s.has_foil()));
    }
    #[test]
    fn player_snapshot_preserves_progression_and_movement() {
        let mut original = Agent::new(DVec3::new(12.0, 151.0, -8.0));
        original.id = voxelcraft::entity::PlayerId(7);
        original.set_mode(GameMode::Creative);
        original.player.vel = DVec3::new(1.0, -2.0, 3.0);
        original.player.yaw = 1.25;
        original.player.pitch = -0.4;
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
        assert_eq!(restored.vitals.health, 13.0);
        assert_eq!(restored.vitals.hunger.food, 7.0);
        assert_eq!(restored.vitals.xp.level, 9);
        assert_eq!(restored.sleeping, Some(0.75));
    }
    fn sync_inventory(
        sender: &mut inventory::Sender,
        receiver: &mut inventory::Receiver,
        inventory: &crate::inventory::Inventory,
    ) {
        while let Some(Packet::InventoryPart { last, bytes }) = sender.next(inventory, Grid::new(2)) {
            receiver.push(last, &bytes).unwrap();
        }
    }

    /// Inventory updates carry the guest's stack contents. Equipment views
    /// preserve visible components such as armor trim.
    #[test]
    fn snapshots_carry_stack_components() {
        use crate::inventory::{
            Stack,
            components::{self, Payload, Trim, TrimMaterial, TrimPattern, WrittenBook},
        };
        use crate::item::{ArmorMaterial, ArmorPiece, Item};
        let helmet = Stack {
            trim: Some(Trim { pattern: TrimPattern::Eye, material: TrimMaterial::Lapis }),
            ..Stack::new(Item::armor(ArmorPiece::Helmet, ArmorMaterial::Diamond), 1)
        };
        let book = Stack::new(Item::BOOK, 1).with_payload(Payload::WrittenBook(WrittenBook {
            title: "Log".into(),
            author: "Host".into(),
            pages: vec!["day one".into(), "day two".into()],
            resolved: true,
            ..Default::default()
        }));
        let bundle = Stack::new(Item::PAPER, 1).with_payload(Payload::Bundle(vec![helmet, book]));
        let dyed = components::apply_dyes(Stack::new(Item::WOLF_ARMOR, 1), &[crate::color::DyeColor::Pink]).unwrap();
        let mut original = Agent::new(DVec3::new(1.0, 151.0, 1.0));
        original.selected = 1;
        original.inventory.slots[0] = Some(bundle);
        original.inventory.slots[1] = Some(book);
        original.inventory.slots[2] = Some(dyed);
        original.inventory.armor[0] = Some(helmet);
        original.inventory.cursor = Some(bundle);
        let mut receiver = inventory::Receiver::default();
        sync_inventory(&mut inventory::Sender::default(), &mut receiver, &original.inventory);
        assert_eq!(receiver.current, original.inventory);
        assert_eq!(receiver.current.get(0).unwrap().bundle_contents(), [helmet, book]);
        let frame = Packet::State(state_lite(&original, "Scribe", None, 0.0)).encode().unwrap();
        let Packet::State(lite) = Packet::decode(&frame[4..]).unwrap() else { panic!("wrong packet") };
        let mut copy = Agent::new(DVec3::ZERO);
        apply(&mut copy, &lite);
        assert_eq!(
            (copy.inventory.get(1), copy.inventory.armor[0]),
            (Some(Stack { payload: None, ..book }), Some(helmet))
        );
    }

    /// v0.18 Track 4: the spawn eggs come after every v0.17 palette entry,
    /// so an older palette code still means the same item; the End crystal,
    /// v0.17's last entry, keeps its code.
    #[test]
    fn legacy_palette_codes_keep_their_items() {
        let crystal = Stack::new(crate::entity::crystal::END_CRYSTAL, 1);
        assert_eq!(encode_slot(SlotRef::Palette(crystal)), Some(LEGACY_END_CRYSTAL_CODE));
        let decoded = decode_slot(LEGACY_END_CRYSTAL_CODE, Container::Inventory, GameMode::Creative, 2);
        assert_eq!(decoded, Some(SlotRef::Palette(crystal)));
        // The eggs (with v1.0 Track 7's Ender Dragon egg), then v1.0 Track
        // 5's trail sherds, all after the End crystal.
        let palette: Vec<_> = crate::item::Item::creative_palette().collect();
        let crystal_at = palette.iter().position(|&i| i == crate::entity::crystal::END_CRYSTAL).unwrap();
        let eggs = voxelcraft::content::spawn_egg::all().count();
        let sherds = usize::from(
            voxelcraft::world::archaeology::TRAIL_ITEM_END - voxelcraft::world::archaeology::TRAIL_ITEM_FIRST,
        );
        let after = &palette[crystal_at + 1..];
        assert_eq!(after.len(), eggs + sherds, "nothing else follows the End crystal");
        assert!(after[..eggs].iter().all(|&i| voxelcraft::content::spawn_egg::is_spawn_egg(i)));
    }
    /// 1000 + 1645: the End crystal was the last of v0.17's 1646 palette entries.
    const LEGACY_END_CRYSTAL_CODE: u16 = 2645;

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
        let slot = encode_slot(SlotRef::Palette(Stack::new(item, 1))).unwrap();
        assert!(
            matches!(decode_slot(slot,inventory,GameMode::Creative,2),Some(SlotRef::Palette(decoded)) if decoded==Stack::new(item, 1))
        );
    }
}
