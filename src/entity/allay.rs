//! Java 26.3 allay: one liked hand item, one inventory slot and bounded flying AI.
//! See docs/verification/2026-11-05-v0.15-allays.md for reference and gaps.
use super::{Ctx, Entities, EntityEvent, ItemEntity, Mob, MobKind, MobWorld, PlayerId, Rng};
use crate::{
    inventory::{Stack, stack_from_str, stack_to_string},
    physics,
    world::{World, block::Block},
};
use glam::{DVec3, IVec3};
use serde_json::{Value, json};

#[derive(Clone, Debug, Default, PartialEq)]
pub struct State {
    pub liked: Option<Stack>,
    pub cargo: Option<Stack>,
    pub player: Option<PlayerId>,
    pub note: Option<IVec3>,
    pub note_ticks: u16,
    pub pickup_ticks: u16,
    pub duplication_ticks: u16,
    pub dancing: bool,
    pub jukebox: Option<IVec3>,
    pub ground_speed: Option<f64>,
    ticks: u64,
    remainder: f64,
    vibration: Option<(IVec3, u16)>,
    sensor: u8,
    wanted: Option<DVec3>,
    pub(super) deposit: Option<DVec3>,
    give_ticks: u8,
}
impl State {
    pub fn save(&self) -> Value {
        json!({"liked":stack_to_string(self.liked),"cargo":stack_to_string(self.cargo),
            "player":self.player.map(|p|p.0),"note":self.note.map(|p|p.to_array()),
            "note_ticks":self.note_ticks,"pickup_ticks":self.pickup_ticks,
            "duplication_ticks":self.duplication_ticks,"dancing":self.dancing,
            "jukebox":self.jukebox.map(|p|p.to_array()),"ground_speed":self.ground_speed,"ticks":self.ticks,
            "remainder":self.remainder,"vibration":self.vibration.map(|(p,t)|(p.to_array(),t)),
            "give_ticks":self.give_ticks})
    }
    pub fn load(v: &Value) -> Self {
        let small = |key: &str, max: u64| v[key].as_u64().unwrap_or(0).min(max) as u16;
        let pos = |v: &Value| serde_json::from_value::<[i32; 3]>(v.clone()).ok().map(IVec3::from_array);
        Self {
            liked: v["liked"].as_str().and_then(stack_from_str).flatten().map(|s| Stack { count: 1, ..s }),
            cargo: v["cargo"].as_str().and_then(stack_from_str).flatten(),
            player: v["player"].as_u64().and_then(|p| p.try_into().ok()).map(PlayerId),
            note: pos(&v["note"]),
            note_ticks: small("note_ticks", 600),
            pickup_ticks: small("pickup_ticks", 60),
            duplication_ticks: small("duplication_ticks", 6000),
            dancing: v["dancing"] == true,
            jukebox: pos(&v["jukebox"]),
            ground_speed: v["ground_speed"].as_f64().filter(|x| x.is_finite() && (0.0..=16.0).contains(x)),
            ticks: v["ticks"].as_u64().unwrap_or(0),
            remainder: v["remainder"].as_f64().filter(|r| (0.0..0.05).contains(r)).unwrap_or(0.),
            vibration: pos(&v["vibration"][0]).zip(v["vibration"][1].as_u64().map(|t| t.min(17) as u16)),
            give_ticks: small("give_ticks", 20) as u8,
            ..Self::default()
        }
    }
    /// `Allay.wantsToPickUp`: `allayConsidersItemEqual` (same item and equal
    /// potion contents, every other component ignored) against the liked item,
    /// then `SimpleContainer.canAddItem` (empty slot, or same item and
    /// components below the stack maximum) for the single inventory slot.
    fn accepts(&self, stack: Stack) -> bool {
        self.liked.is_some_and(|s| considers_equal(s, stack))
            && self.cargo.is_none_or(|s| s.stacks_with(&stack) && s.count < s.max())
    }
    pub fn hear_note(&mut self, note: IVec3) {
        if self.note.is_none_or(|p| p == note) {
            self.note = Some(note);
            self.note_ticks = 600;
        }
    }
    /// `Allay.setJukeboxPlaying`, called by [`Entities::hear_jukeboxes`].
    pub fn set_jukebox_playing(&mut self, at: IVec3, playing: bool) {
        if playing && !self.dancing {
            self.jukebox = Some(at);
            self.dancing = true;
        } else if !playing && self.jukebox.is_none_or(|p| p == at) {
            self.jukebox = None;
            self.dancing = false;
        }
    }
}
/// `Allay.allayConsidersItemEqual`: `ItemStack.isSameItem` and no
/// mismatching `potion_contents`. Plain, splash and lingering-free potions are
/// distinct item IDs here; tipped arrows and lingering potions carry their
/// potion in `entity_data`.
pub fn considers_equal(a: Stack, b: Stack) -> bool {
    a.item == b.item
        && (!matches!(a.item, crate::item::Item::TIPPED_ARROW | crate::item::Item::LINGERING_POTION)
            || a.entity_data == b.entity_data)
}
fn bucket(pos: DVec3) -> IVec3 {
    (pos / 8.).floor().as_ivec3()
}
impl Entities {
    /// `Allay.JukeboxListener`: `jukebox_play` within the event's 10-block
    /// notification radius of an allay's eye sets it dancing by that
    /// jukebox; `jukebox_stop_play` stops it. Each event is also offered to
    /// the vibration system ([`crate::world::jukebox::vibration_hook`]).
    pub(super) fn hear_jukeboxes(&mut self, world: &mut World) {
        use crate::world::jukebox::JukeboxEvent;
        for (p, event) in std::mem::take(&mut world.jukebox_events) {
            crate::world::jukebox::vibration_hook(world, p, event);
            let source = p.as_dvec3() + DVec3::splat(0.5);
            for m in &mut self.mobs {
                if !m.alive() {
                    continue;
                }
                let Some(s) = &mut m.allay else { continue };
                if (m.pos + DVec3::Y * 0.36).distance_squared(source) <= 100. {
                    s.set_jukebox_playing(p, event == JukeboxEvent::Play);
                }
            }
        }
    }

    pub(super) fn hear_allay_notes(&mut self, world: &mut World) {
        if world.allay_notes.is_empty() {
            return;
        }
        let mut notes = std::mem::take(&mut world.allay_notes);
        for m in &mut self.mobs {
            if !m.alive() || !world.is_entity_ticking(m.pos.floor().as_ivec3()) {
                continue;
            }
            let Some(s) = &mut m.allay else { continue };
            if s.vibration.is_some() {
                continue;
            }
            let eye = m.pos + DVec3::Y * 0.36;
            // VibrationSelector chooses the nearest eligible note this tick.
            let selected = notes
                .iter()
                .filter(|&&p| {
                    s.note.is_none_or(|liked| liked == p)
                        && eye.distance_squared(p.as_dvec3() + DVec3::splat(0.5)) <= 256.
                        && !occluded(world, p, eye)
                })
                .min_by(|a, b| {
                    eye.distance_squared(a.as_dvec3() + DVec3::splat(0.5))
                        .total_cmp(&eye.distance_squared(b.as_dvec3() + DVec3::splat(0.5)))
                });
            if let Some(&p) = selected {
                s.vibration = Some((p, eye.distance(p.as_dvec3() + DVec3::splat(0.5)).floor() as u16 + 1));
            }
        }
        notes.clear();
        world.allay_notes = notes;
    }
    pub(super) fn tick_allays<W: MobWorld + ?Sized>(
        &mut self,
        dt: f64,
        world: &W,
        ctx: &Ctx,
        events: &mut Vec<EntityEvent>,
    ) {
        if !self.mobs.iter().any(|m| m.allay.is_some() && m.simulation_active && m.alive()) {
            return;
        }
        let mut buckets: rustc_hash::FxHashMap<IVec3, Vec<usize>> = Default::default();
        for (i, item) in self.items.iter().enumerate() {
            if world.loaded(item.pos.floor().as_ivec3()) {
                buckets.entry(bucket(item.pos)).or_default().push(i);
            }
        }
        for m in &mut self.mobs {
            if !m.simulation_active || !m.alive() {
                continue;
            }
            let Some(s) = &mut m.allay else { continue };
            s.remainder += dt.max(0.);
            while s.remainder + 1e-9 >= 0.05 {
                s.remainder = (s.remainder - 0.05).max(0.);
                s.ticks = s.ticks.wrapping_add(1);
                if s.ticks.is_multiple_of(10) {
                    m.health = (m.health + 1.).min(20.);
                }
                s.duplication_ticks = s.duplication_ticks.saturating_sub(1);
                s.pickup_ticks = s.pickup_ticks.saturating_sub(1);
                s.note_ticks = s.note_ticks.saturating_sub(1);
                if s.note_ticks == 0
                    || s.note.is_some_and(|p| {
                        !world.block(p).is_some_and(crate::world::gadgets::is_note)
                            || p.as_dvec3().distance_squared(m.pos) > 1024. * 1024.
                    })
                {
                    s.note = None;
                }
                if let Some((p, left)) = &mut s.vibration {
                    *left = left.saturating_sub(1);
                    if *left == 0 {
                        let p = *p;
                        s.vibration = None;
                        s.hear_note(p);
                    }
                }
                let panic = m.ai == super::mob::Ai::Panic && m.ai_timer > 0.;
                if panic
                    || s.ticks.is_multiple_of(20)
                        && s.dancing
                        && s.jukebox.is_none_or(|p| {
                            // `shouldStopDancing`: out of range, or no longer a jukebox.
                            (p.as_dvec3() + DVec3::splat(0.5)).distance_squared(m.pos) >= 100.
                                || !world.block(p).is_some_and(crate::world::jukebox::is_jukebox)
                        })
                {
                    s.dancing = false;
                    s.jukebox = None;
                }
                s.deposit = s.note.map(|p| p.as_dvec3() + DVec3::new(0.5, 1.5, 0.5)).or_else(|| {
                    ctx.players
                        .iter()
                        .find(|p| {
                            Some(p.id) == s.player
                                && p.alive
                                && !p.spectator
                                && p.pos.distance_squared(m.pos) < 64. * 64.
                        })
                        .map(|p| p.pos + DVec3::Y * (p.shape.height * 0.9))
                });
                if s.sensor == 0 {
                    s.sensor = 20;
                    s.wanted = None;
                    if self.villager_griefing && s.pickup_ticks == 0 && !panic && !s.dancing {
                        let center = bucket(m.pos);
                        let mut best = 32. * 32.;
                        for x in -4..=4 {
                            for y in -3..=3 {
                                for z in -4..=4 {
                                    if let Some(items) = buckets.get(&(center + IVec3::new(x, y, z))) {
                                        for &i in items {
                                            let item = &self.items[i];
                                            let d = m.pos.distance_squared(item.pos);
                                            if item.stack.count > 0
                                                && (item.pos.y - m.pos.y).abs() <= 16.6
                                                && d < best
                                                && s.accepts(item.stack)
                                                && super::mob::line_of_sight(world, m.pos + DVec3::Y * 0.36, item.pos)
                                            {
                                                best = d;
                                                s.wanted = Some(item.pos);
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
                s.sensor = s.sensor.saturating_sub(1);
                if self.villager_griefing && s.pickup_ticks == 0 && !panic && !s.dancing {
                    let center = bucket(m.pos);
                    let (lo, hi) = MobKind::Allay.shape().aabb(m.pos);
                    for x in -1..=1 {
                        for y in -1..=1 {
                            for z in -1..=1 {
                                if let Some(items) = buckets.get(&(center + IVec3::new(x, y, z))) {
                                    for &i in items {
                                        let item = &mut self.items[i];
                                        let (a, b) = item.aabb();
                                        if item.stack.count == 0
                                            || item.pickup_delay > 0.
                                            || !s.accepts(item.stack)
                                            || !a.cmplt(hi + DVec3::ONE).all()
                                            || !b.cmpgt(lo - DVec3::ONE).all()
                                        {
                                            continue;
                                        }
                                        let n = item.stack.count.min(item.stack.max() - s.cargo.map_or(0, |s| s.count));
                                        let count = s.cargo.map_or(0, |s| s.count) + n;
                                        s.cargo = Some(Stack { count, ..item.stack });
                                        item.stack.count -= n;
                                        s.wanted = None;
                                        s.sensor = 0;
                                    }
                                }
                            }
                        }
                    }
                }
                if !panic
                    && !s.dancing
                    && let (Some(cargo), Some(target)) = (s.cargo, s.deposit)
                {
                    // Behavior timeout 20: start sets walk target, following ticks
                    // throw one item each, then the behavior starts again.
                    if s.give_ticks == 0 {
                        s.give_ticks = 20;
                    } else {
                        s.give_ticks -= 1;
                        if target.distance_squared(m.pos + DVec3::Y * 0.36) < 9. {
                            let pos = m.pos + DVec3::Y * 0.16;
                            let vel = (target + DVec3::Y - pos).normalize_or_zero() * DVec3::new(0.2, 0.3, 0.2) * 20.;
                            self.items.push(ItemEntity::new(Stack { count: 1, ..cargo }, pos, vel, 0.5, &mut self.rng));
                            s.cargo =
                                (cargo.count > 1).then_some(Stack { count: cargo.count.saturating_sub(1), ..cargo });
                            s.pickup_ticks = 60;
                            if s.ticks.is_multiple_of(7) && self.rng.chance(0.9) {
                                events.push(EntityEvent::Sound {
                                    sound: super::MobSound::Allay(Sound::Throw),
                                    pos: m.pos,
                                });
                            }
                        }
                    }
                } else {
                    s.give_ticks = 0;
                }
            }
        }
        self.items.retain(|i| i.stack.count > 0);
    }
    pub(super) fn allay_equipment_drops(&mut self, kind: MobKind, pos: DVec3) {
        if kind != MobKind::Allay {
            return;
        }
        let Some(s) = self
            .mobs
            .iter_mut()
            .find(|m| m.kind == kind && !m.alive() && m.pos.distance_squared(pos) < 0.01)
            .and_then(|m| m.allay.as_mut())
        else {
            return;
        };
        let drops =
            [s.cargo.take(), s.liked.take().filter(|s| !s.enchants.has(crate::enchant::Enchantment::VanishingCurse))];
        for stack in drops.into_iter().flatten() {
            self.items.push(ItemEntity::new(stack, pos, DVec3::ZERO, 0.5, &mut self.rng));
        }
    }
    pub fn allay_sound(&mut self, pos: DVec3, sound: Sound) {
        self.pending_sounds.push(EntityEvent::Sound { sound: super::MobSound::Allay(sound), pos });
    }
    pub fn duplicate_allay(&mut self, index: usize) -> bool {
        let Some(m) = self.mobs.get_mut(index).filter(|m| m.alive()) else { return false };
        let Some(s) = &mut m.allay else { return false };
        if !s.dancing || s.duplication_ticks > 0 {
            return false;
        }
        s.duplication_ticks = 6000;
        let pos = m.pos;
        self.spawn(MobKind::Allay, pos);
        self.mobs.last_mut().unwrap().allay.as_mut().unwrap().duplication_ticks = 6000;
        self.pending_sounds.push(EntityEvent::Sound { sound: super::MobSound::Allay(Sound::Duplicate), pos });
        self.particles.push(crate::particles::Request::Burst(crate::particles::Burst::new(
            crate::particles::Kind::Heart,
            pos + DVec3::Y * 0.5,
            3,
        )));
        true
    }
}
impl Mob {
    pub(super) fn allay_steer<W: MobWorld + ?Sized>(
        &mut self,
        dt: f32,
        world: &W,
        ctx: &Ctx,
        rng: &mut Rng,
    ) -> (Option<DVec3>, f64) {
        self.ai_timer -= dt;
        let s = self.allay.as_ref().unwrap();
        if s.dancing {
            self.stop_navigation();
            return (None, 0.);
        }
        let panic = self.ai == super::mob::Ai::Panic && self.ai_timer > 0.;
        let (target, speed, reach) = if panic {
            (None, 2.5, 0)
        } else if s.cargo.is_some() && s.deposit.is_some() {
            (s.deposit, 2.25, 2)
        } else if let Some(p) = s.wanted {
            (Some(p), 1.75, 0)
        } else if s.deposit.is_some_and(|p| p.distance_squared(self.pos) > 16. * 16.) {
            (s.deposit, 2.25, 4)
        } else {
            (None, 1., 0)
        };
        if let Some(target) = target {
            if self.nav.is_done() || self.ai_timer <= 0. {
                self.ai_timer = 0.5;
                self.walk_to(world, target.floor().as_ivec3(), reach, speed, false);
            }
            if target.distance_squared(self.pos + DVec3::Y * 0.36) < (reach as f64).powi(2) {
                self.stop_navigation();
                return (None, 0.);
            }
        } else if panic {
            self.panic_target(world, ctx, rng, speed);
        } else if self.nav.is_done() && self.ai_timer <= 0. {
            self.ai_timer = (30 + rng.next_int(31)) as f32 * 0.05;
            if rng.next_int(5) < 4
                && let Some(goal) = self.random_pos(world, ctx, rng, 5, 3, false)
            {
                self.walk_to(world, goal, 0, speed, false);
            }
        }
        (self.follow_path(world, speed, false), speed)
    }
}
/// FlyingMoveControl(20,true), travelFlying: both attributes are .1;
/// no gravity/fall damage, 90 degree yaw limit, 20 degree pitch limit.
pub(super) fn travel<W: MobWorld + ?Sized>(m: &mut Mob, dt: f64, world: &W, wish: Option<DVec3>, modifier: f64) {
    let mut dm = m.vel * 0.05;
    let mut input = DVec3::ZERO;
    let attribute = if m.on_ground {
        m.allay.as_ref().and_then(|s| s.ground_speed).unwrap_or(f64::from(0.1_f32))
    } else {
        f64::from(0.1_f32)
    };
    let speed = attribute * m.effects.speed_factor() * modifier;
    if let Some(dir) = wish {
        let yaw = dir.z.atan2(dir.x) as f32;
        let turn = (yaw - m.yaw + std::f32::consts::PI).rem_euclid(std::f32::consts::TAU) - std::f32::consts::PI;
        m.yaw += turn.clamp(-std::f32::consts::FRAC_PI_2, std::f32::consts::FRAC_PI_2);
        let pitch = -dir.y.atan2(dir.x.hypot(dir.z)) as f32;
        m.move_ctl.pitch += (pitch - m.move_ctl.pitch).clamp(-20_f32.to_radians(), 20_f32.to_radians());
        input =
            DVec3::new(m.yaw.cos() as f64, if dir.y.abs() > 1e-5 { dir.y.signum() } else { 0. }, m.yaw.sin() as f64)
                * speed
                * 0.98;
    }
    let water = physics::touches_block(world, m.pos, m.shape(), Block::holds_water);
    let lava = physics::touches_block(world, m.pos, m.shape(), Block::is_lava);
    dm += input.normalize_or_zero() * input.length().min(1.) * if water || lava { 0.02 } else { speed };
    let delta = dm * (dt / 0.05);
    let shape = m.shape();
    let hit = physics::move_box(world, &mut m.pos, &mut dm, delta, shape);
    m.on_ground = hit.on_ground;
    m.blocked = hit.horizontal;
    m.in_water = water;
    m.vel =
        dm * if water {
            0.8
        } else if lava {
            0.5
        } else {
            0.91
        } * 20.;
}
/// VibrationSystem.Listener: six rays between cell centers, each offset
/// 1e-5 toward one face. Block tags, rather than collision shapes, occlude.
fn occluded<W: MobWorld + ?Sized>(world: &W, p: IVec3, to: DVec3) -> bool {
    let to = to.floor() + DVec3::splat(0.5);
    [DVec3::X, DVec3::NEG_X, DVec3::Y, DVec3::NEG_Y, DVec3::Z, DVec3::NEG_Z]
        .into_iter()
        .all(|side| wool_in_line(world, p.as_dvec3() + DVec3::splat(0.5) + side * f64::from(1e-5_f32), to))
}
fn wool_in_line<W: MobWorld + ?Sized>(world: &W, from: DVec3, to: DVec3) -> bool {
    let delta = to - from;
    let step = IVec3::from_array(delta.to_array().map(|v| {
        if v > 0. {
            1
        } else if v < 0. {
            -1
        } else {
            0
        }
    }));
    let inv = DVec3::from_array(delta.to_array().map(|v| if v == 0. { f64::INFINITY } else { 1. / v.abs() }));
    let mut cell = from.floor().as_ivec3();
    let end = to.floor().as_ivec3();
    let mut next = DVec3::ZERO;
    for axis in 0..3 {
        next[axis] =
            if step[axis] > 0 { f64::from(cell[axis] + 1) - from[axis] } else { from[axis] - f64::from(cell[axis]) }
                * inv[axis];
    }
    // A listener is at most 16 blocks away: even diagonal voxel traversal
    // visits fewer than 96 cells. Keep the event hot path explicitly bounded.
    for _ in 0..96 {
        if world.block(cell).is_some_and(|b| b.wool_color().is_some()) {
            return true;
        }
        if cell == end {
            return false;
        }
        let axis = if next.x < next.y && next.x < next.z {
            0
        } else if next.y < next.z {
            1
        } else {
            2
        };
        cell[axis] += step[axis];
        next[axis] += inv[axis];
    }
    false
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum Sound {
    Give,
    Take,
    Throw,
    Duplicate,
}
#[cfg(test)]
mod tests;
