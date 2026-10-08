//! Thrown splash potions, like Java's: they fly under gravity, shatter on a
//! block or whatever they hit, and splash everything within four blocks
//! (two vertically) with an effect scaled by distance. Players throw them
//! (10 blocks a second, aimed 20 degrees high) and witches lob them at
//! their targets.

use glam::DVec3;

use super::{Ctx, EntityEvent, Mob, MobKind, PlayerId, Rng};
use crate::physics::BlockSource;
use crate::potion::Potion;
use crate::simulation::effects::Effect;

/// A player's throw speed (Java's 0.5 blocks a tick).
pub const THROW_SPEED: f64 = 10.0;
/// A witch's throw speed (Java's 0.75).
pub const WITCH_SPEED: f64 = 15.0;
/// Blocks per second squared (Java's 0.05 per tick²).
const GRAVITY: f64 = 20.0;
/// How far a splash reaches horizontally and vertically.
const REACH: f64 = 4.0;
const REACH_Y: f64 = 2.0;
/// Splash potions give 75% of a drinkable potion's duration.
const DURATION_SCALE: f32 = 0.75;
/// Longest move per collision check (blocks).
const STEP: f64 = 0.2;
const MAX_FLIGHT: f32 = 30.0;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ThrownPotion {
    pub potion: Potion,
    /// The throwing player; witches throw with `None`.
    pub owner: Option<PlayerId>,
    pub pos: DVec3,
    /// Position at the start of the last simulation step, for rendering.
    pub previous_pos: DVec3,
    pub vel: DVec3,
    age: f32,
}

/// Aims `dir` twenty degrees higher, Java's throw offset for potions.
pub fn aim_up(dir: DVec3) -> DVec3 {
    let dir = dir.normalize_or(DVec3::X);
    let pitch = dir.y.clamp(-1.0, 1.0).asin() + 20f64.to_radians();
    let flat = DVec3::new(dir.x, 0.0, dir.z).normalize_or(DVec3::X);
    flat * pitch.cos() + DVec3::Y * pitch.sin()
}

impl ThrownPotion {
    pub fn new(potion: Potion, owner: Option<PlayerId>, pos: DVec3, vel: DVec3) -> Self {
        Self { potion, owner, pos, previous_pos: pos, vel, age: 0.0 }
    }

    /// A player's throw from `eye` along their look direction `dir`, adding
    /// their own velocity `carry`.
    pub fn thrown(potion: Potion, owner: PlayerId, eye: DVec3, dir: DVec3, carry: DVec3) -> Self {
        Self::new(potion, Some(owner), eye - DVec3::Y * 0.1, aim_up(dir) * THROW_SPEED + carry)
    }

    /// Moves the potion; returns `false` once it has shattered.
    pub(super) fn update<W: BlockSource + ?Sized>(
        &mut self,
        dt: f64,
        world: &W,
        ctx: &Ctx,
        mobs: &mut [Mob],
        rng: &mut Rng,
        events: &mut Vec<EntityEvent>,
    ) -> bool {
        self.age += dt as f32;
        let water = world.block(self.pos.floor().as_ivec3()).is_some_and(|b| b.is_water());
        self.vel *= if water { 0.8f64 } else { 0.99 }.powf(dt * 20.0);
        self.vel.y -= GRAVITY * dt;
        let delta = self.vel * dt;
        let steps = (delta.length() / STEP).ceil().max(1.0) as u32;
        for _ in 0..steps {
            let next = self.pos + delta / steps as f64;
            if world.block(next.floor().as_ivec3()).is_some_and(|b| b.is_solid()) {
                self.shatter(None, ctx, mobs, rng, events);
                return false;
            }
            self.pos = next;
            let hit_player = ctx.players.iter().any(|t| t.alive && t.contains(next) && self.owner != Some(t.id));
            let hit_mob = mobs.iter().position(|m| {
                let (min, max) = m.aabb();
                m.alive() && next.cmpge(min).all() && next.cmple(max).all()
            });
            if hit_player || hit_mob.is_some() {
                self.shatter(hit_mob, ctx, mobs, rng, events);
                return false;
            }
        }
        if self.age >= MAX_FLIGHT || self.pos.y < -128.0 {
            return false;
        }
        true
    }

    fn shatter(
        &self,
        direct: Option<usize>,
        ctx: &Ctx,
        mobs: &mut [Mob],
        rng: &mut Rng,
        events: &mut Vec<EntityEvent>,
    ) {
        events.push(EntityEvent::PotionSplashed { pos: self.pos, colour: self.potion.colour() });
        splash(self.potion, self.owner, self.pos, direct, ctx, mobs, rng, events);
    }
}

/// Distance from `p` to the box `min..max` (0 inside).
fn box_distance(p: DVec3, min: DVec3, max: DVec3) -> f64 {
    (min - p).max(p - max).max(DVec3::ZERO).length()
}

/// Java's instant effect amount: `base << amplifier`, scaled and rounded.
fn instant_amount(base: f32, amplifier: u8, intensity: f32) -> f32 {
    (intensity * (base * (1u32 << amplifier.min(20)) as f32) + 0.5).floor()
}

/// Splashes `potion` at `pos`. `direct` is a mob the potion hit square on
/// (full strength). Players get [`EntityEvent::PlayerEffect`] or
/// [`EntityEvent::PlayerMagic`]; mobs take instant damage and healing (undead
/// the other way round, witches shrug off most of the harm). Other effects
/// don't exist for mobs yet.
#[allow(clippy::too_many_arguments)]
pub(super) fn splash(
    potion: Potion,
    owner: Option<PlayerId>,
    pos: DVec3,
    direct: Option<usize>,
    ctx: &Ctx,
    mobs: &mut [Mob],
    rng: &mut Rng,
    events: &mut Vec<EntityEvent>,
) {
    let Some((effect, amplifier, ticks)) = potion.info().effect else { return };
    let reach = |min: DVec3, max: DVec3| {
        let near = (min - pos).max(pos - max).max(DVec3::ZERO);
        (near.x <= REACH && near.z <= REACH && near.y <= REACH_Y).then(|| box_distance(pos, min, max))
    };
    let intensity = |dist: Option<f64>| dist.filter(|d| *d < REACH).map(|d| 1.0 - d as f32 / REACH as f32);
    for t in ctx.players.iter().filter(|t| t.alive) {
        let (min, max) = t.shape.aabb(t.pos);
        let Some(scale) = reach(min, max).and_then(|d| intensity(Some(d))) else { continue };
        match effect {
            Effect::InstantDamage => {
                let amount = instant_amount(6.0, amplifier, scale);
                events.push(EntityEvent::PlayerMagic { player: t.id, amount });
            }
            Effect::InstantHealth => {
                let amount = instant_amount(4.0, amplifier, scale);
                events.push(EntityEvent::PlayerMagic { player: t.id, amount: -amount });
            }
            _ => {
                let ticks = (scale * ticks as f32 * DURATION_SCALE + 0.5) as u32;
                if ticks > 20 {
                    events.push(EntityEvent::PlayerEffect { player: t.id, effect, amplifier, ticks });
                }
            }
        }
    }
    if !effect.is_instant() {
        return;
    }
    for (i, mob) in mobs.iter_mut().enumerate() {
        if !mob.alive() {
            continue;
        }
        let (min, max) = mob.aabb();
        let dist = if direct == Some(i) { Some(0.0) } else { reach(min, max) };
        let Some(scale) = intensity(dist) else { continue };
        let undead = mob.kind.creature() == crate::enchant::Creature::Undead;
        // Healing hurts the undead and harming heals them.
        let harms = (effect == Effect::InstantDamage) != undead;
        let base = if effect == Effect::InstantDamage { 6.0 } else { 4.0 };
        let amount = instant_amount(base, amplifier, scale);
        if harms {
            let amount = if mob.kind == MobKind::Witch { amount * 0.15 } else { amount };
            if owner.is_some() {
                mob.player_hit();
            }
            let (kind, burning, at) = (mob.kind, mob.burning, mob.pos);
            let killed = mob.damage(amount, None, rng);
            if owner.is_some() {
                events.push(EntityEvent::MobShot { kind, pos: at, killed, burning });
            }
        } else {
            mob.health = (mob.health + amount).min(mob.kind.max_health());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::entity::Target;
    use crate::physics::test_util::Grid;

    fn ctx(players: Vec<Target>) -> Ctx {
        Ctx {
            players,
            daylight: 1.0,
            spawning: false,
            raining: false,
            dimension: crate::world::terrain::Dimension::Overworld,
        }
    }

    #[test]
    fn thrown_potions_arc_and_shatter_on_the_ground() {
        let world = Grid::flat(10);
        let mut p = ThrownPotion::thrown(
            Potion::from_id("harming").unwrap(),
            PlayerId::HOST,
            DVec3::new(0.5, 11.62, 0.5),
            DVec3::X,
            DVec3::ZERO,
        );
        let mut events = Vec::new();
        let c = ctx(vec![]);
        let mut n = 0;
        while p.update(0.05, &world, &c, &mut [], &mut Rng::new(1), &mut events) {
            n += 1;
            assert!(n < 200);
        }
        assert!(matches!(events[0], EntityEvent::PotionSplashed { pos, .. } if (4.0..14.0).contains(&pos.x)));
    }

    #[test]
    fn splash_scales_with_distance_for_every_player() {
        let harming = Potion::from_id("harming").unwrap();
        let c = ctx(vec![
            Target::new(PlayerId::HOST, DVec3::new(0.0, 10.0, 0.0), true),
            Target::new(PlayerId(2), DVec3::new(2.0, 10.0, 0.0), true),
            Target::new(PlayerId(3), DVec3::new(9.0, 10.0, 0.0), true),
        ]);
        let mut events = Vec::new();
        splash(harming, None, DVec3::new(0.0, 10.0, 0.0), None, &c, &mut [], &mut Rng::new(1), &mut events);
        let amount = |id| {
            events.iter().find_map(|e| match *e {
                EntityEvent::PlayerMagic { player, amount } if player == PlayerId(id) => Some(amount),
                _ => None,
            })
        };
        assert_eq!(amount(0), Some(6.0), "standing in it hits for full strength");
        assert!(amount(2).is_some_and(|a| a < 6.0 && a > 0.0), "{:?}", amount(2));
        assert_eq!(amount(3), None, "out of reach");
    }

    #[test]
    fn splash_effects_last_three_quarters_and_mobs_take_instant_damage() {
        let poison = Potion::from_id("poison").unwrap();
        let c = ctx(vec![Target::new(PlayerId(4), DVec3::new(0.0, 10.0, 0.0), true)]);
        let mut events = Vec::new();
        splash(poison, None, DVec3::new(0.0, 10.0, 0.0), None, &c, &mut [], &mut Rng::new(1), &mut events);
        let EntityEvent::PlayerEffect { effect: Effect::Poison, ticks, .. } = events[0] else { panic!("{events:?}") };
        assert_eq!(ticks, (900.0 * 0.75 + 0.5) as u32);

        let mut mobs = [
            Mob::new(MobKind::Zombie, DVec3::new(1.0, 10.0, 0.0), 0.0),
            Mob::new(MobKind::Witch, DVec3::new(1.0, 10.0, 1.0), 0.0),
        ];
        let harming = Potion::from_id("harming").unwrap();
        splash(
            harming,
            Some(PlayerId::HOST),
            DVec3::new(0.0, 10.0, 0.0),
            Some(0),
            &c,
            &mut mobs,
            &mut Rng::new(1),
            &mut Vec::new(),
        );
        // The undead are healed by harming; their health was already full.
        assert_eq!(mobs[0].health, MobKind::Zombie.max_health());
        assert!(mobs[1].health > MobKind::Witch.max_health() - 2.0, "witches resist magic: {}", mobs[1].health);
        let healing = Potion::from_id("healing").unwrap();
        splash(
            healing,
            Some(PlayerId::HOST),
            DVec3::new(0.0, 10.0, 0.0),
            Some(0),
            &c,
            &mut mobs,
            &mut Rng::new(1),
            &mut Vec::new(),
        );
        assert!(mobs[0].health < MobKind::Zombie.max_health(), "healing hurts the undead");
    }
}
