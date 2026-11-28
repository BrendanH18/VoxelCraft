//! Thrown snowballs and eggs. They fly like Java's (1.5 blocks a tick, 0.03
//! gravity, 0.99 drag), knock a mob back, and vanish on impact. A snowball
//! deals 3 damage to a blaze. An egg has Java's 1/8 chance to hatch one
//! chick, or four chicks on a further 1/32.
//!
//! A Bottle o' Enchanting (`ThrownExperienceBottle`) is thrown 20 degrees
//! high at 0.7 blocks a tick, falls 0.07 blocks a tick², and on any hit
//! shatters into 3 to 11 points of experience.

use glam::DVec3;

use super::{EntityEvent, Mob, MobKind, PlayerId, Rng};
use crate::item::Item;
use crate::physics::BlockSource;

/// Launch speed (Java's 1.5 blocks a tick).
const SPEED: f64 = 30.0;
/// Java's `Projectile.getMovementToShoot` noise per unit of uncertainty.
pub const SHOOT_NOISE: f64 = 0.0172275;
/// `ExperienceBottleItem.use`: `spawnProjectileFromRotation(.., -20, 0.7, 1)`.
pub const BOTTLE_PITCH_OFFSET: f64 = -20.0;
pub const BOTTLE_POWER: f64 = 0.7f32 as f64;
pub const BOTTLE_UNCERTAINTY: f64 = 1.0;
/// `ThrownExperienceBottle.getDefaultGravity`, blocks per tick².
pub const BOTTLE_GRAVITY: f64 = 0.07;
/// `ExperienceBottleItem.createDispenseConfig`: the default dispenser shot
/// (power 1.1, uncertainty 6) with 1.25 times the power and half the uncertainty.
pub const BOTTLE_DISPENSE_POWER: f64 = 1.375;
pub const BOTTLE_DISPENSE_UNCERTAINTY: f64 = 3.0;
/// Level event 2002's colour for the bottle (`-13083194`, `0x385DC6`).
pub const BOTTLE_COLOUR: [u8; 3] = [0x38, 0x5D, 0xC6];
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
    /// v0.17 build-5000 (ITEM-02): a thrown Bottle o' Enchanting.
    ExperienceBottle,
    /// v0.15 Track D (appended): a blue egg (cold chick) and a brown egg
    /// (warm chick).
    BlueEgg,
    BrownEgg,
}

impl Kind {
    /// v1.0 gap Track 8e: the saved id (append-only).
    fn id(self) -> u8 {
        match self {
            Kind::Snowball => 0,
            Kind::Egg => 1,
            Kind::LlamaSpit => 2,
            Kind::WindCharge => 3,
            Kind::BreezeWindCharge => 4,
            Kind::ExperienceBottle => 5,
            Kind::BlueEgg => 6,
            Kind::BrownEgg => 7,
        }
    }

    fn from_id(id: u64) -> Option<Self> {
        Some(match id {
            0 => Kind::Snowball,
            1 => Kind::Egg,
            2 => Kind::LlamaSpit,
            3 => Kind::WindCharge,
            4 => Kind::BreezeWindCharge,
            5 => Kind::ExperienceBottle,
            6 => Kind::BlueEgg,
            7 => Kind::BrownEgg,
            _ => return None,
        })
    }

    /// v1.0 gap Track 8d: the box (`EntityTypes`): 0.25 square, a wind
    /// charge's 0.3125.
    pub fn size(self) -> f64 {
        if self.is_wind() { 0.3125 } else { 0.25 }
    }

    pub fn is_wind(self) -> bool {
        matches!(self, Kind::WindCharge | Kind::BreezeWindCharge)
    }

    /// v0.15 Track D: the chick variant a thrown egg hatches
    /// (`ThrownEgg.onHit` reads the item's `chicken/variant`).
    pub fn egg_variant(self) -> Option<crate::content::farm_variant::FarmVariant> {
        use crate::content::farm_variant::FarmVariant;
        match self {
            Kind::Egg => Some(FarmVariant::Temperate),
            Kind::BlueEgg => Some(FarmVariant::Cold),
            Kind::BrownEgg => Some(FarmVariant::Warm),
            _ => None,
        }
    }

    pub fn from_item(item: Item) -> Option<Self> {
        match item {
            Item::SNOWBALL => Some(Self::Snowball),
            Item::EGG => Some(Self::Egg),
            crate::content::farm_variant::BLUE_EGG => Some(Self::BlueEgg),
            crate::content::farm_variant::BROWN_EGG => Some(Self::BrownEgg),
            crate::world::trial::WIND_CHARGE => Some(Self::WindCharge),
            Item::EXPERIENCE_BOTTLE => Some(Self::ExperienceBottle),
            _ => None,
        }
    }

    /// `getDefaultGravity` in blocks per tick²: thrown items 0.03, llama
    /// spit 0.06, the experience bottle 0.07.
    pub fn gravity(self) -> f64 {
        match self {
            Kind::LlamaSpit => 0.06,
            Kind::ExperienceBottle => BOTTLE_GRAVITY,
            Kind::WindCharge | Kind::BreezeWindCharge => 0.0,
            Kind::Snowball | Kind::Egg | Kind::BlueEgg | Kind::BrownEgg => 0.03,
        }
    }
}

/// Java's `Projectile.shootFromRotation` heading from a look direction:
/// the yaw and pitch of `dir`, with `y_offset` degrees added to the pitch
/// of the vertical part only (so the heading is not a unit vector).
pub fn heading_from_rotation(dir: DVec3, y_offset: f64) -> DVec3 {
    let d = dir.normalize_or(DVec3::X);
    // Java's x rotation: degrees below the horizon.
    let x_rot = -d.y.clamp(-1.0, 1.0).asin();
    DVec3::new(d.x, -(x_rot + y_offset.to_radians()).sin(), d.z)
}

/// Java's `Projectile.getMovementToShoot`: the normalized heading plus
/// `triangle(0, 0.0172275 * uncertainty)` per axis, times `power`, in
/// blocks per tick.
pub fn shot_movement(heading: DVec3, power: f64, uncertainty: f64, rng: &mut Rng) -> DVec3 {
    let n = SHOOT_NOISE * uncertainty;
    let noise = DVec3::new(rng.triangle(0.0, n), rng.triangle(0.0, n), rng.triangle(0.0, n));
    (heading.normalize_or(DVec3::X) + noise) * power
}

/// `ThrownExperienceBottle.onHit`: `3 + nextInt(5) + nextInt(5)` points.
pub fn bottle_experience(rng: &mut Rng) -> u32 {
    3 + rng.next_int(5) + rng.next_int(5)
}

/// Where a step from `from` to `to` (which ends in a solid cell) first
/// enters a solid cell, and the outward normal of the face it crosses
/// (`BlockHitResult`'s location and direction). The cell boundaries the
/// step crosses are walked in order, so a step that clips a floor before
/// reaching the next column hits the floor's top. A step that starts inside
/// a solid cell reports `from` with the reverse of the step.
fn face_hit<W: BlockSource + ?Sized>(world: &W, from: DVec3, to: DVec3) -> (DVec3, DVec3) {
    let (a, b) = (from.floor(), to.floor());
    let delta = to - from;
    let mut crossings: Vec<(f64, usize)> = Vec::with_capacity(3);
    for axis in 0..3 {
        if a[axis] == b[axis] || delta[axis] == 0.0 {
            continue;
        }
        let boundary = if b[axis] > a[axis] { b[axis] } else { a[axis] };
        crossings.push((((boundary - from[axis]) / delta[axis]).clamp(0.0, 1.0), axis));
    }
    crossings.sort_by(|x, y| x.0.total_cmp(&y.0));
    let mut cell = a;
    let mut last = None;
    for &(t, axis) in &crossings {
        let step = (b[axis] - a[axis]).signum();
        cell[axis] += step;
        let mut normal = DVec3::ZERO;
        normal[axis] = -step;
        last = Some((from + delta * t, normal));
        if world.block(cell.as_ivec3()).is_some_and(|block| block.is_solid()) {
            break;
        }
    }
    last.unwrap_or((from, -delta.normalize_or_zero()))
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Thrown {
    pub kind: Kind,
    pub owner: Option<PlayerId>,
    /// The mob that threw it (`uid`); it never hits its own thrower.
    pub shooter: Option<u32>,
    pub pos: DVec3,
    pub previous_pos: DVec3,
    pub(crate) vel: DVec3,
    age: f32,
    origin: DVec3,
    /// v1.0 gap Track 8b: who llama spit already hit on its way through
    /// (mob uids, players as `u32::MAX - id`), standing in for their hurt
    /// cooldown (`invulnerableTime`), which refuses the same spit's 1 again.
    spat_on: [Option<u32>; 4],
    /// v1.0 gap Track 8d: `remainingFireTicks`. Every kind but llama spit
    /// applies block effects (`ThrowableProjectile.tick`,
    /// `AbstractHurtingProjectile.tick`), so fire and lava set it alight,
    /// and water and rain put it out; a burning one is drawn with Java's
    /// flame and lights what its block hit lights. Saved (v1.0 gap Track
    /// 8e), as Java's `Fire`.
    fire: u16,
    /// v1.0 final Track F2: ticks until it can use a portal again
    /// (`Entity.portalCooldown`; not saved).
    pub portal_cooldown: u16,
}

impl Thrown {
    /// v1.0 gap Track 8e: the level file's copy, with its fire.
    pub(super) fn save(&self) -> serde_json::Value {
        use super::saved_projectiles::{owner_json, v3};
        serde_json::json!({
            "kind": self.kind.id(),
            "owner": owner_json(super::projectile::owner_of(self.owner, None)),
            "shooter": self.shooter,
            "pos": v3(self.pos),
            "motion": v3(self.vel),
            "age": self.age,
            "origin": v3(self.origin),
            "fire": self.fire,
        })
    }

    pub(super) fn load(v: &serde_json::Value) -> Option<Self> {
        use super::saved_projectiles as s;
        let pos = s::read_v3(&v["pos"])?;
        Some(Self {
            portal_cooldown: 0,
            kind: Kind::from_id(v["kind"].as_u64()?)?,
            owner: s::player_of(s::read_owner(&v["owner"])),
            shooter: v["shooter"].as_u64().and_then(|u| u32::try_from(u).ok()),
            pos,
            previous_pos: pos,
            vel: s::read_v3(&v["motion"]).unwrap_or(DVec3::ZERO),
            age: s::read_f32(v, "age", 0.0).clamp(0.0, MAX_FLIGHT),
            origin: s::read_v3(&v["origin"]).unwrap_or(pos),
            spat_on: [None; 4],
            fire: s::read_fire(v),
        })
    }

    /// A LAN client's copy: drawn only, never simulated.
    pub(super) fn replica(kind: Kind, pos: DVec3, previous_pos: DVec3) -> Self {
        Self {
            portal_cooldown: 0,
            kind,
            owner: None,
            shooter: None,
            pos,
            previous_pos,
            vel: DVec3::ZERO,
            age: 0.0,
            origin: pos,
            spat_on: [None; 4],
            fire: 0,
        }
    }

    /// A llama spits from `from` at `target` (Java: 1.5 blocks a tick, inaccuracy 10).
    pub fn spit(from: DVec3, target: DVec3, shooter: u32, rng: &mut Rng) -> Self {
        let d = target - from;
        // Java aims a little high over distance to offset the drop.
        let aim = d + DVec3::Y * (d.with_y(0.0).length() * 0.2);
        let mut wobble = || rng.range(-0.0172, 0.0172) as f64 * 10.0 * 0.5;
        let dir = aim.normalize_or(DVec3::X) + DVec3::new(wobble(), wobble(), wobble()) * 0.1;
        Self {
            portal_cooldown: 0,
            kind: Kind::LlamaSpit,
            owner: None,
            shooter: Some(shooter),
            pos: from,
            previous_pos: from,
            vel: dir.normalize() * SPEED,
            age: 0.0,
            origin: from,
            spat_on: [None; 4],
            fire: 0,
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
            portal_cooldown: 0,
            kind: Kind::BreezeWindCharge,
            owner: None,
            shooter: Some(shooter),
            pos: from,
            previous_pos: from,
            vel: aim * super::wind::BREEZE_SPEED,
            age: 0.0,
            origin: from,
            spat_on: [None; 4],
            fire: 0,
        }
    }

    /// Thrown from `eye` along `dir`, carrying the thrower's velocity.
    pub fn launch(kind: Kind, owner: PlayerId, eye: DVec3, dir: DVec3, carry: DVec3, rng: &mut Rng) -> Self {
        if kind == Kind::ExperienceBottle {
            // `spawnProjectileFromRotation`: eye height less 0.1, aimed
            // 20 degrees high, 0.7 blocks a tick with uncertainty 1.
            let heading = heading_from_rotation(dir, BOTTLE_PITCH_OFFSET);
            let vel = shot_movement(heading, BOTTLE_POWER, BOTTLE_UNCERTAINTY, rng) * 20.0 + carry;
            let pos = eye - DVec3::Y * 0.1;
            return Self {
                portal_cooldown: 0,
                kind,
                owner: Some(owner),
                shooter: None,
                pos,
                previous_pos: pos,
                vel,
                age: 0.0,
                origin: pos,
                spat_on: [None; 4],
                fire: 0,
            };
        }
        let mut wobble = || rng.range(-0.017, 0.017) as f64;
        let aim = dir.normalize_or(DVec3::X) + DVec3::new(wobble(), wobble(), wobble());
        let pos = eye - DVec3::Y * 0.1;
        Self {
            portal_cooldown: 0,
            kind,
            owner: Some(owner),
            shooter: None,
            pos,
            previous_pos: pos,
            vel: aim.normalize() * SPEED + carry,
            age: 0.0,
            origin: pos,
            spat_on: [None; 4],
            fire: 0,
        }
    }

    /// `ProjectileDispenseBehavior`: a dispenser's ownerless shot from `pos`
    /// along the facing `step`.
    pub fn dispensed(kind: Kind, pos: DVec3, step: DVec3, power: f64, uncertainty: f64, rng: &mut Rng) -> Self {
        let vel = shot_movement(step, power, uncertainty, rng) * 20.0;
        Self {
            portal_cooldown: 0,
            kind,
            owner: None,
            shooter: None,
            pos,
            previous_pos: pos,
            vel,
            age: 0.0,
            origin: pos,
            spat_on: [None; 4],
            fire: 0,
        }
    }

    /// Velocity in blocks per second.
    pub fn velocity(&self) -> DVec3 {
        self.vel
    }

    /// v1.0 gap Track 8d: on fire (`isOnFire`).
    pub fn burning(&self) -> bool {
        self.fire > 0
    }

    /// v1.0 gap Track 8d: a LAN client's copy burns as the host says.
    pub fn set_burning(&mut self, burning: bool) {
        self.fire = if burning { self.fire.max(1) } else { 0 };
    }

    #[allow(clippy::too_many_arguments)]
    pub(super) fn update<W: super::MobWorld + ?Sized>(
        &mut self,
        dt: f64,
        world: &W,
        mobs: &mut [Mob],
        index: &super::mob_index::MobIndex,
        players: &[super::Target],
        boss: &super::dragon::Boxes,
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
        } else if self.kind == Kind::ExperienceBottle {
            // `ThrowableProjectile.tick`: `applyGravity`, then `applyInertia`.
            self.vel.y -= self.kind.gravity() * 400.0 * dt;
            self.vel *= if water { 0.8f64 } else { 0.99 }.powf(ticks);
        } else {
            self.vel *= if water { 0.8f64 } else { 0.99 }.powf(ticks);
            // Java: thrown items fall 0.03 blocks/tick², llama spit 0.06.
            self.vel.y -= self.kind.gravity() * 400.0 * dt;
        }
        let delta = self.vel * dt;
        // v1.0 gap Track 8d: `applyEffectsFromBlocks` along the move, then
        // `baseTick` burns it down a tick.
        if self.kind != Kind::LlamaSpit {
            let size = self.kind.size();
            super::projectile::tick_fire(&mut self.fire, world, self.pos, self.pos + delta, size / 2.0, size);
        }
        let steps = (delta.length() / STEP).ceil().max(1.0) as u32;
        // v1.0 gap Track 8: the End crystal this spit broke this tick.
        let mut spat = None;
        for _ in 0..steps {
            let next = self.pos + delta / steps as f64;
            if world.block(next.floor().as_ivec3()).is_some_and(|b| b.is_solid()) {
                // v1.0 gap Track 8d: `Projectile.onHitBlock`'s
                // `onProjectileHit` (a burning one lights candles and
                // campfires and primes TNT).
                let (at, normal) = face_hit(world, self.pos, next);
                if self.kind != Kind::LlamaSpit {
                    events.push(EntityEvent::ProjectileBlockHit {
                        cell: next.floor().as_ivec3(),
                        pos: at,
                        normal: normal.as_ivec3(),
                        arrow: false,
                        burning: self.burning(),
                        mob_owned: self.shooter.is_some(),
                        by: super::projectile::owner_of(self.owner, self.shooter),
                    });
                }
                if self.kind == Kind::ExperienceBottle {
                    self.pos = at;
                    self.bottle_burst(at, normal, rng, events);
                    return false;
                }
                self.impact(self.pos, rng, events);
                return false;
            }
            self.pos = next;
            // A bottle breaks on any player but its thrower while it leaves them.
            // `Player.isPickable` is false only for spectators: creative
            // players stop bottles too.
            if self.kind == Kind::ExperienceBottle
                && players.iter().any(|t| {
                    t.alive && !t.spectator && t.contains(next) && (self.owner != Some(t.id) || self.age >= 0.25)
                })
            {
                self.bottle_burst(next, -self.vel, rng, events);
                return false;
            }
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
                            crate::simulation::damage::Cause::BREEZE_WIND_CHARGE
                        } else if let Some(owner) = self.owner {
                            // v1.0 gap Track 8f: named after its thrower.
                            crate::simulation::damage::Cause::PLAYER_WIND_CHARGE
                                .by(Some(crate::entity::trident::Owner::Player(owner)))
                        } else {
                            crate::simulation::damage::Cause::WIND_CHARGE
                        },
                    });
                    // v1.0 gap Track 2g: the breeze is the player's `lastHurtByMob`.
                    if let Some(attacker) = self.shooter {
                        events.push(EntityEvent::OwnerHurt { player: t.id, attacker });
                    }
                }
                self.burst(self.pos, events);
                return false;
            }
            // v1.0 gap Track 8b: `LlamaSpit.onHitEntity` hurts a player and
            // never discards the spit, which flies on (only blocks stop it).
            if self.kind == Kind::LlamaSpit
                && let Some(t) =
                    players.iter().find(|t| t.targetable && t.contains(next) && !self.spat_on(u32::MAX - t.id.0))
            {
                self.spit_on(u32::MAX - t.id.0);
                if self.llama_lives(mobs) {
                    events.push(EntityEvent::PlayerHit {
                        java: None,
                        player: t.id,
                        damage: 1.0,
                        knockback: (DVec3::new(self.vel.x, 0.0, self.vel.z).normalize_or_zero() * 0.5).as_vec3(),
                        cause: crate::simulation::damage::Cause::LLAMA_SPIT,
                    });
                    if let Some(attacker) = self.shooter {
                        events.push(EntityEvent::OwnerHurt { player: t.id, attacker });
                    }
                }
                continue;
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
                    && !(self.kind == Kind::LlamaSpit && self.spat_on(m.uid))
                    && next.cmpge(min).all()
                    && next.cmple(max).all()
                    && hit.is_none_or(|j| i < j)
                {
                    hit = Some(i);
                }
            });
            // v1.0 gap Track 8: `LlamaSpit.onHitEntity` hurts what it hits
            // by 1 (`spit`) while its llama lives (`getOwner` is a
            // `LivingEntity`), and isn't stopped by it: an End crystal
            // breaks (`EndCrystal.hurtServer`, any source but a dragon) and
            // the spit flies on. It can't hurt a dragon (not a player's), so
            // passes its parts.
            if hit.is_none()
                && self.kind == Kind::LlamaSpit
                && let Some(crystal) = boss.crystal_at(next).filter(|&c| spat != Some(c))
                && self.llama_lives(mobs)
            {
                spat = Some(crystal);
                // v1.0 gap Track 8b: the crystal's blast names the llama
                // (`explosion(crystal, llama)`).
                let llama = mobs.iter().find(|m| Some(m.uid) == self.shooter).map(|m| m.kind);
                let blow = super::dragon::Blow::projectile(1.0, false).blamed_on(llama);
                events.push(EntityEvent::DragonStruck { hit: crystal, blow, by: None });
            }
            // v1.0 gap Track 4c: an End crystal or a dragon part
            // (`EnderDragonPart` is pickable; wind charges pass crystals,
            // `AbstractWindCharge.canHitEntity`).
            if hit.is_none()
                && self.kind != Kind::LlamaSpit
                && let Some(struck) = boss.hit_at(next, !self.kind.is_wind())
            {
                self.pos = next;
                // `ThrownExperienceBottle` has no `onHitEntity`: it only bursts.
                if self.kind == Kind::ExperienceBottle {
                    self.bottle_burst(next, -self.vel, rng, events);
                    return false;
                }
                // `Snowball`, `ThrownEgg`: `hurt(thrown, 0)`, which breaks a
                // crystal and does nothing to a dragon (`EnderDragon.hurt`
                // refuses under 0.01); a wind charge's one point, which only
                // a player's takes from the dragon.
                let blow = match self.kind {
                    Kind::WindCharge | Kind::BreezeWindCharge => {
                        super::dragon::Blow::wind_charge(self.kind == Kind::BreezeWindCharge, self.owner.is_some())
                    }
                    _ => super::dragon::Blow::projectile(0.0, self.owner.is_some()),
                };
                // v1.0 gap Track 8b: a mob's (a snow golem's snowball, a
                // breeze's charge) names it in a crystal's blast.
                let thrower =
                    self.shooter.and_then(|uid| mobs.iter().find(|m| m.uid == uid && m.alive()).map(|m| m.kind));
                let blow = blow.blamed_on(thrower);
                if !struck.is_dragon() || blow.damage > 0.0 {
                    events.push(EntityEvent::DragonStruck { hit: struck, blow, by: self.owner });
                }
                self.impact(next, rng, events);
                return false;
            }
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
                // `ThrownExperienceBottle` has no `onHitEntity`: no damage or
                // knockback, only the burst, its orbs thrown back along its motion.
                if self.kind == Kind::ExperienceBottle {
                    self.bottle_burst(next, -self.vel, rng, events);
                    return false;
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
                // v1.0 gap Track 8b: spit hurts a mob only while its llama
                // lives (`getOwner() instanceof LivingEntity`), and flies on.
                let spit = self.kind == Kind::LlamaSpit;
                let uid = mob.uid;
                let lives = spit && self.shooter.is_some_and(|s| mobs.iter().any(|m| m.uid == s && m.alive()));
                let mob = &mut mobs[i];
                let killed = (!spit || lives) && mob.damage(damage, Some(push), rng);
                if killed {
                    events.push(EntityEvent::MobKilled {
                        kind,
                        pos,
                        burning: mob.burning,
                        player_kill: self.owner.is_some(),
                        looting: 0,
                    });
                }
                if spit {
                    self.spit_on(uid);
                    continue;
                }
                self.impact(pos, rng, events);
                return false;
            }
        }
        self.age < MAX_FLIGHT && self.pos.y > -128.0
    }

    /// Llama spit's llama is still there (`getOwner()` resolves).
    fn llama_lives(&self, mobs: &[Mob]) -> bool {
        self.shooter.is_some_and(|uid| mobs.iter().any(|m| m.uid == uid && m.alive()))
    }

    fn spat_on(&self, key: u32) -> bool {
        self.spat_on.contains(&Some(key))
    }

    fn spit_on(&mut self, key: u32) {
        self.spat_on.rotate_right(1);
        self.spat_on[0] = Some(key);
    }

    fn burst(&self, pos: DVec3, events: &mut Vec<EntityEvent>) {
        let burst = if self.kind == Kind::BreezeWindCharge {
            super::wind::WindBurst::breeze(pos)
        } else {
            super::wind::WindBurst::charge(pos, self.owner)
        };
        events.push(EntityEvent::WindBurst(burst));
    }

    /// `ThrownExperienceBottle.onHit`: level events 2002 (splash particles in
    /// the bottle's colour) and 1053 (the splash sound), then the
    /// experience as orbs thrown along `toward`.
    fn bottle_burst(&self, pos: DVec3, toward: DVec3, rng: &mut Rng, events: &mut Vec<EntityEvent>) {
        events.push(EntityEvent::PotionSplashed { pos, colour: BOTTLE_COLOUR });
        events.push(EntityEvent::ExperienceBurst { pos, toward, points: bottle_experience(rng) });
    }

    fn impact(&self, pos: DVec3, rng: &mut Rng, events: &mut Vec<EntityEvent>) {
        if self.kind.is_wind() {
            self.burst(pos, events);
            return;
        }
        // v1.0 gap Track 2g: entity event 3: eight `ItemParticleOption`
        // particles of the thrown item (`Snowball` still, `ThrownEgg` up to
        // 0.04 a tick each way), drawn from the client's own random.
        let item = match self.kind {
            Kind::Snowball => Some(Item::SNOWBALL),
            Kind::Egg => Some(Item::EGG),
            Kind::BlueEgg => Some(crate::content::farm_variant::BLUE_EGG),
            Kind::BrownEgg => Some(crate::content::farm_variant::BROWN_EGG),
            _ => None,
        };
        if let Some(item) = item {
            let mut local =
                Rng::new(pos.x.to_bits() ^ pos.y.to_bits().rotate_left(13) ^ pos.z.to_bits().rotate_left(29));
            for _ in 0..8 {
                let mut spread =
                    || if item == Item::SNOWBALL { 0.0 } else { (f64::from(local.next_f32()) - 0.5) * 0.08 };
                let vel = DVec3::new(spread(), spread(), spread());
                events.push(EntityEvent::ItemParticle { pos, vel, item });
            }
        }
        if let Some(variant) = self.kind.egg_variant()
            && let count = hatch_count(rng)
            && count > 0
        {
            events.push(EntityEvent::Hatched { pos, count, variant });
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
                if !thrown.update(
                    0.05,
                    &world,
                    std::slice::from_mut(&mut mob),
                    &index,
                    &[],
                    &Default::default(),
                    rng,
                    &mut events,
                ) {
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
            if !t.update(0.05, &world, &mut mobs, &index, &[], &Default::default(), &mut rng, &mut events) {
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
        while t.update(0.05, &world, &mut [], &index, &[], &Default::default(), &mut rng, &mut events) {
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
            if !t.update(
                0.05,
                &world,
                std::slice::from_mut(&mut breeze),
                &index,
                &[],
                &Default::default(),
                &mut rng,
                &mut events,
            ) {
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
            if !w.update(
                0.05,
                &world,
                std::slice::from_mut(&mut breeze),
                &index,
                &[],
                &Default::default(),
                &mut rng,
                &mut events,
            ) {
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

    fn bottle_fixture() -> serde_json::Value {
        let v: serde_json::Value =
            serde_json::from_str(include_str!("../../tests/fixtures/java/26.3/thrown_items.json")).unwrap();
        v["experience_bottle"].clone()
    }

    /// Flies `t` in `world` until it breaks, returning the events.
    fn fly(t: &mut Thrown, world: &Grid, mobs: &mut [Mob], players: &[super::super::Target]) -> Vec<EntityEvent> {
        let mut rng = Rng::new(5);
        let mut events = Vec::new();
        for _ in 0..200 {
            let mut index = super::super::mob_index::MobIndex::default();
            index.rebuild(mobs);
            if !t.update(0.05, world, mobs, &index, players, &Default::default(), &mut rng, &mut events) {
                return events;
            }
        }
        panic!("the bottle never broke: {:?}", t.pos);
    }

    fn burst(events: &[EntityEvent]) -> (DVec3, DVec3, u32) {
        events
            .iter()
            .find_map(|e| match *e {
                EntityEvent::ExperienceBurst { pos, toward, points } => Some((pos, toward, points)),
                _ => None,
            })
            .expect("an experience burst")
    }

    #[test]
    fn experience_bottles_fall_like_java_gravity_then_drag() {
        // `ThrowableProjectile.tick`: v.y -= 0.07, v *= 0.99, then move.
        let f = bottle_fixture();
        let gravity = f["entity"]["gravity"].as_f64().unwrap();
        assert_eq!(Kind::ExperienceBottle.gravity(), gravity);
        let world = Grid::flat(-100);
        let start = DVec3::new(0.5, 10.0, 0.5);
        let mut rng = Rng::new(1);
        let mut t = Thrown::dispensed(Kind::ExperienceBottle, start, DVec3::new(1.0, 0.5, 0.0), 0.7, 0.0, &mut rng);
        let mut v = DVec3::new(1.0, 0.5, 0.0).normalize() * 0.7;
        let mut pos = start;
        let index = super::super::mob_index::MobIndex::default();
        let mut events = Vec::new();
        for tick in 0..40 {
            assert!(t.update(0.05, &world, &mut [], &index, &[], &Default::default(), &mut rng, &mut events));
            v.y -= gravity;
            v *= 0.99;
            pos += v;
            assert!((t.pos - pos).length() < 1e-9, "tick {tick}: {} vs {pos}", t.pos);
            assert!((t.velocity() / 20.0 - v).length() < 1e-12);
        }
    }

    #[test]
    fn experience_bottles_burst_with_orbs_along_the_hit_face_or_back_along_their_flight() {
        let colour = bottle_fixture()["hit"]["colour_rgb"].clone();
        let colour: Vec<u8> = colour.as_array().unwrap().iter().map(|c| c.as_u64().unwrap() as u8).collect();
        assert_eq!(BOTTLE_COLOUR.to_vec(), colour);
        let mut rng = Rng::new(2);
        // Dropped onto the floor: the burst is on the top face, orbs thrown up.
        let floor = Grid::flat(0);
        let mut t =
            Thrown::dispensed(Kind::ExperienceBottle, DVec3::new(0.3, 2.0, 0.7), DVec3::NEG_Y, 0.2, 0.0, &mut rng);
        let events = fly(&mut t, &floor, &mut [], &[]);
        let (pos, toward, points) = burst(&events);
        assert!((pos - DVec3::new(0.3, 0.0, 0.7)).length() < 1e-9, "{pos}");
        assert_eq!(toward, DVec3::Y);
        assert!((3..=11).contains(&points));
        assert!(events.iter().any(|e| matches!(e, EntityEvent::PotionSplashed { colour: BOTTLE_COLOUR, .. })));
        // Flying +x into a wall: the west face, orbs thrown back west.
        let mut wall = Grid::flat(-50);
        wall.set(glam::IVec3::new(3, 5, 0), crate::world::block::Block::STONE);
        let mut t = Thrown::dispensed(Kind::ExperienceBottle, DVec3::new(0.5, 5.5, 0.5), DVec3::X, 1.0, 0.0, &mut rng);
        let (pos, toward, _) = burst(&fly(&mut t, &wall, &mut [], &[]));
        assert_eq!(toward, DVec3::NEG_X);
        assert!((pos.x - 3.0).abs() < 1e-9 && (5.0..6.0).contains(&pos.y), "{pos}");
        // A mob: no damage or knockback, orbs thrown back along the bottle's motion.
        let mut pig = Mob::new(MobKind::Pig, DVec3::new(2.5, 0.0, 0.5), 0.0);
        let mut t = Thrown::dispensed(Kind::ExperienceBottle, DVec3::new(0.2, 0.5, 0.5), DVec3::X, 1.0, 0.0, &mut rng);
        let events = fly(&mut t, &floor, std::slice::from_mut(&mut pig), &[]);
        let (_, toward, _) = burst(&events);
        assert!(toward.x < 0.0 && (toward + t.velocity()).length() < 1e-9, "{toward}");
        assert_eq!(pig.health, MobKind::Pig.max_health());
        assert_eq!(pig.vel, DVec3::ZERO);
        assert!(!events.iter().any(|e| matches!(e, EntityEvent::MobKilled { .. })));
        // A player who did not throw it.
        let other = super::super::Target::new(PlayerId(2), DVec3::new(2.5, 0.0, 0.5), true);
        let mut t = Thrown::dispensed(Kind::ExperienceBottle, DVec3::new(0.2, 1.0, 0.5), DVec3::X, 1.0, 0.0, &mut rng);
        let (pos, toward, _) = burst(&fly(&mut t, &floor, &mut [], std::slice::from_ref(&other)));
        assert!(other.contains(pos) && toward.x < 0.0);
    }

    #[test]
    fn experience_bottles_award_three_to_eleven_points_triangularly() {
        let hit = &bottle_fixture()["hit"];
        let base = hit["xp_base"].as_u64().unwrap() as u32;
        let rolls: Vec<u32> = hit["xp_rolls"].as_array().unwrap().iter().map(|r| r.as_u64().unwrap() as u32).collect();
        let (lo, hi) = (base, base + rolls.iter().map(|r| r - 1).sum::<u32>());
        let mut rng = Rng::new(17);
        let mut counts = vec![0u32; hi as usize + 1];
        const N: u32 = 2000;
        for _ in 0..N {
            let xp = bottle_experience(&mut rng);
            assert!((lo..=hi).contains(&xp), "{xp}");
            counts[xp as usize] += 1;
        }
        assert!(counts[lo as usize..].iter().all(|&c| c > 0), "every total appears: {counts:?}");
        let mean = counts.iter().enumerate().map(|(v, &c)| v as f64 * f64::from(c)).sum::<f64>() / f64::from(N);
        let expected = f64::from(base) + rolls.iter().map(|&r| f64::from(r - 1) / 2.0).sum::<f64>();
        // Two uniform 0..4 rolls: variance 4 in all, so 5 standard errors is 0.22.
        assert!((mean - expected).abs() < 0.23, "mean {mean} vs {expected}");
        // The peak is the middle total (5 of 25 pairs).
        let middle = f64::from(counts[expected as usize]) / f64::from(N);
        assert!((middle - 0.2).abs() < 0.045, "{middle}");
    }

    /// `Projectile.canHitEntity` through `Player.isPickable`: creative
    /// players stop a bottle, spectators and the dead do not.
    #[test]
    fn experience_bottles_break_on_creative_players_but_not_spectators() {
        let floor = Grid::flat(0);
        let mut rng = Rng::new(8);
        let throw = |rng: &mut Rng| {
            Thrown::dispensed(Kind::ExperienceBottle, DVec3::new(0.2, 1.0, 0.5), DVec3::X, 1.0, 0.0, rng)
        };
        let creative = super::super::Target::new(PlayerId(2), DVec3::new(2.5, 0.0, 0.5), false);
        let mut t = throw(&mut rng);
        let (pos, _, _) = burst(&fly(&mut t, &floor, &mut [], std::slice::from_ref(&creative)));
        assert!(creative.contains(pos), "{pos}");
        for passive in
            [super::super::Target { spectator: true, ..creative }, super::super::Target { alive: false, ..creative }]
        {
            let mut t = throw(&mut rng);
            let (pos, toward, _) = burst(&fly(&mut t, &floor, &mut [], std::slice::from_ref(&passive)));
            assert!(!passive.contains(pos) && pos.y.abs() < 1e-9, "flew through to the floor: {pos}");
            // A shallow landing that crosses into the next column in the
            // same substep still hits the floor's top face.
            assert_eq!(toward, DVec3::Y, "{pos}");
        }
    }

    /// The burst reaches the world: `Entities::update` turns it into orbs
    /// worth 3 to 11 points a quarter block above the floor, and passes the
    /// splash on to the game.
    #[test]
    fn bursting_bottles_leave_experience_orbs_in_the_world() {
        struct Floor(Grid);
        impl BlockSource for Floor {
            fn block(&self, p: glam::IVec3) -> Option<crate::world::block::Block> {
                self.0.block(p)
            }
        }
        impl super::super::MobWorld for Floor {
            fn loaded(&self, _: glam::IVec3) -> bool {
                true
            }
            fn surface(&self, _: i32, _: i32) -> Option<i32> {
                Some(0)
            }
            fn exposed(&self, _: glam::IVec3) -> bool {
                true
            }
        }
        let world = Floor(Grid::flat(0));
        let ctx = super::super::Ctx {
            spawn_light: crate::simulation::lighting::SpawnLight::DAY,
            players: vec![],
            daylight: 1.0,
            spawning: false,
            raining: false,
            dimension: crate::world::terrain::Dimension::Overworld,
        };
        let mut totals = std::collections::BTreeSet::new();
        for seed in 0..40 {
            let mut e = super::super::Entities::new(seed);
            let mut rng = Rng::new(seed);
            e.thrown.push(Thrown::dispensed(
                Kind::ExperienceBottle,
                DVec3::new(0.5, 3.0, 0.5),
                DVec3::NEG_Y,
                0.3,
                0.0,
                &mut rng,
            ));
            let mut events = Vec::new();
            for _ in 0..40 {
                events = e.update(0.05, &world, &ctx);
                if e.thrown.is_empty() {
                    break;
                }
            }
            assert!(e.thrown.is_empty(), "the bottle broke");
            assert!(events.iter().any(|ev| matches!(ev, EntityEvent::PotionSplashed { colour: BOTTLE_COLOUR, .. })));
            assert!(!events.iter().any(|ev| matches!(ev, EntityEvent::ExperienceBurst { .. })), "handled here");
            let points: u32 = e.orbs.iter().map(|o| o.value * o.count).sum();
            assert!((3..=11).contains(&points), "{points}");
            assert!(e.orbs.iter().all(|o| (0.0..1.0).contains(&o.pos.y)), "{:?}", e.orbs);
            totals.insert(points);
        }
        assert!(totals.len() > 3, "{totals:?}");
    }

    #[test]
    fn rotation_heading_raises_only_the_vertical_part() {
        // Level aim: Java's yd = -sin(0 - 20 degrees).
        let h = heading_from_rotation(DVec3::Z, -20.0);
        assert!((h - DVec3::new(0.0, 20f64.to_radians().sin(), 1.0)).length() < 1e-12, "{h}");
        // Straight down: x and z stay 0, yd = -sin(90 - 20 degrees).
        let h = heading_from_rotation(DVec3::NEG_Y, -20.0);
        assert!(h.x.abs() < 1e-12 && h.z.abs() < 1e-12 && (h.y + 70f64.to_radians().sin()).abs() < 1e-9, "{h}");
    }

    /// v1.0 gap Track 8d: a snowball applies block effects
    /// (`ThrowableProjectile.tick`): through lava it catches fire, and its
    /// block hit (`onProjectileHit`) carries that, so the TNT it hits is
    /// primed; water puts it out.
    #[test]
    fn a_snowball_through_lava_burns_and_lights_what_it_hits() {
        use crate::world::block::Block;
        for (through, lit) in [(Block::LAVA, true), (Block::FIRE, true), (Block::WATER, false), (Block::AIR, false)] {
            let mut world = Grid::flat(0);
            world.set(glam::IVec3::new(2, 1, 0), through);
            world.set(glam::IVec3::new(6, 1, 0), Block::TNT);
            let mut rng = Rng::new(1);
            let mut t = Thrown::launch(
                Kind::Snowball,
                PlayerId::HOST,
                DVec3::new(0.5, 1.6, 0.5),
                DVec3::X,
                DVec3::ZERO,
                &mut rng,
            );
            if through == Block::WATER {
                t.fire = 100;
            }
            let (index, mut events) = (super::super::mob_index::MobIndex::default(), Vec::new());
            for _ in 0..10 {
                if !t.update(0.05, &world, &mut [], &index, &[], &Default::default(), &mut rng, &mut events) {
                    break;
                }
            }
            let hit = events.iter().find_map(|e| match e {
                EntityEvent::ProjectileBlockHit { cell, burning, .. } => Some((*cell, *burning)),
                _ => None,
            });
            assert_eq!(hit, Some((glam::IVec3::new(6, 1, 0), lit)), "through {through:?}");
        }
    }
}
