//! Thrown snowballs and eggs. They fly like Java's (1.5 blocks a tick, 0.03
//! gravity, 0.99 drag), knock a mob back, and vanish on impact. A snowball
//! deals 3 damage to a blaze. An egg has Java's 1/8 chance to hatch one
//! chick, or four chicks on a further 1/32.

use glam::DVec3;

use super::{EntityEvent, Mob, MobKind, PlayerId, Rng};
use crate::item::Item;
use crate::physics::BlockSource;

/// Launch speed (Java's 1.5 blocks a tick).
const SPEED: f64 = 30.0;
/// Blocks per second squared (Java's 0.03 per tick²).
const GRAVITY: f64 = 12.0;
const STEP: f64 = 0.2;
const MAX_FLIGHT: f32 = 30.0;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    Snowball,
    Egg,
}

impl Kind {
    pub fn from_item(item: Item) -> Option<Self> {
        match item {
            Item::SNOWBALL => Some(Self::Snowball),
            Item::EGG => Some(Self::Egg),
            _ => None,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Thrown {
    pub kind: Kind,
    pub owner: Option<PlayerId>,
    pub pos: DVec3,
    pub previous_pos: DVec3,
    vel: DVec3,
    age: f32,
    origin: DVec3,
}

impl Thrown {
    /// Thrown from `eye` along `dir`, carrying the thrower's velocity.
    pub fn launch(kind: Kind, owner: PlayerId, eye: DVec3, dir: DVec3, carry: DVec3, rng: &mut Rng) -> Self {
        let mut wobble = || rng.range(-0.017, 0.017) as f64;
        let aim = dir.normalize_or(DVec3::X) + DVec3::new(wobble(), wobble(), wobble());
        let pos = eye - DVec3::Y * 0.1;
        Self {
            kind,
            owner: Some(owner),
            pos,
            previous_pos: pos,
            vel: aim.normalize() * SPEED + carry,
            age: 0.0,
            origin: pos,
        }
    }

    pub(super) fn update<W: BlockSource + ?Sized>(
        &mut self,
        dt: f64,
        world: &W,
        mobs: &mut [Mob],
        index: &super::mob_index::MobIndex,
        rng: &mut Rng,
        events: &mut Vec<EntityEvent>,
    ) -> bool {
        self.age += dt as f32;
        let ticks = dt * 20.0;
        let water = world.block(self.pos.floor().as_ivec3()).is_some_and(|b| b.holds_water());
        self.vel *= if water { 0.8f64 } else { 0.99 }.powf(ticks);
        self.vel.y -= GRAVITY * dt;
        let delta = self.vel * dt;
        let steps = (delta.length() / STEP).ceil().max(1.0) as u32;
        for _ in 0..steps {
            let next = self.pos + delta / steps as f64;
            if world.block(next.floor().as_ivec3()).is_some_and(|b| b.is_solid()) {
                self.impact(self.pos, rng, events);
                return false;
            }
            self.pos = next;
            let mut hit = None;
            // All mob boxes are at most four blocks high/two blocks wide.
            // Search reused local buckets rather than scanning every mob for
            // every projectile substep. A mob shot must first leave its source.
            index.visit(next - DVec3::Y * 2.0, 4.0, |i| {
                let m = &mobs[i];
                let (min, max) = m.aabb();
                let source = self.owner.is_none()
                    && self.age < 0.25
                    && self.origin.cmpge(min).all()
                    && self.origin.cmple(max).all();
                if m.alive() && !source && next.cmpge(min).all() && next.cmple(max).all() && hit.is_none_or(|j| i < j) {
                    hit = Some(i);
                }
            });
            if let Some(i) = hit {
                let mob = &mut mobs[i];
                let push = DVec3::new(self.vel.x, 0.0, self.vel.z).normalize_or_zero() * 2.0 + DVec3::Y * 2.0;
                let damage = if self.kind == Kind::Snowball && mob.kind == MobKind::Blaze { 3.0 } else { 0.0 };
                let kind = mob.kind;
                let pos = mob.pos;
                let killed = mob.damage(damage, Some(push), rng);
                if killed {
                    events.push(EntityEvent::MobKilled {
                        kind,
                        pos,
                        burning: mob.burning,
                        player_kill: self.owner.is_some(),
                        looting: 0,
                    });
                }
                self.impact(pos, rng, events);
                return false;
            }
        }
        self.age < MAX_FLIGHT && self.pos.y > -128.0
    }

    fn impact(&self, pos: DVec3, rng: &mut Rng, events: &mut Vec<EntityEvent>) {
        if self.kind == Kind::Egg
            && let count = hatch_count(rng)
            && count > 0
        {
            events.push(EntityEvent::Hatched { pos, count });
        }
    }
}

/// Java's thrown egg: 1/8 to spawn one chick, and 1/32 of those spawn four.
pub fn hatch_count(rng: &mut Rng) -> u8 {
    if !rng.chance(1.0 / 8.0) {
        return 0;
    }
    if rng.chance(1.0 / 32.0) { 4 } else { 1 }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::physics::test_util::Grid;

    #[test]
    fn snowballs_hurt_blazes_and_only_knock_other_mobs_back() {
        let world = Grid::flat(0);
        let fly = |kind_mob: Mob, rng: &mut Rng| {
            let mut thrown =
                Thrown::launch(Kind::Snowball, PlayerId::HOST, DVec3::new(0.2, 0.45, 0.5), DVec3::X, DVec3::ZERO, rng);
            let mut mob = kind_mob;
            let mut events = Vec::new();
            let mut index = super::super::mob_index::MobIndex::default();
            index.rebuild(std::slice::from_ref(&mob));
            let mut hit = false;
            for _ in 0..8 {
                if !thrown.update(0.05, &world, std::slice::from_mut(&mut mob), &index, rng, &mut events) {
                    hit = true;
                    break;
                }
            }
            assert!(hit, "snowball should land");
            mob
        };
        let mut rng = Rng::new(4);
        let blaze = fly(Mob::new(MobKind::Blaze, DVec3::new(1.4, 0.0, 0.5), 0.0), &mut rng);
        assert!((blaze.health - (MobKind::Blaze.max_health() - 3.0)).abs() < 1e-4, "{}", blaze.health);
        assert!(blaze.vel.length_squared() > 0.0);
        let pig = fly(Mob::new(MobKind::Pig, DVec3::new(1.4, 0.0, 0.5), 0.0), &mut rng);
        assert_eq!(pig.health, MobKind::Pig.max_health());
        assert!(pig.vel.x.abs() > 0.1 || pig.vel.z.abs() > 0.1);
    }

    #[test]
    fn golem_snowball_leaves_source_and_does_not_credit_player() {
        let world = Grid::flat(0);
        let mut rng = Rng::new(4);
        let mut mobs = [
            Mob::new(MobKind::SnowGolem, DVec3::new(0.5, 1.0, 0.5), 0.0),
            Mob::new(MobKind::Blaze, DVec3::new(3.5, 1.0, 0.5), 0.0),
        ];
        mobs[1].health = 3.0;
        let mut t = Thrown::launch(
            Kind::Snowball,
            PlayerId::HOST,
            mobs[0].pos + DVec3::Y * 1.2,
            DVec3::X,
            DVec3::ZERO,
            &mut rng,
        );
        t.owner = None;
        let mut index = super::super::mob_index::MobIndex::default();
        index.rebuild(&mobs);
        let mut events = Vec::new();
        for _ in 0..8 {
            if !t.update(0.05, &world, &mut mobs, &index, &mut rng, &mut events) {
                break;
            }
        }
        assert_eq!(mobs[0].health, 4.0);
        assert!(
            events.iter().any(|e| matches!(e, EntityEvent::MobKilled { kind: MobKind::Blaze, player_kill: false, .. }))
        );
    }
    #[test]
    fn eggs_hatch_about_one_chick_in_eight() {
        let mut rng = Rng::new(9);
        let mut singles = 0;
        let mut fours = 0;
        for _ in 0..16000 {
            match hatch_count(&mut rng) {
                0 => {}
                1 => singles += 1,
                4 => fours += 1,
                n => panic!("unexpected clutch {n}"),
            }
        }
        let clutches = singles + fours;
        assert!((clutches as f32 / 16000.0 - 0.125).abs() < 0.015, "{clutches}");
        assert!(fours > 10 && (fours as f32 / clutches as f32 - 1.0 / 32.0).abs() < 0.02);
    }
}
