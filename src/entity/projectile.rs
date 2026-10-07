//! Arrows: ballistic projectiles that stick in blocks. Skeleton arrows hurt
//! the player; the player's own arrows hurt mobs and can be picked back up.

use glam::{DVec3, Vec3};

use super::{Ctx, EntityEvent, Mob, Rng};
use crate::physics::BlockSource;

const SPEED: f64 = 22.0;
/// Speed of an arrow from a fully drawn bow (Minecraft's 3 blocks a tick).
pub const BOW_SPEED: f64 = 55.0;
/// Damage of a fully drawn bow's arrow before the critical bonus.
const BOW_DAMAGE: f64 = 6.0;
const GRAVITY: f64 = 20.0;
/// Seconds a stuck arrow stays before vanishing, and a flying arrow lives.
const STUCK_TIME: f32 = 8.0;
const PLAYER_STUCK_TIME: f32 = 60.0;
const MAX_FLIGHT: f32 = 10.0;
/// Longest move per collision check (blocks), so arrows can't skip a block.
const STEP: f64 = 0.2;

pub struct Arrow {
    pub pos: DVec3,
    /// Position at the start of the last simulation step, for rendering.
    pub previous_pos: DVec3,
    vel: DVec3,
    /// Direction it points (kept once stuck).
    pub dir: Vec3,
    age: f32,
    stuck: bool,
    /// Shot by the player: hits mobs instead of the player.
    pub from_player: bool,
    /// The player can collect it once stuck (not arrows shot in creative).
    pub pickup: bool,
    /// A fully drawn shot: deals a random bonus on hit.
    pub critical: bool,
    /// The bow's enchantments: power, punch and flame.
    pub enchants: crate::enchant::Enchants,
}

impl Arrow {
    /// An arrow from `from` aimed to arc onto `target`, with a little spread.
    pub fn aimed(from: DVec3, target: DVec3, rng: &mut Rng) -> Self {
        let d = target - from;
        // Lead the drop: over flight time t the arrow sinks g t² / 2.
        let t = d.length() / SPEED;
        let aim = d + DVec3::Y * (0.5 * GRAVITY * t * t);
        let spread = DVec3::new(rng.range(-1.0, 1.0) as f64, rng.range(-1.0, 1.0) as f64, rng.range(-1.0, 1.0) as f64);
        let vel = (aim.normalize_or(DVec3::X) + spread * 0.03) * SPEED;
        Self {
            pos: from,
            previous_pos: from,
            vel,
            dir: vel.normalize().as_vec3(),
            age: 0.0,
            stuck: false,
            from_player: false,
            pickup: false,
            critical: false,
            enchants: Default::default(),
        }
    }

    /// An arrow loosed by the player from `eye` along `dir` with bow
    /// `power` 0..1.
    pub fn shot(eye: DVec3, dir: DVec3, power: f32, pickup: bool) -> Self {
        let dir = dir.normalize_or(DVec3::X);
        let vel = dir * BOW_SPEED * power as f64;
        Self {
            pos: eye + dir * 0.3,
            previous_pos: eye + dir * 0.3,
            vel,
            dir: dir.as_vec3(),
            age: 0.0,
            stuck: false,
            from_player: true,
            pickup,
            critical: power >= 1.0,
            enchants: Default::default(),
        }
    }

    /// Moves the arrow; returns `false` once it should be removed.
    #[allow(clippy::too_many_arguments)]
    pub(super) fn update<W: BlockSource + ?Sized>(
        &mut self,
        dt: f64,
        world: &W,
        ctx: &Ctx,
        mobs: &mut [Mob],
        mut fight: Option<&mut super::dragon::Fight>,
        rng: &mut Rng,
        events: &mut Vec<EntityEvent>,
    ) -> bool {
        self.age += dt as f32;
        if self.stuck {
            // Arrows fall out when the block they're in is broken.
            if !world.block(self.pos.floor().as_ivec3()).is_some_and(|b| b.is_solid()) {
                self.stuck = false;
                self.vel = DVec3::ZERO;
                return true;
            }
            return self.age < if self.from_player { PLAYER_STUCK_TIME } else { STUCK_TIME };
        }
        self.vel.y -= GRAVITY * dt;
        let delta = self.vel * dt;
        let steps = (delta.length() / STEP).ceil().max(1.0) as u32;
        for _ in 0..steps {
            let from = self.pos;
            let step = delta / steps as f64;
            self.pos += step;
            // A block the step crosses (even at a corner) stops the arrow,
            // and hides anything behind it.
            let block = first_solid(world, from, step);
            let reach = block.unwrap_or(1.0);
            if self.from_player {
                // Whatever the step enters first: a mob, or a crystal or
                // dragon part (fractions of the step).
                let mob = mobs
                    .iter()
                    .enumerate()
                    .filter(|(_, m)| m.alive())
                    .filter_map(|(i, m)| {
                        let (min, max) = m.aabb();
                        crate::physics::ray_aabb(from, step, min, max).filter(|&t| t <= reach).map(|t| (i, t))
                    })
                    .min_by(|a, b| a.1.total_cmp(&b.1));
                let boss = fight.as_deref().and_then(|f| f.raycast(from, step, reach));
                if let Some((hit, _)) = boss.filter(|&(_, t)| mob.is_none_or(|(_, m)| t < m))
                    && let Some(fight) = fight.as_deref_mut()
                {
                    let damage = (self.vel.length() / BOW_SPEED * BOW_DAMAGE).ceil() as f32;
                    if fight.strike(hit, damage, None, true) {
                        return false;
                    }
                    // Bounced off a perched dragon's scales.
                    self.vel = -self.vel * 0.1;
                    return true;
                }
                if let Some((i, _)) = mob {
                    let mob = &mut mobs[i];
                    // Endermen dodge arrows by teleporting; the arrow flies on.
                    if mob.kind == super::MobKind::Enderman {
                        mob.teleport_pending = true;
                    } else {
                        use crate::enchant::Enchantment;
                        let speed = self.vel.length();
                        // Power raises Java's base damage of 2 by 0.5 per level + 0.5.
                        let power = self.enchants.level(Enchantment::Power) as f64;
                        let base = if power > 0.0 { (2.5 + 0.5 * power) / 2.0 } else { 1.0 };
                        let mut damage = (speed / BOW_SPEED * BOW_DAMAGE * base).ceil() as f32;
                        if self.critical {
                            damage += (rng.next_f32() * (damage / 2.0 + 1.0)).floor();
                        }
                        // Punch adds 0.6 blocks/tick of shove per level.
                        let punch = 1.0 + 3.0 * self.enchants.level(Enchantment::Punch) as f64;
                        let push =
                            DVec3::new(self.vel.x, 0.0, self.vel.z).normalize_or_zero() * 4.0 * punch + DVec3::Y * 4.0;
                        if self.enchants.has(Enchantment::Flame) {
                            mob.ignite(5.0);
                        }
                        let killed = mob.damage(damage, Some(push), rng);
                        events.push(EntityEvent::MobShot { kind: mob.kind, pos: mob.pos, killed });
                        return false;
                    }
                }
            }
            if let Some(t) = block {
                // Stuck just inside the block it hit.
                self.pos = from + step * t + step.normalize_or_zero() * 0.01;
                self.stuck = true;
                self.age = 0.0;
                return true;
            }
            if self.from_player {
                continue;
            }
            if let Some(hit) = ctx.players.iter().find(|t| t.targetable && t.contains(self.pos)) {
                let push = DVec3::new(self.vel.x, 0.0, self.vel.z).normalize_or_zero() * 3.0 + DVec3::Y * 3.0;
                let damage = (self.vel.length() / SPEED * 3.0).ceil() as f32;
                events.push(EntityEvent::PlayerHit {
                    player: hit.id,
                    damage,
                    knockback: push.as_vec3(),
                    cause: "was shot by a skeleton",
                });
                return false;
            }
        }
        if self.vel.length_squared() > 1e-6 {
            self.dir = self.vel.normalize().as_vec3();
        }
        self.age < MAX_FLIGHT && self.pos.y > -64.0
    }

    pub fn is_stuck(&self) -> bool {
        self.stuck
    }
}

/// How far along `step` (0..=1) from `from` it first enters a solid cell.
fn first_solid<W: BlockSource + ?Sized>(world: &W, from: DVec3, step: DVec3) -> Option<f64> {
    let to = from + step;
    let (lo, hi) = (from.min(to).floor().as_ivec3(), from.max(to).floor().as_ivec3());
    let mut first: Option<f64> = None;
    for y in lo.y..=hi.y {
        for z in lo.z..=hi.z {
            for x in lo.x..=hi.x {
                let cell = glam::IVec3::new(x, y, z);
                if !world.block(cell).is_some_and(|b| b.is_solid()) {
                    continue;
                }
                let min = cell.as_dvec3();
                if let Some(t) = crate::physics::ray_aabb(from, step, min, min + DVec3::ONE).filter(|&t| t <= 1.0) {
                    first = Some(first.map_or(t, |f: f64| f.min(t)));
                }
            }
        }
    }
    first
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::entity::{MobKind, PlayerId, Target};
    use crate::physics::test_util::Grid;
    use crate::world::block::Block;
    use crate::world::terrain::Dimension;
    use glam::IVec3;

    #[test]
    fn a_clipped_block_corner_stops_the_arrow_before_a_mob_behind_it() {
        let mut world = Grid::flat(10);
        world.set(IVec3::new(0, 20, 0), Block::STONE);
        // One step from the air beside the block, across its corner, into
        // the air past it, with a cow standing just beyond.
        let (from, to) = (DVec3::new(-0.04, 20.5, 0.92), DVec3::new(0.06, 20.5, 1.02));
        assert!(world.block((to).floor().as_ivec3()).is_some_and(|b| !b.is_solid()));
        let t = first_solid(&world, from, to - from).expect("the corner is crossed");
        assert!((0.3..0.5).contains(&t), "{t}");

        let mut mobs = [Mob::new(MobKind::Cow, DVec3::new(0.06, 20.0, 1.46), 0.0)];
        let dt = 1.0 / 60.0;
        let mut arrow = Arrow::shot(from, (to - from).normalize(), 1.0, true);
        arrow.pos = from;
        arrow.vel = (to - from) / dt + DVec3::Y * GRAVITY * dt;
        let ctx = Ctx {
            players: vec![Target::new(PlayerId::HOST, DVec3::new(50.0, 10.0, 0.0), true)],
            daylight: 1.0,
            spawning: false,
            raining: false,
            dimension: Dimension::Overworld,
        };
        let mut events = Vec::new();
        assert!(arrow.update(dt, &world, &ctx, &mut mobs, None, &mut Rng::new(1), &mut events));
        assert!(arrow.is_stuck());
        assert_eq!(arrow.pos.floor().as_ivec3(), IVec3::new(0, 20, 0));
        assert!(events.is_empty(), "{events:?}");
        assert_eq!(mobs[0].health, MobKind::Cow.max_health());
    }
}
