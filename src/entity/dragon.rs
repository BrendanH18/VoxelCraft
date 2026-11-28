//! The End fight, after Java's `EndDragonFight`, `EnderDragon` and its
//! phases: End crystals burn on the pillars and heal the dragon, which
//! circles them, strafes players with fireballs, perches on the exit portal
//! to breathe fire, and on death rises in light, drops its experience and
//! opens the exit portal.
//!
//! The dragon runs on whole 20 Hz ticks with Java's per-tick numbers. Its
//! yaw uses Java's convention (degrees, heading `(sin, -cos)`), and like
//! Java it has no collision: it smashes through everything but the End's
//! own blocks.

use glam::{DVec3, IVec3};

use super::crystal::{Blow as CrystalBlow, Crystals, Destroyed};
use super::{Ctx, EntityEvent, MobSound, PlayerId, Rng, Target};
use crate::physics::{self, BlockSource};
use crate::world::block::Block;
use crate::world::end::{EndGen, Pillar};

pub mod gateway;
mod path;
mod wild;
pub use wild::{SHAPE as WILD_SHAPE, Wild, WildDragon};
#[cfg(test)]
mod track4c_tests;
#[cfg(test)]
mod track8_tests;
#[cfg(test)]
mod track8b_tests;

pub const MAX_HEALTH: f32 = 200.0;
/// Experience for the first kill, and for any later one.
pub const FIRST_XP: u32 = 12_000;
pub const LATER_XP: u32 = 500;
/// An End crystal's blast.
pub const CRYSTAL_POWER: f32 = super::crystal::POWER;
/// Players within this distance of the island centre see the boss bar
/// (Java's fight area).
pub const ARENA: f64 = 192.0;
const TICK: f64 = 0.05;
/// Ticks the death takes, rising and glowing.
pub const DEATH_TICKS: u32 = 200;
/// Where a new dragon appears (`EnderDragonFight.createNewDragon`:
/// `DRAGON_SPAWN_Y` 128 over the fight's origin).
pub const SPAWN: DVec3 = DVec3::new(0.0, 128.0, 0.0);
/// `DragonRespawnStage`'s beam and growl point, `(0, 128, 0)`.
pub const RESPAWN_BEAM: IVec3 = IVec3::new(0, 128, 0);
/// `EnderDragonFight.TIME_BETWEEN_CRYSTAL_SCANS`.
pub const CRYSTAL_SCAN_TICKS: u32 = 100;
/// `CRYSTAL_DESTROY_TARGETING`: a crystal destroyed by no player turns
/// the dragon on the nearest player this close.
pub const CRYSTAL_DESTROY_RANGE: f64 = 64.0;
/// `tryRespawn`: the crystals stand this far out from the portal's pillar.
pub const RESPAWN_EDGE: i32 = 3;
/// `PREPARING_TO_SUMMON_PILLARS` lasts this long (then moves on at this time).
pub const PREPARE_TICKS: u32 = 100;
/// `SUMMONING_PILLARS`: a pillar every 40 ticks, rebuilt at the 39th.
pub const PILLAR_TICKS: u32 = 40;
/// The block radius cleared around a pillar's cap, and its blast's power.
pub const PILLAR_CLEAR: i32 = 10;
pub const PILLAR_BLAST: f32 = 5.0;
/// v1.0 final F8: the uid a damage source names the Ender Dragon by (it
/// is no [`super::Mob`]; mob uids count up from 1 and never reach it).
/// [`super::Entities::attacker`] names it "Ender Dragon".
pub const DRAGON_UID: u32 = u32::MAX;
pub const DRAGON_OWNER: super::trident::Owner = super::trident::Owner::Mob(DRAGON_UID);

/// `SUMMONING_DRAGON`: growls from 80, the dragon at 100.
pub const SUMMON_TICKS: u32 = 100;
pub const SUMMON_GROWL_FROM: u32 = 80;
/// Crystals heal a dragon this close (`checkCrystals`' box grown by 32).
pub const HEAL_RANGE: f64 = 32.0;
/// `EntityType.ENDER_DRAGON`'s eye height: 0.85 of its 8 blocks.
pub const EYE_HEIGHT: f64 = 6.8;

/// One hit box of the dragon (Java's `EnderDragonPart`s).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Part {
    Head,
    Neck,
    Body,
    Tail(u8),
    Wing(u8),
}

impl Part {
    /// Width and height in blocks.
    fn size(self) -> (f64, f64) {
        match self {
            Part::Head => (1.0, 1.0),
            Part::Neck => (3.0, 3.0),
            Part::Body => (5.0, 3.0),
            Part::Tail(_) => (2.0, 2.0),
            Part::Wing(_) => (4.0, 2.0),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Phase {
    /// Circling the pillars.
    HoldingPattern,
    /// Heading for a player to shoot a fireball at.
    Strafe(PlayerId),
    /// Flying in toward the exit portal.
    LandingApproach,
    Landing,
    /// Perched, breathing a cloud of fire in front.
    SittingFlaming,
    /// Perched, looking around for someone close.
    SittingScanning,
    /// Perched, roaring at someone in front.
    SittingAttacking,
    Takeoff,
    /// Diving at where a player stood.
    Charging,
    Dying,
    /// Java's `HOVERING`, the phase a dragon no fight owns starts in: it
    /// holds where it appeared until hurt enough to take off. Java counts it
    /// as sitting, but arrows still hurt it (`DragonHoverPhase` keeps the
    /// default `onHurt`).
    Hovering,
}

impl Phase {
    pub fn sitting(self) -> bool {
        matches!(self, Phase::SittingFlaming | Phase::SittingScanning | Phase::SittingAttacking | Phase::Hovering)
    }

    /// `AbstractDragonSittingPhase.onHurt`: perched dragons shrug off arrows.
    pub fn deflects_arrows(self) -> bool {
        matches!(self, Phase::SittingFlaming | Phase::SittingScanning | Phase::SittingAttacking)
    }
}

pub struct Dragon {
    pub pos: DVec3,
    pub previous_pos: DVec3,
    /// Blocks per tick.
    vel: DVec3,
    /// Degrees, Java's convention: the dragon faces `(sin, -cos)`.
    pub yaw: f32,
    pub previous_yaw: f32,
    /// Java's `yRotA`: turning speed.
    yaw_speed: f32,
    pub health: f32,
    pub phase: Phase,
    /// Ticks in this phase.
    phase_ticks: u32,
    target: Option<DVec3>,
    /// The phase's `currentPath`: `None` while it has none (Java's
    /// `null`), else the waypoints still ahead, next last; empty once done.
    path: Option<Vec<DVec3>>,
    /// `DragonHoldingPatternPhase.clockwise`.
    clockwise: bool,
    /// `DragonStrafePlayerPhase.holdingPatternClockwise`.
    strafe_clockwise: bool,
    /// Where the strafed player was last seen: Java's `attackTarget` is
    /// the player entity itself, kept until the strafe ends (in creative,
    /// dead or gone, it is still the target).
    strafe_at: Option<DVec3>,
    /// Its 24 path nodes ([`path::nodes`]), kept once every column was
    /// loaded (`nodes_ready`); Java computes them on the first
    /// `findClosestNode`.
    nodes: [DVec3; 24],
    nodes_ready: bool,
    /// Ticks spent aiming at the strafed player.
    fireball_charge: u32,
    /// Flame bursts since landing (it takes off after four).
    flame_count: u32,
    sitting_damage: f32,
    /// The crystal healing it (Java's `nearestCrystal`, drawn as a beam).
    pub healer: Option<u32>,
    /// Seconds of the red hurt flash left.
    pub hurt: f32,
    /// Damage taken during the current immunity window (Java's
    /// `invulnerableTime`): only harder hits do the difference.
    immune: (u32, f32),
    pub death_ticks: u32,
    /// Wing beat, in turns; `previous_flap` for interpolation.
    pub flap: f32,
    pub previous_flap: f32,
    /// Yaw and height over the last 64 ticks, newest at `history_at`.
    history: [(f32, f64); 64],
    history_at: usize,
    /// Hit boxes, refreshed each tick.
    parts: [(Part, DVec3, DVec3); 8],
    /// Ticks until the next growl.
    growl: u32,
    /// `inWall`: the last `checkWalls` found a block it can't break.
    in_wall: bool,
    /// `tickCount`: ticks since it was made or loaded.
    ticks: u32,
}

/// A dragon fireball (Java's `DragonFireball`): speeds up like a blaze's and
/// bursts into a lingering cloud of breath.
pub struct DragonFireball {
    pub pos: DVec3,
    pub previous_pos: DVec3,
    /// Blocks per tick.
    vel: DVec3,
    dir: DVec3,
    age: u32,
}

/// A spreading cloud of dragon's breath (Java's `AreaEffectCloud` with
/// instant damage, applied at half strength).
pub struct BreathCloud {
    pub pos: DVec3,
    pub radius: f32,
    grow: f32,
    pub age: u32,
    pub duration: u32,
    damage: f32,
    /// Ticks until each player in it can be hurt again.
    cooldowns: Vec<(PlayerId, u32)>,
}

/// `DragonRespawnStage`, in order (the respawn ritual).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Stage {
    Start,
    PreparingToSummonPillars,
    SummoningPillars,
    SummoningDragon,
}

impl Stage {
    /// The saved name (`getSerializedName`).
    pub fn name(self) -> &'static str {
        match self {
            Stage::Start => "start",
            Stage::PreparingToSummonPillars => "preparing_to_summon_pillars",
            Stage::SummoningPillars => "summoning_pillars",
            Stage::SummoningDragon => "summoning_dragon",
        }
    }

    fn named(name: &str) -> Option<Self> {
        [Stage::Start, Stage::PreparingToSummonPillars, Stage::SummoningPillars, Stage::SummoningDragon]
            .into_iter()
            .find(|s| s.name() == name)
    }
}

/// A respawn ritual under way (`respawnStage`, `respawnTime`,
/// `respawnCrystals`).
#[derive(Clone, Debug, PartialEq)]
pub struct Respawn {
    pub stage: Stage,
    /// Ticks in this stage.
    pub time: u32,
    /// The crystals on the exit portal's edges that started it.
    pub crystals: Vec<u32>,
}

/// The fight in one End.
pub struct Fight {
    pub dragon: Option<Dragon>,
    pub fireballs: Vec<DragonFireball>,
    pub clouds: Vec<BreathCloud>,
    /// The dragon of this End has died at least once.
    pub previously_killed: bool,
    /// The exit portal's centre.
    pub podium: IVec3,
    /// The ten obsidian pillars (Java's spikes).
    pillars: [Pillar; 10],
    /// `aliveCrystals`: crystals on the pillars at the last scan.
    alive_crystals: u32,
    /// Ticks since that scan (`ticksSinceCrystalsScanned`).
    crystal_scan: u32,
    /// The respawn ritual, while one runs.
    pub respawn: Option<Respawn>,
    /// A saved ritual whose crystals (saved by position) are matched to
    /// the loaded ones on the next tick.
    respawn_saved: Option<(Stage, u32, Vec<DVec3>)>,
    /// A crystal was placed in this End: `tryRespawn` once the podium is loaded.
    respawn_check: bool,
    /// The pillar crystals an older save recorded as a mask, before
    /// crystals were saved as entities (`None` once they are).
    pub legacy_crystals: Option<u32>,
    /// Game time not yet run as whole ticks.
    pending: f64,
    /// The dragon died but the exit portal hasn't opened yet: part of it
    /// isn't loaded.
    unopened: bool,
    /// Gateway spots in the order kills open them.
    gateway_order: [IVec3; 20],
    /// How many gateways kills have opened.
    gateways: usize,
    /// Indices into `gateway_order` waiting for their chunk to load.
    unbuilt: Vec<usize>,
    /// Gateway pairs that lead to each other.
    links: Vec<(IVec3, IVec3)>,
    /// v1.0 gap Track 4: the gateways' block entities (age and cooldown).
    pub gateway_entities: gateway::Gateways,
    /// The `mobGriefing` game rule is off (the dragon breaks nothing and
    /// every block in its way slows it); set each tick by the entities.
    pub griefing_off: bool,
}

impl Fight {
    /// What a LAN client needs to draw the fight and its boss bar.
    pub fn replica_json(&self) -> serde_json::Value {
        use serde_json::json;
        let v3 = |p: DVec3| [p.x, p.y, p.z];
        json!({
            "podium": [self.podium.x, self.podium.y, self.podium.z],
            "dragon": self.dragon.as_ref().map(dragon_replica_json),
            "fireballs": self.fireballs.iter().map(|f| json!([v3(f.pos), v3(f.previous_pos), v3(f.dir)])).collect::<Vec<_>>(),
            "gateways": self.gateway_entities.replica_json(),
        })
    }

    /// A LAN client's fight: built from [`Fight::replica_json`] and never simulated.
    pub fn replica(v: &serde_json::Value) -> Option<Self> {
        let podium = v["podium"].as_array()?;
        let podium = IVec3::new(
            podium.first()?.as_i64()? as i32,
            podium.get(1)?.as_i64()? as i32,
            podium.get(2)?.as_i64()? as i32,
        );
        let mut fight = Self {
            dragon: None,
            fireballs: Vec::new(),
            clouds: Vec::new(),
            previously_killed: false,
            podium,
            pillars: [Pillar { x: 0, z: 0, radius: 0, top: 0, guarded: false, java: false }; 10],
            alive_crystals: 0,
            crystal_scan: 0,
            respawn: None,
            respawn_saved: None,
            respawn_check: false,
            legacy_crystals: None,
            pending: 0.0,
            unopened: false,
            gateway_order: [IVec3::ZERO; 20],
            gateways: 0,
            unbuilt: Vec::new(),
            links: Vec::new(),
            gateway_entities: Default::default(),
            griefing_off: false,
        };
        fight.apply_replica(v);
        Some(fight)
    }

    /// Updates a client fight from the host's latest [`Fight::replica_json`].
    pub fn apply_replica(&mut self, v: &serde_json::Value) {
        let d = &v["dragon"];
        let pos = d["pos"].as_array().is_some();
        let dragon = self.dragon.get_or_insert_with(|| Dragon::new(replica_v3(&d["pos"]).unwrap_or_default()));
        if !pos || !apply_dragon_replica(dragon, d) {
            self.dragon = None;
        }
        self.fireballs = fireballs_from_replica(&v["fireballs"]);
        self.gateway_entities.apply_replica(&v["gateways"]);
    }

    /// A fresh fight: a dragon, and (as [`Fight::legacy_crystals`]) a
    /// crystal for every pillar, which the caller spawns.
    pub fn new(end: &EndGen) -> Self {
        let mut fight = Self::empty(end);
        fight.dragon = Some(Dragon::new(SPAWN));
        fight.legacy_crystals = Some((1 << 10) - 1);
        fight
    }

    /// Spawns the pillar crystals [`Fight::legacy_crystals`] lists (a new
    /// fight's, or an older save's), standing on the bedrock caps with the
    /// bottom plate showing.
    pub fn spawn_pillar_crystals(&mut self, crystals: &mut Crystals) {
        let Some(mask) = self.legacy_crystals.take() else { return };
        for (i, p) in self.pillars.iter().enumerate() {
            if mask & (1 << i) != 0 {
                crystals.spawn(p.crystal(), None, true, false);
            }
        }
        self.scan_crystals(crystals);
    }

    /// After a load: the pillar crystals of an older save (or a new End)
    /// spawn unless the crystals were `saved` as entities, then the
    /// pillars are counted.
    pub fn restore_crystals(&mut self, crystals: &mut Crystals, saved: bool) {
        if saved {
            self.legacy_crystals = None;
        }
        self.spawn_pillar_crystals(crystals);
        self.scan_crystals(crystals);
    }

    /// The pillars, in Java's spike order.
    pub fn pillars(&self) -> &[Pillar; 10] {
        &self.pillars
    }

    fn empty(end: &EndGen) -> Self {
        Self {
            dragon: None,
            fireballs: Vec::new(),
            clouds: Vec::new(),
            previously_killed: false,
            podium: end.podium(),
            pillars: *end.pillars(),
            alive_crystals: 0,
            crystal_scan: 0,
            respawn: None,
            respawn_saved: None,
            respawn_check: false,
            legacy_crystals: None,
            pending: 0.0,
            unopened: false,
            gateway_order: end.gateways(),
            gateways: 0,
            unbuilt: Vec::new(),
            links: Vec::new(),
            gateway_entities: Default::default(),
            griefing_off: false,
        }
    }

    /// Restores a saved fight (see [`Fight::serialize`]); a missing or
    /// unreadable one starts fresh.
    pub fn load(end: &EndGen, text: Option<&str>) -> Self {
        let Some(text) = text else { return Self::new(end) };
        let mut sections = text.split(';');
        let fields: Vec<&str> = sections.next().unwrap_or_default().split(',').collect();
        let (alive, previously, health, mask) = match fields[..] {
            [a, p, h, m, ..] => (a, p, h, m),
            _ => return Self::new(end),
        };
        let mut fight = Self::empty(end);
        fight.gateways = fields.get(4).and_then(|g| g.parse().ok()).unwrap_or(0usize).min(20);
        let unbuilt: u32 = fields.get(5).and_then(|g| g.parse().ok()).unwrap_or(0);
        fight.unbuilt = (0..20).filter(|i| unbuilt & (1 << i) != 0).collect();
        let sections: Vec<&str> = sections.collect();
        fight.respawn_check = sections.contains(&"check_respawn");
        // The respawn ritual: `r stage time` then each crystal's feet.
        fight.respawn_saved = sections.iter().find_map(|r| {
            let mut f = r.strip_prefix("r ")?.split(' ');
            let stage = Stage::named(f.next()?)?;
            let time = f.next()?.parse().ok()?;
            let at = f
                .filter_map(|p| {
                    let v: Vec<f64> = p.split(',').filter_map(|v| v.parse().ok()).collect();
                    match v[..] {
                        [x, y, z] => Some(DVec3::new(x, y, z)),
                        _ => None,
                    }
                })
                .collect();
            Some((stage, time, at))
        });
        fight.links = sections
            .iter()
            .filter_map(|link| {
                let n: Vec<i32> = link.split(' ').filter_map(|v| v.parse().ok()).collect();
                let [a, b, c, d, e, f] = n[..] else { return None };
                Some((IVec3::new(a, b, c), IVec3::new(d, e, f)))
            })
            .collect();
        for s in &sections {
            fight.gateway_entities.load_section(s);
        }
        // Saves from before gateways kept their age: the gateways kills
        // opened and the exits built for them, past their spawn beams.
        let opened: Vec<IVec3> =
            (0..fight.gateways).filter(|i| !fight.unbuilt.contains(i)).map(|i| fight.gateway_order[i]).collect();
        for p in opened.into_iter().chain(fight.links.iter().flat_map(|&(a, b)| [a, b])) {
            fight.gateway_entities.known(p);
        }
        fight.previously_killed = previously == "1";
        fight.unopened = alive == "3";
        match alive {
            "1" => {
                let mut dragon = Dragon::new(SPAWN);
                dragon.health = health.parse::<f32>().unwrap_or(MAX_HEALTH).clamp(1.0, MAX_HEALTH);
                // Saved in the death flight: `DragonDeathPhase` begins again.
                if sections.contains(&"death_flight") {
                    dragon.phase = Phase::Dying;
                    dragon.health = 1.0;
                }
                fight.dragon = Some(dragon);
            }
            // Saved mid-death: finish dying (and paying out) over the portal.
            "2" => {
                let ticks = health.parse::<u32>().unwrap_or(0).min(DEATH_TICKS - 1);
                let mut dragon = Dragon::new(fight.podium.as_dvec3() + DVec3::new(0.5, 20.0 + ticks as f64 * 0.1, 0.5));
                dragon.health = 0.0;
                dragon.phase = Phase::Dying;
                dragon.death_ticks = ticks;
                fight.dragon = Some(dragon);
            }
            _ => {}
        }
        // Saves from before v0.16 Track G kept the pillar crystals as a
        // mask; newer ones write `-` and save crystals as entities.
        fight.legacy_crystals = mask.parse().ok();
        fight
    }

    /// `state,previously_killed,health,-,gateways,unbuilt mask` then
    /// `;a b c d e f` per gateway link, `;check_respawn` for a pending
    /// ritual check, `;death_flight` while the dying dragon flies to the
    /// portal, and `;r stage time x,y,z ...` during the ritual.
    /// The coordinates are the crystals' feet. The state is 0
    /// for dead, 1 alive, 2 dying (health holds the death's ticks so far)
    /// and 3 dead with the exit portal still to open. The fourth field was
    /// the pillar crystal mask before crystals were saved as entities.
    pub fn serialize(&self, crystals: &Crystals) -> String {
        let (state, health) = match &self.dragon {
            Some(d) if d.health <= 0.0 => (2, d.death_ticks as f32),
            Some(d) => (1, d.health),
            None if self.unopened => (3, 0.0),
            None => (0, 0.0),
        };
        let unbuilt: u32 = self.unbuilt.iter().map(|i| 1 << i).sum();
        let mask = self.legacy_crystals.map_or("-".to_string(), |m| m.to_string());
        let mut out = format!("{state},{},{health},{mask},{},{unbuilt}", self.previously_killed as u8, self.gateways);
        for (a, b) in &self.links {
            out += &format!(";{} {} {} {} {} {}", a.x, a.y, a.z, b.x, b.y, b.z);
        }
        out += &self.gateway_entities.serialize();
        if self.respawn_check {
            out += ";check_respawn";
        }
        // The death flight (`DYING` at one health): older builds read the
        // section as nothing and load a one-health dragon.
        if self.dragon.as_ref().is_some_and(|d| d.phase == Phase::Dying && d.health > 0.0) {
            out += ";death_flight";
        }
        let saved = self.respawn.as_ref().map(|r| {
            let at: Vec<DVec3> = r.crystals.iter().filter_map(|&id| crystals.get(id)).map(|c| c.pos).collect();
            (r.stage, r.time, at)
        });
        if let Some((stage, time, at)) = saved.or_else(|| self.respawn_saved.clone()) {
            out += &format!(";r {} {time}", stage.name());
            for p in at {
                out += &format!(" {},{},{}", p.x, p.y, p.z);
            }
        }
        out
    }

    /// The dragon's death is over: open the exit portal and a new gateway.
    fn killed(&mut self, events: &mut Vec<EntityEvent>) {
        events.push(EntityEvent::DragonKilled { first: !self.previously_killed });
        self.previously_killed = true;
        if self.gateways < self.gateway_order.len() {
            self.unbuilt.push(self.gateways);
            self.gateways += 1;
        }
    }

    /// Where the gateway at `cell` leads, once someone has used it.
    pub fn gateway_exit(&self, cell: IVec3) -> Option<IVec3> {
        self.links.iter().find_map(|&(a, b)| {
            if a == cell {
                Some(b)
            } else if b == cell {
                Some(a)
            } else {
                None
            }
        })
    }

    /// Joins two gateways (an entrance and the exit built for it).
    pub fn link_gateways(&mut self, a: IVec3, b: IVec3) {
        if self.gateway_exit(a).is_none() {
            self.links.push((a, b));
        }
        // The exit built for a gateway is a new block entity.
        self.gateway_entities.placed(b);
    }

    /// How far into the next tick the fight is, for drawing.
    pub fn partial_tick(&self) -> f32 {
        (self.pending / TICK).clamp(0.0, 1.0) as f32
    }

    /// The boss bar: the dragon's health fraction, if a living dragon is
    /// near `player`.
    pub fn boss_bar(&self, player: DVec3) -> Option<f32> {
        let d = self.dragon.as_ref()?;
        (player.with_y(0.0).length() < ARENA).then_some(d.health.max(0.0) / MAX_HEALTH)
    }

    /// Advances the fight, and the End's crystals with it, by `dt` game
    /// seconds, with no other dragons about (the tests' fights).
    #[cfg(test)]
    #[allow(clippy::too_many_arguments)]
    pub(super) fn update<W: BlockSource + ?Sized>(
        &mut self,
        dt: f64,
        world: &W,
        ctx: &Ctx,
        rng: &mut Rng,
        crystals: &mut Crystals,
        events: &mut Vec<EntityEvent>,
    ) {
        self.update_beside(dt, world, ctx, rng, crystals, &Wild::default(), events);
    }

    /// [`Fight::update`] beside the dragons no fight owns, whose parts the
    /// fight's fireballs meet too (v1.0 gap Track 8).
    #[allow(clippy::too_many_arguments)]
    pub(super) fn update_beside<W: BlockSource + ?Sized>(
        &mut self,
        dt: f64,
        world: &W,
        ctx: &Ctx,
        rng: &mut Rng,
        crystals: &mut Crystals,
        wild: &Wild,
        events: &mut Vec<EntityEvent>,
    ) {
        self.pending += dt;
        while self.pending >= TICK - 1e-9 {
            self.pending -= TICK;
            self.restore_ritual(crystals);
            crystals.tick(world, true, events);
            self.tick(world, ctx, rng, crystals, wild, events);
        }
    }

    /// `EndCrystalItem.useOn` placed a crystal in this End: queue a ritual check.
    pub fn crystal_placed(&mut self) {
        self.respawn_check = true;
    }

    #[allow(clippy::too_many_arguments)]
    fn tick<W: BlockSource + ?Sized>(
        &mut self,
        world: &W,
        ctx: &Ctx,
        rng: &mut Rng,
        crystals: &mut Crystals,
        wild: &Wild,
        events: &mut Vec<EntityEvent>,
    ) {
        self.settle_destroyed(ctx, crystals, events);
        self.crystal_scan += 1;
        if self.crystal_scan >= CRYSTAL_SCAN_TICKS {
            self.scan_crystals(crystals);
        }
        if self.respawn_check {
            self.try_respawn(world, crystals, events);
        }
        // `EnderDragonFight.tick` runs only while players are in the
        // fight's area and the arena is loaded.
        let watched =
            ctx.players.iter().any(|t| t.alive && t.pos.distance_squared(RESPAWN_BEAM.as_dvec3()) < ARENA * ARENA);
        if self.respawn.is_some() && watched && self.arena_loaded(world) {
            self.respawn_tick(rng, crystals, events);
        }
        self.gateway_entities.tick(world);
        let order = self.gateway_order;
        let gateways = &mut self.gateway_entities;
        self.unbuilt.retain(|&i| {
            let ready = loaded(world, order[i], IVec3::new(1, 2, 1), -2..=2);
            if ready {
                events.push(EntityEvent::BuildGateway { pos: order[i] });
                gateways.placed(order[i]);
            }
            !ready
        });
        if let Some(mut dragon) = self.dragon.take() {
            let mut arena = Arena {
                fight: true,
                alive_crystals: self.alive_crystals,
                previously_killed: self.previously_killed,
                podium: self.podium,
                fireballs: &mut self.fireballs,
                clouds: &mut self.clouds,
                top: crate::world::terrain::Dimension::End.max_y(),
                floor: crate::world::terrain::Dimension::End.min_y(),
                griefing: !self.griefing_off,
            };
            let keep = dragon.tick(&mut arena, world, ctx, rng, crystals, events);
            if keep {
                self.dragon = Some(dragon);
            } else {
                self.unopened = true;
            }
        }
        // The exit portal (and egg) opens once all of it is loaded.
        if self.unopened && loaded(world, self.podium, IVec3::new(4, 0, 4), -1..=4) {
            self.unopened = false;
            self.killed(events);
        }
        let mut fireballs = std::mem::take(&mut self.fireballs);
        if !fireballs.is_empty() {
            // Every dragon's parts are pickable (v1.0 gap Track 8).
            let boxes = Boxes::new(Some(self), wild, crystals);
            fireballs.retain_mut(|f| f.tick(world, ctx, &boxes, &mut self.clouds));
        }
        self.fireballs = fireballs;
        self.clouds.retain_mut(|c| c.tick(ctx, events));
    }

    /// `updateCrystalCount`: the crystals in the pillars' columns.
    fn scan_crystals(&mut self, crystals: &Crystals) {
        self.crystal_scan = 0;
        self.alive_crystals = self
            .pillars
            .iter()
            .map(|p| {
                let r = f64::from(p.radius);
                let min = DVec3::new(f64::from(p.x) - r, -1.0e7, f64::from(p.z) - r);
                let max = DVec3::new(f64::from(p.x) + r, 1.0e7, f64::from(p.z) + r);
                crystals.touching(min, max).count() as u32
            })
            .sum();
    }

    /// Crystals on the pillars at the last scan (`aliveCrystals`).
    pub fn alive_crystals(&self) -> u32 {
        self.alive_crystals
    }

    /// A ritual saved mid-way finds its crystals by position, before any
    /// hit on one of them is resolved (so the hit still ends the ritual).
    pub(super) fn restore_ritual(&mut self, crystals: &Crystals) {
        if let Some((stage, time, at)) = self.respawn_saved.take() {
            let ids = at
                .iter()
                .filter_map(|p| crystals.list.iter().find(|c| c.pos.distance_squared(*p) < 1e-6).map(|c| c.id))
                .collect();
            self.respawn = Some(Respawn { stage, time, crystals: ids });
        }
    }

    /// [`Fight::crystal_destroyed`] for each crystal gone since it last ran.
    pub(super) fn settle_destroyed(&mut self, ctx: &Ctx, crystals: &mut Crystals, events: &mut Vec<EntityEvent>) {
        for d in std::mem::take(&mut crystals.destroyed) {
            self.crystal_destroyed(d, ctx, crystals, events);
        }
    }

    /// `onCrystalDestroyed`: a ritual crystal ends the ritual; any other
    /// recounts the pillars, hurts the dragon if it was healing it, and
    /// turns a circling dragon on the player behind it (or the nearest
    /// within 64).
    fn crystal_destroyed(&mut self, d: Destroyed, ctx: &Ctx, crystals: &mut Crystals, events: &mut Vec<EntityEvent>) {
        if self.respawn.as_ref().is_some_and(|r| r.crystals.contains(&d.id)) {
            self.abort_respawn(crystals, events);
            return;
        }
        self.scan_crystals(crystals);
        let Some(dragon) = &mut self.dragon else { return };
        let player = match d.by {
            Some(id) => ctx.players.iter().find(|t| t.id == id).copied(),
            None => ctx
                .players
                .iter()
                .filter(|t| {
                    t.targetable && t.pos.distance_squared(d.pos) <= CRYSTAL_DESTROY_RANGE * CRYSTAL_DESTROY_RANGE
                })
                .min_by(|a, b| a.pos.distance_squared(d.pos).total_cmp(&b.pos.distance_squared(d.pos)))
                .copied(),
        };
        if dragon.healer == Some(d.id) {
            dragon.healer = None;
            dragon.hurt_by(Part::Head, 10.0, true);
        }
        // `DragonHoldingPatternPhase.onCrystalDestroyed`: only a player the
        // dragon `canAttack` (not in creative or spectator, not dead).
        if let (Phase::HoldingPattern, Some(player)) = (dragon.phase, player.filter(|t| t.targetable)) {
            dragon.strafe(&player);
        }
    }

    /// The dragon is dead and no ritual runs (`dragonKilled`, no `respawnStage`).
    pub fn dragon_dead(&self) -> bool {
        self.dragon.is_none() && !self.unopened && self.previously_killed
    }

    /// `tryRespawn`: a crystal on each of the exit portal's four edges (in
    /// the cells three out from the pillar, one above the rim) starts the
    /// ritual with every crystal found there.
    fn try_respawn<W: BlockSource + ?Sized>(&mut self, world: &W, crystals: &Crystals, events: &mut Vec<EntityEvent>) {
        if !self.dragon_dead() || self.respawn.is_some() {
            self.respawn_check = false;
            return;
        }
        // A reload can tick before the portal's chunks are available to close it.
        if !loaded(world, self.podium, IVec3::new(4, 0, 4), -1..=3) {
            return;
        }
        self.respawn_check = false;
        let center = self.podium + IVec3::Y;
        let mut found = Vec::new();
        // `Direction.Plane.HORIZONTAL`: north, east, south, west.
        for d in [IVec3::NEG_Z, IVec3::X, IVec3::Z, IVec3::NEG_X] {
            let cell = (center + d * RESPAWN_EDGE).as_dvec3();
            let here: Vec<u32> = crystals.touching(cell, cell + DVec3::ONE).map(|c| c.id).collect();
            if here.is_empty() {
                return;
            }
            found.extend(here);
        }
        // `respawnDragon`: the exit portal goes out (its bedrock and portal
        // blocks to end stone, then the shut podium again).
        events.push(EntityEvent::ExitPortal { active: false });
        self.respawn = Some(Respawn { stage: Stage::Start, time: 0, crystals: found });
    }

    /// `abortRespawnSequence`: the pillar crystals lose their beams and
    /// invulnerability and the exit portal opens again.
    fn abort_respawn(&mut self, crystals: &mut Crystals, events: &mut Vec<EntityEvent>) {
        self.respawn = None;
        self.reset_spike_crystals(crystals);
        events.push(EntityEvent::ExitPortal { active: true });
    }

    /// `resetSpikeCrystals`.
    fn reset_spike_crystals(&self, crystals: &mut Crystals) {
        for p in &self.pillars {
            let r = f64::from(p.radius);
            let min = DVec3::new(f64::from(p.x) - r, -1.0e7, f64::from(p.z) - r);
            let max = DVec3::new(f64::from(p.x) + r, 1.0e7, f64::from(p.z) + r);
            let ids: Vec<u32> = crystals.touching(min, max).map(|c| c.id).collect();
            for id in ids {
                if let Some(c) = crystals.get_mut(id) {
                    c.invulnerable = false;
                    c.beam = None;
                }
            }
        }
    }

    /// Whether the pillars and exit portal the ritual works on are loaded.
    fn arena_loaded<W: BlockSource + ?Sized>(&self, world: &W) -> bool {
        world.block(self.podium).is_some()
            && self.pillars.iter().all(|p| {
                let r = p.radius.max(2) + 1;
                loaded(world, IVec3::new(p.x, p.top, p.z), IVec3::new(r, 0, r), 0..=0)
            })
    }

    /// One tick of the ritual (`DragonRespawnStage.tick` with
    /// `respawnTime++`).
    fn respawn_tick(&mut self, rng: &mut Rng, crystals: &mut Crystals, events: &mut Vec<EntityEvent>) {
        let Some(r) = &mut self.respawn else { return };
        r.crystals.retain(|&id| crystals.get(id).is_some());
        if r.crystals.is_empty() {
            self.abort_respawn(crystals, events);
            return;
        }
        let time = r.time;
        r.time += 1;
        let ids = r.crystals.clone();
        let growl = |events: &mut Vec<EntityEvent>| {
            // Level event 3001: the dragon's growl at (0, 128, 0), heard
            // across the island (volume 64).
            events.push(EntityEvent::Sound { sound: MobSound::DragonGrowl, pos: RESPAWN_BEAM.as_dvec3() });
        };
        let beam = |crystals: &mut Crystals, to: Option<IVec3>| {
            for &id in &ids {
                if let Some(c) = crystals.get_mut(id) {
                    c.beam = to;
                }
            }
        };
        match r.stage {
            Stage::Start => {
                beam(crystals, Some(RESPAWN_BEAM));
                self.set_stage(Stage::PreparingToSummonPillars);
            }
            Stage::PreparingToSummonPillars => {
                if time < PREPARE_TICKS {
                    if time == 0 || (50..=52).contains(&time) || time >= 95 {
                        growl(events);
                    }
                } else {
                    self.set_stage(Stage::SummoningPillars);
                }
            }
            Stage::SummoningPillars => {
                let (start, end) = (time % PILLAR_TICKS == 0, time % PILLAR_TICKS == PILLAR_TICKS - 1);
                if start || end {
                    let index = (time / PILLAR_TICKS) as usize;
                    if let Some(p) = self.pillars.get(index) {
                        if start {
                            beam(crystals, Some(IVec3::new(p.x, p.top + 1, p.z)));
                        } else {
                            events.push(EntityEvent::RespawnPillar { index });
                        }
                    } else if start {
                        self.set_stage(Stage::SummoningDragon);
                    }
                }
            }
            Stage::SummoningDragon => {
                if time >= SUMMON_TICKS {
                    // `setRespawnStage(END)`: the new dragon, at a random heading.
                    self.respawn = None;
                    let mut dragon = Dragon::new(SPAWN);
                    dragon.yaw = rng.next_f32() * 360.0;
                    dragon.previous_yaw = dragon.yaw;
                    self.dragon = Some(dragon);
                    self.reset_spike_crystals(crystals);
                    for &id in &ids {
                        let Some(c) = crystals.get(id) else { continue };
                        // `level.explode(crystal, .., 6.0F, NONE)`: no blocks break.
                        events.push(EntityEvent::QuietBlast { center: c.pos, power: CRYSTAL_POWER });
                        crystals.discard(id);
                    }
                } else if time >= SUMMON_GROWL_FROM {
                    growl(events);
                } else if time == 0 {
                    beam(crystals, Some(RESPAWN_BEAM));
                } else if time < 5 {
                    growl(events);
                }
            }
        }
    }

    /// `setRespawnStage`: the next stage starts at time 0.
    fn set_stage(&mut self, stage: Stage) {
        if let Some(r) = &mut self.respawn {
            r.stage = stage;
            r.time = 0;
        }
    }

    /// The dragon or its part a ray hits within `max_dist`. In `DYING`
    /// (the flight to the portal and the death that follows) the parts
    /// are still there to hit: Java's `EnderDragonPart` stays pickable and
    /// alive until the dragon is removed, and `EnderDragon.hurt` refuses
    /// the hit, so an arrow bounces off (`ProjectileDeflection.REVERSE`)
    /// rather than flying through.
    pub fn raycast(&self, origin: DVec3, dir: DVec3, max_dist: f64) -> Option<(Hit, f64)> {
        let parts = self.dragon.iter().flat_map(|d| {
            d.parts.iter().filter_map(|&(part, min, max)| {
                physics::ray_aabb(origin, dir, min, max).map(|t| (Hit::Dragon(part), t))
            })
        });
        parts.filter(|&(_, t)| t <= max_dist).min_by(|a, b| a.1.total_cmp(&b.1))
    }

    /// The dragon part `p` is inside of.
    pub fn hit_at(&self, p: DVec3) -> Option<Hit> {
        let inside = |(min, max): (DVec3, DVec3)| p.cmpge(min).all() && p.cmple(max).all();
        let d = self.dragon.as_ref()?;
        d.parts.iter().find(|&&(_, min, max)| inside((min, max))).map(|&(part, ..)| Hit::Dragon(part))
    }

    /// Applies a hit on the dragon; returns whether it landed
    /// ([`Dragon::hurt`]). Arrows bounce off a perched dragon.
    pub fn strike(&mut self, hit: Hit, blow: Blow) -> bool {
        match hit {
            Hit::Crystal(_) | Hit::Wild(..) => false,
            Hit::Dragon(part) => self.dragon.as_mut().is_some_and(|d| d.hurt(part, blow)),
        }
    }

    /// A blast at `center` ([`Dragon::blast`]).
    pub fn explode<W: BlockSource + ?Sized>(&mut self, world: &W, center: DVec3, power: f32) {
        if let Some(d) = &mut self.dragon {
            d.blast(world, center, power);
        }
    }

    /// A wind burst's push on the dragon ([`Dragon::push_from_blast`]).
    pub fn wind_burst<W: BlockSource + ?Sized>(&mut self, world: &W, burst: &super::wind::WindBurst) {
        if let Some(d) = &mut self.dragon {
            d.push_from_blast(world, burst.center, burst.radius, burst.knockback);
        }
    }
}

/// What a dragon's phases need from where it flies: the End fight's island
/// or, for a dragon no fight owns (Java's `dragonFight == null`, as a spawn
/// egg or a summon makes one), the world around its fight origin.
pub(super) struct Arena<'a> {
    /// Whether an `EnderDragonFight` owns the dragon.
    pub(super) fight: bool,
    /// The fight's `aliveCrystals`; 0 with no fight.
    pub(super) alive_crystals: u32,
    pub(super) previously_killed: bool,
    /// The exit portal's centre, or the fight origin (`BlockPos.ZERO` for
    /// a dragon with no fight).
    pub(super) podium: IVec3,
    pub(super) fireballs: &'a mut Vec<DragonFireball>,
    pub(super) clouds: &'a mut Vec<BreathCloud>,
    /// The dimension's top and floor (for its node heights).
    pub(super) top: i32,
    pub(super) floor: i32,
    /// The `mobGriefing` game rule (`checkWalls`).
    pub(super) griefing: bool,
}

impl Arena<'_> {
    /// The egg's cell: Java's `getHeightmapPos(MOTION_BLOCKING_NO_LEAVES,
    /// getPodiumLocation(fightOrigin))`, the cell the dragon perches in.
    fn egg(&self, world: &(impl BlockSource + ?Sized)) -> IVec3 {
        self.perch(world).floor().as_ivec3()
    }

    /// The perch: on top of the exit portal's pillar (and egg), or with no
    /// fight Java's `getHeightmapPos(MOTION_BLOCKING_NO_LEAVES,
    /// getPodiumLocation(fightOrigin))`, the top of the origin's column.
    fn perch(&self, world: &(impl BlockSource + ?Sized)) -> DVec3 {
        if !self.fight {
            let y = path::surface(world, self.podium.x, self.podium.z, self.top, self.floor).unwrap_or(self.podium.y);
            return DVec3::new(self.podium.x as f64 + 0.5, y as f64, self.podium.z as f64 + 0.5);
        }
        let mut top = self.podium + IVec3::Y * 4;
        while world.block(top).is_some_and(|b| b != Block::AIR) && top.y < self.podium.y + 8 {
            top.y += 1;
        }
        top.as_dvec3() + DVec3::new(0.5, 0.0, 0.5)
    }
}

impl Arena<'_> {
    /// `DragonDeathPhase`'s target: the bottom centre of the cell above
    /// the exit portal's column (`MOTION_BLOCKING`, so the egg counts), or
    /// for a dragon no fight owns, of the column at its fight origin.
    fn death_target(&self, world: &(impl BlockSource + ?Sized)) -> DVec3 {
        let (x, z) = (self.podium.x, self.podium.z);
        let mut y = self.top;
        while y > self.floor && world.block(IVec3::new(x, y - 1, z)).is_none_or(|b| !(b.is_solid() || b.is_fluid())) {
            y -= 1;
        }
        DVec3::new(x as f64 + 0.5, y as f64, z as f64 + 0.5)
    }
}

/// Whether every chunk of the box `center ± reach` (with `dy` above and
/// below the centre) is loaded: checking its corners covers them all.
fn loaded<W: BlockSource + ?Sized>(world: &W, center: IVec3, reach: IVec3, dy: std::ops::RangeInclusive<i32>) -> bool {
    [-1, 1].iter().all(|&sx| {
        [-1, 1].iter().all(|&sz| {
            [*dy.start(), *dy.end()]
                .iter()
                .all(|&y| world.block(center + IVec3::new(sx * reach.x, y, sz * reach.z)).is_some())
        })
    })
}

/// What a player's attack or projectile hit: an End crystal (by id), a
/// part of the fight's dragon, or a part of a dragon no fight owns (by id).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Hit {
    Crystal(u32),
    Dragon(Part),
    Wild(u32, Part),
}

impl Hit {
    /// A dragon (a living entity), not a crystal.
    pub fn is_dragon(self) -> bool {
        !matches!(self, Hit::Crystal(_))
    }
}

/// The replica and save code of a phase (Java's phase ids differ; these
/// are VoxelCraft's own, append-only).
fn phase_code(p: Phase) -> u64 {
    match p {
        Phase::HoldingPattern => 0,
        Phase::Strafe(_) => 1,
        Phase::LandingApproach => 2,
        Phase::Landing => 3,
        Phase::SittingFlaming => 4,
        Phase::SittingScanning => 5,
        Phase::SittingAttacking => 6,
        Phase::Takeoff => 7,
        Phase::Charging => 8,
        Phase::Dying => 9,
        Phase::Hovering => 10,
    }
}

fn phase_from_code(code: u64) -> Phase {
    match code {
        1 => Phase::Strafe(PlayerId::HOST),
        2 => Phase::LandingApproach,
        3 => Phase::Landing,
        4 => Phase::SittingFlaming,
        5 => Phase::SittingScanning,
        6 => Phase::SittingAttacking,
        7 => Phase::Takeoff,
        8 => Phase::Charging,
        9 => Phase::Dying,
        10 => Phase::Hovering,
        _ => Phase::HoldingPattern,
    }
}

/// What a LAN client needs to draw one dragon.
fn dragon_replica_json(d: &Dragon) -> serde_json::Value {
    let v3 = |p: DVec3| [p.x, p.y, p.z];
    serde_json::json!({
        "pos": v3(d.pos), "prev": v3(d.previous_pos), "yaw": d.yaw, "prev_yaw": d.previous_yaw,
        "health": d.health, "phase": phase_code(d.phase), "flap": d.flap, "prev_flap": d.previous_flap,
        "hurt": d.hurt, "death": d.death_ticks, "healer": d.healer,
    })
}

fn replica_v3(a: &serde_json::Value) -> Option<DVec3> {
    let p = DVec3::new(a.get(0)?.as_f64()?, a.get(1)?.as_f64()?, a.get(2)?.as_f64()?);
    p.is_finite().then_some(p)
}

/// Updates a client's dragon from [`dragon_replica_json`]; false when it
/// has no position.
fn apply_dragon_replica(dragon: &mut Dragon, d: &serde_json::Value) -> bool {
    let Some(pos) = replica_v3(&d["pos"]) else { return false };
    dragon.pos = pos;
    dragon.previous_pos = replica_v3(&d["prev"]).unwrap_or(pos);
    dragon.yaw = d["yaw"].as_f64().unwrap_or(0.0) as f32;
    dragon.previous_yaw = d["prev_yaw"].as_f64().unwrap_or(dragon.yaw as f64) as f32;
    dragon.health = d["health"].as_f64().unwrap_or(0.0).clamp(0.0, MAX_HEALTH as f64) as f32;
    dragon.flap = d["flap"].as_f64().unwrap_or(0.0) as f32;
    dragon.previous_flap = d["prev_flap"].as_f64().unwrap_or(dragon.flap as f64) as f32;
    dragon.hurt = d["hurt"].as_f64().unwrap_or(0.0) as f32;
    dragon.death_ticks = d["death"].as_u64().unwrap_or(0).min(1000) as u32;
    dragon.healer = d["healer"].as_u64().map(|i| i as u32);
    dragon.phase = phase_from_code(d["phase"].as_u64().unwrap_or(0));
    // The neck and tail follow the yaw and height history, as on the host.
    dragon.history_at = (dragon.history_at + 1) % 64;
    dragon.history[dragon.history_at] = (dragon.yaw, dragon.pos.y);
    dragon.place_parts();
    true
}

/// A client's dragon fireballs from the host's list.
fn fireballs_from_replica(v: &serde_json::Value) -> Vec<DragonFireball> {
    v.as_array()
        .map(|fs| {
            fs.iter()
                .take(32)
                .filter_map(|f| {
                    let pos = replica_v3(&f[0])?;
                    Some(DragonFireball {
                        pos,
                        previous_pos: replica_v3(&f[1]).unwrap_or(pos),
                        vel: DVec3::ZERO,
                        dir: replica_v3(&f[2]).unwrap_or(DVec3::X),
                        age: 0,
                    })
                })
                .collect()
        })
        .unwrap_or_default()
}

/// The End crystals (in any dimension) and, in the End, the fight, as a
/// ray meets them: the nearest within `max_dist`.
pub fn raycast(
    fight: Option<&Fight>,
    wild: &Wild,
    crystals: &Crystals,
    origin: DVec3,
    dir: DVec3,
    max_dist: f64,
) -> Option<(Hit, f64)> {
    let crystal = crystals.raycast(origin, dir, max_dist).map(|(id, t)| (Hit::Crystal(id), t));
    let dragon = fight.and_then(|f| f.raycast(origin, dir, max_dist));
    let others = wild.raycast(origin, dir, max_dist);
    crystal.into_iter().chain(dragon).chain(others).min_by(|a, b| a.1.total_cmp(&b.1))
}

/// The boxes a projectile can strike this tick, taken once the dragons
/// have moved: every End crystal and every dragon's parts (a dying
/// dragon's too: `EnderDragonPart` stays pickable).
#[derive(Clone, Debug, Default)]
pub struct Boxes {
    pub list: Vec<(Hit, DVec3, DVec3)>,
    /// Each living dragon as the one `LivingEntity` potions and clouds
    /// reach (its own 16 × 8 box at its feet), named by its body: a dragon
    /// hurt as a whole is hurt through its body (`EnderDragon.hurtServer`).
    pub dragons: Vec<(Hit, DVec3)>,
    /// v1.0 gap Track 8b: the bodies of the dragons perched as the boxes
    /// were taken, whose scales set arrow-like projectiles alight
    /// ([`Dragon::scorches`]).
    pub perched: Vec<Hit>,
}

impl Boxes {
    pub fn new(fight: Option<&Fight>, wild: &Wild, crystals: &Crystals) -> Self {
        let mut list: Vec<(Hit, DVec3, DVec3)> = crystals
            .list
            .iter()
            .map(|c| {
                let (min, max) = c.aabb();
                (Hit::Crystal(c.id), min, max)
            })
            .collect();
        let mut dragons = Vec::new();
        let mut perched = Vec::new();
        let scorching = |d: &Dragon| d.scorches(Blow::arrow(1.0, false));
        if let Some(d) = fight.and_then(|f| f.dragon.as_ref()) {
            list.extend(d.parts.iter().map(|&(part, min, max)| (Hit::Dragon(part), min, max)));
            if d.health > 0.0 {
                dragons.push((Hit::Dragon(Part::Body), d.pos));
            }
            if scorching(d) {
                perched.push(Hit::Dragon(Part::Body));
            }
        }
        for w in &wild.dragons {
            list.extend(w.dragon.parts.iter().map(|&(part, min, max)| (Hit::Wild(w.id, part), min, max)));
            if w.dragon.health > 0.0 {
                dragons.push((Hit::Wild(w.id, Part::Body), w.dragon.pos));
            }
            if scorching(&w.dragon) {
                perched.push(Hit::Wild(w.id, Part::Body));
            }
        }
        Self { list, dragons, perched }
    }

    /// v1.0 gap Track 8b: whether `blow` on `hit` sets the projectile
    /// alight ([`Dragon::scorches`], as the dragons stood when the boxes
    /// were taken).
    pub fn scorches(&self, hit: Hit, blow: Blow) -> bool {
        let body = match hit {
            Hit::Crystal(_) => return false,
            Hit::Dragon(_) => Hit::Dragon(Part::Body),
            Hit::Wild(id, _) => Hit::Wild(id, Part::Body),
        };
        blow.arrow && !blow.fire && self.perched.contains(&body)
    }

    pub fn is_empty(&self) -> bool {
        self.list.is_empty() && self.dragons.is_empty()
    }

    /// The crystal (unless `crystals` is false) or dragon part `p` is in.
    pub fn hit_at(&self, p: DVec3, crystals: bool) -> Option<Hit> {
        self.list
            .iter()
            .filter(|(hit, ..)| crystals || hit.is_dragon())
            .find(|&&(_, min, max)| p.cmpge(min).all() && p.cmple(max).all())
            .map(|&(hit, ..)| hit)
    }

    /// The End crystal `p` is in (v1.0 gap Track 8: what llama spit meets).
    pub fn crystal_at(&self, p: DVec3) -> Option<Hit> {
        self.list
            .iter()
            .find(|&&(hit, min, max)| matches!(hit, Hit::Crystal(_)) && p.cmpge(min).all() && p.cmple(max).all())
            .map(|&(hit, ..)| hit)
    }

    /// The nearest crystal (unless `crystals` is false) or dragon part the
    /// move `from + dir * t`, `t` up to `max_dist`, enters.
    pub fn raycast(&self, from: DVec3, dir: DVec3, max_dist: f64, crystals: bool) -> Option<(Hit, f64)> {
        self.list
            .iter()
            .filter(|(hit, ..)| crystals || hit.is_dragon())
            .filter_map(|&(hit, min, max)| physics::ray_aabb(from, dir, min, max).map(|t| (hit, t)))
            .filter(|&(_, t)| t <= max_dist)
            .min_by(|a, b| a.1.total_cmp(&b.1))
    }
}

impl super::Entities {
    /// v1.0 gap Track 4c: the hits and heals projectiles (other than
    /// arrows), potions and clouds dealt the crystals and dragons this
    /// tick ([`EntityEvent::DragonStruck`], [`EntityEvent::DragonHealed`]).
    pub(super) fn apply_dragon_hits(&mut self, events: &mut Vec<EntityEvent>) {
        let ours = |e: &EntityEvent| matches!(e, EntityEvent::DragonStruck { .. } | EntityEvent::DragonHealed { .. });
        if !events.iter().any(ours) {
            return;
        }
        let mut targets =
            Targets { fight: self.fight.as_mut(), wild: &mut self.wild_dragons, crystals: &mut self.crystals };
        for e in events.iter() {
            match *e {
                EntityEvent::DragonStruck { hit, blow, by } => {
                    targets.strike(hit, blow, by);
                }
                EntityEvent::DragonHealed { hit, amount } => targets.heal(hit, amount),
                _ => {}
            }
        }
        events.retain(|e| !ours(e));
    }
}

/// The End crystal or dragon part the point `p` is in (see [`Boxes`]).
pub fn hit_at(fight: Option<&Fight>, wild: &Wild, crystals: &Crystals, p: DVec3) -> Option<Hit> {
    crystals
        .touching(p, p)
        .next()
        .map(|c| Hit::Crystal(c.id))
        .or_else(|| fight.and_then(|f| f.hit_at(p)))
        .or_else(|| wild.hit_at(p))
}

/// What projectiles strike in flight: the crystals, the dragons no fight
/// owns and, in the End, the fight.
pub struct Targets<'a> {
    pub fight: Option<&'a mut Fight>,
    pub wild: &'a mut Wild,
    pub crystals: &'a mut Crystals,
}

impl Targets<'_> {
    pub fn raycast(&self, origin: DVec3, dir: DVec3, max_dist: f64) -> Option<(Hit, f64)> {
        raycast(self.fight.as_deref(), self.wild, self.crystals, origin, dir, max_dist)
    }

    /// `LivingEntity.heal` on the dragon `hit` names (a splash of Healing).
    pub fn heal(&mut self, hit: Hit, amount: f32) {
        let dragon = match hit {
            Hit::Crystal(_) => None,
            Hit::Dragon(_) => self.fight.as_deref_mut().and_then(|f| f.dragon.as_mut()),
            Hit::Wild(id, _) => self.wild.dragons.iter_mut().find(|w| w.id == id).map(|w| &mut w.dragon),
        };
        if let Some(d) = dragon {
            d.heal_by(amount);
        }
    }

    /// v1.0 gap Track 8: whether `blow` on `hit` sets the projectile alight
    /// ([`Dragon::scorches`]).
    pub fn scorches(&self, hit: Hit, blow: Blow) -> bool {
        let dragon = match hit {
            Hit::Crystal(_) => None,
            Hit::Dragon(_) => self.fight.as_deref().and_then(|f| f.dragon.as_ref()),
            Hit::Wild(id, _) => self.wild.dragons.iter().find(|w| w.id == id).map(|w| &w.dragon),
        };
        dragon.is_some_and(|d| d.scorches(blow))
    }

    /// A projectile's `blow`, from player `by`; returns whether it landed
    /// (`hurtServer`'s result: arrows bounce off a perched or dying
    /// dragon). Any hit breaks an End crystal.
    pub fn strike(&mut self, hit: Hit, blow: Blow, by: Option<PlayerId>) -> bool {
        match hit {
            Hit::Crystal(id) => {
                let other = blow.blame.map_or(CrystalBlow::Other, CrystalBlow::Mob);
                let blow = by.map_or(other, |id| CrystalBlow::Player { id, creative: false });
                self.crystals.hit(id, blow)
            }
            Hit::Dragon(_) => self.fight.as_deref_mut().is_some_and(|f| f.strike(hit, blow)),
            Hit::Wild(id, part) => self.wild.strike(id, part, blow),
        }
    }
}

/// `EnderDragon.hurt`: hits anywhere but the head and neck do a quarter,
/// plus up to one (26.3 counts the neck fully too).
fn part_damage(part: Part, damage: f32) -> f32 {
    if matches!(part, Part::Head | Part::Neck) { damage } else { damage / 4.0 + damage.min(1.0) }
}

/// A hit on a dragon part as `EnderDragonPart.hurtServer` and
/// `EnderDragon.hurt` read its `DamageSource` (v1.0 gap Track 4c).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Blow {
    pub damage: f32,
    /// `source.getEntity() instanceof Player`: only a player's hits (and
    /// explosions) take health; anyone else's land and do nothing.
    pub player: bool,
    /// `#always_hurts_ender_dragons` (`#is_explosion`: explosions,
    /// fireworks).
    pub explosion: bool,
    /// The direct entity is an `AbstractArrow` (arrows, tridents) or a
    /// `WindCharge` (not a breeze's): perched, the dragon takes nothing
    /// (`AbstractDragonSittingPhase.onHurt`).
    pub arrow: bool,
    /// `#is_fire` (fireballs): the dragon is fire immune
    /// (`isInvulnerableToBase`).
    pub fire: bool,
    /// v1.0 gap Track 8b: the mob behind it (`source.getEntity()`), which an
    /// End crystal's blast names ([`super::crystal::blame`]).
    pub blame: Option<super::MobKind>,
}

impl Blow {
    const fn new(damage: f32, player: bool) -> Self {
        Self { damage, player, explosion: false, arrow: false, fire: false, blame: None }
    }

    /// The same blow from mob `kind`, if any.
    pub const fn blamed_on(self, kind: Option<super::MobKind>) -> Self {
        Self { blame: kind, ..self }
    }

    /// A player's melee hit.
    pub const fn melee(damage: f32) -> Self {
        Self::new(damage, true)
    }

    /// v1.0 gap Track 2d: a mob's hit (a spear stab): it lands, but only a
    /// player's or an explosion takes health.
    pub const fn mob(damage: f32) -> Self {
        Self::new(damage, false)
    }

    /// An arrow's or a trident's hit; `player` when a player shot it.
    pub const fn arrow(damage: f32, player: bool) -> Self {
        Self { arrow: true, ..Self::new(damage, player) }
    }

    /// Any other projectile's hit (`thrown`, `mobProjectile`, `spit`,
    /// `witherSkull`, `windCharge` from a breeze).
    pub const fn projectile(damage: f32, player: bool) -> Self {
        Self::new(damage, player)
    }

    /// A wind charge's hit (`AbstractWindCharge.onHitEntity`, 1 damage): a
    /// player's charge is a `WindCharge`, which a perched dragon shrugs
    /// off; a breeze's isn't. `player`: a player threw it.
    pub const fn wind_charge(breeze: bool, player: bool) -> Self {
        Self { arrow: !breeze, ..Self::new(super::wind::HIT_DAMAGE, player) }
    }

    /// An explosion or a firework's blast.
    pub const fn explosion(damage: f32) -> Self {
        Self { explosion: true, ..Self::new(damage, false) }
    }

    /// A fireball's direct hit (`DamageTypes.FIREBALL`).
    pub const fn fire(damage: f32, player: bool) -> Self {
        Self { fire: true, ..Self::new(damage, player) }
    }
}

fn part_center(d: &Dragon, part: Part) -> DVec3 {
    d.parts.iter().find(|p| p.0 == part).map_or(d.pos, |&(_, min, max)| (min + max) / 2.0)
}

/// Java's `Mth.wrapDegrees(float)`.
fn wrap_degrees(a: f32) -> f32 {
    let mut a = a % 360.0;
    if a >= 180.0 {
        a -= 360.0;
    }
    if a < -180.0 {
        a += 360.0;
    }
    a
}

/// `(float) (Math.PI / 180.0)`.
const DEGREES: f32 = (std::f64::consts::PI / 180.0) as f32;

/// `Vec3.normalize`: nothing below a length of 1e-5.
fn normalize(v: DVec3) -> DVec3 {
    let length = (v.x * v.x + v.y * v.y + v.z * v.z).sqrt();
    if length < f64::from(1.0e-5f32) { DVec3::ZERO } else { DVec3::new(v.x / length, v.y / length, v.z / length) }
}

/// `Mth.atan2`: Java's table-driven arctangent.
fn mth_atan2(mut y: f64, mut x: f64) -> f64 {
    use std::sync::OnceLock;
    static TABLES: OnceLock<([f64; 257], [f64; 257])> = OnceLock::new();
    let (asin, cos) = TABLES.get_or_init(|| {
        let asin: [f64; 257] = std::array::from_fn(|i| (i as f64 / 256.0).asin());
        (asin, asin.map(f64::cos))
    });
    let d2 = x * x + y * y;
    if d2.is_nan() {
        return f64::NAN;
    }
    let neg_y = y < 0.0;
    if neg_y {
        y = -y;
    }
    let neg_x = x < 0.0;
    if neg_x {
        x = -x;
    }
    let steep = y > x;
    if steep {
        std::mem::swap(&mut x, &mut y);
    }
    // `fastInvSqrt`.
    let half = 0.5 * d2;
    let guess = f64::from_bits((6_910_469_410_427_058_090_i64 - ((d2.to_bits() as i64) >> 1)) as u64);
    let rinv = guess * (1.5 - half * guess * guess);
    x *= rinv;
    y *= rinv;
    // `FRAC_BIAS`, 2^44: the sum's low bits are `y` in 256ths.
    let bias = f64::from_bits(4_805_340_802_404_319_232);
    let yp = bias + y;
    let index = yp.to_bits() as i32 as usize;
    let (phi, c_phi) = (asin[index], cos[index]);
    let s_phi = yp - bias;
    let sd = y * c_phi - x * s_phi;
    let d = (6.0 + sd * sd) * sd * 0.166_666_666_666_666_66;
    let mut theta = phi + d;
    if steep {
        theta = std::f64::consts::FRAC_PI_2 - theta;
    }
    if neg_x {
        theta = std::f64::consts::PI - theta;
    }
    if neg_y {
        theta = -theta;
    }
    theta
}

/// The horizontal unit heading of a Java yaw.
fn heading(yaw: f32) -> DVec3 {
    let r = yaw.to_radians();
    DVec3::new(r.sin() as f64, 0.0, -r.cos() as f64)
}

/// `Entity.calculateViewVector(xRot, yRot)`, with Java's float tables.
fn view_vector(x_rot: f32, y_rot: f32) -> DVec3 {
    use crate::knockback::{mth_cos, mth_sin};
    let rx = x_rot * (std::f64::consts::PI / 180.0) as f32;
    let ry = -y_rot * (std::f64::consts::PI / 180.0) as f32;
    let (y_cos, y_sin) = (mth_cos(f64::from(ry)), mth_sin(f64::from(ry)));
    let (x_cos, x_sin) = (mth_cos(f64::from(rx)), mth_sin(f64::from(rx)));
    DVec3::new(f64::from(y_sin * x_cos), f64::from(-x_sin), f64::from(y_cos * x_cos))
}

/// The centre of the egg's cell (`BlockPos.distToCenterSqr`).
fn egg_center(egg: IVec3) -> DVec3 {
    egg.as_dvec3() + DVec3::splat(0.5)
}

/// `level.getNearestPlayer` with conditions that have no range: the
/// targetable player nearest `at`.
fn nearest_player(ctx: &Ctx, at: DVec3) -> Option<Target> {
    ctx.players
        .iter()
        .filter(|t| t.targetable)
        .min_by(|a, b| a.pos.distance_squared(at).total_cmp(&b.pos.distance_squared(at)))
        .copied()
}

impl Dragon {
    fn new(pos: DVec3) -> Self {
        let mut d = Self {
            pos,
            previous_pos: pos,
            vel: DVec3::ZERO,
            yaw: 0.0,
            previous_yaw: 0.0,
            yaw_speed: 0.0,
            health: MAX_HEALTH,
            phase: Phase::HoldingPattern,
            phase_ticks: 0,
            target: None,
            path: None,
            clockwise: false,
            strafe_clockwise: false,
            strafe_at: None,
            nodes: [DVec3::ZERO; 24],
            nodes_ready: false,
            fireball_charge: 0,
            flame_count: 0,
            sitting_damage: 0.0,
            healer: None,
            hurt: 0.0,
            immune: (0, 0.0),
            death_ticks: 0,
            flap: 0.0,
            previous_flap: 0.0,
            history: [(0.0, pos.y); 64],
            history_at: 0,
            parts: [(Part::Body, pos, pos); 8],
            growl: 100,
            in_wall: false,
            ticks: 0,
        };
        d.place_parts();
        d
    }

    /// Yaw and height `ticks` ago.
    pub fn latency(&self, ticks: usize) -> (f32, f64) {
        self.history[(self.history_at + 64 - ticks.min(63)) % 64]
    }

    pub fn parts(&self) -> &[(Part, DVec3, DVec3); 8] {
        &self.parts
    }

    /// `ServerExplosion.hurtEntities` on the dragon: the dragon itself
    /// (hurt as its body) and each of its parts is an entity the blast
    /// reaches, measured from its feet with its own box's exposure, and
    /// each is hurt for its share (`#is_explosion` always hurts); the
    /// immunity window leaves the worst of them. The dragon itself is
    /// pushed too (v1.0 gap Track 4c).
    fn blast<W: BlockSource + ?Sized>(&mut self, world: &W, center: DVec3, power: f32) {
        let (min, max) = wild::SHAPE.aabb(self.pos);
        let whole = (Part::Body, self.pos, min, max);
        let parts = self
            .parts
            .map(|(part, min, max)| (part, DVec3::new((min.x + max.x) / 2.0, min.y, (min.z + max.z) / 2.0), min, max));
        for (part, feet, min, max) in std::iter::once(whole).chain(parts) {
            if feet.distance(center) > f64::from(power) * 2.0 {
                continue;
            }
            let exposure = crate::world::explosion::seen_percent(world, center, min, max);
            if let Some((damage, _)) = super::explosion_damage(power, feet.distance(center), exposure) {
                self.hurt(part, Blow::explosion(damage));
            }
        }
        self.push_from_blast(world, center, power, 1.0);
    }

    /// `ServerExplosion.hurtEntities`' push on the dragon itself, from a
    /// blast of `radius` with knockback multiplier `knockback`: added to its
    /// motion (blocks a tick), which carries it once it next flies. The
    /// dragon has no explosion knockback resistance.
    pub(super) fn push_from_blast<W: BlockSource + ?Sized>(
        &mut self,
        world: &W,
        center: DVec3,
        radius: f32,
        knockback: f32,
    ) {
        let d = self.pos.distance(center) / (f64::from(radius) * 2.0);
        if d > 1.0 || d.is_nan() || self.health <= 0.0 {
            return;
        }
        let eye = self.pos + DVec3::Y * EYE_HEIGHT;
        let (min, max) = wild::SHAPE.aabb(self.pos);
        let exposure = f64::from(crate::world::explosion::seen_percent(world, center, min, max));
        let dir = eye - center;
        // `Vec3.normalize`: nothing below 1e-5.
        let dir = if dir.length() < 1.0e-5 { DVec3::ZERO } else { dir / dir.length() };
        self.vel += dir * ((1.0 - d) * exposure * f64::from(knockback));
    }

    fn set_phase(&mut self, phase: Phase) {
        if self.phase == Phase::Dying || self.phase == phase {
            return;
        }
        self.phase = phase;
        self.phase_ticks = 0;
        self.target = None;
        self.path = None;
        self.fireball_charge = 0;
        self.strafe_at = None;
        if phase == Phase::SittingFlaming {
            self.flame_count += 1;
        }
    }

    /// `DragonHoldingPatternPhase.strafePlayer` and `onCrystalDestroyed`:
    /// the strafe begins with the player as its `attackTarget`.
    fn strafe(&mut self, player: &Target) {
        self.set_phase(Phase::Strafe(player.id));
        self.strafe_at = Some(player.pos);
    }

    /// `LivingEntity.heal`: nothing for a dragon already dead.
    fn heal_by(&mut self, amount: f32) {
        if self.health > 0.0 {
            self.health = (self.health + amount).min(MAX_HEALTH);
        }
    }

    /// v1.0 gap Track 8: `AbstractDragonSittingPhase.onHurt` sets the
    /// direct entity of an arrow's, trident's or wind charge's hit alight
    /// for a second (`igniteForSeconds(1)`) as it takes the damage away;
    /// `EnderDragon.hurt` asks it in any phase but `DYING`, and fire never
    /// gets past the parts' immunity.
    pub(super) fn scorches(&self, blow: Blow) -> bool {
        blow.arrow && !blow.fire && self.health > 0.0 && self.phase.deflects_arrows()
    }

    /// `EnderDragonPart.hurtServer` and `EnderDragon.hurt`: whether the hit
    /// landed (Java's result, which decides whether an arrow bounces off),
    /// not whether it took health: only a player's hits and explosions do.
    /// Nothing lands on a dying dragon or through its fire immunity.
    pub(super) fn hurt(&mut self, part: Part, blow: Blow) -> bool {
        if blow.fire || self.phase == Phase::Dying || self.health <= 0.0 {
            return false;
        }
        let damage = if blow.arrow && self.phase.deflects_arrows() { 0.0 } else { blow.damage };
        if part_damage(part, damage) < 0.01 {
            return false;
        }
        if blow.player || blow.explosion {
            self.hurt_by(part, damage, true);
        }
        true
    }

    /// Damage to `part`; `counts` is false for sources that can't hurt the
    /// dragon. Returns whether it took any.
    fn hurt_by(&mut self, part: Part, damage: f32, counts: bool) -> bool {
        if self.phase == Phase::Dying || !counts {
            return false;
        }
        let damage = part_damage(part, damage);
        if damage < 0.01 {
            return false;
        }
        // Java's ten-tick window: a second hit only does what it adds.
        let dealt = if self.immune.0 > 0 {
            if damage <= self.immune.1 {
                return false;
            }
            damage - self.immune.1
        } else {
            self.hurt = 0.5;
            damage
        };
        self.immune = (10, damage);
        let before = self.health;
        self.health = (self.health - dealt).max(0.0);
        if self.health <= 0.0 {
            // `handleKillingBlow`: in flight the dragon is held at one
            // health and enters `DYING` to fly to the portal first; perched
            // (or hovering) it dies where it is.
            let perched = self.phase.sitting();
            self.set_phase(Phase::Dying);
            self.health = if perched { 0.0 } else { 1.0 };
            return true;
        }
        if self.phase.sitting() {
            self.sitting_damage += before - self.health;
            if self.sitting_damage > 0.25 * MAX_HEALTH {
                self.sitting_damage = 0.0;
                self.set_phase(Phase::Takeoff);
            }
        }
        true
    }

    /// One tick; returns `false` once the death is over.
    #[allow(clippy::too_many_arguments)]
    fn tick<W: BlockSource + ?Sized>(
        &mut self,
        arena: &mut Arena<'_>,
        world: &W,
        ctx: &Ctx,
        rng: &mut Rng,
        crystals: &Crystals,
        events: &mut Vec<EntityEvent>,
    ) -> bool {
        // `processFlappingMovement` first (`isFlapping`): a flap sounds as
        // the last tick's wing beat took its cosine up through -0.3,
        // perched or not.
        if self.previous_flap.cos_turns() <= -0.3 && self.flap.cos_turns() >= -0.3 {
            events.push(EntityEvent::Sound { sound: MobSound::DragonFlap, pos: self.pos });
        }
        self.previous_pos = self.pos;
        self.previous_yaw = self.yaw;
        self.previous_flap = self.flap;
        self.hurt = (self.hurt - TICK as f32).max(0.0);
        if self.immune.0 > 0 {
            self.immune.0 -= 1;
        }
        self.ticks += 1;
        // `isDeadOrDying`: `tickDeath` runs and `aiStep` does nothing else.
        if self.health <= 0.0 {
            return self.tick_death(arena, events);
        }
        self.phase_ticks += 1;
        // `EnderDragon.aiStep` from here, in its order: `checkCrystals`,
        // the wing beat, the yaw wrapped and recorded with the height, the
        // phase, the flight, the parts, then `checkWalls`.
        self.heal(crystals, rng);

        // Wing beats: slower the faster it flies, quicker while climbing,
        // half as quick in a wall (`inWall`), a steady 0.1 perched.
        let flap = 0.2 / (self.vel.with_y(0.0).length() as f32 * 10.0 + 1.0) * 2f64.powf(self.vel.y) as f32;
        self.flap += if self.phase.sitting() {
            0.1
        } else if self.in_wall {
            flap * 0.5
        } else {
            flap
        };
        // `EnderDragon.aiStep` on the client: unless sitting (hovering
        // counts), `--growlTime < 0` growls at the dragon and waits
        // `200 + nextInt(200)` more.
        if !self.phase.sitting() {
            if self.growl == 0 {
                self.growl = 200 + (rng.next_f32() * 200.0) as u32;
                events.push(EntityEvent::Sound { sound: MobSound::DragonRoar, pos: self.pos });
            } else {
                self.growl -= 1;
            }
        }

        self.yaw = wrap_degrees(self.yaw);
        self.history_at = (self.history_at + 1) % 64;
        self.history[self.history_at] = (self.yaw, self.pos.y);
        self.think(arena, world, ctx, rng, events);
        // `DragonSittingAttackingPhase.doClientTick`: the roar is a growl
        // every tick of the phase.
        if self.phase == Phase::SittingAttacking {
            events.push(EntityEvent::Sound { sound: MobSound::DragonRoar, pos: self.pos });
        }
        self.fly();
        self.place_parts();
        self.check_walls(world, arena.griefing, rng, events);
        self.hit_players(ctx, events);
        true
    }

    /// Java's phase logic (`DragonPhaseInstance.doServerTick`). As in
    /// `EnderDragon.aiStep`, when a phase hands over to another, the new
    /// phase runs its first tick at once.
    fn think<W: BlockSource + ?Sized>(
        &mut self,
        arena: &mut Arena<'_>,
        world: &W,
        ctx: &Ctx,
        rng: &mut Rng,
        events: &mut Vec<EntityEvent>,
    ) {
        let before = self.phase;
        self.think_phase(arena, world, ctx, rng, events);
        if self.phase != before {
            self.phase_ticks += 1;
            self.think_phase(arena, world, ctx, rng, events);
        }
    }

    fn think_phase<W: BlockSource + ?Sized>(
        &mut self,
        arena: &mut Arena<'_>,
        world: &W,
        ctx: &Ctx,
        rng: &mut Rng,
        events: &mut Vec<EntityEvent>,
    ) {
        let near = |r: f64, from: DVec3| {
            ctx.players
                .iter()
                .filter(|t| t.targetable && t.pos.distance_squared(from) < r * r)
                .min_by(|a, b| a.pos.distance_squared(from).total_cmp(&b.pos.distance_squared(from)))
                .copied()
        };
        match self.phase {
            // `DragonHoverPhase`: hold where it was.
            Phase::Hovering => {
                if self.target.is_none() {
                    self.target = Some(self.pos);
                }
            }
            Phase::HoldingPattern => {
                if self.target_reached() {
                    self.holding_target(arena, world, ctx, rng);
                }
            }
            Phase::Strafe(id) => {
                // `DragonStrafePlayerPhase`: the target stays the player
                // whatever becomes of them; only a strafe with none (after
                // a reload, as Java saves no target) goes back to circling.
                if let Some(t) = ctx.players.iter().find(|t| t.id == id) {
                    self.strafe_at = Some(t.pos);
                }
                let Some(at) = self.strafe_at else {
                    self.set_phase(Phase::HoldingPattern);
                    return;
                };
                if self.phase_ticks == 1 {
                    self.strafe_route(arena, world, at, rng);
                }
                // `DragonStrafePlayerPhase.doServerTick`: once its path is
                // done it heads for the player, a little above them.
                if self.path.as_ref().is_some_and(Vec::is_empty) {
                    let dist = (at - self.pos).with_y(0.0).length();
                    let lift = (f64::from(0.4f32) + dist / 80.0 - 1.0).min(10.0);
                    self.target = Some(at + DVec3::Y * lift);
                }
                if self.target_reached() {
                    self.circle(arena, world, rng, true);
                }
                let aim = (at - self.pos).with_y(0.0).normalize_or_zero();
                let facing = heading(self.yaw);
                let close = at.distance_squared(self.pos) < 64.0 * 64.0;
                if close && super::mob::line_of_sight(world, self.head(), at + DVec3::Y * 0.9) {
                    self.fireball_charge += 1;
                    let angle = facing.dot(aim).clamp(-1.0, 1.0).acos().to_degrees() + 0.5;
                    if self.fireball_charge >= 5 && angle < 10.0 {
                        // `head.getX() - viewVector.x`, `head.getY(0.5) +
                        // 0.5`: the dragon's view vector points away from
                        // its head, so a block ahead of the head, level
                        // with its top.
                        let from = self.head() + facing + DVec3::Y * 0.5;
                        let dir = (at + DVec3::Y * 0.9 - from).normalize_or(facing);
                        arena.fireballs.push(DragonFireball::new(from, dir));
                        events.push(EntityEvent::Sound { sound: MobSound::DragonShoot, pos: from });
                        self.set_phase(Phase::HoldingPattern);
                    }
                } else {
                    self.fireball_charge = self.fireball_charge.saturating_sub(1);
                }
            }
            // `DragonLandingApproachPhase.findNewTarget`: from the nearest
            // node to the one away from the player nearest the egg, then
            // down to the egg.
            Phase::LandingApproach => {
                if self.target_reached() {
                    if self.path.as_ref().is_none_or(Vec::is_empty) {
                        let current = self.closest_node(arena, world);
                        let egg = arena.egg(world);
                        // `NEAR_EGG_TARGETING` has no range.
                        let goal = match nearest_player(ctx, egg.as_dvec3()) {
                            Some(t) => {
                                let aim = t.pos.with_y(0.0).normalize_or_zero();
                                self.closest_node_to(arena, DVec3::new(-aim.x * 40.0, 105.0, -aim.z * 40.0))
                            }
                            None => self.closest_node_to(arena, DVec3::new(40.0, egg.y as f64, 0.0)),
                        };
                        self.path = self.find_path(arena, current, goal, Some(egg.as_dvec3()));
                    }
                    self.navigate(rng);
                    if self.path.as_ref().is_some_and(Vec::is_empty) {
                        self.set_phase(Phase::Landing);
                    }
                }
            }
            // `DragonLandingPhase`: down to the bottom centre of the egg's
            // cell (fixed on its first tick); within a block of it the flame
            // count resets and the dragon sits scanning.
            Phase::Landing => {
                let perch = *self.target.get_or_insert_with(|| arena.perch(world));
                if perch.distance_squared(self.pos) < 1.0 {
                    self.flame_count = 0;
                    self.set_phase(Phase::SittingScanning);
                }
            }
            Phase::SittingFlaming => {
                if self.phase_ticks >= 200 {
                    let next = if self.flame_count >= 4 { Phase::Takeoff } else { Phase::SittingScanning };
                    self.set_phase(next);
                } else if self.phase_ticks == 10 {
                    // A cloud of breath in front, on the ground.
                    let ahead = (self.head() - self.pos).with_y(0.0).normalize_or_zero();
                    let mut at = self.head() + ahead * 2.5;
                    let start = at.y;
                    while world.block(at.floor().as_ivec3()) == Some(Block::AIR) {
                        at.y -= 1.0;
                        if at.y < 0.0 {
                            at.y = start;
                            break;
                        }
                    }
                    at.y = at.y.floor() + 1.0;
                    arena.clouds.push(BreathCloud::new(at, 5.0, 0.0, 200, 3.0));
                    events.push(EntityEvent::Sound { sound: MobSound::DragonShoot, pos: at });
                }
            }
            // `DragonSittingScanningPhase`: a player within 20 (and 10
            // up or down) is roared at after 25 ticks, turned to before;
            // with no one, after 100 ticks it charges the nearest within
            // 150 or takes off.
            Phase::SittingScanning => {
                let scanned = ctx
                    .players
                    .iter()
                    .filter(|t| {
                        t.targetable
                            && t.pos.distance_squared(self.pos) < 20.0 * 20.0
                            && (t.pos.y - self.pos.y).abs() <= 10.0
                    })
                    .min_by(|a, b| a.pos.distance_squared(self.pos).total_cmp(&b.pos.distance_squared(self.pos)))
                    .copied();
                if let Some(t) = scanned {
                    if self.phase_ticks > 25 {
                        self.set_phase(Phase::SittingAttacking);
                    } else {
                        // Turn to face them: the angle from the body, the
                        // turn toward them from the head.
                        let aim = (t.pos - self.pos).with_y(0.0).normalize_or_zero();
                        let to = (t.pos - self.head()).with_y(0.0);
                        let facing = heading(self.yaw);
                        let angle = facing.dot(aim).clamp(-1.0, 1.0).acos().to_degrees() + 0.5;
                        if !(0.0..=10.0).contains(&angle) {
                            let want = 180.0 - (to.x.atan2(to.z) as f32).to_degrees();
                            let turn = wrap_degrees(want - self.yaw).clamp(-100.0, 100.0);
                            let len = to.length() as f32 + 1.0;
                            self.yaw_speed = self.yaw_speed * 0.8 + turn * 0.7 / len.min(40.0) / len;
                            self.yaw += self.yaw_speed;
                        }
                    }
                } else if self.phase_ticks >= 100 {
                    match near(150.0, self.pos) {
                        Some(t) => {
                            self.set_phase(Phase::Charging);
                            self.target = Some(t.pos);
                        }
                        None => self.set_phase(Phase::Takeoff),
                    }
                }
            }
            // `attackingTicks++ >= 40`: the 41st tick.
            Phase::SittingAttacking => {
                if self.phase_ticks > 40 {
                    self.set_phase(Phase::SittingFlaming);
                }
            }
            // `DragonTakeoffPhase`: on its first tick, a path out along the
            // head's look to the ring; then once 10 blocks from the egg, the
            // holding pattern.
            Phase::Takeoff => {
                let egg = arena.egg(world);
                if self.phase_ticks > 1 && self.path.is_some() {
                    if egg_center(egg).distance_squared(self.pos) >= 10.0 * 10.0 {
                        self.set_phase(Phase::HoldingPattern);
                    }
                } else {
                    let current = self.closest_node(arena, world);
                    // `getHeadLookVector` while taking off: pitched down by
                    // the distance from the egg.
                    let dist = (egg_center(egg).distance_squared(self.pos).sqrt() as f32 / 4.0).max(1.0);
                    let look = view_vector(-(6.0 / dist) * 1.5 * 5.0, self.yaw);
                    let goal = self.closest_node_to(arena, DVec3::new(-look.x * 40.0, 105.0, -look.z * 40.0)) as i32;
                    let goal =
                        if arena.alive_crystals > 0 { goal.rem_euclid(12) as usize } else { path::inner_ring(goal) };
                    self.path = self.find_path(arena, current, goal, None);
                    self.navigate(rng);
                }
            }
            Phase::Charging => {
                let reached = self.target.is_none_or(|t| {
                    let d = t.distance_squared(self.pos);
                    !(100.0..=22500.0).contains(&d)
                });
                if reached || self.phase_ticks > 200 {
                    self.fireball_charge += 1;
                    if self.fireball_charge >= 10 {
                        self.set_phase(Phase::HoldingPattern);
                    }
                }
            }
            // `DragonDeathPhase`: toward the top of the exit portal's
            // column, held at one health until within 10 blocks (or beyond
            // 150), when it dies (`tickDeath` takes over).
            Phase::Dying => {
                let to = *self.target.get_or_insert_with(|| arena.death_target(world));
                let d = to.distance_squared(self.pos);
                self.health = if (100.0..=22500.0).contains(&d) { 1.0 } else { 0.0 };
            }
        }
    }

    /// Whether the dragon is close to (or impossibly far from) its target.
    fn target_reached(&self) -> bool {
        self.target.is_none_or(|t| !(100.0..=22500.0).contains(&t.distance_squared(self.pos)))
    }

    /// `EnderDragon.findClosestNode()`: works out the nodes the first time
    /// (and again while a column is unloaded), then the node nearest the
    /// dragon.
    fn closest_node<W: BlockSource + ?Sized>(&mut self, arena: &Arena<'_>, world: &W) -> usize {
        if !self.nodes_ready {
            let (nodes, complete) = path::nodes(world, arena.top, arena.floor);
            self.nodes = nodes;
            self.nodes_ready = complete;
        }
        self.closest_node_to(arena, self.pos)
    }

    /// `EnderDragon.findClosestNode(x, y, z)`.
    fn closest_node_to(&self, arena: &Arena<'_>, p: DVec3) -> usize {
        path::closest_node(&self.nodes, p, path::min_node(arena.alive_crystals))
    }

    /// `EnderDragon.findPath`.
    fn find_path(&self, arena: &Arena<'_>, from: usize, to: usize, last: Option<DVec3>) -> Option<Vec<DVec3>> {
        path::find_path(&self.nodes, from, to, path::min_node(arena.alive_crystals), last)
    }

    /// `navigateToNextPathNode`: on to the path's next node, up to 20
    /// blocks above it (Java adds the float to the whole-block height in
    /// floats).
    fn navigate(&mut self, rng: &mut Rng) {
        if let Some(node) = self.path.as_mut().and_then(Vec::pop) {
            let y = node.y as f32 + rng.next_f32() * 20.0;
            self.target = Some(DVec3::new(node.x, f64::from(y), node.z));
        }
    }

    /// `DragonHoldingPatternPhase.findNewTarget`: at the end of a path,
    /// maybe land, or strafe the player nearest the egg; otherwise on
    /// round the ring.
    fn holding_target<W: BlockSource + ?Sized>(&mut self, arena: &Arena<'_>, world: &W, ctx: &Ctx, rng: &mut Rng) {
        if self.path.as_ref().is_some_and(Vec::is_empty) {
            let crystals = arena.alive_crystals;
            if rng.next_int(crystals + 3) == 0 {
                self.set_phase(Phase::LandingApproach);
                return;
            }
            let egg = arena.egg(world);
            // `NEW_TARGET_TARGETING` has no range.
            if let Some(t) = nearest_player(ctx, egg.as_dvec3()) {
                let odds = (egg_center(egg).distance_squared(t.pos) / 512.0 + 2.0).min(f64::from(i32::MAX)) as u32;
                if rng.next_int(odds) == 0 || rng.next_int(crystals + 2) == 0 {
                    self.strafe(&t);
                    return;
                }
            }
        }
        self.circle(arena, world, rng, false);
    }

    /// The path part of `findNewTarget` in the holding pattern and the
    /// strafe (each keeps its own direction): with no path, or at its end,
    /// a path from the nearest node to the next one round (or, one time in
    /// eight, turning back from six further on); then on to its next node.
    fn circle<W: BlockSource + ?Sized>(&mut self, arena: &Arena<'_>, world: &W, rng: &mut Rng, strafing: bool) {
        if self.path.as_ref().is_none_or(Vec::is_empty) {
            let current = self.closest_node(arena, world);
            let clockwise = if strafing { &mut self.strafe_clockwise } else { &mut self.clockwise };
            let mut to = current as i32;
            if rng.next_int(8) == 0 {
                *clockwise = !*clockwise;
                to += 6;
            }
            to += if *clockwise { 1 } else { -1 };
            // The holding pattern keeps a fight's dragon on the outer
            // ring's numbers (its `aliveCrystals() >= 0` always holds);
            // the strafe only while crystals are alive.
            let outer = if strafing { arena.alive_crystals > 0 } else { arena.fight };
            let to = if outer { to.rem_euclid(12) as usize } else { path::inner_ring(to) };
            self.path = self.find_path(arena, current, to, None);
        }
        self.navigate(rng);
    }

    /// `DragonStrafePlayerPhase.setTarget`: a path from the nearest node to
    /// the one nearest the player, ending at the player's block a little
    /// above them.
    fn strafe_route<W: BlockSource + ?Sized>(&mut self, arena: &Arena<'_>, world: &W, at: DVec3, rng: &mut Rng) {
        let current = self.closest_node(arena, world);
        let goal = self.closest_node_to(arena, at);
        let (x, z) = (at.x.floor(), at.z.floor());
        let (dx, dz) = (x - self.pos.x, z - self.pos.z);
        let lift = (f64::from(0.4f32) + (dx * dx + dz * dz).sqrt() / 80.0 - 1.0).min(10.0);
        let last = DVec3::new(x, (at.y + lift).floor(), z);
        self.path = self.find_path(arena, current, goal, Some(last));
        if self.path.is_some() {
            self.navigate(rng);
        }
    }

    /// Fly speed and turn rate of the phase (Java's `getFlySpeed` and
    /// `getTurnSpeed`).
    fn flight(&self) -> (f32, f32) {
        let speed = self.vel.with_y(0.0).length() as f32 + 1.0;
        let default_turn = 0.7 / speed.min(40.0) / speed;
        match self.phase {
            Phase::Landing => (1.5, speed.min(40.0) / speed),
            // `DragonHoverPhase.getFlySpeed`.
            Phase::Hovering => (1.0, default_turn),
            // `DragonDeathPhase.getFlySpeed`.
            Phase::Dying => (3.0, default_turn),
            Phase::Charging => (3.0, default_turn),
            _ => (0.6, default_turn),
        }
    }

    /// `EnderDragon.aiStep`'s movement toward the phase's target, in
    /// Java's arithmetic: its floats where it uses floats (the yaw, its
    /// turn, `Mth.sin`, `Mth.cos` and `Mth.atan2` tables), its doubles
    /// elsewhere, in the same order. With no target (`getFlyTargetLocation`
    /// is null while sitting, the hover aside) the dragon doesn't move and
    /// keeps its motion for when it flies again. `inWall` is the last
    /// tick's `checkWalls` of the head, neck and body: blocks it can't
    /// break slow it to 0.8.
    fn fly(&mut self) {
        if self.phase.sitting() && self.phase != Phase::Hovering {
            return;
        }
        let Some(target) = self.target else { return };
        let (fly_speed, turn_speed) = self.flight();
        let (xdd, mut ydd, zdd) = (target.x - self.pos.x, target.y - self.pos.y, target.z - self.pos.z);
        let dist = xdd * xdd + ydd * ydd + zdd * zdd;
        let max = f64::from(fly_speed);
        let horizontal = (xdd * xdd + zdd * zdd).sqrt();
        if horizontal > 0.0 {
            ydd = (ydd / horizontal).clamp(-max, max);
        }
        self.vel.y += ydd * 0.01;
        self.yaw = wrap_degrees(self.yaw);
        let aim = normalize(target - self.pos);
        let r = f64::from(self.yaw * DEGREES);
        let dir = normalize(DVec3::new(
            f64::from(crate::knockback::mth_sin(r)),
            self.vel.y,
            -f64::from(crate::knockback::mth_cos(r)),
        ));
        let dot = ((dir.dot(aim) as f32 + 0.5) / 1.5).max(0.0);
        if xdd.abs() > f64::from(1.0e-5f32) || zdd.abs() > f64::from(1.0e-5f32) {
            let want = 180.0 - mth_atan2(xdd, zdd) as f32 * (180.0f32 / std::f32::consts::PI);
            let turn = wrap_degrees(want - self.yaw).clamp(-50.0, 50.0);
            self.yaw_speed *= 0.8;
            self.yaw_speed += turn * turn_speed;
            self.yaw += self.yaw_speed * 0.1;
        }
        let span = (2.0 / (dist + 1.0)) as f32;
        // `moveRelative(0.06F * .., (0, 0, -1))`.
        let speed = f64::from(0.06f32 * (dot * span + (1.0 - span)));
        let r = f64::from(self.yaw * DEGREES);
        let (sin, cos) = (f64::from(crate::knockback::mth_sin(r)), f64::from(crate::knockback::mth_cos(r)));
        let (mx, mz) = (0.0 * speed, -speed);
        self.vel += DVec3::new(mx * cos - mz * sin, 0.0, mz * cos + mx * sin);
        self.pos += if self.in_wall { self.vel * f64::from(0.8f32) } else { self.vel };
        let actual = normalize(self.vel);
        let slide = 0.8 + 0.15 * (actual.dot(dir) + 1.0) / 2.0;
        self.vel = DVec3::new(self.vel.x * slide, self.vel.y * f64::from(0.91f32), self.vel.z * slide);
    }

    /// `checkWalls` over the head, neck and body as they now stand, in
    /// every phase: every block in them that isn't air or
    /// `#dragon_transparent` breaks, without drops, under `mobGriefing`
    /// unless it is `#dragon_immune` (a fluid stays, as `removeBlock`
    /// leaves it, and a waterlogged block leaves its water); any other is a
    /// wall (`hitWall`), which slows the next move and wing beat
    /// (`inWall`). Without `mobGriefing` every such block is a wall. Each
    /// part that broke something posts level event 2008 at a random cell of
    /// its box (v1.0 gap Track 8): an explosion particle, and no sound.
    fn check_walls<W: BlockSource + ?Sized>(
        &mut self,
        world: &W,
        griefing: bool,
        rng: &mut Rng,
        events: &mut Vec<EntityEvent>,
    ) {
        let mut wall = false;
        for i in [3, 4, 0] {
            let (_, min, max) = self.parts[i];
            let (lo, hi) = (min.floor().as_ivec3(), max.floor().as_ivec3());
            let mut broke = false;
            for x in lo.x..=hi.x {
                for y in lo.y..=hi.y {
                    for z in lo.z..=hi.z {
                        let cell = IVec3::new(x, y, z);
                        // An unloaded cell reads as void air.
                        let Some(b) = world.block(cell).filter(|&b| !transparent(b)) else { continue };
                        if griefing && !immune(b) {
                            // `removeBlock` sets `fluidState.createLegacyBlock()`:
                            // water stays as it was (nothing removed), but
                            // (v1.0 gap Track 8b) a bubble column turns into
                            // a water source, which counts.
                            if !b.is_fluid() || crate::world::bubble::is_column(b) {
                                events.push(EntityEvent::BreakBlock { cell });
                                broke = true;
                            }
                        } else {
                            wall = true;
                        }
                    }
                }
            }
            if broke {
                let mut pick = |lo: i32, hi: i32| lo + rng.next_int((hi - lo + 1) as u32) as i32;
                let cell = IVec3::new(pick(lo.x, hi.x), pick(lo.y, hi.y), pick(lo.z, hi.z));
                events.push(EntityEvent::DragonSmashed { cell });
            }
        }
        self.in_wall = wall;
    }

    /// Head position (centre of its box).
    pub fn head(&self) -> DVec3 {
        part_center(self, Part::Head)
    }

    /// Java's part offsets, from the yaw and recent heights.
    fn place_parts(&mut self) {
        let r = self.yaw.to_radians();
        let (s, c) = (r.sin() as f64, r.cos() as f64);
        let tilt = ((self.latency(5).1 - self.latency(10).1) * 10.0).to_radians();
        let (tc, ts) = (tilt.cos(), tilt.sin());
        let head_y = if self.phase.sitting() { -1.0 } else { self.latency(5).1 - self.latency(0).1 };
        let hr = (self.yaw - self.yaw_speed * 0.01).to_radians();
        let (hs, hc) = (hr.sin() as f64, hr.cos() as f64);
        let base = self.latency(5);
        let tail = |k: usize| {
            let old = self.latency(12 + k * 2);
            let a = (self.yaw + wrap_degrees(old.0 - base.0)).to_radians();
            let (ts2, tc2) = (a.sin() as f64, a.cos() as f64);
            let back = (k + 1) as f64 * 2.0;
            DVec3::new(
                -(s * 1.5 + ts2 * back) * tc,
                old.1 - base.1 - (back + 1.5) * ts + 1.5,
                (c * 1.5 + tc2 * back) * tc,
            )
        };
        let offsets = [
            (Part::Body, DVec3::new(s * 0.5, 0.0, -c * 0.5)),
            (Part::Wing(0), DVec3::new(c * 4.5, 2.0, s * 4.5)),
            (Part::Wing(1), DVec3::new(-c * 4.5, 2.0, -s * 4.5)),
            (Part::Head, DVec3::new(hs * 6.5 * tc, head_y + ts * 6.5, -hc * 6.5 * tc)),
            (Part::Neck, DVec3::new(hs * 5.5 * tc, head_y + ts * 5.5, -hc * 5.5 * tc)),
            (Part::Tail(0), tail(0)),
            (Part::Tail(1), tail(1)),
            (Part::Tail(2), tail(2)),
        ];
        self.parts = offsets.map(|(part, offset)| {
            let (w, h) = part.size();
            let feet = self.pos + offset;
            (part, feet - DVec3::new(w / 2.0, 0.0, w / 2.0), feet + DVec3::new(w / 2.0, h, w / 2.0))
        });
    }

    /// `checkCrystals`: heals a point every ten ticks while a crystal is
    /// healing it, and one tick in ten picks the nearest crystal within its
    /// box grown by 32.
    fn heal(&mut self, crystals: &Crystals, rng: &mut Rng) {
        // Java heals before the phase runs, so in the death flight
        // `DragonDeathPhase` puts the health straight back to one.
        let dying = self.phase == Phase::Dying;
        if self.healer.is_some_and(|id| crystals.get(id).is_none()) {
            self.healer = None;
        } else if self.healer.is_some() && self.ticks.is_multiple_of(10) && self.health < MAX_HEALTH && !dying {
            self.health = (self.health + 1.0).min(MAX_HEALTH);
        }
        if rng.next_f32() < 0.1 {
            // `EnderDragon` is 16 wide and 8 tall.
            let grow = DVec3::splat(HEAL_RANGE);
            let (min, max) = (self.pos - DVec3::new(8.0, 0.0, 8.0) - grow, self.pos + DVec3::new(8.0, 8.0, 8.0) + grow);
            self.healer = crystals
                .touching(min, max)
                .min_by(|a, b| a.pos.distance_squared(self.pos).total_cmp(&b.pos.distance_squared(self.pos)))
                .map(|c| c.id);
        }
    }

    /// Wings shove players away (and hurt them in flight); the head and neck
    /// bite.
    fn hit_players(&self, ctx: &Ctx, events: &mut Vec<EntityEvent>) {
        if self.immune.0 > 0 {
            return;
        }
        let body = part_center(self, Part::Body);
        let overlaps = |t: &Target, min: DVec3, max: DVec3| {
            let (pmin, pmax) = crate::player::SHAPE.aabb(t.pos);
            pmin.cmplt(max).all() && pmax.cmpgt(min).all()
        };
        for t in ctx.players.iter().filter(|t| t.targetable) {
            let wing = self.parts[1..3]
                .iter()
                .any(|&(_, min, max)| overlaps(t, min - DVec3::new(4.0, 4.0, 4.0), max + DVec3::new(4.0, 0.0, 4.0)));
            let bite = self.parts[3..5].iter().any(|&(_, min, max)| overlaps(t, min - DVec3::ONE, max + DVec3::ONE));
            if bite {
                events.push(EntityEvent::PlayerHit {
                    java: None,
                    player: t.id,
                    damage: 10.0,
                    knockback: glam::Vec3::ZERO,
                    // v1.0 final F8: `mobAttack(dragon)`.
                    cause: crate::simulation::damage::Cause::ENDER_DRAGON.by(Some(DRAGON_OWNER)),
                });
            } else if wing {
                let away = (t.pos - body).with_y(0.0);
                let d2 = away.length_squared().max(0.1);
                // Java pushes 4/d blocks a tick; in blocks a second, capped.
                let push = (away / d2 * 4.0 * 20.0).clamp_length_max(30.0) + DVec3::Y * 4.0;
                // Perched, the wings only shove.
                if self.phase.sitting() {
                    events.push(EntityEvent::Shove { player: t.id, velocity: push.as_vec3() });
                } else {
                    events.push(EntityEvent::PlayerHit {
                        java: None,
                        player: t.id,
                        damage: 5.0,
                        knockback: push.as_vec3(),
                        // v1.0 final F8: `mobAttack(dragon)`.
                        cause: crate::simulation::damage::Cause::ENDER_DRAGON.by(Some(DRAGON_OWNER)),
                    });
                }
            }
        }
    }

    /// Rises and spins for ten seconds, spilling experience toward the end.
    fn tick_death(&mut self, arena: &mut Arena<'_>, events: &mut Vec<EntityEvent>) -> bool {
        self.death_ticks += 1;
        self.healer = None;
        let xp = if arena.previously_killed { LATER_XP } else { FIRST_XP };
        if self.death_ticks == 1 {
            events.push(EntityEvent::Sound { sound: MobSound::DragonDeath, pos: self.pos });
        }
        if self.death_ticks > 150 && self.death_ticks.is_multiple_of(5) {
            events.push(EntityEvent::DragonXp { pos: self.pos, points: (xp as f32 * 0.08) as u32 });
        }
        self.pos.y += 0.1;
        self.yaw += 20.0;
        self.flap += 0.02;
        self.place_parts();
        if self.death_ticks >= DEATH_TICKS {
            events.push(EntityEvent::DragonXp { pos: self.pos, points: (xp as f32 * 0.2) as u32 });
            // v1.0 Track 9: `EnderDragon.tickDeath` ends with `entity_die`.
            events.push(EntityEvent::Vibration {
                pos: self.pos,
                event: crate::world::vibration::GameEvent::EntityDie,
                source: crate::world::vibration::EventSource::NONE,
            });
            return false;
        }
        true
    }
}

/// Air and Java's `#dragon_transparent` (`light`, `#fire`): nothing the
/// dragon meets.
fn transparent(b: Block) -> bool {
    b == Block::AIR || b.base() == Block::FIRE || b == crate::world::nether_biome_blocks::SOUL_FIRE
}

/// Java 26.3's `#dragon_immune` (of its blocks the game has; barriers,
/// command, structure, jigsaw and test blocks aren't registered).
fn immune(b: Block) -> bool {
    let base = b.base();
    b == Block::BEDROCK
        || b == Block::END_PORTAL
        || base == Block::END_PORTAL_FRAME
        || b == Block::END_GATEWAY
        || b == crate::world::redstone_blocks::MOVING
        || b == Block::OBSIDIAN
        || b == Block::CRYING_OBSIDIAN
        || b == Block::END_STONE
        || b == Block::IRON_BARS
        || crate::world::utilities::anchor_charges(b).is_some()
        || b == crate::world::jukebox::REINFORCED_DEEPSLATE
}

impl DragonFireball {
    fn new(from: DVec3, dir: DVec3) -> Self {
        Self { pos: from, previous_pos: from, vel: dir * 0.1, dir, age: 0 }
    }

    pub fn heading(&self) -> glam::Vec3 {
        self.dir.as_vec3()
    }

    /// Moves one tick; on hitting a block, a player, an End crystal or a
    /// dragon part (its own dragon's too: the dragon isn't pickable, so
    /// `leftOwner` holds at once, and `ownedBy` never names a part) it
    /// bursts into a cloud of breath, on the nearest player or dragon within
    /// four blocks. Its owner's breath never hurts a dragon.
    fn tick<W: BlockSource + ?Sized>(
        &mut self,
        world: &W,
        ctx: &Ctx,
        boss: &Boxes,
        clouds: &mut Vec<BreathCloud>,
    ) -> bool {
        self.previous_pos = self.pos;
        self.age += 1;
        self.vel = (self.vel + self.dir * 0.1) * 0.95;
        let steps = (self.vel.length() / 0.2).ceil().max(1.0) as u32;
        for _ in 0..steps {
            let next = self.pos + self.vel / steps as f64;
            let hit_block = world.block(next.floor().as_ivec3()).is_some_and(|b| b.is_solid());
            let hit_player = ctx.players.iter().any(|t| t.targetable && t.contains(next));
            let hit_boss = boss.hit_at(next, true).is_some();
            if hit_block || hit_player || hit_boss {
                let near = ctx
                    .players
                    .iter()
                    .filter(|t| t.targetable)
                    .map(|t| t.pos)
                    .chain(boss.dragons.iter().map(|&(_, feet)| feet))
                    .filter(|p| p.distance_squared(self.pos) < 16.0)
                    .min_by(|a, b| a.distance_squared(self.pos).total_cmp(&b.distance_squared(self.pos)));
                let at = near.unwrap_or(self.pos);
                clouds.push(BreathCloud::new(at, 3.0, 4.0 / 600.0, 600, 6.0));
                return false;
            }
            self.pos = next;
        }
        self.age < 600 && self.pos.y > -64.0
    }
}

impl BreathCloud {
    pub(super) fn new(pos: DVec3, radius: f32, grow: f32, duration: u32, damage: f32) -> Self {
        Self { pos, radius, grow, age: 0, duration, damage, cooldowns: Vec::new() }
    }

    /// Spreads and hurts players standing in it, once a second each.
    fn tick(&mut self, ctx: &Ctx, events: &mut Vec<EntityEvent>) -> bool {
        self.age += 1;
        self.radius += self.grow;
        self.cooldowns.retain_mut(|(_, t)| {
            *t -= 1;
            *t > 0
        });
        // Java waits ten ticks before the first dose.
        if self.age > 10 {
            for t in ctx.players.iter().filter(|t| t.targetable) {
                let d = t.pos - self.pos;
                let inside = d.y > -crate::player::HEIGHT && d.y < 0.5 && d.with_y(0.0).length() <= self.radius as f64;
                if inside && !self.cooldowns.iter().any(|&(id, _)| id == t.id) {
                    self.cooldowns.push((t.id, 20));
                    events.push(EntityEvent::PlayerHit {
                        java: None,
                        player: t.id,
                        damage: self.damage,
                        knockback: glam::Vec3::ZERO,
                        // v1.0 final F8: `indirectMagic(cloud, dragon)`.
                        cause: crate::simulation::damage::Cause::DRAGON_BREATH.by(Some(DRAGON_OWNER)),
                    });
                }
            }
        }
        self.age < self.duration
    }
}

trait CosTurns {
    fn cos_turns(self) -> f32;
}

impl CosTurns for f32 {
    fn cos_turns(self) -> f32 {
        (self * std::f32::consts::TAU).cos()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::world::terrain::Dimension;

    /// v1.0 gap Track 4b: `EnderDragon.aiStep`'s flight against the pinned
    /// Java 26.3 server's own, tick for tick
    /// (`tests/fixtures/java/26.3/dragon_flight.json`, written by
    /// `tools/extract_java_dragon_flight_fixtures.py`): a hovering dragon
    /// summoned moving and turned away coming back to where it stood, and
    /// a dying dragon with no fight flying (speed 3) to the top of (0, 0)
    /// on a superflat world, the grass at y -60, until within 10 blocks.
    #[test]
    fn the_dragon_flies_as_javas_ai_step() {
        let f: serde_json::Value =
            serde_json::from_str(include_str!("../../tests/fixtures/java/26.3/dragon_flight.json")).unwrap();
        let v3 =
            |v: &serde_json::Value| DVec3::new(v[0].as_f64().unwrap(), v[1].as_f64().unwrap(), v[2].as_f64().unwrap());
        for (name, sc) in f["scenarios"].as_object().unwrap() {
            let ticks = sc["ticks"].as_array().unwrap();
            assert!(ticks.len() > 20, "{name}: {}", ticks.len());
            let first = &ticks[0];
            let mut d = Dragon::new(v3(&first["pos"]));
            d.vel = v3(&first["motion"]);
            d.yaw = first["yaw"].as_f64().unwrap() as f32;
            let dying = name == "dying";
            d.phase = if dying { Phase::Dying } else { Phase::Hovering };
            // `DragonHoverPhase`: where it stood on its first tick;
            // `DragonDeathPhase`: the bottom centre of the top of (0, 0).
            d.target = Some(if dying { DVec3::new(0.5, -60.0, 0.5) } else { d.pos });
            for (i, t) in ticks.iter().enumerate().skip(1) {
                d.fly();
                let (pos, vel, yaw) = (v3(&t["pos"]), v3(&t["motion"]), t["yaw"].as_f64().unwrap() as f32);
                assert!(
                    d.pos.distance(pos) < 1e-9 && d.vel.distance(vel) < 1e-9 && (d.yaw - yaw).abs() < 1e-4,
                    "{name} tick {i}: {:?} {:?} {} against Java's {pos:?} {vel:?} {yaw}",
                    d.pos,
                    d.vel,
                    d.yaw
                );
                // `DragonDeathPhase` let it die within 10 blocks.
                if dying && t["health"].as_f64().unwrap() <= 0.0 {
                    let target = DVec3::new(0.5, -60.0, 0.5);
                    assert!(
                        ticks[i - 1]["pos"].is_array() && v3(&ticks[i - 1]["pos"]).distance_squared(target) < 100.0
                    );
                }
            }
        }
    }

    struct Air;
    impl BlockSource for Air {
        fn block(&self, _p: IVec3) -> Option<Block> {
            Some(Block::AIR)
        }
    }

    fn ctx(players: Vec<Target>) -> Ctx {
        Ctx {
            spawn_light: crate::simulation::lighting::SpawnLight::NIGHT,
            players,
            daylight: 0.0,
            spawning: false,
            raining: false,
            dimension: Dimension::End,
        }
    }

    /// A killing blow on a perched dragon, which dies where it sits (in
    /// flight it would first fly to the portal).
    fn kill_perched(d: &mut Dragon) {
        d.phase = Phase::SittingScanning;
        d.hurt_by(Part::Head, 500.0, true);
        assert_eq!((d.phase, d.health), (Phase::Dying, 0.0));
    }

    /// A fresh fight with its crystal on every pillar.
    fn fresh(end: &EndGen) -> (Fight, Crystals) {
        let mut fight = Fight::new(end);
        let mut crystals = Crystals::default();
        fight.spawn_pillar_crystals(&mut crystals);
        (fight, crystals)
    }

    #[test]
    fn a_new_fight_has_a_crystal_on_every_pillar_and_saves() {
        let end = EndGen::new(3);
        let (fight, crystals) = fresh(&end);
        assert_eq!(crystals.len(), 10);
        assert!(crystals.list.iter().all(|c| c.show_bottom && !c.invulnerable && c.beam.is_none()));
        assert_eq!(fight.alive_crystals(), 10);
        assert_eq!(fight.serialize(&crystals), "1,0,200,-,0,0");
        // An older save's mask still gives its pillars their crystals.
        let mut loaded = Fight::load(&end, Some("1,1,57.5,5"));
        let mut old = Crystals::default();
        loaded.spawn_pillar_crystals(&mut old);
        assert_eq!(old.len(), 2);
        assert_eq!(old.list[1].pos, end.pillars()[2].crystal());
        assert!(loaded.previously_killed);
        assert_eq!(loaded.dragon.as_ref().unwrap().health, 57.5);
        let mut dead = Fight::load(&end, Some("0,1,0,0"));
        let mut none = Crystals::default();
        dead.spawn_pillar_crystals(&mut none);
        assert!(dead.dragon.is_none() && none.is_empty());
        // A newer save leaves its crystals to the entity save.
        let mut newer = Fight::load(&end, Some("1,0,200,-,0,0"));
        newer.spawn_pillar_crystals(&mut none);
        assert!(none.is_empty());

        // Saved while dying: the death resumes and still pays out in full.
        let (mut dying, mut crystals) = fresh(&end);
        kill_perched(dying.dragon.as_mut().unwrap());
        let mut events = Vec::new();
        let mut rng = Rng::new(1);
        for _ in 0..100 {
            dying.update(TICK, &Air, &ctx(Vec::new()), &mut rng, &mut crystals, &mut events);
        }
        assert_eq!(dying.serialize(&crystals), "2,0,100,-,0,0");
        let mut reloaded = Fight::load(&end, Some(&dying.serialize(&crystals)));
        for _ in 0..DEATH_TICKS {
            reloaded.update(TICK, &Air, &ctx(Vec::new()), &mut rng, &mut crystals, &mut events);
        }
        let xp: u32 = events
            .iter()
            .filter_map(|e| if let EntityEvent::DragonXp { points, .. } = e { Some(*points) } else { None })
            .sum();
        assert_eq!(xp, FIRST_XP);
        assert!(events.contains(&EntityEvent::DragonKilled { first: true }));
        // With the kill's gateway, a new block entity (age 0, not loaded
        // here so not ticking).
        let g = end.gateways()[0];
        assert_eq!(reloaded.serialize(&crystals), format!("0,1,0,-,1,0;g {} {} {} 0 0", g.x, g.y, g.z));
    }

    /// The island for path tests: end stone up to y 59 and the pillars'
    /// obsidian and caps.
    struct Island([Pillar; 10]);
    impl BlockSource for Island {
        fn block(&self, p: IVec3) -> Option<Block> {
            if let Some(b) = self.0.iter().find_map(|s| s.block_at(p)).filter(|&b| b != Block::AIR) {
                return Some(b);
            }
            Some(if p.y < 60 { Block::END_STONE } else { Block::AIR })
        }
    }

    /// v1.0 Track 7b: the fight's dragon builds Java's nodes from the
    /// heightmap (so nodes over a pillar sit above its cap) with a floor of
    /// 73, where it used to read the island's terrain alone with a floor
    /// of 10, and half-block centres. This replaces the old
    /// "it spends most of a minute beyond 30 blocks" check, which Java's
    /// one-in-13 landing roll at each path's end doesn't promise.
    #[test]
    fn the_fight_dragon_flies_javas_nodes_round_the_outer_ring() {
        let end = EndGen::new(5);
        let island = Island(*end.pillars());
        let (mut fight, mut crystals) = fresh(&end);
        assert_eq!(fight.alive_crystals(), 10);
        let mut rng = Rng::new(1);
        let mut events = Vec::new();
        fight.update(TICK, &island, &ctx(Vec::new()), &mut rng, &mut crystals, &mut events);
        let d = fight.dragon.as_ref().unwrap();
        assert!(d.nodes_ready);
        let mut over_pillar = 0;
        for (i, node) in d.nodes.iter().enumerate() {
            let (x, z) = path::node_column(i);
            assert_eq!((node.x, node.z), (x as f64, z as f64), "whole-block nodes");
            let lift = if (12..20).contains(&i) { 15 } else { 5 };
            // The heightmap: over the highest motion-blocking block, which
            // over a pillar is its obsidian, bedrock cap or cage.
            let ground = (0..=256).rev().find(|&y| island.block(IVec3::new(x, y, z)).unwrap().is_solid()).unwrap() + 1;
            if end.pillars().iter().any(|s| s.holds(x, z)) {
                over_pillar += 1;
                assert!(ground > 70, "node {i} stands over a pillar");
            } else {
                assert_eq!(ground, 60);
            }
            assert_eq!(node.y, f64::from((ground + lift).max(path::MIN_NODE_Y)), "node {i}");
        }
        assert!(over_pillar > 0, "some inner nodes stand over a pillar");
        // With crystals alive, the holding pattern flies node to node, each
        // target up to 20 over its node, round the outer ring (nodes 0-11;
        // a turn back may cross the inner ones).
        let nodes = d.nodes;
        let mut seen = Vec::new();
        for _ in 0..20 * 120 {
            fight.update(TICK, &island, &ctx(Vec::new()), &mut rng, &mut crystals, &mut events);
            let d = fight.dragon.as_ref().unwrap();
            assert!(d.pos.is_finite());
            if d.phase == Phase::HoldingPattern
                && let Some(t) = d.target
            {
                let i = nodes.iter().position(|n| n.x == t.x && n.z == t.z);
                let i = i.unwrap_or_else(|| panic!("{t} is not a node"));
                assert!((nodes[i].y..nodes[i].y + 20.0).contains(&t.y), "{t} over {}", nodes[i]);
                if i < 12 && !seen.contains(&i) {
                    seen.push(i);
                }
            }
        }
        assert!(seen.len() >= 4, "it went round the outer ring: {seen:?}");
    }

    /// With no crystal left, Java's holding pattern still picks an outer
    /// node for a fight's dragon (`aliveCrystals() >= 0`), but `findPath`
    /// may no longer use nodes 0-11, so it flies the inner nodes towards it.
    #[test]
    fn with_no_crystals_the_fight_dragon_keeps_to_the_inner_nodes() {
        let end = EndGen::new(5);
        let island = Island(*end.pillars());
        let mut fight = Fight::new(&end);
        let mut crystals = Crystals::default();
        fight.legacy_crystals = None;
        let mut rng = Rng::new(3);
        let mut events = Vec::new();
        let mut targets = 0;
        for _ in 0..20 * 120 {
            fight.update(TICK, &island, &ctx(Vec::new()), &mut rng, &mut crystals, &mut events);
            assert_eq!(fight.alive_crystals(), 0);
            let d = fight.dragon.as_ref().unwrap();
            if matches!(d.phase, Phase::HoldingPattern | Phase::Takeoff)
                && let Some(t) = d.target
            {
                assert!(d.nodes[12..].iter().any(|n| n.x == t.x && n.z == t.z), "{t} is not an inner node");
                targets += 1;
            }
        }
        assert!(targets > 0);
    }

    /// Java's strafe: a path from the nearest node to the one nearest the
    /// player, ending at the player's block a little above them; once it's
    /// flown, the dragon heads straight for the player.
    #[test]
    fn a_strafe_follows_javas_path_to_the_player() {
        let end = EndGen::new(5);
        let island = Island(*end.pillars());
        let (mut fight, mut crystals) = fresh(&end);
        let player = Target::new(PlayerId::HOST, DVec3::new(10.3, 61.0, -5.6), true);
        let mut rng = Rng::new(4);
        let mut events = Vec::new();
        fight.update(TICK, &island, &ctx(Vec::new()), &mut rng, &mut crystals, &mut events);
        let d = fight.dragon.as_mut().unwrap();
        d.pos = DVec3::new(60.0, 90.0, 0.0);
        d.set_phase(Phase::Strafe(PlayerId::HOST));
        fight.update(TICK, &island, &ctx(vec![player]), &mut rng, &mut crystals, &mut events);
        let d = fight.dragon.as_ref().unwrap();
        let path = d.path.clone().expect("a path to the player");
        // The final node: the player's block, `floor(y + min(0.4F + dist / 80 - 1, 10))` up.
        let (dx, dz) = (10.0 - d.previous_pos.x, -6.0 - d.previous_pos.z);
        let lift = (f64::from(0.4f32) + (dx * dx + dz * dz).sqrt() / 80.0 - 1.0).min(10.0);
        assert_eq!(path.first(), Some(&DVec3::new(10.0, (61.0 + lift).floor(), -6.0)));
        let t = d.target.unwrap();
        assert!(d.nodes.iter().any(|n| n.x == t.x && n.z == t.z), "first it flies to a node: {t}");
    }

    #[test]
    fn head_hits_count_fully_and_body_hits_quarter() {
        let end = EndGen::new(5);
        let mut fight = Fight::new(&end);
        assert!(fight.strike(Hit::Dragon(Part::Head), Blow::melee(8.0)));
        assert_eq!(fight.dragon.as_ref().unwrap().health, 192.0);
        // A weaker hit inside the immunity window lands (`EnderDragon.hurt`
        // is true) but does nothing.
        assert!(fight.strike(Hit::Dragon(Part::Body), Blow::melee(8.0)));
        assert_eq!(fight.dragon.as_ref().unwrap().health, 192.0);
        fight.dragon.as_mut().unwrap().immune = (0, 0.0);
        assert!(fight.strike(Hit::Dragon(Part::Wing(0)), Blow::melee(8.0)));
        assert_eq!(fight.dragon.as_ref().unwrap().health, 189.0);
        // 26.3 counts the neck as fully as the head.
        fight.dragon.as_mut().unwrap().immune = (0, 0.0);
        assert!(fight.strike(Hit::Dragon(Part::Neck), Blow::melee(8.0)));
        assert_eq!(fight.dragon.as_ref().unwrap().health, 181.0);
    }

    #[test]
    fn breaking_the_healing_crystal_hurts_the_dragon_and_explodes() {
        let end = EndGen::new(5);
        let (mut fight, mut crystals) = fresh(&end);
        let healer = crystals.list[4].id;
        fight.dragon.as_mut().unwrap().healer = Some(healer);
        let player = Target::new(PlayerId::HOST, DVec3::new(0.0, 70.0, 0.0), true);
        let mut wild = Wild::default();
        let mut targets = Targets { fight: Some(&mut fight), wild: &mut wild, crystals: &mut crystals };
        assert!(targets.strike(Hit::Crystal(healer), Blow::arrow(1.0, true), Some(PlayerId::HOST)));
        let mut events = Vec::new();
        fight.update(TICK, &Air, &ctx(vec![player]), &mut Rng::new(1), &mut crystals, &mut events);
        assert_eq!(crystals.len(), 9);
        assert_eq!(fight.alive_crystals(), 9);
        assert!(events.iter().any(|e| matches!(e, EntityEvent::Explosion { power, .. } if *power == CRYSTAL_POWER)));
        let d = fight.dragon.as_ref().unwrap();
        assert!(d.health <= 190.0);
        // Circling, it turns on the player who did it.
        assert_eq!(d.phase, Phase::Strafe(PlayerId::HOST));
    }

    #[test]
    fn death_takes_ten_seconds_and_pays_out_experience() {
        let end = EndGen::new(5);
        let (mut fight, mut crystals) = fresh(&end);
        crystals.list.clear();
        // Perched, a killing blow ends it where it sits.
        kill_perched(fight.dragon.as_mut().unwrap());
        let mut events = Vec::new();
        let mut rng = Rng::new(1);
        for _ in 0..DEATH_TICKS + 5 {
            fight.update(TICK, &Air, &ctx(Vec::new()), &mut rng, &mut crystals, &mut events);
        }
        assert!(fight.dragon.is_none());
        assert!(fight.previously_killed);
        let xp: u32 = events
            .iter()
            .filter_map(|e| if let EntityEvent::DragonXp { points, .. } = e { Some(*points) } else { None })
            .sum();
        assert_eq!(xp, 10 * 960 + 2400);
        assert!(events.contains(&EntityEvent::DragonKilled { first: true }));
        // The kill's gateway goes up at the first of the 20 spots.
        let gateway = end.gateways()[0];
        assert!(events.contains(&EntityEvent::BuildGateway { pos: gateway }));
        // The new gateway's block entity starts at age 0 (its spawn beam).
        let saved = format!("0,1,0,-,1,0;g {} {} {} 0 0", gateway.x, gateway.y, gateway.z);
        assert_eq!(fight.serialize(&crystals), saved);
        assert!(fight.gateway_entities.get(gateway).is_some_and(|g| g.spawning()));

        // Gateway links survive a save.
        let exit = IVec3::new(900, 80, -400);
        fight.link_gateways(gateway, exit);
        fight.link_gateways(exit, gateway);
        let loaded = Fight::load(&end, Some(&fight.serialize(&crystals)));
        assert_eq!(loaded.gateway_exit(gateway), Some(exit));
        assert_eq!(loaded.gateway_exit(exit), Some(gateway));
        assert_eq!(loaded.serialize(&crystals), fight.serialize(&crystals));
    }

    /// v1.0 gap Track 4: `handleKillingBlow` in flight holds the dragon
    /// at one health in `DYING`, and `DragonDeathPhase` flies it (at fly
    /// speed 3) to the top of the portal's column; it dies within 10 blocks
    /// of it, and only then does `tickDeath` rise and pay out.
    #[test]
    fn a_dragon_killed_in_flight_flies_to_the_portal_first() {
        let end = EndGen::new(5);
        let island = Island(*end.pillars());
        let (mut fight, mut crystals) = fresh(&end);
        crystals.list.clear();
        let mut rng = Rng::new(2);
        let mut events = Vec::new();
        fight.update(TICK, &island, &ctx(Vec::new()), &mut rng, &mut crystals, &mut events);
        let d = fight.dragon.as_mut().unwrap();
        d.pos = DVec3::new(80.0, 100.0, 30.0);
        d.hurt_by(Part::Head, 500.0, true);
        assert_eq!((d.phase, d.health), (Phase::Dying, 1.0));
        // Dead is dead: no more damage in `DYING`.
        assert!(!d.hurt_by(Part::Head, 50.0, true));
        // A save in the flight reloads it, still flying to the portal.
        let saved = fight.serialize(&crystals);
        assert!(saved.starts_with("1,0,1,") && saved.contains(";death_flight"), "{saved}");
        let loaded = Fight::load(&end, Some(&saved));
        let d = loaded.dragon.as_ref().unwrap();
        assert_eq!((d.phase, d.health), (Phase::Dying, 1.0));
        // The portal's column tops out on the island here (y 60).
        let goal = DVec3::new(0.5, 60.0, 0.5);
        let mut ticks = 0;
        while fight.dragon.as_ref().is_some_and(|d| d.health > 0.0) {
            fight.update(TICK, &island, &ctx(Vec::new()), &mut rng, &mut crystals, &mut events);
            ticks += 1;
            assert!(ticks < 2000, "never reached the portal");
            assert!(!events.iter().any(|e| matches!(e, EntityEvent::DragonXp { .. })));
        }
        // It died on the tick it came within 10 blocks of the target.
        let d = fight.dragon.as_ref().unwrap();
        assert_eq!(d.target, Some(goal));
        assert_eq!(d.death_ticks, 0);
        assert!(d.previous_pos.distance_squared(goal) < 10.0 * 10.0, "{}", d.previous_pos);
        assert!(ticks > 20, "it flew {ticks} ticks");
        for _ in 0..DEATH_TICKS + 5 {
            fight.update(TICK, &island, &ctx(Vec::new()), &mut rng, &mut crystals, &mut events);
        }
        assert!(fight.dragon.is_none());
        assert!(events.contains(&EntityEvent::DragonKilled { first: true }));
    }

    /// v1.0 gap Track 4b: in `DYING`, in the flight to the portal and in
    /// the death that follows, the parts are still there to hit, and
    /// nothing hurts them: an arrow bounces off (Java's `EnderDragon.hurt`
    /// refuses the hit and the arrow is deflected) rather than flying on
    /// through.
    #[test]
    fn a_dying_dragon_is_hit_but_not_hurt() {
        let end = EndGen::new(5);
        let island = Island(*end.pillars());
        let (mut fight, mut crystals) = fresh(&end);
        crystals.list.clear();
        let mut rng = Rng::new(2);
        let mut events = Vec::new();
        fight.update(TICK, &island, &ctx(Vec::new()), &mut rng, &mut crystals, &mut events);
        let d = fight.dragon.as_mut().unwrap();
        d.pos = DVec3::new(80.0, 100.0, 30.0);
        d.place_parts();
        d.hurt_by(Part::Head, 500.0, true);
        assert_eq!((d.phase, d.health), (Phase::Dying, 1.0));
        let head = part_center(d, Part::Head);
        let from = head + DVec3::new(0.0, 0.0, -20.0);
        let (hit, t) = fight.raycast(from, DVec3::Z, 40.0).expect("the dying dragon's parts are hit");
        assert!(t < 20.0);
        assert!(fight.hit_at(head).is_some());
        assert!(!fight.strike(hit, Blow::arrow(9.0, true)), "nothing hurts a dying dragon");
        assert!(!fight.strike(hit, Blow::melee(9.0)));
        assert_eq!(fight.dragon.as_ref().unwrap().health, 1.0);
        // Nor once it has died and rises (`tickDeath`).
        fight.dragon.as_mut().unwrap().health = 0.0;
        let head = part_center(fight.dragon.as_ref().unwrap(), Part::Head);
        let (hit, _) = fight.raycast(head + DVec3::new(0.0, 0.0, -20.0), DVec3::Z, 40.0).unwrap();
        assert!(!fight.strike(hit, Blow::arrow(9.0, true)));
    }

    /// v1.0 gap Track 4b: the roar (`SITTING_ATTACKING`) is Java's client
    /// growl every tick of the phase; the growl now and then
    /// (`growlTime`) is only for a dragon not sitting, first after 101
    /// ticks.
    #[test]
    fn the_roar_growls_every_tick_and_the_perched_dragon_is_otherwise_quiet() {
        let roars = |events: &[EntityEvent]| {
            events.iter().filter(|e| matches!(e, EntityEvent::Sound { sound: MobSound::DragonRoar, .. })).count()
        };
        let end = EndGen::new(5);
        let island = Island(*end.pillars());
        let (mut fight, mut crystals) = fresh(&end);
        let mut rng = Rng::new(4);
        let mut events = Vec::new();
        fight.update(TICK, &island, &ctx(Vec::new()), &mut rng, &mut crystals, &mut events);
        let d = fight.dragon.as_mut().unwrap();
        d.set_phase(Phase::SittingScanning);
        d.growl = 0;
        events.clear();
        for _ in 0..20 {
            fight.update(TICK, &island, &ctx(Vec::new()), &mut rng, &mut crystals, &mut events);
        }
        assert_eq!(roars(&events), 0, "a sitting dragon doesn't growl by itself");
        fight.dragon.as_mut().unwrap().set_phase(Phase::SittingAttacking);
        events.clear();
        let mut ticks = 0;
        while fight.dragon.as_ref().unwrap().phase == Phase::SittingAttacking {
            fight.update(TICK, &island, &ctx(Vec::new()), &mut rng, &mut crystals, &mut events);
            ticks += 1;
        }
        assert_eq!(roars(&events), ticks - 1, "one growl a tick of the roar");
        // In flight: the first growl on the 101st tick.
        let mut d = Dragon::new(DVec3::new(0.0, 100.0, 0.0));
        let mut events = Vec::new();
        let mut arena_fireballs = Vec::new();
        let mut arena_clouds = Vec::new();
        let mut arena = Arena {
            fight: false,
            alive_crystals: 0,
            previously_killed: true,
            podium: IVec3::ZERO,
            fireballs: &mut arena_fireballs,
            clouds: &mut arena_clouds,
            top: 256,
            floor: 0,
            griefing: true,
        };
        d.phase = Phase::HoldingPattern;
        let first = (1..=200)
            .find(|_| {
                events.clear();
                d.tick(&mut arena, &Air, &ctx(Vec::new()), &mut rng, &Crystals::default(), &mut events);
                roars(&events) > 0
            })
            .unwrap();
        assert_eq!(first, 101);
    }

    /// v1.0 gap Track 4: `DragonLandingPhase` comes down on the bottom
    /// centre of the egg's cell and, within a block of it, resets the flame
    /// count and sits scanning (`SITTING_SCANNING`); with no one about it
    /// takes off after 100 ticks.
    #[test]
    fn a_landing_dragon_sits_scanning_on_the_portal() {
        let end = EndGen::new(5);
        let island = Island(*end.pillars());
        let (mut fight, mut crystals) = fresh(&end);
        let mut rng = Rng::new(3);
        let mut events = Vec::new();
        fight.update(TICK, &island, &ctx(Vec::new()), &mut rng, &mut crystals, &mut events);
        let d = fight.dragon.as_mut().unwrap();
        d.pos = DVec3::new(30.0, 85.0, 0.0);
        d.flame_count = 3;
        d.set_phase(Phase::Landing);
        let mut ticks = 0;
        while fight.dragon.as_ref().unwrap().phase == Phase::Landing {
            fight.update(TICK, &island, &ctx(Vec::new()), &mut rng, &mut crystals, &mut events);
            ticks += 1;
            assert!(ticks < 1000, "never landed");
        }
        let d = fight.dragon.as_ref().unwrap();
        assert_eq!(d.phase, Phase::SittingScanning);
        assert_eq!(d.flame_count, 0);
        // No snapping: it is where its flight took it, within a block of
        // the perch (above the podium's pillar; this island stops at 60).
        assert!(fight.podium.y + 4 >= 60);
        let perch = fight.podium.as_dvec3() + DVec3::new(0.5, 4.0, 0.5);
        assert!(d.previous_pos.distance_squared(perch) < 1.0, "{}", d.previous_pos);
        for _ in 0..100 {
            fight.update(TICK, &island, &ctx(Vec::new()), &mut rng, &mut crystals, &mut events);
        }
        assert_ne!(fight.dragon.as_ref().unwrap().phase, Phase::SittingScanning);
    }

    /// v1.0 gap Track 4: a player within 20 blocks but more than 10 above
    /// or below isn't scanned; one level with it is roared at after 25
    /// ticks, and the roar lasts 40 ticks more (`attackingTicks++ >= 40`).
    #[test]
    fn the_scanning_dragon_looks_for_players_level_with_it() {
        let end = EndGen::new(5);
        let island = Island(*end.pillars());
        let (mut fight, mut crystals) = fresh(&end);
        let mut rng = Rng::new(3);
        let mut events = Vec::new();
        fight.update(TICK, &island, &ctx(Vec::new()), &mut rng, &mut crystals, &mut events);
        let d = fight.dragon.as_mut().unwrap();
        d.pos = DVec3::new(0.5, 60.0, 0.5);
        d.set_phase(Phase::SittingScanning);
        let above = Target::new(PlayerId::HOST, DVec3::new(5.0, 72.0, 5.0), true);
        for _ in 0..30 {
            fight.update(TICK, &island, &ctx(vec![above]), &mut rng, &mut crystals, &mut events);
        }
        assert_eq!(fight.dragon.as_ref().unwrap().phase, Phase::SittingScanning);
        let level = Target::new(PlayerId::HOST, DVec3::new(5.0, 61.0, 5.0), true);
        fight.update(TICK, &island, &ctx(vec![level]), &mut rng, &mut crystals, &mut events);
        let d = fight.dragon.as_ref().unwrap();
        assert_eq!(d.phase, Phase::SittingAttacking);
        // `attackingTicks` was 0 on the roar's first tick (the scan's
        // last): it flames on the 40th tick after.
        for _ in 0..39 {
            fight.update(TICK, &island, &ctx(vec![level]), &mut rng, &mut crystals, &mut events);
        }
        assert_eq!(fight.dragon.as_ref().unwrap().phase, Phase::SittingAttacking);
        fight.update(TICK, &island, &ctx(vec![level]), &mut rng, &mut crystals, &mut events);
        assert_eq!(fight.dragon.as_ref().unwrap().phase, Phase::SittingFlaming);
    }

    /// v1.0 gap Track 4: `DragonStrafePlayerPhase` keeps its
    /// `attackTarget`: a player who turns creative is still strafed (and
    /// shot at), where VoxelCraft used to give up on them.
    #[test]
    fn a_player_turning_creative_is_still_strafed() {
        let end = EndGen::new(5);
        let island = Island(*end.pillars());
        let (mut fight, mut crystals) = fresh(&end);
        let mut rng = Rng::new(4);
        let mut events = Vec::new();
        fight.update(TICK, &island, &ctx(Vec::new()), &mut rng, &mut crystals, &mut events);
        let d = fight.dragon.as_mut().unwrap();
        d.pos = DVec3::new(60.0, 90.0, 0.0);
        let player = Target::new(PlayerId::HOST, DVec3::new(10.3, 61.0, -5.6), true);
        d.strafe(&player);
        fight.update(TICK, &island, &ctx(vec![player]), &mut rng, &mut crystals, &mut events);
        let creative = Target::new(PlayerId::HOST, DVec3::new(10.3, 61.0, -5.6), false);
        let mut shot = false;
        for _ in 0..600 {
            fight.update(TICK, &island, &ctx(vec![creative]), &mut rng, &mut crystals, &mut events);
            match fight.dragon.as_ref().unwrap().phase {
                Phase::Strafe(PlayerId::HOST) => {}
                _ => {
                    shot = !fight.fireballs.is_empty();
                    break;
                }
            }
        }
        assert!(shot, "the strafe ended without a fireball");
        // Breaking a crystal in creative doesn't start one (`canAttack`).
        let d = fight.dragon.as_mut().unwrap();
        d.set_phase(Phase::HoldingPattern);
        let id = crystals.list[0].id;
        let at = crystals.list[0].pos;
        fight.crystal_destroyed(
            Destroyed { id, pos: at, by: Some(PlayerId::HOST) },
            &ctx(vec![creative]),
            &mut crystals,
            &mut events,
        );
        assert_eq!(fight.dragon.as_ref().unwrap().phase, Phase::HoldingPattern);
    }

    /// Loaded only within `radius` blocks of the origin (horizontally).
    struct Near(i32);
    impl BlockSource for Near {
        fn block(&self, p: IVec3) -> Option<Block> {
            (p.x.abs() <= self.0 && p.z.abs() <= self.0).then_some(Block::AIR)
        }
    }

    #[test]
    fn the_exit_portal_waits_for_its_whole_footprint() {
        let end = EndGen::new(5);
        let (mut fight, mut crystals) = fresh(&end);
        crystals.list.clear();
        kill_perched(fight.dragon.as_mut().unwrap());
        let mut events = Vec::new();
        let mut rng = Rng::new(1);
        for _ in 0..DEATH_TICKS + 5 {
            fight.update(TICK, &Near(2), &ctx(Vec::new()), &mut rng, &mut crystals, &mut events);
        }
        assert!(!events.iter().any(|e| matches!(e, EntityEvent::DragonKilled { .. })));
        assert!(fight.serialize(&crystals).starts_with("3,0,"));
        fight.update(TICK, &Near(16), &ctx(Vec::new()), &mut rng, &mut crystals, &mut events);
        assert!(events.contains(&EntityEvent::DragonKilled { first: true }));
    }

    #[test]
    fn crystals_hit_together_all_blow_up() {
        let end = EndGen::new(5);
        let (mut fight, mut crystals) = fresh(&end);
        let ids: Vec<u32> = [5, 9, 0].iter().map(|&i| crystals.list[i].id).collect();
        for &id in &ids {
            crystals.hit(id, CrystalBlow::Other);
        }
        let mut events = Vec::new();
        fight.update(TICK, &Air, &ctx(Vec::new()), &mut Rng::new(1), &mut crystals, &mut events);
        assert_eq!(crystals.len(), 7);
        let blasts = events.iter().filter(|e| matches!(e, EntityEvent::Explosion { .. })).count();
        assert_eq!(blasts, 3);
        assert!(ids.iter().all(|&id| crystals.get(id).is_none()));
    }

    #[test]
    fn strafing_dragons_spit_fireballs_that_leave_breath() {
        let end = EndGen::new(5);
        let (mut fight, mut crystals) = fresh(&end);
        let player = Target::new(PlayerId::HOST, DVec3::new(0.0, 70.0, 0.0), true);
        let d = fight.dragon.as_mut().unwrap();
        d.pos = DVec3::new(0.0, 90.0, 50.0);
        d.yaw = 180.0;
        d.set_phase(Phase::Strafe(PlayerId::HOST));
        let mut rng = Rng::new(2);
        let mut events = Vec::new();
        let mut shot = false;
        for _ in 0..20 * 30 {
            fight.update(TICK, &Air, &ctx(vec![player]), &mut rng, &mut crystals, &mut events);
            shot |= !fight.fireballs.is_empty();
            if !fight.clouds.is_empty() {
                break;
            }
        }
        assert!(shot, "the dragon never fired");
        assert_eq!(fight.clouds.len(), 1);
        assert!(fight.clouds[0].pos.distance(player.pos) < 4.0);
    }

    #[test]
    fn perched_dragons_shrug_off_arrows() {
        let end = EndGen::new(5);
        let mut fight = Fight::new(&end);
        fight.dragon.as_mut().unwrap().phase = Phase::SittingScanning;
        assert!(!fight.strike(Hit::Dragon(Part::Head), Blow::arrow(9.0, true)));
        assert!(fight.strike(Hit::Dragon(Part::Head), Blow::melee(9.0)));
    }

    #[test]
    fn breath_clouds_hurt_once_a_second() {
        let mut cloud = BreathCloud::new(DVec3::ZERO, 3.0, 0.0, 100, 6.0);
        let players = vec![Target::new(PlayerId::HOST, DVec3::new(1.0, 0.0, 0.0), true)];
        let mut events = Vec::new();
        for _ in 0..40 {
            cloud.tick(&ctx(players.clone()), &mut events);
        }
        assert_eq!(events.len(), 2);
    }

    /// A dead dragon's End with `n` crystals on the exit portal's edges.
    fn ritual_ready(end: &EndGen, edges: usize) -> (Fight, Crystals, Vec<u32>) {
        let mut fight = Fight::load(end, Some("0,1,0,-,1,0"));
        let mut crystals = Crystals::default();
        fight.spawn_pillar_crystals(&mut crystals);
        let center = fight.podium + IVec3::Y;
        let ids = [IVec3::NEG_Z, IVec3::X, IVec3::Z, IVec3::NEG_X]
            .iter()
            .take(edges)
            .map(|&d| crystals.place((center + d * 3).as_dvec3() + DVec3::new(0.5, 0.0, 0.5)))
            .collect();
        fight.crystal_placed();
        (fight, crystals, ids)
    }

    /// Runs `ticks` ticks with a player by the portal, doing what the game
    /// does for [`EntityEvent::RespawnPillar`] (a new invulnerable crystal
    /// beaming at the centre) and collecting the events.
    fn run(fight: &mut Fight, crystals: &mut Crystals, ticks: u32, events: &mut Vec<(u32, EntityEvent)>) {
        let player = Target::new(PlayerId::HOST, DVec3::new(0.0, 70.0, 6.0), true);
        let mut rng = Rng::new(9);
        for t in 0..ticks {
            let mut out = Vec::new();
            fight.update(TICK, &Air, &ctx(vec![player]), &mut rng, crystals, &mut out);
            for e in out {
                if let EntityEvent::RespawnPillar { index } = e {
                    let p = fight.pillars()[index];
                    crystals.spawn(p.crystal(), Some(RESPAWN_BEAM), true, true);
                }
                events.push((t, e));
            }
        }
    }

    #[test]
    fn four_edge_crystals_respawn_the_dragon_on_javas_timetable() {
        let end = EndGen::new(11);
        let (mut fight, mut crystals, ritual) = ritual_ready(&end, 4);
        // The old pillar crystals go in the pillar blasts (here, by hand).
        for id in crystals.list.iter().filter(|c| c.show_bottom).map(|c| c.id).collect::<Vec<_>>() {
            crystals.discard(id);
        }
        let mut events = Vec::new();
        run(&mut fight, &mut crystals, 1, &mut events);
        assert!(events.iter().any(|(_, e)| *e == EntityEvent::ExitPortal { active: false }));
        let r = fight.respawn.clone().expect("the ritual started");
        assert_eq!(r.stage, Stage::PreparingToSummonPillars);
        assert_eq!(r.crystals, ritual);
        assert!(ritual.iter().all(|&id| crystals.get(id).unwrap().beam == Some(RESPAWN_BEAM)));

        // START took one tick; PREPARING_TO_SUMMON_PILLARS 101, growling at
        // 0, 50-52 and 95-99; each pillar's beam at 40 i and rebuild at
        // 40 i + 39 of SUMMONING_PILLARS (401 ticks); SUMMONING_DRAGON 101.
        run(&mut fight, &mut crystals, 101 + 401 + 100, &mut events);
        assert!(fight.dragon.is_none(), "not yet");
        let growls: Vec<u32> = events
            .iter()
            .filter(|(_, e)| matches!(e, EntityEvent::Sound { sound: MobSound::DragonGrowl, .. }))
            .map(|&(t, _)| t)
            .collect();
        // (Ticks counted from PREPARING_TO_SUMMON_PILLARS' first.)
        let mut expected: Vec<u32> = vec![0, 50, 51, 52, 95, 96, 97, 98, 99];
        expected.extend((1..5).map(|t| 101 + 401 + t));
        expected.extend((80..100).map(|t| 101 + 401 + t));
        assert_eq!(growls, expected);
        let pillars: Vec<(u32, usize)> = events
            .iter()
            .filter_map(|&(t, e)| if let EntityEvent::RespawnPillar { index } = e { Some((t, index)) } else { None })
            .collect();
        assert_eq!(pillars, (0..10).map(|i| (101 + 40 * i as u32 + 39, i)).collect::<Vec<_>>());
        run(&mut fight, &mut crystals, 1, &mut events);
        let d = fight.dragon.as_ref().expect("the dragon is back");
        assert_eq!(d.health, MAX_HEALTH);
        // Made at (0, 128, 0) and flown one tick since.
        assert!(d.pos.distance(SPAWN) < 2.0, "{}", d.pos);
        assert!(fight.respawn.is_none());
        // The ritual crystals blow up without breaking blocks and are gone;
        // the new pillar crystals lose their beams and invulnerability.
        let quiet = events.iter().filter(|(_, e)| matches!(e, EntityEvent::QuietBlast { .. })).count();
        assert_eq!(quiet, 4);
        assert!(ritual.iter().all(|&id| crystals.get(id).is_none()));
        assert_eq!(crystals.len(), 10);
        assert!(crystals.list.iter().all(|c| !c.invulnerable && c.beam.is_none()));

        // The second kill: 500 experience, no egg, the next gateway.
        kill_perched(fight.dragon.as_mut().unwrap());
        let mut out = Vec::new();
        let mut rng = Rng::new(1);
        crystals.list.clear();
        for _ in 0..DEATH_TICKS + 5 {
            fight.update(TICK, &Air, &ctx(Vec::new()), &mut rng, &mut crystals, &mut out);
        }
        let xp: u32 = out
            .iter()
            .filter_map(|e| if let EntityEvent::DragonXp { points, .. } = e { Some(*points) } else { None })
            .sum();
        assert_eq!(xp, 10 * 40 + 100);
        assert!(out.contains(&EntityEvent::DragonKilled { first: false }));
        assert!(out.contains(&EntityEvent::BuildGateway { pos: end.gateways()[1] }));
    }

    #[test]
    fn the_ritual_needs_all_four_edges_players_and_its_crystals() {
        let end = EndGen::new(11);
        // Three edges: nothing.
        let (mut fight, mut crystals, _) = ritual_ready(&end, 3);
        let mut events = Vec::new();
        run(&mut fight, &mut crystals, 5, &mut events);
        assert!(fight.respawn.is_none() && events.iter().all(|(_, e)| !matches!(e, EntityEvent::ExitPortal { .. })));
        // A living dragon: nothing.
        let (mut fight, mut crystals, _) = ritual_ready(&end, 4);
        fight.dragon = Some(Dragon::new(SPAWN));
        run(&mut fight, &mut crystals, 5, &mut events);
        assert!(fight.respawn.is_none());

        // With nobody near, the ritual waits.
        let (mut fight, mut crystals, ritual) = ritual_ready(&end, 4);
        let mut out = Vec::new();
        let mut rng = Rng::new(1);
        for _ in 0..50 {
            fight.update(TICK, &Air, &ctx(Vec::new()), &mut rng, &mut crystals, &mut out);
        }
        assert_eq!(fight.respawn.as_ref().unwrap().stage, Stage::Start);

        // Breaking one of its crystals ends it: the portal opens again and
        // the rebuilt pillars' crystals stop beaming.
        run(&mut fight, &mut crystals, 200, &mut events);
        assert_eq!(fight.respawn.as_ref().unwrap().stage, Stage::SummoningPillars);
        assert!(crystals.list.iter().any(|c| c.invulnerable));
        crystals.hit(ritual[2], CrystalBlow::Player { id: PlayerId::HOST, creative: false });
        events.clear();
        run(&mut fight, &mut crystals, 1, &mut events);
        assert!(fight.respawn.is_none());
        assert!(events.iter().any(|(_, e)| *e == EntityEvent::ExitPortal { active: true }));
        assert!(crystals.list.iter().all(|c| !c.invulnerable && (c.beam.is_none() || ritual.contains(&c.id))));
        assert!(fight.dragon.is_none());
    }

    /// A save settles a hit still queued for the next tick (Java resolves
    /// it at once): the ritual's crystal goes and blows up, the ritual ends
    /// and the portal opens, before anything is written.
    #[test]
    fn a_save_settles_a_queued_hit_on_a_ritual_crystal() {
        let end = EndGen::new(11);
        let (mut fight, mut crystals, ritual) = ritual_ready(&end, 4);
        let mut events = Vec::new();
        run(&mut fight, &mut crystals, 1 + 101 + 10, &mut events);
        assert!(fight.respawn.is_some());
        assert!(crystals.hit(ritual[2], CrystalBlow::Player { id: PlayerId::HOST, creative: false }));
        let mut e = crate::entity::Entities::new(1);
        e.fight = Some(fight);
        e.crystals = crystals;
        let mut out = Vec::new();
        e.settle_crystal_hits(&ctx(Vec::new()), &mut out);
        assert!(!e.crystals.hits_pending());
        assert!(e.crystals.get(ritual[2]).is_none(), "the hit crystal is gone");
        assert!(out.iter().any(|ev| matches!(ev, EntityEvent::Explosion { .. })), "with its blast");
        assert!(out.contains(&EntityEvent::ExitPortal { active: true }), "the ritual ends");
        assert!(e.fight.as_ref().unwrap().respawn.is_none());
    }

    /// A hit on a ritual crystal right after loading a mid-ritual save,
    /// settled before the fight's first tick, still ends the ritual.
    #[test]
    fn a_hit_before_a_loaded_ritual_ticks_still_ends_it() {
        let end = EndGen::new(11);
        let (mut fight, mut crystals, _) = ritual_ready(&end, 4);
        let mut events = Vec::new();
        run(&mut fight, &mut crystals, 1 + 101 + 10, &mut events);
        let (text, saved) = (fight.serialize(&crystals), crystals.serialize());
        let mut e = crate::entity::Entities::new(1);
        e.fight = Some(Fight::load(&end, Some(&text)));
        e.crystals.load(&saved);
        let ritual = e.fight.as_ref().unwrap().respawn_saved.as_ref().unwrap().2[0];
        let id = e.crystals.list.iter().find(|c| c.pos.distance_squared(ritual) < 1e-6).unwrap().id;
        assert!(e.crystals.hit(id, CrystalBlow::Player { id: PlayerId::HOST, creative: false }));
        let mut out = Vec::new();
        e.settle_crystal_hits(&ctx(Vec::new()), &mut out);
        assert!(out.contains(&EntityEvent::ExitPortal { active: true }), "the ritual ends");
        assert!(e.fight.as_ref().unwrap().respawn.is_none());
    }

    #[test]
    fn a_save_before_the_fight_ticks_preserves_the_ritual_start() {
        use crate::{
            inventory::Stack,
            session::{
                PlayerSession,
                interact::{UseContext, Used},
            },
            world::{
                World,
                chunk::{ChunkData, chunk_of},
                terrain::Generator,
            },
        };
        use std::{collections::BTreeMap, sync::Arc};

        let generator = Arc::new(Generator::for_dimension(11, Dimension::End));
        let end = generator.end().unwrap();
        let podium = end.podium();
        let mut world = World::new_remote(Arc::clone(&generator), 2);
        for x in [-4, 4] {
            for z in [-4, 4] {
                for y in [-1, 3] {
                    let chunk = chunk_of(podium + IVec3::new(x, y, z));
                    world.receive_chunk(chunk, Arc::new(ChunkData::Uniform(Block::AIR)));
                }
            }
        }
        world.set_exit_portal(true, false);
        let mut entities = crate::entity::Entities::new(11);
        entities.fight = Some(Fight::load(end, Some("0,1,0,-,1,0")));
        let mut player = PlayerSession::new(PlayerId::HOST, DVec3::ZERO);
        player.inventory.slots[0] = Some(Stack::new(crate::entity::crystal::END_CRYSTAL, 4));
        for outward in [IVec3::NEG_Z, IVec3::X, IVec3::Z, IVec3::NEG_X] {
            let base = podium + outward * 3;
            let center = base.as_dvec3() + DVec3::splat(0.5);
            player.player.pos = center + (outward * 2).as_dvec3() + DVec3::Y * 0.5;
            let aim = center - player.player.eye();
            player.player.yaw = aim.z.atan2(aim.x) as f32;
            player.player.pitch = aim.y.atan2(aim.x.hypot(aim.z)) as f32;
            let placed = player.use_held(0, &mut world, &mut entities, &UseContext { sneaking: false, others: &[] });
            assert!(matches!(placed.unwrap().unwrap().used, Used::CrystalPlaced { .. }));
        }
        assert!(player.inventory.get(0).is_none());

        let fight = entities.fight.as_ref().unwrap();
        assert!(fight.respawn.is_none());
        let mut props = BTreeMap::new();
        entities.save_props(&mut props);
        props.insert("dragon".into(), fight.serialize(&entities.crystals));

        let mut reloaded = crate::entity::Entities::new(11);
        reloaded.load_props(&props);
        let mut fight = Fight::load(end, props.get("dragon").map(String::as_str));
        fight.restore_crystals(&mut reloaded.crystals, props.contains_key("end_crystals"));
        assert_eq!(reloaded.crystals.len(), 4);

        let mut reloaded_world = World::new_remote(Arc::clone(&generator), 2);
        let portal = podium - IVec3::X;
        let missing_chunk = chunk_of(portal);
        let mut last_chunk = None;
        for (pos, chunk) in world.network_chunks(chunk_of(podium), 2) {
            if pos == missing_chunk {
                last_chunk = Some(chunk);
            } else {
                reloaded_world.receive_chunk(pos, chunk);
            }
        }
        assert_eq!(reloaded_world.get_block(podium + IVec3::X), Some(Block::END_PORTAL));
        assert_eq!(reloaded_world.get_block(portal), None);
        let mut events = Vec::new();
        let mut rng = Rng::new(9);
        let ctx = ctx(vec![Target::new(PlayerId::HOST, player.player.pos, true)]);
        fight.update(TICK, &reloaded_world, &ctx, &mut rng, &mut reloaded.crystals, &mut events);
        assert!(fight.respawn.is_none(), "the ritual waits for the podium chunks");
        assert!(!events.contains(&EntityEvent::ExitPortal { active: false }));

        reloaded_world.receive_chunk(missing_chunk, last_chunk.expect("the missing side of the portal"));
        assert_eq!(reloaded_world.get_block(portal), Some(Block::END_PORTAL));
        fight.update(TICK, &reloaded_world, &ctx, &mut rng, &mut reloaded.crystals, &mut events);
        let ritual = fight.respawn.as_ref().expect("the ritual starts as soon as the podium loads");
        assert_eq!((ritual.stage, ritual.time, ritual.crystals.len()), (Stage::Start, 0, 4));
        for event in events {
            if let EntityEvent::ExitPortal { active } = event {
                reloaded_world.set_exit_portal(active, false);
            }
        }
        assert_eq!(reloaded_world.get_block(portal), Some(Block::AIR));
    }

    #[test]
    fn a_ritual_survives_a_save_mid_way() {
        let end = EndGen::new(11);
        let (mut fight, mut crystals, _) = ritual_ready(&end, 4);
        let mut events = Vec::new();
        run(&mut fight, &mut crystals, 1 + 101 + 123, &mut events);
        let r = fight.respawn.clone().unwrap();
        assert_eq!((r.stage, r.time), (Stage::SummoningPillars, 123));
        let (text, saved) = (fight.serialize(&crystals), crystals.serialize());
        assert!(text.contains(";r summoning_pillars 123 "), "{text}");

        let mut fight = Fight::load(&end, Some(&text));
        let mut crystals = Crystals::default();
        crystals.load(&saved);
        fight.spawn_pillar_crystals(&mut crystals);
        let mut later = Vec::new();
        // The rest: 401 - 123 + 101 ticks.
        run(&mut fight, &mut crystals, 401 - 123 + 100, &mut later);
        assert!(fight.dragon.is_none());
        run(&mut fight, &mut crystals, 1, &mut later);
        assert!(fight.dragon.is_some(), "the reloaded ritual finished on time");
        let rebuilt = later.iter().filter(|(_, e)| matches!(e, EntityEvent::RespawnPillar { .. })).count();
        let before = events.iter().filter(|(_, e)| matches!(e, EntityEvent::RespawnPillar { .. })).count();
        assert_eq!(before + rebuilt, 10);
    }
}
