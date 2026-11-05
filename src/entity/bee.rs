//! Bee lifecycle and goal rules from pinned Java 26.3. No natural spawn list:
//! bees originate in the tree decorator's nests, or shared animal breeding.
use super::{Ctx, Entities, EntityEvent, Mob, MobKind, MobWorld, PlayerId, Rng};
use crate::world::{World, bees, block::Block};
use glam::{DVec3, IVec3};

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct State {
    pub hive: Option<[i32; 3]>,
    pub flower: Option<[i32; 3]>,
    pub nectar: bool,
    pub stung: bool,
    pub sting_ticks: u16,
    pub anger: u16,
    pub target: Option<u32>,
    pub since_pollination: u32,
    pub cannot_enter: u16,
    pub flower_cooldown: u16,
    pub pollination: u16,
    pub crops: u8,
    pub underwater: u16,
    pub search: u16,
    pub buzz: u16,
    pub enter: bool,
    pub boost: bool,
}
pub fn flower(b: Block) -> bool {
    if let Some((lower, true)) = crate::world::overworld_blocks::double_of(b) {
        return flower(lower);
    }
    // The engine's registered members of Java's bee food/flower tags.
    matches!(
        b.name(),
        "dandelion"
            | "open eyeblossom"
            | "wither rose"
            | "mangrove propagule"
            | "hanging mangrove propagule"
            | "cherry leaves"
            | "chorus flower"
            | "spore blossom"
            | "cactus flower"
            | "poppy"
            | "blue orchid"
            | "allium"
            | "azure bluet"
            | "red tulip"
            | "orange tulip"
            | "white tulip"
            | "pink tulip"
            | "oxeye daisy"
            | "cornflower"
            | "lily of the valley"
            | "sunflower"
            | "lilac"
            | "rose bush"
            | "peony"
            | "flowering azalea"
            | "flowering azalea leaves"
            | "pink petals"
            | "wildflowers"
            | "torchflower"
            | "pitcher plant"
    )
}
pub(crate) fn near_fire<W: MobWorld + ?Sized>(world: &W, p: IVec3) -> bool {
    (-1..=1).any(|x| {
        (-1..=1).any(|y| (-1..=1).any(|z| world.block(p + IVec3::new(x, y, z)).is_some_and(|b| b.fire_age().is_some())))
    })
}
impl State {
    pub fn save(self) -> serde_json::Value {
        serde_json::json!([
            self.hive,
            self.flower,
            self.nectar,
            self.stung,
            self.sting_ticks,
            self.anger,
            self.target,
            self.since_pollination,
            self.cannot_enter,
            self.flower_cooldown,
            self.pollination,
            self.crops,
            self.underwater,
            self.search,
            self.buzz
        ])
    }
    pub fn load(v: &serde_json::Value) -> Self {
        let n = |i: usize| v[i].as_u64().unwrap_or(0);
        let small = |i| n(i).min(u16::MAX as u64) as u16;
        Self {
            hive: serde_json::from_value(v[0].clone()).unwrap_or_default(),
            flower: serde_json::from_value(v[1].clone()).unwrap_or_default(),
            nectar: v[2] == true,
            stung: v[3] == true,
            sting_ticks: small(4),
            anger: small(5),
            target: v[6].as_u64().and_then(|v| v.try_into().ok()),
            since_pollination: n(7).min(u32::MAX as u64) as u32,
            cannot_enter: small(8),
            flower_cooldown: small(9),
            pollination: small(10),
            crops: n(11).min(10) as u8,
            underwater: small(12),
            search: small(13),
            buzz: small(14),
            enter: false,
            boost: false,
        }
    }
}
impl State {
    pub fn anger(&mut self, player: PlayerId, ticks: u16) {
        self.anger = ticks;
        self.target = Some(player.0);
        self.pollination = 0;
    }
}
/// The same saved mob payload as a free bee, plus BeehiveBlockEntity's
/// ticks in hive. Names, effects and breeding data survive transport too.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Occupant {
    /// The bee's saved mob record, or null for a natural nest bee that has
    /// never left (a fresh bee), so unvisited nests stay small in memory
    /// and in the `beehives` save.
    pub data: serde_json::Value,
    pub nectar: bool,
    pub ticks: u32,
}
impl Occupant {
    pub fn from_mob(m: &Mob) -> Self {
        let mut data = super::nether::save_mob(m);
        data["identity"]["leash"] = serde_json::Value::Null;
        data["riding"] = serde_json::Value::Null;
        Self { data, nectar: m.bee.is_some_and(|s| s.nectar), ticks: 0 }
    }
    pub fn natural(ticks: u32) -> Self {
        Self { data: serde_json::Value::Null, nectar: false, ticks }
    }
    pub fn save(&self) -> serde_json::Value {
        serde_json::json!({"data":self.data,"nectar":self.nectar,"ticks":self.ticks})
    }
    pub fn load(v: &serde_json::Value) -> Option<Self> {
        if !v["data"].is_null() {
            let m = super::nether::load_mob(&v["data"])?;
            if m.kind != MobKind::Bee || !m.alive() {
                return None;
            }
        }
        Some(Self {
            data: v["data"].clone(),
            nectar: v["nectar"].as_bool()?,
            ticks: v["ticks"].as_u64()?.try_into().ok()?,
        })
    }
    pub(crate) fn mob(&self) -> Option<Mob> {
        if self.data.is_null() {
            return Some(Mob::new(MobKind::Bee, DVec3::ZERO, 0.));
        }
        super::nether::load_mob(&self.data)
    }
}
impl Entities {
    /// Authoritative entity/world transaction. Existing event handling remains
    /// unchanged; this also commits bee entry/crop actions and ticks occupants.
    pub fn update_world(
        &mut self,
        dt: f64,
        world: &mut World,
        ctx: &Ctx,
        difficulty: crate::simulation::difficulty::Difficulty,
    ) -> Vec<EntityEvent> {
        self.tick_copper_golems(dt, world);
        self.finish_copper_statues(world);
        world.tick_beehives(self, dt, ctx);
        let events = self.update_difficulty(dt, world, ctx, difficulty);
        let mut i = 0;
        while i < self.mobs.len() {
            let m = &mut self.mobs[i];
            let Some(s) = m.bee.as_mut() else {
                i += 1;
                continue;
            };
            if std::mem::take(&mut s.boost) {
                for d in 1..=2 {
                    if self.mobs[i].bee.is_some_and(|s| s.crops >= 10) {
                        break;
                    }
                    let p = self.mobs[i].pos.floor().as_ivec3() - IVec3::Y * d;
                    if world.bee_grow_crop(p) {
                        self.mobs[i].bee.as_mut().unwrap().crops += 1;
                    }
                }
            }
            let m = &mut self.mobs[i];
            let s = m.bee.as_mut().unwrap();
            if std::mem::take(&mut s.enter) && m.alive() {
                let at = m.bee.and_then(|s| s.hive).map(IVec3::from_array);
                let lead = m.leash.is_some();
                let pos = m.pos;
                if let Some(p) = at
                    && world.bee_enter(p, Occupant::from_mob(m))
                {
                    self.mobs.swap_remove(i);
                    if lead {
                        self.scatter(crate::inventory::Stack::new(crate::item::Item::LEAD, 1), pos);
                    }
                    continue;
                }
            }
            i += 1;
        }
        events
    }
    pub fn confirm_bee_sting(&mut self, uid: u32, hurt: bool) {
        if hurt && let Some(s) = self.mobs.iter_mut().find(|m| m.uid == uid && m.alive()).and_then(|m| m.bee.as_mut()) {
            s.stung = true;
            s.anger = 0;
            s.target = None;
        }
    }
    pub fn anger_bees(&mut self, p: IVec3, player: PlayerId) {
        let min = p.as_dvec3() - DVec3::new(8., 6., 8.);
        let max = (p + IVec3::ONE).as_dvec3() + DVec3::new(8., 6., 8.);
        for m in &mut self.mobs {
            let (lo, hi) = m.aabb();
            if m.alive()
                && lo.cmplt(max).all()
                && hi.cmpgt(min).all()
                && let Some(s) = &mut m.bee
                && s.target.is_none()
            {
                s.anger(player, 400 + self.rng.next_int(381) as u16);
            }
        }
    }
}
/// Server AI counters advance independently of the movement goal.
pub(crate) fn upkeep(m: &mut Mob, dt: f32, rng: &mut Rng, events: &mut Vec<EntityEvent>) {
    if dt <= 0. || !m.alive() || m.bee.is_none() {
        return;
    }
    let mut s = m.bee.unwrap_or_default();
    let ticks = (dt * 20.).round().max(1.) as u16;
    s.cannot_enter = s.cannot_enter.saturating_sub(ticks);
    s.flower_cooldown = s.flower_cooldown.saturating_sub(ticks);
    s.search = s.search.saturating_sub(ticks);
    s.anger = s.anger.saturating_sub(ticks);
    s.buzz = s.buzz.saturating_sub(ticks);
    if s.buzz == 0 {
        s.buzz = 20;
        events.push(EntityEvent::Sound { sound: super::MobSound::Ambient(MobKind::Bee), pos: m.pos });
    }
    if m.in_water {
        s.underwater = s.underwater.saturating_add(ticks);
        if s.underwater > 20 && m.hurt <= 0. {
            m.damage(1., None, rng);
        }
    } else {
        s.underwater = 0;
    }
    if s.stung {
        s.sting_ticks = s.sting_ticks.saturating_add(ticks);
        if s.sting_ticks.is_multiple_of(5)
            && rng.next_int(1200u32.saturating_sub(s.sting_ticks as u32).clamp(1, 1200)) == 0
        {
            m.damage(m.health, None, rng);
        }
    }
    if !s.nectar {
        s.since_pollination = s.since_pollination.saturating_add(ticks as u32);
    }
    m.bee = Some(s);
}
/// Called after the common breed/tempt goals; aggressive bees take priority.
pub(crate) fn steer<W: MobWorld + ?Sized>(
    m: &mut Mob,
    dt: f32,
    world: &W,
    ctx: &Ctx,
    rng: &mut Rng,
    events: &mut Vec<EntityEvent>,
) -> (Option<DVec3>, f64) {
    if dt <= 0. {
        return (None, 0.);
    }
    let mut s = m.bee.unwrap_or_default();
    let ticks = (dt * 20.).round().max(1.) as u16;
    let mut goal = None;
    if s.anger > 0 && !s.stung {
        if let Some(t) = ctx.players.iter().find(|t| Some(t.id.0) == s.target && t.targetable) {
            goal = Some(t.pos + DVec3::Y);
            if m.pos.distance(t.pos + DVec3::Y) < 1.0 && super::mob::line_of_sight(world, m.pos, t.pos + DVec3::Y) {
                let poison = match m.difficulty {
                    crate::simulation::difficulty::Difficulty::Normal => 200,
                    crate::simulation::difficulty::Difficulty::Hard => 360,
                    _ => 0,
                };
                if m.attack_cooldown <= 0. {
                    events.push(EntityEvent::BeeSting { uid: m.uid, player: t.id, poison });
                    m.attack_cooldown = 1.;
                }
            }
        }
    } else {
        let wants_hive = s.cannot_enter == 0
            && s.pollination == 0
            && !s.stung
            && !s.hive.is_some_and(|p| near_fire(world, IVec3::from_array(p)))
            && (s.nectar || s.since_pollination > 3600 || ctx.raining || world.bee_night(ctx.daylight));
        if let Some(p) = s.hive.map(IVec3::from_array)
            && (world.block(p).is_some_and(|b| bees::hive(b).is_none()) || world.bee_hive_full(p))
        {
            s.hive = None;
        }
        if ctx.raining {
            s.pollination = 0;
        }
        if wants_hive {
            if s.hive.is_none() && s.search == 0 {
                s.search = 200;
                let mut best: Option<(i32, IVec3)> = None;
                world.bee_hives(m.pos.floor().as_ivec3(), &mut |p, full| {
                    let d = (p - m.pos.floor().as_ivec3()).length_squared();
                    if !full && d <= 400 && !near_fire(world, p) && best.is_none_or(|(n, _)| d < n) {
                        best = Some((d, p));
                    }
                });
                s.hive = best.map(|(_, p)| p.to_array());
            }
            if let Some(p) = s.hive.map(IVec3::from_array)
                && let Some((_, f, _)) = world.block(p).and_then(bees::hive)
            {
                let entry = (p + f.offset()).as_dvec3() + DVec3::new(0.5, 0.1, 0.5);
                goal = Some(entry);
                if m.pos.distance(entry) < 0.7 {
                    s.enter = true;
                }
            }
        }
        if goal.is_none() && !s.nectar && s.flower_cooldown == 0 && !ctx.raining {
            if let Some(p) = s.flower.map(IVec3::from_array)
                && !world.block(p).is_some_and(flower)
            {
                s.flower = None;
                s.pollination = 0;
            }
            if s.flower.is_none() && s.search == 0 {
                s.search = 20;
                let center = m.pos.floor().as_ivec3();
                // Java flower search range five; one bounded search each second.
                let mut best: Option<(i32, IVec3)> = None;
                for dy in -5..=5 {
                    for dx in -5..=5 {
                        for dz in -5..=5 {
                            let d = IVec3::new(dx, dy, dz);
                            let p = center + d;
                            if world.block(p).is_some_and(flower)
                                && world.block(p + IVec3::Y) == Some(Block::AIR)
                                && best.is_none_or(|(n, _)| d.length_squared() < n)
                            {
                                best = Some((d.length_squared(), p));
                            }
                        }
                    }
                }
                s.flower = best.map(|(_, p)| p.to_array());
                if s.flower.is_none() {
                    s.flower_cooldown = 20 + (rng.next_u64() % 41) as u16;
                }
            }
            if let Some(p) = s.flower.map(IVec3::from_array) {
                let at = p.as_dvec3() + DVec3::new(0.5, 0.6, 0.5);
                goal = Some(at);
                if m.pos.distance(at) < 0.6 {
                    s.pollination = s.pollination.saturating_add(ticks);
                    if s.pollination > 400 && rng.chance(0.8) {
                        s.nectar = true;
                        s.pollination = 0;
                        s.crops = 0;
                        s.since_pollination = 0;
                        s.flower_cooldown = 200;
                    }
                    m.bee = Some(s);
                    return (None, 0.);
                }
            }
        }
        if s.nectar && s.crops < 10 && s.hive.is_some() && rng.chance(0.7 / 30.) {
            s.boost = true;
        }
    }
    m.bee = Some(s);
    if let Some(at) = goal {
        if m.pos.distance_squared(at) < 0.04 {
            return (None, 0.);
        }
        let wish = m.pursue(world, at, 1., true, false, rng);
        return (wish, 1.);
    }
    // Fly along the engine's bounded Java flying navigator for idle wandering.
    m.ai_timer -= dt;
    if m.ai_timer <= 0. {
        m.ai_timer = rng.range(1., 3.);
        let p = m.pos + DVec3::new(rng.range(-8., 8.) as f64, rng.range(-2., 2.) as f64, rng.range(-8., 8.) as f64);
        m.walk_to(world, p.floor().as_ivec3(), 0, 1., false);
    }
    (m.follow_path(world, 1., false), 1.)
}
/// FlyingMoveControl + no-gravity LivingEntity air travel, in Java ticks.
/// Inputs retain Java's 0.98 attenuation; air acceleration is 0.02 and drag 0.91/0.98.
pub(crate) fn travel<W: MobWorld + ?Sized>(m: &mut Mob, dt: f64, world: &W, wish: Option<DVec3>, speed: f64) {
    let mut dm = m.vel * 0.05;
    if let Some(dir) = wish {
        let target = dir.z.atan2(dir.x) as f32;
        let turn = (target - m.yaw + std::f32::consts::PI).rem_euclid(std::f32::consts::TAU) - std::f32::consts::PI;
        m.yaw += turn.clamp(-std::f32::consts::FRAC_PI_2, std::f32::consts::FRAC_PI_2);
        let forward = DVec3::new(m.yaw.cos() as f64, 0., m.yaw.sin() as f64);
        let cur = if m.on_ground { 0.3 } else { 0.6 } * speed;
        let input = forward * cur + DVec3::Y * (if dir.y.abs() > 1e-5 { dir.y.signum() * cur } else { 0. });
        dm += input.normalize_or_zero() * input.length().min(1.) * 0.98 * 0.02;
    }
    let delta = dm * (dt / 0.05);
    let shape = m.shape();
    let hit = crate::physics::move_box(world, &mut m.pos, &mut dm, delta, shape);
    m.on_ground = hit.on_ground;
    m.in_water = world.block(m.pos.floor().as_ivec3()).is_some_and(Block::holds_water);
    m.vel = dm * DVec3::new(0.91, 0.98, 0.91) / 0.05;
}
