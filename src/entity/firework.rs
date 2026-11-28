//! Java 26.3 `FireworkRocketEntity` (v0.16 Track B): rockets launched from
//! a block, fired from crossbows and dispensers (`shotAtAngle`), and held by
//! a gliding player for a boost.
//!
//! A free rocket speeds up 1.15 times horizontally and 0.04 upward each
//! tick unless it was shot at an angle (then it flies straight, without
//! gravity); it lives `10 * (1 + flight) + rand(6) + rand(7)` ticks, then
//! bursts. Hitting a mob or player bursts it at once; hitting a block does
//! only when it carries stars. A burst with stars hurts the holder by
//! `5 + 2 * stars` and every other living thing within five blocks it can
//! see by that times `sqrt((5 - distance) / 5)` (`fireworks`, an explosion
//! type). Rockets aren't saved: each lives under four seconds, as evoker
//! fangs.

use glam::{DVec3, Vec3};

use super::{Entities, EntityEvent, MobSound, PlayerId, Rng};
use crate::content::fireworks::{self as fw, Cue};
use crate::inventory::Stack;
use crate::inventory::components::{Payload, StackPayload};
use crate::physics::{BlockSource, Shape};

/// `EntityTypes.FIREWORK_ROCKET`'s 0.25-block box.
pub const SHAPE: Shape = Shape::new(0.125, 0.25);
/// `FireworkRocketEntity`'s horizontal speed-up and upward push a tick.
pub const HORIZONTAL_ACCELERATION: f64 = 1.15;
pub const VERTICAL_ACCELERATION: f64 = 0.04;
/// `random.triangle(0.0, 0.002297)` of the launch drift.
pub const LAUNCH_DRIFT: f64 = 0.002297;
pub const LAUNCH_SPEED: f64 = 0.05;
/// The burst's reach and base damage (`dealExplosionDamage`).
pub const DAMAGE_RADIUS: f64 = 5.0;
pub const BASE_DAMAGE: f32 = 5.0;
pub const DAMAGE_PER_STAR: f32 = 2.0;
/// `CrossbowItem.FIREWORK_POWER` and the dispenser's `power(0.5)`.
pub const CROSSBOW_POWER: f64 = 1.6;
pub const DISPENSER_POWER: f64 = 0.5;
/// `FireworkRocketItem.ROCKET_PLACEMENT_OFFSET`.
pub const PLACEMENT_OFFSET: f64 = 0.15;

/// `FireworkParticles.Starter`'s lifetime: two ticks a star, less one, and
/// fifteen more when any star twinkles (its twinkle sounds after).
pub fn starter_lifetime(explosions: &[fw::Explosion]) -> u32 {
    let base = (explosions.len() as u32 * 2).saturating_sub(1);
    if explosions.iter().any(|e| e.twinkle) { base + 15 } else { base }
}

/// The flight time of a rocket of `flight` duration with the two rolls
/// (`nextInt(6)`, `nextInt(7)`).
pub fn lifetime(flight: u8, roll6: u32, roll7: u32) -> u32 {
    10 * (1 + u32::from(flight)) + roll6.min(5) + roll7.min(6)
}

/// The damage a burst of `stars` stars deals at `distance` (`None` out of
/// reach; the holder takes it at distance zero).
pub fn burst_damage(stars: usize, distance: f64) -> Option<f32> {
    if stars == 0 || distance > DAMAGE_RADIUS {
        return None;
    }
    let full = BASE_DAMAGE + stars as f32 * DAMAGE_PER_STAR;
    Some(full * ((DAMAGE_RADIUS - distance) / DAMAGE_RADIUS).sqrt() as f32)
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Rocket {
    pub pos: DVec3,
    /// Position at the start of the last tick, for rendering.
    pub previous_pos: DVec3,
    /// Blocks per tick.
    pub vel: DVec3,
    pub life: u32,
    pub lifetime: u32,
    /// The rocket's `fireworks` (`None`: the item default).
    pub fireworks: Option<StackPayload>,
    pub owner: Option<PlayerId>,
    /// The gliding player holding it (`attachedToEntity`).
    pub attached: Option<PlayerId>,
    pub shot_at_angle: bool,
    horizontal_collision: bool,
    /// v1.0 Track 6: the mob that launched it (a celebrating villager),
    /// which it never hits (`Projectile.canHitEntity` skips the owner).
    pub shooter: Option<u32>,
    /// v1.0 gap Track 8d: `remainingFireTicks` (it applies block effects,
    /// `FireworkRocketEntity.tick`). Saved (v1.0 gap Track 8e), as Java's `Fire`.
    pub(crate) fire: u16,
}

impl Rocket {
    /// v1.0 gap Track 8e: the level file's copy (`FireworkRocketEntity`'s
    /// `Life`, `LifeTime`, `FireworksItem` and `ShotAtAngle`, with `Fire`).
    /// A rocket a glider holds is saved loose: Java doesn't save
    /// `attachedToEntity` either.
    pub(super) fn save(&self) -> Option<serde_json::Value> {
        use super::saved_projectiles::{owner_json, v3};
        let stack = Stack { payload: self.fireworks, ..Stack::new(fw::FIREWORK_ROCKET, 1) };
        Some(serde_json::json!({
            "item": crate::inventory::stack_to_string(Some(stack)),
            "pos": v3(self.pos),
            "motion": v3(self.vel),
            "life": self.life,
            "lifetime": self.lifetime,
            "owner": owner_json(super::projectile::owner_of(self.owner, self.shooter)),
            "shot_at_angle": self.shot_at_angle,
            "fire": self.fire,
        }))
    }

    pub(super) fn load(v: &serde_json::Value) -> Option<Self> {
        use super::saved_projectiles as s;
        let stack = crate::inventory::stack_from_str(v["item"].as_str()?).flatten()?;
        let pos = s::read_v3(&v["pos"])?;
        let owner = s::read_owner(&v["owner"]);
        let small = |key: &str| v[key].as_u64().and_then(|n| u32::try_from(n).ok());
        Some(Self {
            pos,
            previous_pos: pos,
            vel: s::read_v3(&v["motion"]).unwrap_or(DVec3::ZERO),
            life: small("life").unwrap_or(0),
            lifetime: small("lifetime").unwrap_or(0),
            fireworks: stack.payload.filter(|p| matches!(p.get(), Payload::Fireworks(_))),
            owner: s::player_of(owner),
            attached: None,
            shot_at_angle: v["shot_at_angle"].as_bool().unwrap_or(false),
            horizontal_collision: false,
            shooter: s::mob_of(owner),
            fire: s::read_fire(v),
        })
    }

    /// v1.0 gap Track 8d: on fire (`isOnFire`).
    pub fn burning(&self) -> bool {
        self.fire > 0
    }

    /// `new FireworkRocketEntity(level, x, y, z, stack)`.
    pub fn new(pos: DVec3, stack: &Stack, owner: Option<PlayerId>, rng: &mut Rng) -> Self {
        let fireworks = stack.payload.filter(|p| matches!(p.get(), Payload::Fireworks(_)));
        let flight = fw::fireworks(stack).flight;
        let vel = DVec3::new(rng.triangle(0.0, LAUNCH_DRIFT), LAUNCH_SPEED, rng.triangle(0.0, LAUNCH_DRIFT));
        let lifetime = lifetime(flight, rng.next_int(6), rng.next_int(7));
        Self {
            pos,
            previous_pos: pos,
            vel,
            life: 0,
            lifetime,
            fireworks,
            owner,
            attached: None,
            shot_at_angle: false,
            horizontal_collision: false,
            shooter: None,
            fire: 0,
        }
    }

    /// A rocket held by a gliding player (`FireworkRocketItem.use`).
    pub fn attached(player: PlayerId, feet: DVec3, stack: &Stack, rng: &mut Rng) -> Self {
        Self { attached: Some(player), ..Self::new(feet, stack, Some(player), rng) }
    }

    /// A rocket shot at an angle (crossbows and dispensers):
    /// `Projectile.shoot` along `dir` at `power` blocks a tick with
    /// `uncertainty`.
    pub fn shot(
        pos: DVec3,
        stack: &Stack,
        owner: Option<PlayerId>,
        dir: DVec3,
        power: f64,
        uncertainty: f64,
        rng: &mut Rng,
    ) -> Self {
        let mut r = Self::new(pos, stack, owner, rng);
        r.shot_at_angle = true;
        let d = crate::knockback::java_normalize(dir);
        let noise = 0.0172275 * uncertainty;
        r.vel = DVec3::new(rng.triangle(d.x, noise), rng.triangle(d.y, noise), rng.triangle(d.z, noise)) * power;
        r
    }

    /// A LAN client's copy: drawn only, never simulated.
    pub(super) fn replica(pos: DVec3, previous_pos: DVec3) -> Self {
        Self {
            pos,
            previous_pos,
            vel: DVec3::ZERO,
            life: 1,
            lifetime: u32::MAX,
            fireworks: None,
            owner: None,
            attached: None,
            shot_at_angle: true,
            horizontal_collision: false,
            shooter: None,
            fire: 0,
        }
    }

    /// The stars it bursts into.
    pub fn explosions(&self) -> &'static [fw::Explosion] {
        match self.fireworks.map(StackPayload::get) {
            Some(Payload::Fireworks(f)) => &f.explosions,
            _ => &[],
        }
    }

    /// The direction the model points (`updateRotation`).
    pub fn heading(&self) -> Vec3 {
        self.vel.as_vec3().normalize_or(Vec3::Y)
    }
}

/// The first mob or player box the segment `from..to` passes through,
/// with how far along it (0..1).
fn first_hit(
    from: DVec3,
    to: DVec3,
    mobs: &[super::Mob],
    players: &[super::Target],
    skip: Option<PlayerId>,
    shooter: Option<u32>,
) -> Option<f64> {
    let dir = to - from;
    let grow = DVec3::splat(SHAPE.half_width);
    let mut best: Option<f64> = None;
    let mut consider = |min: DVec3, max: DVec3| {
        if let Some(t) = crate::physics::ray_aabb(from, dir, min - grow, max + grow)
            && t <= 1.0
            && best.is_none_or(|b| t < b)
        {
            best = Some(t);
        }
    };
    for m in mobs.iter().filter(|m| m.alive() && Some(m.uid) != shooter) {
        let (min, max) = m.aabb();
        consider(min, max);
    }
    for p in players.iter().filter(|p| p.alive && !p.spectator && Some(p.id) != skip) {
        let (min, max) = p.shape.aabb(p.pos);
        consider(min, max);
    }
    best
}

impl Entities {
    /// v0.16 Track B: a rocket launched from a block face at `pos`.
    pub fn launch_firework(&mut self, stack: &Stack, pos: DVec3, owner: Option<super::PlayerId>) {
        let r = Rocket::new(pos, stack, owner, &mut self.rng);
        self.fireworks.push(r);
    }

    /// v1.0 Track 6: a rocket a mob launches from `pos`
    /// (`new FireworkRocketEntity(level, shooter, x, y, z, stack)`).
    pub(super) fn launch_mob_firework(&mut self, stack: &Stack, pos: DVec3, shooter: u32) {
        let r = Rocket { shooter: Some(shooter), ..Rocket::new(pos, stack, None, &mut self.rng) };
        self.fireworks.push(r);
    }

    /// v0.16 Track B: a rocket a gliding player uses, attached to them.
    pub fn boost_firework(&mut self, stack: &Stack, player: PlayerId, feet: DVec3) {
        let r = Rocket::attached(player, feet, stack, &mut self.rng);
        self.fireworks.push(r);
    }

    /// v0.16 Track B: a crossbow's rocket from the shooter's eye less 0.15
    /// (`CrossbowItem.createProjectile`), along `dir`.
    pub fn shoot_firework(&mut self, stack: &Stack, eye: DVec3, dir: DVec3, owner: PlayerId) {
        let pos = eye - DVec3::Y * f64::from(0.15f32);
        let r = Rocket::shot(pos, stack, Some(owner), dir, CROSSBOW_POWER, 1.0, &mut self.rng);
        self.fireworks.push(r);
        self.sculk.shot(pos, crate::world::vibration::EventSource::player(owner, false));
    }

    /// v0.16 Track B: `FireworkRocketItem.asProjectile` from a dispenser at
    /// `block` facing `step`: just outside the face, shot at 0.5 with
    /// uncertainty 1, with the dispenser's level event 1004 sound.
    pub fn dispense_firework(&mut self, stack: Stack, block: glam::IVec3, step: glam::IVec3) {
        let pos = block.as_dvec3() + DVec3::splat(0.5) + step.as_dvec3() * 0.500_009_999_999_747_4;
        let one = Stack { count: 1, ..stack };
        let r = Rocket::shot(pos, &one, None, step.as_dvec3(), DISPENSER_POWER, 1.0, &mut self.rng);
        self.fireworks.push(r);
        self.sculk.shot(pos, crate::world::vibration::EventSource::NONE);
        self.pending_sounds.push(EntityEvent::Sound { sound: MobSound::Flight(Cue::Shoot), pos });
    }

    /// One tick of every rocket: flight, hits, the launch sound and trail,
    /// and bursts with their damage.
    pub(super) fn update_fireworks<W: super::MobWorld + ?Sized>(
        &mut self,
        world: &W,
        ctx: &super::Ctx,
        boss: &super::dragon::Boxes,
        events: &mut Vec<EntityEvent>,
    ) {
        // Twinkles after their bursts' sparks.
        if !self.firework_twinkles.is_empty() {
            for (pos, delay) in &mut self.firework_twinkles {
                *delay = delay.saturating_sub(1);
                if *delay == 0 {
                    events.push(EntityEvent::Sound { sound: MobSound::Flight(Cue::Twinkle), pos: *pos });
                }
            }
            self.firework_twinkles.retain(|(_, delay)| *delay > 0);
        }
        if self.fireworks.is_empty() {
            return;
        }
        let mut rockets = std::mem::take(&mut self.fireworks);
        rockets.retain_mut(|r| {
            r.previous_pos = r.pos;
            let mut burst = false;
            if let Some(holder) = r.attached {
                // `isAttachedToEntity`: follow the holder and push it along
                // while it glides.
                if let Some(t) = ctx.players.iter().find(|t| t.id == holder && t.alive) {
                    if t.fall_flying {
                        events.push(EntityEvent::RocketBoost { player: holder });
                    }
                    r.vel = t.pos - r.pos;
                    r.pos = t.pos;
                }
            } else {
                if !r.shot_at_angle {
                    let h = if r.horizontal_collision { 1.0 } else { HORIZONTAL_ACCELERATION };
                    r.vel = DVec3::new(r.vel.x * h, r.vel.y + VERTICAL_ACCELERATION, r.vel.z * h);
                }
                let movement = r.vel;
                let skip = (r.life < 2).then_some(r.owner).flatten();
                // v1.0 gap Track 4c: an End crystal or a dragon part stops it
                // too (`onHitEntity` explodes; its blast reaches only the
                // living, so a crystal stands).
                let entity_hit =
                    first_hit(r.pos, r.pos + movement, &self.mobs, &ctx.players, skip.or(r.owner), r.shooter)
                        .map(|_| ())
                        .or_else(|| boss.raycast(r.pos, movement, 1.0, true).map(|_| ()));
                let (mut pos, mut vel) = (r.pos, movement);
                let hit = crate::physics::move_box(world, &mut pos, &mut vel, movement, SHAPE);
                let block_hit = hit.on_ground || hit.ceiling || hit.horizontal;
                r.horizontal_collision = hit.horizontal;
                // v1.0 gap Track 8d: `applyEffectsFromBlocks` along the
                // move, then `baseTick`'s burn.
                super::projectile::tick_fire(&mut r.fire, world, r.pos, pos, SHAPE.half_width, SHAPE.height);
                // A burning rocket's `onHitBlock` lights a candle or
                // campfire or primes TNT (`onProjectileHit`).
                if block_hit && r.burning() {
                    let toward = if hit.on_ground {
                        -DVec3::Y * 0.05
                    } else if hit.ceiling {
                        DVec3::Y * (SHAPE.height + 0.05)
                    } else {
                        movement.with_y(0.0).normalize_or_zero() * (SHAPE.half_width + 0.05)
                    };
                    let cell = (pos + toward).floor().as_ivec3();
                    events.push(super::projectile::block_hit(
                        cell,
                        pos,
                        true,
                        r.shooter.is_some(),
                        super::projectile::owner_of(r.owner, r.shooter),
                    ));
                }
                r.pos = pos;
                // `setDeltaMovement(movement)`: the move doesn't slow it.
                r.vel = movement;
                if entity_hit.is_some() || (block_hit && !r.explosions().is_empty()) {
                    burst = true;
                }
            }
            if r.life == 0 {
                events.push(EntityEvent::Sound { sound: MobSound::Flight(Cue::Launch), pos: r.pos });
            }
            r.life += 1;
            if r.attached.is_none() {
                // The `FIREWORK` trail: down against the climb, scattered sideways.
                let mut b = crate::particles::Burst::new(crate::particles::Kind::FireworkSpark, r.pos, 1);
                b.velocity = DVec3::new(0.0, -r.vel.y * 0.5, 0.0);
                b.velocity_spread = DVec3::new(0.05, 0.0, 0.05);
                self.particles.push(crate::particles::Request::Burst(b));
            }
            if burst || r.life > r.lifetime {
                self.burst_firework(r, world, ctx, boss, events);
                return false;
            }
            true
        });
        // Rockets added during the tick (none today) go after the old ones.
        rockets.append(&mut self.fireworks);
        self.fireworks = rockets;
    }

    /// `FireworkRocketEntity.explode`: the particles and sounds (entity
    /// event 17) and `dealExplosionDamage`.
    fn burst_firework<W: BlockSource + ?Sized>(
        &mut self,
        r: &Rocket,
        world: &W,
        ctx: &super::Ctx,
        boss: &super::dragon::Boxes,
        events: &mut Vec<EntityEvent>,
    ) {
        events.push(EntityEvent::FireworkBurst { pos: r.pos, vel: r.vel, fireworks: r.fireworks });
        // `FireworkRocketEntity.explode`: `explode` from the rocket's owner.
        let source = r.owner.map_or(crate::world::vibration::EventSource::NONE, |p| {
            crate::world::vibration::EventSource::player(p, false)
        });
        self.sculk.post(r.pos, crate::world::vibration::GameEvent::Explode, source, None);
        let explosions = r.explosions();
        let stars = explosions.len();
        if stars == 0 {
            // `ClientLevel.createFireworks` without stars: a few puffs of smoke.
            let mut b =
                crate::particles::Burst::new(crate::particles::Kind::Poof, r.pos, 1 + self.rng.next_int(3) as u16);
            b.velocity_spread = DVec3::splat(0.05);
            self.particles.push(crate::particles::Request::Burst(b));
            return;
        }
        // `FireworkParticles.Starter`: its blast (large for three stars or a
        // large ball), its sparks, and a twinkle once the last star is done.
        let large = stars >= 3 || explosions.iter().any(|e| e.shape == fw::Shape::LargeBall);
        let blast = if large { Cue::LargeBlast } else { Cue::Blast };
        events.push(EntityEvent::Sound { sound: MobSound::Flight(blast), pos: r.pos });
        self.particles.push(crate::particles::Request::Firework { pos: r.pos, vel: r.vel, fireworks: r.fireworks });
        if explosions.iter().any(|e| e.twinkle) && self.firework_twinkles.len() < 64 {
            self.firework_twinkles.push((r.pos, starter_lifetime(explosions) + 1));
        }
        if let Some(holder) = r.attached {
            let full = BASE_DAMAGE + stars as f32 * DAMAGE_PER_STAR;
            events.push(EntityEvent::PlayerFirework { player: holder, damage: full, from: r.pos });
        }
        // Line of sight to the feet or the middle of the body (`getY(0.5 * step)`).
        let sees = |pos: DVec3, height: f64| {
            (0..2).any(|step| super::clear_line(world, r.pos, pos + DVec3::Y * (height * 0.5 * f64::from(step))))
        };
        for p in ctx.players.iter().filter(|p| p.alive && !p.spectator && Some(p.id) != r.attached) {
            let d = p.pos.distance(r.pos);
            if let Some(damage) = burst_damage(stars, d)
                && sees(p.pos, p.shape.height)
            {
                events.push(EntityEvent::PlayerFirework { player: p.id, damage, from: r.pos });
            }
        }
        let owner = r.owner;
        // v1.0 gap Track 4c: each dragon is a `LivingEntity` measured from
        // its feet; `fireworks` is `#is_explosion`, so it takes the blast
        // (through its body) whoever launched the rocket.
        for &(hit, feet) in &boss.dragons {
            if let Some(damage) = burst_damage(stars, feet.distance(r.pos))
                && sees(feet, super::dragon::WILD_SHAPE.height)
            {
                events.push(EntityEvent::DragonStruck { hit, blow: super::dragon::Blow::explosion(damage), by: owner });
            }
        }
        for i in 0..self.mobs.len() {
            let m = &self.mobs[i];
            if !m.alive() {
                continue;
            }
            let (min, max) = m.aabb();
            let d = m.pos.distance(r.pos);
            let Some(damage) = burst_damage(stars, d) else { continue };
            if !sees(m.pos, max.y - min.y) {
                continue;
            }
            let m = &mut self.mobs[i];
            if let Some(owner) = owner {
                m.player_hit_by(owner);
            }
            // `calculateHorizontalHurtKnockbackDirection`: from the rocket
            // to the victim, which `knockback` turns into a pull toward it.
            let push = crate::knockback::Push::hurt(m.pos, r.pos);
            let before = m.health;
            let killed = m.damage_pushed(damage, &[push], &mut self.rng);
            if killed || m.health < before {
                events.push(EntityEvent::MobShot {
                    kind: m.kind,
                    pos: m.pos,
                    killed,
                    burning: m.burning,
                    player_kill: owner.is_some(),
                    owner,
                });
            }
        }
    }

    /// Rockets near `at` for a LAN client: position and previous position.
    pub fn fireworks_near(&self, at: DVec3, radius: f64) -> Vec<Vec<f64>> {
        self.fireworks
            .iter()
            .filter(|r| r.attached.is_none() && r.pos.distance_squared(at) < radius * radius)
            .take(256)
            .map(|r| {
                // v1.0 gap Track 8d: a seventh number, 1, when it burns.
                let mut v = vec![r.pos.x, r.pos.y, r.pos.z, r.previous_pos.x, r.previous_pos.y, r.previous_pos.z];
                if r.burning() {
                    v.push(1.0);
                }
                v
            })
            .collect()
    }

    /// Replaces a LAN client's rockets with the host's.
    pub fn replace_fireworks(&mut self, list: &serde_json::Value) {
        self.fireworks.clear();
        let Some(list) = list.as_array() else { return };
        for v in list.iter().take(256) {
            let Some(a) = v.as_array() else { continue };
            let n: Vec<f64> = a.iter().filter_map(serde_json::Value::as_f64).collect();
            if (6..=7).contains(&n.len()) && n.iter().all(|x| x.is_finite()) {
                let mut r = Rocket::replica(DVec3::new(n[0], n[1], n[2]), DVec3::new(n[3], n[4], n[5]));
                r.fire = u16::from(n.get(6) == Some(&1.0));
                self.fireworks.push(r);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::physics::test_util::Grid;

    #[test]
    fn lifetimes_and_damage_follow_java() {
        assert_eq!(lifetime(1, 0, 0), 20);
        assert_eq!(lifetime(3, 5, 6), 51);
        assert_eq!(burst_damage(0, 0.0), None);
        assert_eq!(burst_damage(1, 0.0), Some(7.0));
        assert_eq!(burst_damage(3, 0.0), Some(11.0));
        assert!((burst_damage(1, 3.75).unwrap() - 3.5).abs() < 1e-5);
        assert_eq!(burst_damage(2, 5.5), None);
    }

    /// v1.0 gap Track 4c: a rocket with a star that bursts within five
    /// blocks of a dragon's feet, in sight of them, hurts it through its
    /// body (`fireworks` is `#is_explosion`); one bursting by its head, far
    /// from its feet, doesn't.
    #[test]
    fn a_firework_blast_reaches_the_dragon_from_its_feet() {
        let ctx = super::super::Ctx {
            spawn_light: crate::simulation::lighting::SpawnLight::DAY,
            players: Vec::new(),
            daylight: 1.0,
            spawning: false,
            raining: false,
            dimension: crate::world::terrain::Dimension::Overworld,
        };
        let stack = fw::rocket(fw::Fireworks { flight: 1, explosions: vec![fw::Explosion::default_small_ball()] }, 1);
        let world = Grid::flat(10);
        for (offset, hurt) in [(DVec3::new(-2.0, 0.5, 0.0), true), (DVec3::new(0.0, 0.5, -9.0), false)] {
            let mut e = Entities::new(1);
            e.wild_dragons.spawn(DVec3::new(0.5, 40.0, 0.5), 0.0);
            let feet = e.wild_dragons.dragons[0].dragon.pos;
            let mut r = Rocket::new(feet + offset, &stack, Some(crate::entity::PlayerId::HOST), &mut Rng::new(2));
            r.life = r.lifetime;
            e.fireworks.push(r);
            let mut events = Vec::new();
            let boss = super::super::dragon::Boxes::new(None, &e.wild_dragons, &e.crystals);
            e.update_fireworks(&world, &ctx, &boss, &mut events);
            assert!(e.fireworks.is_empty());
            let struck = events.iter().any(|ev| matches!(ev, EntityEvent::DragonStruck { blow, .. } if blow.explosion));
            assert_eq!(struck, hurt, "{offset}");
            e.apply_dragon_hits(&mut events);
            let lost = super::super::dragon::MAX_HEALTH - e.wild_dragons.dragons[0].dragon.health;
            assert_eq!(lost > 0.0, hurt, "{offset}: {lost}");
        }
    }

    #[test]
    fn free_rockets_climb_and_speed_up() {
        let mut rng = Rng::new(3);
        let mut r = Rocket::new(DVec3::new(0.5, 11.0, 0.5), &Stack::new(fw::FIREWORK_ROCKET, 1), None, &mut rng);
        assert!(r.vel.y == LAUNCH_SPEED && r.vel.x.abs() <= LAUNCH_DRIFT);
        assert!((20..=31).contains(&r.lifetime));
        let world = Grid::flat(10);
        let mut e = Entities::new(1);
        e.fireworks.push(r);
        let ctx = super::super::Ctx {
            spawn_light: crate::simulation::lighting::SpawnLight::DAY,
            players: Vec::new(),
            daylight: 1.0,
            spawning: false,
            raining: false,
            dimension: crate::world::terrain::Dimension::Overworld,
        };
        let mut events = Vec::new();
        for _ in 0..5 {
            e.update_fireworks(&world, &ctx, &Default::default(), &mut events);
        }
        r = e.fireworks[0];
        // Five ticks of +0.04: 0.05 + 0.2.
        assert!((r.vel.y - 0.25).abs() < 1e-9, "{}", r.vel.y);
        assert!(events.iter().any(|ev| matches!(ev, EntityEvent::Sound { sound: MobSound::Flight(Cue::Launch), .. })));
        for _ in 0..40 {
            e.update_fireworks(&world, &ctx, &Default::default(), &mut events);
        }
        assert!(e.fireworks.is_empty(), "burst once its lifetime ran out");
        assert!(events.iter().any(|ev| matches!(ev, EntityEvent::FireworkBurst { .. })));
    }
}
