//! Water creatures and structure residents. Timers are in seconds; bucket
//! components and one-time population markers survive save/load.
use super::{Ctx, Entities, EntityEvent, Mob, MobKind, MobWorld, Rng};
use crate::{
    inventory::{Inventory, Stack},
    item::Item,
    physics,
    simulation::{difficulty::Difficulty, effects::Effect},
    world::{
        World,
        block::Block,
        temples::Kind,
        terrain::{Biome, Dimension, SEA_LEVEL},
    },
};
use glam::{DVec3, IVec3};

#[derive(Clone, Debug)]
pub struct State {
    pub variant: u8,
    pub puff: u8,
    pub beam: Option<DVec3>,
    pub charge: f32,
    target: Option<super::PlayerId>,
    school: Option<DVec3>,
    prey: Option<(u32, DVec3)>,
    sense_timer: f32,
    pub(super) play_dead: f32,
    air: f32,
    dry: f32,
    damage_tick: f32,
    wander: f32,
    direction: DVec3,
    effect_timer: f32,
    contact_timer: f32,
}
impl Default for State {
    fn default() -> Self {
        Self::new()
    }
}
impl State {
    pub fn new() -> Self {
        Self {
            target: None,
            school: None,
            prey: None,
            sense_timer: 0.0,
            play_dead: 0.0,
            variant: 0,
            puff: 0,
            beam: None,
            charge: 0.0,
            air: 240.0,
            dry: 0.0,
            damage_tick: 0.0,
            wander: 0.0,
            direction: DVec3::ZERO,
            effect_timer: 0.0,
            contact_timer: 0.0,
        }
    }
}
pub(super) fn on_spawn(m: &mut Mob, rng: &mut Rng) {
    if let Some(a) = &mut m.aquatic {
        a.variant = rng.next_int(if m.kind == MobKind::Axolotl { 4 } else { 12 }) as u8;
    }
}
fn wet<W: physics::BlockSource + ?Sized>(w: &W, p: DVec3) -> bool {
    w.block(p.floor().as_ivec3()).is_some_and(Block::holds_water)
}

pub(super) fn environment<W: MobWorld + ?Sized>(m: &mut Mob, dt: f32, w: &W) {
    if !m.alive() {
        return;
    }
    let Some(a) = &mut m.aquatic else { return };
    let submerged = wet(w, m.pos + DVec3::Y * (m.kind.shape().height * 0.8));
    let dolphin = m.kind == MobKind::Dolphin;
    let axolotl = m.kind == MobKind::Axolotl;
    let guardian = matches!(m.kind, MobKind::Guardian | MobKind::ElderGuardian);
    if guardian {
        return;
    }
    if dolphin {
        a.air = if submerged { a.air - dt } else { 240.0 };
        a.dry = if wet(w, m.pos) || w.rains_on(m.pos.floor().as_ivec3()) { 0.0 } else { a.dry + dt };
    } else {
        a.dry = if submerged || (axolotl && w.rains_on(m.pos.floor().as_ivec3())) { 0.0 } else { a.dry + dt };
    }
    let suffocating =
        if dolphin { a.air <= 0.0 || a.dry >= 120.0 } else { a.dry >= if axolotl { 300.0 } else { 15.0 } };
    if suffocating {
        a.damage_tick += dt;
        if a.damage_tick >= 1.0 {
            m.health -= 2.0;
            a.damage_tick %= 1.0;
        }
    } else {
        a.damage_tick = 0.0;
    }
}

pub(super) fn think<W: MobWorld + ?Sized>(
    m: &mut Mob,
    dt: f32,
    w: &W,
    ctx: &Ctx,
    rng: &mut Rng,
    events: &mut Vec<EntityEvent>,
) -> (Option<DVec3>, f64) {
    let shape = m.shape();
    let eye = m.pos + DVec3::Y * shape.height * 0.5;
    let a = m.aquatic.as_mut().unwrap();
    a.effect_timer -= dt;
    a.contact_timer -= dt;
    let guardian = matches!(m.kind, MobKind::Guardian | MobKind::ElderGuardian);
    if m.kind == MobKind::ElderGuardian && a.effect_timer <= 0.0 {
        a.effect_timer = 60.0;
        for t in ctx.players.iter().filter(|t| t.targetable && t.alive && t.pos.distance_squared(m.pos) < 2500.0) {
            events.push(EntityEvent::PlayerEffect {
                player: t.id,
                effect: Effect::MiningFatigue,
                amplifier: 2,
                ticks: 6000,
            });
        }
    }
    if guardian {
        let target = ctx
            .players
            .iter()
            .filter(|t| t.targetable && t.alive)
            .filter(|t| {
                let d = t.pos.distance_squared(m.pos);
                d < 256.0
                    && (m.kind == MobKind::ElderGuardian || d > 9.0)
                    && super::mob::line_of_sight(w, eye, t.pos + DVec3::Y)
            })
            .min_by(|l, r| l.pos.distance_squared(m.pos).total_cmp(&r.pos.distance_squared(m.pos)));
        if let Some(t) = target {
            let to = t.pos + DVec3::Y;
            if a.target != Some(t.id) {
                a.charge = -0.5;
            }
            a.target = Some(t.id);
            a.beam = Some(to);
            a.charge += dt;
            m.yaw = ((to.z - eye.z) as f32).atan2((to.x - eye.x) as f32);
            let duration = if m.kind == MobKind::ElderGuardian { 3.0 } else { 4.0 };
            if a.charge >= duration {
                let magic = if m.difficulty == Difficulty::Hard { 3.0 } else { 1.0 };
                events.push(EntityEvent::PlayerBeam {
                    player: t.id,
                    physical: if m.kind == MobKind::ElderGuardian { 8.0 } else { 6.0 },
                    magic,
                });
                a.charge = -1.0;
            }
            return (None, 0.0);
        }
        a.target = None;
        a.beam = None;
        a.charge = 0.0;
    }
    if m.kind == MobKind::Pufferfish {
        let near = ctx.players.iter().any(|t| t.targetable && t.alive && t.pos.distance_squared(m.pos) < 4.0);
        a.charge = if near { (a.charge + dt).min(2.0) } else { (a.charge - dt * 0.3).max(0.0) };
        a.puff = if a.charge > 0.5 { 2 } else { u8::from(a.charge > 0.0) };
        if a.puff > 0 && a.contact_timer <= 0.0 {
            for t in ctx.players.iter().filter(|t| t.targetable && t.alive && t.pos.distance_squared(m.pos) < 1.0) {
                events.push(EntityEvent::PlayerSting {
                    player: t.id,
                    damage: 1.0 + a.puff as f32,
                    cause: "was stung by a pufferfish",
                });
                events.push(EntityEvent::PlayerEffect {
                    player: t.id,
                    effect: Effect::Poison,
                    amplifier: 0,
                    ticks: 60 * a.puff as u32,
                });
                a.contact_timer = 1.0;
            }
        }
    }
    if m.kind == MobKind::Dolphin && a.effect_timer <= 0.0 {
        a.effect_timer = 1.0;
        for t in ctx.players.iter().filter(|t| t.alive && t.pos.distance_squared(m.pos) < 81.0 && wet(w, t.pos)) {
            events.push(EntityEvent::PlayerEffect {
                player: t.id,
                effect: Effect::DolphinsGrace,
                amplifier: 0,
                ticks: 100,
            });
        }
    }
    if m.kind == MobKind::Axolotl {
        if a.play_dead > 0.0 {
            a.play_dead -= dt;
            m.health = (m.health + dt * 0.4).min(m.kind.max_health());
            return (None, 0.0);
        }

        if let Some((uid, pos)) = a.prey.filter(|(_, pos)| wet(w, *pos)) {
            let delta = pos - m.pos;
            if delta.length_squared() < 1.5 && m.attack_cooldown <= 0.0 {
                m.attack_cooldown = 1.0;
                events.push(EntityEvent::MobHit {
                    target: uid,
                    attacker: m.uid,
                    damage: 2.0,
                    knockback: delta.normalize_or_zero() * 0.3,
                });
            }
            return (Some(delta.normalize_or_zero()), 2.0);
        }
    }
    if let Some(leader) = a.school {
        let delta = leader - m.pos;
        if delta.length_squared() > 1.5 && delta.length_squared() < 256.0 && wet(w, m.pos + delta.normalize_or_zero()) {
            return (Some(delta.normalize_or_zero()), 1.4);
        }
    }
    a.wander -= dt;
    let oxygen = m.kind == MobKind::Dolphin && a.air < 20.0;
    if a.wander <= 0.0 || !wet(w, m.pos + a.direction * 1.5) || physics::overlaps_solid(w, m.pos + a.direction, shape) {
        a.wander = rng.range(1.0, 3.0);
        a.direction = DVec3::new(
            rng.range(-1.0, 1.0) as f64,
            if oxygen { 1.0 } else { rng.range(-0.45, 0.45) as f64 },
            rng.range(-1.0, 1.0) as f64,
        )
        .normalize_or_zero();
        // Don't swim into an unloaded chunk or leave water while wandering.
        if !oxygen && (!wet(w, m.pos + a.direction) || physics::overlaps_solid(w, m.pos + a.direction, shape)) {
            a.direction = DVec3::ZERO;
        }
    }
    (
        Some(a.direction),
        if m.kind == MobKind::Dolphin {
            2.8
        } else if m.kind == MobKind::ElderGuardian {
            0.65
        } else {
            1.2
        },
    )
}

pub(super) fn pillager<W: MobWorld + ?Sized>(
    m: &mut Mob,
    _dt: f32,
    w: &W,
    ctx: &Ctx,
    events: &mut Vec<EntityEvent>,
) -> (Option<DVec3>, f64) {
    let Some(t) = ctx.nearest_target(m.pos) else { return (None, 0.0) };
    let delta = t.pos - m.pos;
    if delta.length_squared() > 1024.0 {
        return (None, 0.0);
    }
    let from = m.pos + DVec3::Y * 1.55;
    let visible = super::mob::line_of_sight(w, from, t.pos + DVec3::Y);
    m.charged = visible && delta.length_squared() < 225.0;
    if m.charged && m.attack_cooldown <= 0.0 {
        events.push(EntityEvent::Shoot { from, target: t.pos + DVec3::Y, cause: "was shot by a pillager" });
        m.attack_cooldown = 2.5;
        m.attack_anim = 0.4;
    }
    let dir = (delta * DVec3::new(1.0, 0.0, 1.0)).normalize_or_zero();
    m.yaw = (dir.z as f32).atan2(dir.x as f32);
    (if m.charged { None } else { Some(dir) }, 2.4)
}

pub(super) fn spawn_spot<W: MobWorld + ?Sized>(
    w: &W,
    kind: MobKind,
    x: i32,
    z: i32,
    center_y: i32,
    rng: &mut Rng,
) -> Option<DVec3> {
    let underground =
        matches!(kind, MobKind::GlowSquid | MobKind::Axolotl) || (kind == MobKind::TropicalFish && center_y < 40);
    let (lo, hi) = if underground {
        ((center_y - 12).max(-63), (center_y + 12).min(if kind == MobKind::GlowSquid { 29 } else { 62 }))
    } else if kind == MobKind::Guardian {
        (39, 62)
    } else {
        (50, SEA_LEVEL - 1)
    };
    if lo > hi {
        return None;
    }
    let y = lo + rng.next_int((hi - lo + 1) as u32) as i32;
    let p = IVec3::new(x, y, z);
    let pos = p.as_dvec3() + DVec3::new(0.5, 0.0, 0.5);
    if !w.loaded(p)
        || !wet(w, pos)
        || !wet(w, pos + DVec3::Y * kind.shape().height)
        || physics::overlaps_solid(w, pos, kind.shape())
    {
        return None;
    }
    match kind {
        MobKind::Guardian
            if !w.structures_near(p, 0).iter().any(|b| b.kind == Kind::Monument && b.spawns.contains(p)) =>
        {
            return None;
        }
        MobKind::GlowSquid
            if w.block_light(p) != 0
                || super::sky_light(w, pos) > 0.0
                || !(1..=5).any(|dy| {
                    w.block(p - IVec3::Y * dy).is_some_and(|b| matches!(b, Block::STONE | Block::DEEPSLATE))
                }) =>
        {
            return None;
        }
        MobKind::Axolotl
            if w.cave_biome(p) != Biome::LushCaves
                || !(1..=5).any(|dy| w.block(p - IVec3::Y * dy) == Some(Block::CLAY)) =>
        {
            return None;
        }
        MobKind::TropicalFish if underground && w.cave_biome(p) != Biome::LushCaves => return None,
        _ => {}
    }
    Some(pos)
}

pub fn bucket_kind(item: Item) -> Option<MobKind> {
    Some(match item {
        Item::COD_BUCKET => MobKind::Cod,
        Item::SALMON_BUCKET => MobKind::Salmon,
        Item::TROPICAL_FISH_BUCKET => MobKind::TropicalFish,
        Item::PUFFERFISH_BUCKET => MobKind::Pufferfish,
        Item::AXOLOTL_BUCKET => MobKind::Axolotl,
        _ => return None,
    })
}
fn bucket_item(kind: MobKind) -> Option<Item> {
    Some(match kind {
        MobKind::Cod => Item::COD_BUCKET,
        MobKind::Salmon => Item::SALMON_BUCKET,
        MobKind::TropicalFish => Item::TROPICAL_FISH_BUCKET,
        MobKind::Pufferfish => Item::PUFFERFISH_BUCKET,
        MobKind::Axolotl => Item::AXOLOTL_BUCKET,
        _ => return None,
    })
}

/// Java's baby turtle growing time: 20 minutes.
pub const TURTLE_GROW_SECS: f32 = 1200.0;

impl Entities {
    /// Baby turtles grow up after twenty minutes and shed a scute.
    pub(super) fn grow_turtles(&mut self, dt: f32) {
        let mut scutes = Vec::new();
        for m in &mut self.mobs {
            if m.kind == MobKind::Turtle && m.baby && m.alive() {
                m.grow += dt;
                if m.grow >= TURTLE_GROW_SECS {
                    m.baby = false;
                    scutes.push(m.pos);
                }
            }
        }
        for pos in scutes {
            self.drop_from_block(Stack::new(Item::TURTLE_SCUTE, 1), pos.floor().as_ivec3());
        }
    }

    /// Turtles hatching from eggs at `cell`.
    pub fn hatch_turtles(&mut self, cell: glam::IVec3, count: u8) {
        for i in 0..count {
            let pos = cell.as_dvec3() + DVec3::new(0.3 + i as f64 * 0.15, 0.0, 0.5);
            self.spawn(MobKind::Turtle, pos);
            let m = self.mobs.last_mut().unwrap();
            m.baby = true;
            m.persistent = true;
        }
    }

    /// Shared bucket action for keyboard, controller and CLI players. Source
    /// raycasts stop through walls; entity capture is limited by the same reach.
    #[allow(clippy::too_many_arguments)]
    pub fn use_water_creature_bucket(
        &mut self,
        world: &mut World,
        inventory: &mut Inventory,
        slot: usize,
        creative: bool,
        eye: DVec3,
        dir: DVec3,
        reach: f64,
    ) -> bool {
        let Some(stack) = inventory.get(slot) else { return false };
        if stack.item == Item::WATER_BUCKET {
            let distance =
                world.raycast(eye, dir, reach).map_or(reach, |(p, _)| (p.as_dvec3() + DVec3::splat(0.5)).distance(eye));
            let Some((i, _)) = self.raycast(eye, dir, distance) else { return false };
            let m = &self.mobs[i];
            let Some(item) = bucket_item(m.kind) else { return false };
            let mut filled = Stack::new(item, 1);
            filled.entity_data = 1
                | ((m.aquatic.as_ref().unwrap().variant as u32) << 1)
                | (((m.health.clamp(0.0, 255.0) * 100.0).round() as u32) << 9);
            self.mobs.swap_remove(i);
            if !creative {
                inventory.slots[slot] = Some(filled);
            } else if inventory.add_stack(filled) > 0 {
                self.throw(filled, eye, dir);
            }
            return true;
        }
        if bucket_kind(stack.item).is_none() {
            return false;
        }
        let Some((p, n)) = world.raycast_sources(eye, dir, reach) else { return false };
        let at = if world.get_block(p).is_some_and(|b| b.is_replaceable() || b.holds_water()) { p } else { p + n };
        if !world.get_block(at).is_some_and(|b| b.is_replaceable() || b.holds_water()) {
            return false;
        }
        if world.generator.dimension != Dimension::Nether {
            world.set_block(at, Block::WATER);
        }
        self.release_water_creature(stack, at.as_dvec3() + DVec3::new(0.5, 0.0, 0.5));
        if !creative {
            inventory.slots[slot] = Some(Stack::new(Item::BUCKET, 1));
        }
        true
    }
    /// Dispensers use the same bucket payload as players.
    pub fn release_water_creature(&mut self, stack: Stack, pos: DVec3) -> bool {
        let Some(kind) = bucket_kind(stack.item) else { return false };
        self.spawn(kind, pos);
        let m = self.mobs.last_mut().unwrap();
        m.persistent = true;
        if stack.entity_data & 1 != 0 {
            m.aquatic.as_mut().unwrap().variant = ((stack.entity_data >> 1) & 255) as u8;
            m.health = ((stack.entity_data >> 9) as f32 / 100.0).clamp(0.01, kind.max_health());
        }
        true
    }
    pub(super) fn aquatic_sense<W: MobWorld + ?Sized>(&mut self, dt: f32, w: &W) {
        if !self.mobs.iter().any(|m| m.aquatic.is_some()) {
            return;
        }
        if self.nether_view.is_empty() {
            self.nether_view.extend(self.mobs.iter().filter(|m| m.alive()).map(|m| super::nether::Seen {
                uid: m.uid,
                kind: m.kind,
                pos: m.pos,
                baby: m.baby,
                huntable: false,
            }));
        }
        for m in &mut self.mobs {
            let Some(a) = &mut m.aquatic else { continue };
            a.sense_timer -= dt;
            if a.sense_timer > 0.0 {
                continue;
            }
            a.sense_timer = 0.5;
            a.school = None;
            a.prey = None;
            if matches!(m.kind, MobKind::Cod | MobKind::Salmon | MobKind::TropicalFish) {
                a.school = self
                    .nether_view
                    .iter()
                    .filter(|v| v.kind == m.kind && v.uid < m.uid && v.pos.distance_squared(m.pos) < 64.0)
                    .min_by_key(|v| v.uid)
                    .map(|v| v.pos);
            }
            if m.kind == MobKind::Axolotl {
                a.prey = self
                    .nether_view
                    .iter()
                    .filter(|v| {
                        v.uid != m.uid
                            && matches!(
                                v.kind,
                                MobKind::Cod
                                    | MobKind::Salmon
                                    | MobKind::TropicalFish
                                    | MobKind::Squid
                                    | MobKind::GlowSquid
                                    | MobKind::Guardian
                                    | MobKind::Drowned
                            )
                            && v.pos.distance_squared(m.pos) < 64.0
                            && wet(w, v.pos)
                    })
                    .min_by(|l, r| l.pos.distance_squared(m.pos).total_cmp(&r.pos.distance_squared(m.pos)))
                    .map(|v| (v.uid, v.pos));
            }
        }
    }
    pub(super) fn populate_structures<W: MobWorld + ?Sized>(
        &mut self,
        dt: f32,
        w: &W,
        ctx: &Ctx,
        difficulty: Difficulty,
    ) {
        if ctx.dimension != Dimension::Overworld {
            return;
        }
        self.structure_timer -= dt;
        if self.structure_timer > 0.0 {
            return;
        }
        self.structure_timer = 1.0;
        for t in &ctx.players {
            for b in w.structures_near(t.pos.floor().as_ivec3(), 80) {
                if self.structures_populated.contains(&b.bounds.min) {
                    if ctx.spawning && difficulty != Difficulty::Peaceful && self.rng.chance(0.15) {
                        let spawn = match b.kind {
                            Kind::Outpost => Some((MobKind::Pillager, 3)),
                            Kind::SwampHut => Some((MobKind::Witch, 1)),
                            _ => None,
                        };
                        if let Some((kind, cap)) = spawn {
                            let count = self
                                .mobs
                                .iter()
                                .filter(|m| m.kind == kind && m.alive() && b.spawns.contains(m.pos.floor().as_ivec3()))
                                .count();
                            let p = b.residents.iter().find(|(k, _)| *k == kind).map(|(_, p)| *p);
                            if count < cap
                                && let Some(p) = p
                                && super::clear_of_players(ctx, p.as_dvec3())
                                && w.loaded(p)
                                && !physics::overlaps_solid(w, p.as_dvec3() + DVec3::new(0.5, 0.0, 0.5), kind.shape())
                            {
                                self.spawn(kind, p.as_dvec3() + DVec3::new(0.5, 0.0, 0.5));
                            }
                        }
                    }
                    continue;
                }
                if b.residents.is_empty() || !b.residents.iter().all(|(_, p)| w.loaded(*p)) {
                    continue;
                }
                // Wait for blocks, not just the chunk addresses, before recording it.
                if b.residents
                    .iter()
                    .any(|(k, p)| physics::overlaps_solid(w, p.as_dvec3() + DVec3::new(0.5, 0.0, 0.5), k.shape()))
                {
                    continue;
                }
                self.structures_populated.insert(b.bounds.min);
                for &(kind, p) in &b.residents {
                    if difficulty == Difficulty::Peaceful && kind.is_hostile() {
                        continue;
                    }
                    self.spawn(kind, p.as_dvec3() + DVec3::new(0.5, 0.0, 0.5));
                    let m = self.mobs.last_mut().unwrap();
                    m.persistent = kind != MobKind::Pillager;
                    if kind == MobKind::Cat {
                        m.wool_color = crate::color::DyeColor::Black;
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        entity::{PlayerId, Target},
        physics::test_util::Grid,
        world::terrain::Generator,
    };
    use std::{
        sync::Arc,
        time::{Duration, Instant},
    };
    fn water() -> Grid {
        let mut w = Grid::flat(-64);
        for x in -5..=20 {
            for y in -2..=8 {
                for z in -5..=5 {
                    w.set(IVec3::new(x, y, z), Block::WATER);
                }
            }
        }
        w
    }
    fn ctx(p: DVec3) -> Ctx {
        Ctx {
            players: vec![Target::new(PlayerId(7), p, true)],
            daylight: 1.0,
            spawning: false,
            raining: false,
            dimension: Dimension::Overworld,
        }
    }
    #[test]
    fn swimming_stays_underwater_instead_of_bobbing_to_the_surface() {
        let w = water();
        let mut m = Mob::new(MobKind::Cod, DVec3::new(0.5, 0.0, 0.5), 0.0);
        let mut r = Rng::new(4);
        let mut e = vec![];
        for _ in 0..100 {
            m.update(0.05, &w, &Ctx { players: vec![], ..ctx(DVec3::ZERO) }, &mut r, &mut e);
        }
        assert!(m.pos.y < 6.0);
        assert!(m.in_water);
        assert_eq!(m.health, 3.0);
        assert!(m.pos.distance(DVec3::new(0.5, 0.0, 0.5)) > 0.5);
    }
    #[test]
    fn fish_suffocate_and_water_resets_the_air_timer() {
        let mut m = Mob::new(MobKind::Cod, DVec3::ZERO, 0.0);
        let dry = Grid::flat(-64);
        environment(&mut m, 15.5, &dry);
        assert_eq!(m.health, 1.0);
        environment(&mut m, 1.0, &water());
        assert_eq!(m.aquatic.as_ref().unwrap().dry, 0.0);
        environment(&mut m, 14.0, &dry);
        assert_eq!(m.health, 1.0);
    }
    #[test]
    fn guardian_beam_charges_breaks_on_occlusion_and_respects_target_identity() {
        let mut w = water();
        let mut m = Mob::new(MobKind::Guardian, DVec3::ZERO, 0.0);
        let c = ctx(DVec3::new(10.0, 0.0, 0.0));
        let mut r = Rng::new(1);
        let mut e = vec![];
        for _ in 0..80 {
            think(&mut m, 0.05, &w, &c, &mut r, &mut e);
        }
        assert!(!e.iter().any(|e| matches!(e, EntityEvent::PlayerBeam { .. })));
        w.set(IVec3::new(5, 0, 0), Block::STONE);
        w.set(IVec3::new(5, 1, 0), Block::STONE);
        think(&mut m, 0.05, &w, &c, &mut r, &mut e);
        assert!(m.aquatic.as_ref().unwrap().beam.is_none());
        w.set(IVec3::new(5, 0, 0), Block::WATER);
        w.set(IVec3::new(5, 1, 0), Block::WATER);
        for _ in 0..92 {
            think(&mut m, 0.05, &w, &c, &mut r, &mut e);
        }
        assert!(
            e.iter().any(
                |e| matches!(e,EntityEvent::PlayerBeam{player,physical,..} if *player==PlayerId(7)&&*physical==6.0)
            )
        );
        assert!(e.iter().any(|e| matches!(e,EntityEvent::PlayerBeam{magic,..} if *magic==1.0)));
        let mut other = c;
        other.players[0].id = PlayerId(8);
        think(&mut m, 0.05, &w, &other, &mut r, &mut e);
        assert!(m.aquatic.as_ref().unwrap().charge < 0.0);
    }
    #[test]
    fn elders_fatigue_survival_players_even_through_walls_and_ignore_creative() {
        let w = water();
        let mut m = Mob::new(MobKind::ElderGuardian, DVec3::ZERO, 0.0);
        let mut c = ctx(DVec3::new(40.0, 0.0, 0.0));
        c.players.push(Target::new(PlayerId(8), DVec3::Y, false));
        let mut e = vec![];
        think(&mut m, 0.05, &w, &c, &mut Rng::new(1), &mut e);
        assert_eq!(
            e.iter()
                .filter(|e| matches!(
                    e,
                    EntityEvent::PlayerEffect { effect: Effect::MiningFatigue, amplifier: 2, ticks: 6000, .. }
                ))
                .count(),
            1
        );
        e.clear();
        think(&mut m, 1.0, &w, &c, &mut Rng::new(1), &mut e);
        assert!(e.is_empty());
    }
    #[test]
    fn pufferfish_stings_and_dolphins_grant_a_separate_water_effect() {
        let w = water();
        let c = ctx(DVec3::new(0.3, 0.0, 0.0));
        let mut r = Rng::new(1);
        let mut e = vec![];
        let mut p = Mob::new(MobKind::Pufferfish, DVec3::ZERO, 0.0);
        think(&mut p, 0.6, &w, &c, &mut r, &mut e);
        assert!(e.iter().any(|e| matches!(e, EntityEvent::PlayerEffect { effect: Effect::Poison, ticks: 120, .. })));
        let mut d = Mob::new(MobKind::Dolphin, DVec3::ZERO, 0.0);
        think(&mut d, 0.05, &w, &c, &mut r, &mut e);
        assert!(
            e.iter().any(|e| matches!(e, EntityEvent::PlayerEffect { effect: Effect::DolphinsGrace, ticks: 100, .. }))
        );
    }
    #[test]
    fn axolotls_hunt_fish_and_school_leaders_are_stable() {
        let w = water();
        let mut e = Entities::new(4);
        for x in [0.5, 3.5, 5.5] {
            e.spawn(MobKind::Cod, DVec3::new(x, 0.0, 0.0));
        }
        e.spawn(MobKind::Axolotl, DVec3::ZERO);
        e.nether_sense(0.05, &w, &ctx(DVec3::ZERO));
        e.aquatic_sense(0.05, &w);
        assert_eq!(e.mobs[1].aquatic.as_ref().unwrap().school, Some(e.mobs[0].pos));
        assert_eq!(e.mobs[3].aquatic.as_ref().unwrap().prey.unwrap().0, e.mobs[0].uid);
        let mut events = vec![];
        think(&mut e.mobs[3], 0.05, &w, &ctx(DVec3::ZERO), &mut Rng::new(1), &mut events);
        assert!(events.iter().any(|e| matches!(e, EntityEvent::MobHit { damage: 2.0, .. })));
    }
    fn world() -> World {
        let mut w = World::new_headless(Arc::new(Generator::new(1)), Default::default(), 2);
        let end = Instant::now() + Duration::from_secs(20);
        while !w.column_loaded(0, 0) || w.pending_jobs() > 0 {
            w.update(DVec3::new(0.0, 150.0, 0.0));
            assert!(Instant::now() < end);
            std::thread::sleep(Duration::from_millis(1));
        }
        w
    }
    #[test]
    fn bucket_components_round_trip_and_capture_cannot_pass_through_walls() {
        let mut w = world();
        let eye = DVec3::new(0.5, 151.2, 0.5);
        let mut e = Entities::new(1);
        e.spawn(MobKind::Axolotl, DVec3::new(2.5, 151.0, 0.5));
        e.mobs[0].aquatic.as_mut().unwrap().variant = 3;
        e.mobs[0].health = 6.25;
        let mut inv = Inventory::default();
        inv.slots[0] = Some(Stack::new(Item::WATER_BUCKET, 1));
        w.set_block(IVec3::new(1, 151, 0), Block::STONE);
        assert!(!e.use_water_creature_bucket(&mut w, &mut inv, 0, false, eye, DVec3::X, 6.0));
        w.set_block(IVec3::new(1, 151, 0), Block::AIR);
        assert!(e.use_water_creature_bucket(&mut w, &mut inv, 0, false, eye, DVec3::X, 6.0));
        assert!(e.mobs.is_empty());
        let encoded = crate::inventory::stack_to_string(inv.get(0));
        assert_eq!(crate::inventory::stack_from_str(&encoded), Some(inv.get(0)));
        w.set_block(IVec3::new(4, 151, 0), Block::STONE);
        assert!(e.use_water_creature_bucket(&mut w, &mut inv, 0, false, eye, DVec3::X, 6.0));
        let m = &e.mobs[0];
        assert!(m.persistent);
        assert_eq!(m.health, 6.25);
        assert_eq!(m.aquatic.as_ref().unwrap().variant, 3);
        assert_eq!(inv.get(0).unwrap().item, Item::BUCKET);
        let saved = e.nether_mobs_to_string();
        let mut restored = Entities::new(2);
        restored.load_nether_mobs(&saved);
        assert_eq!(restored.mobs[0].aquatic.as_ref().unwrap().variant, 3);
        assert_eq!(restored.mobs[0].health, 6.25);
    }
    #[test]
    fn guardian_beam_keeps_magic_damage_when_armor_reduces_the_physical_hit() {
        let mut e = Entities::new(1);
        let mut a = crate::agent::Agent::new(DVec3::ZERO);
        a.inventory.armor = [
            Some(Stack::new(Item::armor(crate::item::ArmorPiece::Helmet, crate::item::ArmorMaterial::Diamond), 1)),
            Some(Stack::new(Item::armor(crate::item::ArmorPiece::Chestplate, crate::item::ArmorMaterial::Diamond), 1)),
            Some(Stack::new(Item::armor(crate::item::ArmorPiece::Leggings, crate::item::ArmorMaterial::Diamond), 1)),
            Some(Stack::new(Item::armor(crate::item::ArmorPiece::Boots, crate::item::ArmorMaterial::Diamond), 1)),
        ];
        let expected =
            crate::simulation::survival::armor_reduce(6.0, a.inventory.armor_points(), a.inventory.armor_toughness())
                + 1.0;
        let taken = a.hurt_beam(6.0, 1.0, &mut e);
        assert!((taken - expected).abs() < 1e-5);
        let mut bare = crate::agent::Agent::new(DVec3::ZERO);
        assert_eq!(bare.hurt_beam(6.0, 1.0, &mut e), 7.0);
    }

    #[test]
    fn guardian_spikes_are_fixed_contact_damage_and_credit_the_actual_hitter() {
        let mut e = Entities::new(1);
        e.spawn(MobKind::Guardian, DVec3::ZERO);
        e.melee_for(0, DVec3::X, None, 0.0, false, None, PlayerId(12));
        assert!(
            e.pending_sounds
                .iter()
                .any(|e| matches!(e, EntityEvent::PlayerSting { player: PlayerId(12), damage: 2.0, .. }))
        );
    }

    #[test]
    fn water_creatures_require_water_and_only_monuments_spawn_guardians() {
        let mut r = Rng::new(1);
        let mut w = water();
        assert!(spawn_spot(&w, MobKind::Cod, 0, 0, 60, &mut r).is_none());
        for y in 50..=63 {
            w.set(IVec3::new(0, y, 0), Block::WATER);
        }
        assert!(spawn_spot(&w, MobKind::Cod, 0, 0, 60, &mut r).is_some());
        assert!(spawn_spot(&w, MobKind::Guardian, 0, 0, 60, &mut r).is_none());
    }
    #[test]
    fn persistent_structure_markers_prevent_resurrection_after_reload() {
        let mut e = Entities::new(1);
        e.structures_populated.insert(IVec3::new(-10, 39, 20));
        e.spawn(MobKind::ElderGuardian, DVec3::ZERO);
        e.mobs[0].persistent = true;
        let mut restored = Entities::new(2);
        restored.load_nether_mobs(&e.nether_mobs_to_string());
        assert_eq!(restored.count(MobKind::ElderGuardian), 1);
        assert_eq!(restored.structures_populated, e.structures_populated);
        e.mobs.clear();
        let mut dead = Entities::new(2);
        dead.load_nether_mobs(&e.nether_mobs_to_string());
        assert!(dead.mobs.is_empty());
        assert_eq!(dead.structures_populated, e.structures_populated);
    }

    #[test]
    fn turtles_hatch_as_babies_and_grow_up_shedding_a_scute() {
        let mut e = Entities::new(3);
        e.hatch_turtles(glam::IVec3::new(0, 64, 0), 2);
        assert_eq!(e.mobs.iter().filter(|m| m.kind == MobKind::Turtle && m.baby).count(), 2);
        e.grow_turtles(TURTLE_GROW_SECS + 1.0);
        assert!(e.mobs.iter().all(|m| !m.baby));
        assert_eq!(e.items.iter().filter(|i| i.stack.item == Item::TURTLE_SCUTE).count(), 2);
        assert!(crate::entity::can_spawn_on(MobKind::Turtle, Block::SAND, 1.0));
        assert!(!crate::entity::can_spawn_on(MobKind::Turtle, Block::GRASS, 1.0));
        assert!(MobKind::Turtle.biome_chance(crate::world::terrain::Biome::Beach) > 0.0);
        assert_eq!(MobKind::Turtle.biome_chance(crate::world::terrain::Biome::Plains), 0.0);
    }
}
