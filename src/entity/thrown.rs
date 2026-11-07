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
    /// A llama's spit (Java's LlamaSpit): 1 damage to whatever it hits.
    LlamaSpit,
    /// v0.16 Track C: a player's wind charge and a breeze's. They fly
    /// straight (no gravity or drag), hurt what they hit by 1 and burst
    /// (see `wind`).
    WindCharge,
    BreezeWindCharge,
}

impl Kind {
    pub fn is_wind(self) -> bool {
        matches!(self, Kind::WindCharge | Kind::BreezeWindCharge)
    }

    pub fn from_item(item: Item) -> Option<Self> {
        match item {
            Item::SNOWBALL => Some(Self::Snowball),
            Item::EGG => Some(Self::Egg),
            crate::world::trial::WIND_CHARGE => Some(Self::WindCharge),
            _ => None,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Thrown {
    pub kind: Kind,
    pub owner: Option<PlayerId>,
    /// The mob that threw it (`uid`); it never hits its own thrower.
    pub shooter: Option<u32>,
    pub pos: DVec3,
    pub previous_pos: DVec3,
    vel: DVec3,
    age: f32,
    origin: DVec3,
}

impl Thrown {
    /// A LAN client's copy: drawn only, never simulated.
    pub(super) fn replica(kind: Kind, pos: DVec3, previous_pos: DVec3) -> Self {
        Self { kind, owner: None, shooter: None, pos, previous_pos, vel: DVec3::ZERO, age: 0.0, origin: pos }
    }

    /// A llama spits from `from` at `target` (Java: 1.5 blocks a tick, inaccuracy 10).
    pub fn spit(from: DVec3, target: DVec3, shooter: u32, rng: &mut Rng) -> Self {
        let d = target - from;
        // Java aims a little high over distance to offset the drop.
        let aim = d + DVec3::Y * (d.with_y(0.0).length() * 0.2);
        let mut wobble = || rng.range(-0.0172, 0.0172) as f64 * 10.0 * 0.5;
        let dir = aim.normalize_or(DVec3::X) + DVec3::new(wobble(), wobble(), wobble()) * 0.1;
        Self {
            kind: Kind::LlamaSpit,
            owner: None,
            shooter: Some(shooter),
            pos: from,
            previous_pos: from,
            vel: dir.normalize() * SPEED,
            age: 0.0,
            origin: from,
        }
    }

    /// A breeze's wind charge from `from` along `dir` (`Shoot.tick`:
    /// 0.7 blocks a tick, Java's `shoot` gaussian spread of
    /// `0.0075 * inaccuracy`).
    pub fn breeze_charge(from: DVec3, dir: DVec3, inaccuracy: f64, shooter: u32, rng: &mut Rng) -> Self {
        let mut gauss = || {
            let u1 = f64::from(rng.next_f32()).max(1e-7);
            let u2 = f64::from(rng.next_f32());
            (-2.0 * u1.ln()).sqrt() * (std::f64::consts::TAU * u2).cos()
        };
        let d = dir.normalize_or(DVec3::X);
        let aim = d + DVec3::new(gauss(), gauss(), gauss()) * 0.0075 * inaccuracy;
        Self {
            kind: Kind::BreezeWindCharge,
            owner: None,
            shooter: Some(shooter),
            pos: from,
            previous_pos: from,
            vel: aim * super::wind::BREEZE_SPEED,
            age: 0.0,
            origin: from,
        }
    }

    /// Thrown from `eye` along `dir`, carrying the thrower's velocity.
    pub fn launch(kind: Kind, owner: PlayerId, eye: DVec3, dir: DVec3, carry: DVec3, rng: &mut Rng) -> Self {
        let mut wobble = || rng.range(-0.017, 0.017) as f64;
        let aim = dir.normalize_or(DVec3::X) + DVec3::new(wobble(), wobble(), wobble());
        let pos = eye - DVec3::Y * 0.1;
        Self {
            kind,
            owner: Some(owner),
            shooter: None,
            pos,
            previous_pos: pos,
            vel: aim.normalize() * SPEED + carry,
            age: 0.0,
            origin: pos,
        }
    }

    #[allow(clippy::too_many_arguments)]
    pub(super) fn update<W: BlockSource + ?Sized>(
        &mut self,
        dt: f64,
        world: &W,
        mobs: &mut [Mob],
        index: &super::mob_index::MobIndex,
        players: &[super::Target],
        rng: &mut Rng,
        events: &mut Vec<EntityEvent>,
    ) -> bool {
        self.age += dt as f32;
        let ticks = dt * 20.0;
        let water = world.block(self.pos.floor().as_ivec3()).is_some_and(|b| b.holds_water());
        if self.kind.is_wind() {
            // `AbstractHurtingProjectile` with no acceleration and inertia
            // 1: straight on; Java bursts it 30 blocks above the build limit.
            if self.pos.y > 350.0 {
                self.burst(self.pos, events);
                return false;
            }
        } else {
            self.vel *= if water { 0.8f64 } else { 0.99 }.powf(ticks);
            // Java: thrown items fall 0.03 blocks/tick², llama spit 0.06.
            self.vel.y -= if self.kind == Kind::LlamaSpit { GRAVITY * 2.0 } else { GRAVITY } * dt;
        }
        let delta = self.vel * dt;
        let steps = (delta.length() / STEP).ceil().max(1.0) as u32;
        for _ in 0..steps {
            let next = self.pos + delta / steps as f64;
            if world.block(next.floor().as_ivec3()).is_some_and(|b| b.is_solid()) {
                self.impact(self.pos, rng, events);
                return false;
            }
            self.pos = next;
            // Wind charges hit players too: a breeze's any player, a
            // player's anyone but its thrower.
            if self.kind.is_wind()
                && let Some(t) = players
                    .iter()
                    .find(|t| t.targetable && t.contains(next) && (self.owner != Some(t.id) || self.age >= 0.25))
            {
                if self.kind == Kind::BreezeWindCharge || self.owner != Some(t.id) {
                    events.push(EntityEvent::PlayerHit {
                        java: None,
                        player: t.id,
                        damage: super::wind::HIT_DAMAGE,
                        knockback: (DVec3::new(self.vel.x, 0.0, self.vel.z).normalize_or_zero() * 0.5).as_vec3(),
                        cause: if self.kind == Kind::BreezeWindCharge {
                            "was slain by a breeze"
                        } else {
                            "was blasted by a wind charge"
                        },
                    });
                }
                self.burst(self.pos, events);
                return false;
            }
            if self.kind == Kind::LlamaSpit
                && let Some(t) = players.iter().find(|t| t.targetable && t.contains(next))
            {
                events.push(EntityEvent::PlayerHit {
                    java: None,
                    player: t.id,
                    damage: 1.0,
                    knockback: (DVec3::new(self.vel.x, 0.0, self.vel.z).normalize_or_zero() * 0.5).as_vec3(),
                    cause: "was spat on by a llama",
                });
                return false;
            }
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
                if m.alive()
                    && !source
                    && Some(m.uid) != self.shooter
                    && next.cmpge(min).all()
                    && next.cmple(max).all()
                    && hit.is_none_or(|j| i < j)
                {
                    hit = Some(i);
                }
            });
            if let Some(i) = hit {
                let mob = &mut mobs[i];
                // `Breeze.deflection`: every projectile but wind charges
                // turns back (`ProjectileDeflection.REVERSE`).
                if mob.kind == MobKind::Breeze && !self.kind.is_wind() {
                    self.vel *= -0.5;
                    self.shooter = Some(mob.uid);
                    self.origin = self.pos;
                    events.push(EntityEvent::Sound {
                        sound: super::MobSound::Breeze(super::breeze::Cue::Deflect),
                        pos: mob.pos,
                    });
                    return true;
                }
                let push = DVec3::new(self.vel.x, 0.0, self.vel.z).normalize_or_zero() * 2.0 + DVec3::Y * 2.0;
                let damage = match self.kind {
                    Kind::Snowball if mob.kind == MobKind::Blaze => 3.0,
                    Kind::LlamaSpit => 1.0,
                    // `Breeze.isInvulnerableTo` a breeze's own damage.
                    Kind::BreezeWindCharge if mob.kind == MobKind::Breeze => 0.0,
                    Kind::WindCharge | Kind::BreezeWindCharge => super::wind::HIT_DAMAGE,
                    _ => 0.0,
                };
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

    fn burst(&self, pos: DVec3, events: &mut Vec<EntityEvent>) {
        let burst = if self.kind == Kind::BreezeWindCharge {
            super::wind::WindBurst::breeze(pos)
        } else {
            super::wind::WindBurst::charge(pos, self.owner)
        };
        events.push(EntityEvent::WindBurst(burst));
    }

    fn impact(&self, pos: DVec3, rng: &mut Rng, events: &mut Vec<EntityEvent>) {
        if self.kind.is_wind() {
            self.burst(pos, events);
            return;
        }
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
                if !thrown.update(0.05, &world, std::slice::from_mut(&mut mob), &index, &[], rng, &mut events) {
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
            if !t.update(0.05, &world, &mut mobs, &index, &[], &mut rng, &mut events) {
                break;
            }
        }
        assert_eq!(mobs[0].health, 4.0);
        assert!(
            events.iter().any(|e| matches!(e, EntityEvent::MobKilled { kind: MobKind::Blaze, player_kill: false, .. }))
        );
    }
    #[test]
    fn wind_charges_fly_straight_and_burst() {
        let world = Grid::flat(0);
        let mut rng = Rng::new(3);
        let mut t = Thrown::launch(
            Kind::WindCharge,
            PlayerId::HOST,
            DVec3::new(0.5, 3.0, 0.5),
            DVec3::new(1.0, -0.2, 0.0),
            DVec3::ZERO,
            &mut rng,
        );
        let index = super::super::mob_index::MobIndex::default();
        let mut events = Vec::new();
        let mut steps = 0;
        while t.update(0.05, &world, &mut [], &index, &[], &mut rng, &mut events) {
            steps += 1;
            assert!(steps < 40);
        }
        let Some(EntityEvent::WindBurst(b)) = events.last() else { panic!("{events:?}") };
        assert_eq!((b.radius, b.owner), (super::super::wind::CHARGE_RADIUS, Some(PlayerId::HOST)));
        assert!(b.center.y < 0.3 && b.center.x > 10.0, "a straight line at -0.2 slope: {}", b.center);
    }

    #[test]
    fn breezes_deflect_snowballs_but_not_wind_charges() {
        let world = Grid::flat(0);
        let mut rng = Rng::new(8);
        let index_for = |m: &Mob| {
            let mut i = super::super::mob_index::MobIndex::default();
            i.rebuild(std::slice::from_ref(m));
            i
        };
        let mut breeze = Mob::new(MobKind::Breeze, DVec3::new(2.5, 0.0, 0.5), 0.0);
        breeze.uid = 9;
        let mut t =
            Thrown::launch(Kind::Snowball, PlayerId::HOST, DVec3::new(0.2, 1.0, 0.5), DVec3::X, DVec3::ZERO, &mut rng);
        let mut events = Vec::new();
        for _ in 0..4 {
            let index = index_for(&breeze);
            if !t.update(0.05, &world, std::slice::from_mut(&mut breeze), &index, &[], &mut rng, &mut events) {
                break;
            }
        }
        assert!(t.vel.x < 0.0, "the snowball turned back");
        assert!(events.iter().any(|e| matches!(e, EntityEvent::Sound { .. })));
        let mut w = Thrown::launch(
            Kind::WindCharge,
            PlayerId::HOST,
            DVec3::new(0.2, 1.0, 0.5),
            DVec3::X,
            DVec3::ZERO,
            &mut rng,
        );
        let mut events = Vec::new();
        for _ in 0..4 {
            let index = index_for(&breeze);
            if !w.update(0.05, &world, std::slice::from_mut(&mut breeze), &index, &[], &mut rng, &mut events) {
                break;
            }
        }
        assert!(events.iter().any(|e| matches!(e, EntityEvent::WindBurst(_))));
        assert_eq!(breeze.health, MobKind::Breeze.max_health() - super::super::wind::HIT_DAMAGE);
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
