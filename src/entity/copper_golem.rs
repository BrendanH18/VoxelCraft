//! Original implementation from Java 26.3 CopperGolem/CopperGolemAi and
//! TransportItemsBetweenContainers behavior observations. No reference code or art.
use super::{Entities, EntityEvent, Mob, MobKind, MobSound, MobWorld, Rng};
use crate::{
    inventory::{Stack, stack_from_str, stack_to_string},
    world::{World, block::Block, copper::Stage, double_chest},
};
use glam::{DVec3, IVec3};
use serde_json::{Value, json};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Sound {
    Spawn,
    Spin,
    Step,
    Get,
    NoGet,
    Drop,
    NoDrop,
    Shear,
    Statue,
    Hurt,
    Death,
}

#[derive(Clone, Copy, Debug)]
struct Target {
    pos: IVec3,
    identity: u64,
    partner: Option<IVec3>,
}

/// Main hand is transport cargo; Java's antenna uses the SADDLE equipment slot.
/// Wax is represented by `next_weather_age == -2`, unset by -1, exactly as Java.
#[derive(Clone, Debug)]
pub struct State {
    pub stage: Stage,
    pub next_weather_age: i64,
    pub cargo: Option<Stack>,
    pub antenna: Option<Stack>,
    /// 0 idle, 1 getting, 2 getting nothing, 3 dropping, 4 dropping nothing.
    pub pose: u8,
    pub interaction_ticks: u8,
    pub opened_chest: Option<IVec3>,
    pub cooldown: u16,
    /// Latched until the integrator successfully converts, or waxing/scraping clears it.
    pub statue_ready: bool,
    target: Option<Target>,
    visited: Vec<IVec3>,
    unreachable: Vec<IVec3>,
    visited_ttl: u16,
    unreachable_ttl: u16,
    queued: bool,
    last_bolt: Option<u64>,
    remainder: f64,
    initialized: bool,
    spin: u16,
    last_step: f32,
    idle_delay: f32,
}
impl Default for State {
    fn default() -> Self {
        Self {
            stage: Stage::Unaffected,
            next_weather_age: -1,
            cargo: None,
            antenna: None,
            pose: 0,
            interaction_ticks: 0,
            opened_chest: None,
            cooldown: 60,
            statue_ready: false,
            target: None,
            visited: Vec::new(),
            unreachable: Vec::new(),
            visited_ttl: 0,
            unreachable_ttl: 0,
            queued: false,
            last_bolt: None,
            remainder: 0.,
            initialized: false,
            spin: 0,
            last_step: 0.,
            idle_delay: 0.,
        }
    }
}
impl State {
    pub fn waxed(&self) -> bool {
        self.next_weather_age == -2
    }
    /// Returns whether the parent must consume one honeycomb.
    pub fn wax(&mut self) -> bool {
        if self.waxed() {
            return false;
        }
        self.next_weather_age = -2;
        self.statue_ready = false;
        true
    }
    /// One axe use removes wax first, otherwise exactly one oxidation stage.
    pub fn scrape(&mut self) -> bool {
        if self.waxed() {
            self.next_weather_age = -1;
        } else if self.stage != Stage::Unaffected {
            self.stage = Stage::ALL[self.stage as usize - 1];
            self.next_weather_age = -1;
        } else {
            return false;
        }
        self.statue_ready = false;
        true
    }
    /// Java calls the base lightning damage too. An aged waxed golem loses its wax;
    /// an unaffected waxed golem retains it. Repeated flashes clean once per bolt.
    pub fn lightning(&mut self, bolt: u64) {
        if self.last_bolt.replace(bolt) != Some(bolt) && self.stage != Stage::Unaffected {
            self.stage = Stage::ALL[self.stage as usize - 1];
            self.next_weather_age = -1;
            self.statue_ready = false;
        }
    }
    /// Parent throws this stack toward the empty-handed player; no reconstruction.
    pub fn take_cargo(&mut self) -> Option<Stack> {
        let item = self.cargo.take();
        self.interrupt();
        item
    }
    /// Equippable poppy, only in an empty antenna slot (Java Equippable.equipOnTarget).
    /// Outer None rejects; Some(None) equips. Caller owns held-item consumption.
    pub fn equip_antenna(&mut self, stack: Stack) -> Option<Option<Stack>> {
        if stack.item != crate::item::Item::from(Block::POPPY) || stack.count != 1 || self.antenna.is_some() {
            return None;
        }
        Some(self.antenna.replace(stack))
    }
    /// Parent applies one shears durability and emits Shear when Some is returned.
    pub fn shear(&mut self) -> Option<Stack> {
        if self.antenna.is_some_and(|s| s.item == crate::item::Item::from(Block::POPPY)) {
            self.antenna.take()
        } else {
            None
        }
    }
    pub fn spawn(stage: Stage, waxed: bool) -> Self {
        Self { stage, next_weather_age: if waxed { -2 } else { -1 }, ..Self::default() }
    }
    pub fn interrupt(&mut self) {
        self.target = None;
        self.pose = 0;
        self.interaction_ticks = 0;
        self.opened_chest = None;
        self.queued = false;
    }
    fn clear_memories(&mut self) {
        self.visited.clear();
        self.unreachable.clear();
    }
    fn rest(&mut self) {
        self.interrupt();
        self.cooldown = 140;
        self.clear_memories();
    }
    fn weather(&mut self, now: i64, air: bool, rng: &mut Rng) {
        if self.waxed() {
            return;
        }
        if self.next_weather_age == -1 {
            self.next_weather_age = now.saturating_add(504_000 + i64::from(rng.next_int(48_001)));
            return;
        }
        let was_oxidized = self.stage == Stage::Oxidized;
        if !was_oxidized && now >= self.next_weather_age {
            self.stage = Stage::ALL[self.stage as usize + 1];
            self.next_weather_age = if self.stage == Stage::Oxidized {
                0
            } else {
                self.next_weather_age.saturating_add(504_000 + i64::from(rng.next_int(48_001)))
            };
        }
        if was_oxidized && air && rng.next_f32() <= 0.0058 {
            self.statue_ready = true;
        }
    }
    /// Includes all Stack components and absolute weather deadline. Transient chest
    /// ownership/navigation restart safely on load; cargo never returns to its source.
    pub fn look(&self) -> Value {
        json!({"stage":self.stage as u8,"next_weather_age":self.next_weather_age,
            "cargo":stack_to_string(self.cargo),"antenna":stack_to_string(self.antenna),
            "ready":self.statue_ready,"pose":self.pose,"interaction_ticks":self.interaction_ticks})
    }
    /// Java serializes both position sets as expiring Brain memories. TTL is
    /// loaded ticks, unlike the absolute weather deadline; unloading pauses it.
    pub fn save(&self) -> Value {
        let mut v = self.look();
        v["visited"] = json!(self.visited.iter().map(|p| p.to_array()).collect::<Vec<_>>());
        v["unreachable"] = json!(self.unreachable.iter().map(|p| p.to_array()).collect::<Vec<_>>());
        v["visited_ttl"] = json!(self.visited_ttl);
        v["unreachable_ttl"] = json!(self.unreachable_ttl);
        v
    }
    pub fn load(&mut self, v: &Value) {
        self.stage = Stage::ALL[v["stage"].as_u64().unwrap_or(0).min(3) as usize];
        self.next_weather_age = v["next_weather_age"].as_i64().unwrap_or(-1).max(-2);
        self.cargo = v["cargo"].as_str().and_then(stack_from_str).flatten();
        self.antenna = v["antenna"].as_str().and_then(stack_from_str).flatten();
        self.statue_ready = v["ready"].as_bool().unwrap_or(false) && self.stage == Stage::Oxidized && !self.waxed();
        self.interrupt();
        let positions = |v: &Value, max: usize| -> Vec<IVec3> {
            v.as_array()
                .into_iter()
                .flatten()
                .take(max)
                .filter_map(|v| {
                    let a = v.as_array()?;
                    if a.len() != 3 {
                        return None;
                    }
                    Some(IVec3::new(
                        i32::try_from(a[0].as_i64()?).ok()?,
                        i32::try_from(a[1].as_i64()?).ok()?,
                        i32::try_from(a[2].as_i64()?).ok()?,
                    ))
                })
                .collect()
        };
        self.visited = positions(&v["visited"], 10);
        self.unreachable = positions(&v["unreachable"], 50);
        self.visited_ttl = v["visited_ttl"].as_u64().unwrap_or(0).min(6000) as u16;
        self.unreachable_ttl = v["unreachable_ttl"].as_u64().unwrap_or(0).min(6000) as u16;
    }
    pub(crate) fn load_look(&mut self, v: &Value) {
        self.load(v);
        self.pose = v["pose"].as_u64().unwrap_or(0).min(4) as u8;
        self.interaction_ticks = v["interaction_ticks"].as_u64().unwrap_or(0).min(60) as u8;
    }
}
fn wanted(block: Block, pickup: bool) -> bool {
    double_chest::state(block).is_some_and(|s| {
        if pickup {
            s.kind.copper().is_some()
        } else {
            matches!(s.kind, double_chest::Kind::Plain | double_chest::Kind::Trapped)
        }
    })
}
fn can_place(slots: &[Option<Stack>], cargo: Stack) -> bool {
    slots.iter().all(Option::is_none) || slots.iter().flatten().any(|s| s.item == cargo.item)
}
/// Slot-order insertion (including empty slots before later matching stacks).
/// Eligibility uses item type; merges use every component. Returns exact remainder.
fn transfer(slots: &mut [Option<Stack>], cargo: &mut Option<Stack>) -> bool {
    if let Some(mut hand) = *cargo {
        if !can_place(slots, hand) {
            return false;
        }
        for slot in slots {
            match slot {
                None => {
                    *slot = Some(hand);
                    *cargo = None;
                    return true;
                }
                Some(s) if s.stacks_with(&hand) => {
                    let n = hand.count.min(s.item.max_stack().saturating_sub(s.count));
                    s.count += n;
                    hand.count -= n;
                    if hand.count == 0 {
                        *cargo = None;
                        return true;
                    }
                }
                _ => {}
            }
        }
        *cargo = Some(hand);
        true
    } else {
        for slot in slots {
            if let Some(s) = slot {
                let count = s.count.min(16);
                let mut hand = *s;
                hand.count = count;
                s.count -= count;
                if s.count == 0 {
                    *slot = None;
                }
                *cargo = Some(hand);
                return true;
            }
        }
        false
    }
}
/// Java collision-shape bounds expanded horizontally by range, vertically by .5.
fn within(world: &World, m: &Mob, p: IVec3, at: DVec3, range: f64) -> bool {
    let s = m.shape();
    let (lo, hi) = s.aabb(at);
    let shape = world
        .get_block(p)
        .and_then(double_chest::shape)
        .unwrap_or(crate::world::shape::Box16 { min: [1, 0, 1], max: [15, 14, 15] });
    let p = p.as_dvec3();
    let expand = DVec3::new(range, 0.5, range);
    let min = p + DVec3::from_array(shape.min.map(|v| f64::from(v) / 16.)) - expand;
    let max = p + DVec3::from_array(shape.max.map(|v| f64::from(v) / 16.)) + expand;
    lo.cmplt(max).all() && hi.cmpgt(min).all()
}
fn visible(world: &World, m: &Mob, at: DVec3, target: IVec3) -> bool {
    let center = target.as_dvec3() + DVec3::splat(0.5);
    let from = at + DVec3::Y * m.shape().height * 0.5;
    [DVec3::X, DVec3::NEG_X, DVec3::Y, DVec3::NEG_Y, DVec3::Z, DVec3::NEG_Z]
        .iter()
        .any(|side| super::mob::line_of_sight(world, from, center + *side * 0.5))
}
impl Mob {
    pub(crate) fn copper_steer<W: MobWorld + ?Sized>(
        &mut self,
        dt: f32,
        world: &W,
        ctx: &super::Ctx,
        rng: &mut Rng,
    ) -> (Option<DVec3>, f64) {
        self.ai_timer -= dt;
        let state = self.copper_golem.as_mut().unwrap();
        state.idle_delay = (state.idle_delay - dt).max(0.);
        let s = self.copper_golem.as_ref().unwrap();
        if s.pose != 0 || s.queued {
            return (None, 0.);
        }
        let speed = self.goal_speed(1.0);
        if s.target.is_some() {
            return (self.follow_path(world, speed, false), speed);
        }
        // Idle/panic uses existing bounded ground navigation. Damage interruption
        // clears transport first; Java's panic multiplier is 1.5.
        if self.ai == super::mob::Ai::Panic && self.ai_timer > 0. {
            return self.copper_idle(world, ctx, rng, 1.5);
        }
        if s.cooldown > 0 {
            return self.copper_idle(world, ctx, rng, 1.0);
        }
        (None, 0.)
    }
    fn copper_idle<W: MobWorld + ?Sized>(
        &mut self,
        world: &W,
        ctx: &super::Ctx,
        rng: &mut Rng,
        multiplier: f64,
    ) -> (Option<DVec3>, f64) {
        let speed = self.goal_speed(multiplier);
        if multiplier > 1. {
            self.panic_target(world, ctx, rng, speed);
        } else if self.nav.is_done() && self.copper_golem.as_ref().unwrap().idle_delay <= 0. {
            // CopperGolemAi.RunOne: equally weighted small LandRandomPos stroll
            // and DoNothing(30,60), only while the transport cooldown is present.
            if rng.next_int(2) == 0 {
                if let Some(goal) = self.random_pos(world, ctx, rng, 2, 2, true) {
                    self.walk_to(world, goal, 0, speed, false);
                }
            } else {
                self.copper_golem.as_mut().unwrap().idle_delay = (30 + rng.next_int(31)) as f32 * 0.05;
            }
        }
        (self.follow_path(world, speed, false), speed)
    }
}
impl Entities {
    /// Drain owned stacks in the death-loot transaction, before an immediate save.
    pub(super) fn copper_equipment_drops(&mut self, kind: MobKind, pos: DVec3) {
        if kind != MobKind::CopperGolem {
            return;
        }
        let Some(s) = self
            .mobs
            .iter_mut()
            .find(|m| m.kind == kind && !m.alive() && m.pos.distance_squared(pos) < 0.01)
            .and_then(|m| m.copper_golem.as_mut())
        else {
            return;
        };
        // Java CopperGolem.dropEquipment calls dropPreservedEquipment after
        // ordinary loot: guaranteed slots ignore Vanishing and MOB_DROPS.
        for stack in [s.cargo.take(), s.antenna.take()].into_iter().flatten() {
            self.items.push(super::ItemEntity::new(stack, pos, DVec3::ZERO, 0.5, &mut self.rng));
        }
        s.interrupt();
    }
    /// Construction/reactivation helper. Parent commits its block mutation before
    /// invoking this; this method allocates no block/item IDs and edits no blocks.
    pub fn spawn_copper_golem(&mut self, pos: DVec3, stage: Stage, waxed: bool) -> usize {
        let index = self.mobs.len();
        self.spawn(MobKind::CopperGolem, pos);
        self.ensure_mob_ids();
        self.mobs[index].copper_golem = Some(Box::new(State::spawn(stage, waxed)));
        self.pending_sounds.push(EntityEvent::Sound { sound: MobSound::CopperGolem(Sound::Spawn, stage as u8), pos });
        index
    }
    /// Atomic mutable-world transport tick, called by update_world before physics.
    /// `clock_time` is the saved world's absolute Java game tick. No statue module
    /// dependency: integrator must accept readiness and preserve/drop state itself.
    pub fn tick_copper_golems(&mut self, dt: f64, world: &mut World) {
        if !self.mobs.iter().any(|m| m.copper_golem.is_some()) {
            world.set_golem_chest_viewers([]);
            return;
        }
        let mut open_counts: rustc_hash::FxHashMap<IVec3, u16> = Default::default();
        let mut cat_blocks: rustc_hash::FxHashSet<IVec3> = Default::default();
        for m in &self.mobs {
            if m.alive()
                && let Some(p) = m.copper_golem.as_ref().and_then(|s| s.opened_chest)
            {
                *open_counts.entry(world.chest_halves(p).0).or_default() += 1;
            }
            if m.kind == MobKind::Cat && m.animal.as_ref().is_some_and(|a| a.sitting) {
                // Java queries cats whose AABB intersects the unit cube above
                // either half, including cats standing on the 14/16-height lid.
                let (lo, hi) = m.shape().aabb(m.pos);
                for x in lo.x.floor() as i32..hi.x.ceil() as i32 {
                    for z in lo.z.floor() as i32..hi.z.ceil() as i32 {
                        for y in lo.y.floor() as i32 - 1..hi.y.ceil() as i32 - 1 {
                            cat_blocks.insert(IVec3::new(x, y, z));
                        }
                    }
                }
            }
        }
        for i in 0..self.mobs.len() {
            if self.mobs[i].copper_golem.is_none()
                || !self.mobs[i].alive()
                || !world.is_entity_ticking(self.mobs[i].pos.floor().as_ivec3())
            {
                continue;
            }
            let mut s = self.mobs[i].copper_golem.take().unwrap();
            if let Some(p) = s.opened_chest {
                let count = open_counts.entry(world.chest_halves(p).0).or_default();
                *count = count.saturating_sub(1);
            }
            s.remainder += dt.max(0.);
            while s.remainder + 1e-9 >= 0.05 {
                s.remainder = (s.remainder - 0.05).max(0.);
                let m = &mut self.mobs[i];
                m.persistent = true;
                if !s.initialized {
                    s.cooldown = 60 + self.rng.next_int(40) as u16;
                    s.initialized = true;
                    s.spin = 210 + self.rng.next_int(40) as u16;
                }
                s.weather(
                    self.clock_time,
                    world.get_block(m.pos.floor().as_ivec3()) == Some(Block::AIR),
                    &mut self.rng,
                );
                if s.pose == 0 && s.spin == 0 {
                    s.spin = 210 + self.rng.next_int(40) as u16;
                    self.pending_sounds.push(EntityEvent::Sound {
                        sound: MobSound::CopperGolem(Sound::Spin, s.stage as u8),
                        pos: m.pos,
                    });
                } else {
                    s.spin = s.spin.saturating_sub(1);
                }
                if (m.limb_phase - s.last_step).abs() >= std::f32::consts::PI {
                    s.last_step = m.limb_phase;
                    self.pending_sounds.push(EntityEvent::Sound {
                        sound: MobSound::CopperGolem(Sound::Step, s.stage as u8),
                        pos: m.pos,
                    });
                }
                if s.visited_ttl == 0 {
                    s.visited.clear();
                } else {
                    s.visited_ttl -= 1;
                }
                if s.unreachable_ttl == 0 {
                    s.unreachable.clear();
                } else {
                    s.unreachable_ttl -= 1;
                }
                if m.leash.is_some() || (m.ai == super::mob::Ai::Panic && m.ai_timer > 0.) || m.burning {
                    s.interrupt();
                    continue;
                }
                if s.cooldown > 0 {
                    s.cooldown -= 1;
                    continue;
                }
                if let Some(t) = s.target {
                    let blocked_cat = [Some(t.pos), t.partner].into_iter().flatten().any(|p| cat_blocks.contains(&p));
                    if world.transport_chest_identity(t.pos) != Some(t.identity)
                        || world.chest_partner(t.pos) != t.partner
                        || !world.get_block(t.pos).is_some_and(|b| wanted(b, s.cargo.is_none()))
                        || !world.chest_opens(t.pos)
                        || blocked_cat
                    {
                        s.interrupt();
                    }
                }
                if s.target.is_none() {
                    let m = &self.mobs[i];
                    let pos = m.pos.floor().as_ivec3();
                    let passenger = m.riding.is_some();
                    let mut best = None;
                    let mut dist = f64::INFINITY;
                    world.visit_transport_chests(
                        pos,
                        if passenger { 1 } else { 32 },
                        if passenger { 1 } else { 8 },
                        &mut |p| {
                            let Some(block) = world.get_block(p) else { return };
                            let partner = world.chest_partner(p);
                            if !wanted(block, s.cargo.is_none())
                                || [Some(p), partner]
                                    .into_iter()
                                    .flatten()
                                    .any(|p| s.visited.contains(&p) || s.unreachable.contains(&p))
                            {
                                return;
                            }
                            let d = m.pos.distance_squared(p.as_dvec3() + DVec3::splat(0.5));
                            if d < dist {
                                dist = d;
                                best = Some(Target {
                                    pos: p,
                                    identity: world.transport_chest_identity(p).unwrap(),
                                    partner,
                                });
                            }
                        },
                    );
                    if let Some(t) = best {
                        s.target = Some(t);
                        s.visited.push(t.pos);
                        s.visited_ttl = 6000;
                        if s.visited.len() > 10 {
                            s.rest();
                        }
                        self.mobs[i].stop_navigation();
                    } else {
                        s.rest();
                    }
                    continue;
                }
                let t = s.target.unwrap();
                let m = &self.mobs[i];
                if s.pose != 0 {
                    if !within(world, m, t.pos, m.pos, 2.) {
                        s.interrupt();
                        continue;
                    }
                    s.interaction_ticks += 1;
                    if s.interaction_ticks == 9 {
                        let sound = match s.pose {
                            1 => Sound::Get,
                            2 => Sound::NoGet,
                            3 => Sound::Drop,
                            _ => Sound::NoDrop,
                        };
                        self.pending_sounds.push(EntityEvent::Sound {
                            sound: MobSound::CopperGolem(sound, s.stage as u8),
                            pos: m.pos,
                        });
                    }
                    if s.interaction_ticks >= 60 {
                        let success =
                            world.with_chest_slots(t.pos, |slots| transfer(slots, &mut s.cargo)).unwrap_or(false);
                        if success && (s.pose <= 2 || s.cargo.is_none()) {
                            s.clear_memories();
                        }
                        s.interrupt();
                        self.mobs[i].stop_navigation();
                    }
                    continue;
                }
                let (first, _) = world.chest_halves(t.pos);
                let occupied = world.player_chest_viewers(t.pos) > 0 || open_counts.get(&first).is_some_and(|n| *n > 0);
                if occupied && within(world, m, t.pos, m.pos, 3.) {
                    s.queued = true;
                    self.mobs[i].stop_navigation();
                    continue;
                }
                s.queued = false;
                let range = if m.nav.path.as_ref().is_some_and(|p| p.is_done()) { 1. } else { 0.5 };
                if within(world, m, t.pos, m.pos, range) && visible(world, m, m.pos, t.pos) {
                    let matches = if let Some(cargo) = s.cargo {
                        let empty = (0..world.container_slots(t.pos)).all(|n| world.chest_slot(t.pos, n).is_none());
                        empty
                            || (0..world.container_slots(t.pos))
                                .any(|n| world.chest_slot(t.pos, n).is_some_and(|a| a.item == cargo.item))
                    } else {
                        (0..world.container_slots(t.pos)).any(|n| world.chest_slot(t.pos, n).is_some())
                    };
                    s.pose = match (s.cargo.is_none(), matches) {
                        (true, true) => 1,
                        (true, false) => 2,
                        (false, true) => 3,
                        _ => 4,
                    };
                    s.opened_chest = Some(t.pos);
                    s.interaction_ticks = 1;
                    self.mobs[i].stop_navigation();
                } else {
                    let m = &mut self.mobs[i];
                    let speed = m.goal_speed(1.);
                    if m.nav.is_done() {
                        m.walk_to(world, t.pos, 0, speed, false);
                    }
                    let endpoint = m.nav.path.as_ref().and_then(|p| {
                        p.end().map(|_| p.entity_pos(p.nodes.len() - 1, m.shape().half_width as f32 * 2.))
                    });
                    if endpoint.is_some_and(|at| !within(world, m, t.pos, at, 1.) || !visible(world, m, at, t.pos)) {
                        s.visited.retain(|p| *p != t.pos);
                        s.unreachable.push(t.pos);
                        s.visited_ttl = 6000;
                        s.unreachable_ttl = 6000;
                        if s.unreachable.len() > 50 {
                            s.rest();
                        } else {
                            s.interrupt();
                        }
                        m.stop_navigation();
                    }
                }
            }
            if let Some(p) = s.opened_chest {
                *open_counts.entry(world.chest_halves(p).0).or_default() += 1;
            }
            self.mobs[i].copper_golem = Some(s);
        }
        let open: Vec<_> =
            self.mobs.iter().filter(|m| m.alive()).filter_map(|m| m.copper_golem.as_ref()?.opened_chest).collect();
        world.set_golem_chest_viewers(open);
    }
}
#[cfg(test)]
mod tests;
