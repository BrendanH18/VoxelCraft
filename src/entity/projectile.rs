//! Arrows: ballistic projectiles that stick in blocks. Skeleton arrows hurt
//! the player; the player's own arrows hurt mobs and can be picked back up.

use glam::{DVec3, Vec3};

use super::{Ctx, EntityEvent, Mob, PlayerId, Rng};
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
    /// True even for an empty tipped arrow (no potion component).
    pub tipped: bool,
    pub potion: Option<crate::potion::Potion>,
    pub custom_effect: Option<(crate::simulation::effects::Effect, u8, u32)>,
    /// v0.15 Track D: a spectral arrow (`SpectralArrow`), which gives what
    /// it hits Glowing for 200 ticks and picks up as a spectral arrow.
    pub spectral: bool,
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
    /// The actual player who fired this arrow, independent of pickup rights.
    pub owner: Option<PlayerId>,
    /// The mob that shot it (`uid`), if any.
    pub shooter: Option<u32>,
    /// The breeze that turned it back, which it can't hit again; not saved.
    pub deflector: Option<u32>,
    /// A player's arrow in a world with PvP on: it can hit other players.
    pub pvp: bool,
    /// Death message for players hit by this projectile.
    cause: crate::simulation::damage::Cause,
    /// The player can collect it once stuck (not arrows shot in creative).
    pub pickup: bool,
    /// A fully drawn shot: deals a random bonus on hit.
    pub critical: bool,
    /// The bow's enchantments: power, punch and flame; a crossbow's piercing.
    pub enchants: crate::enchant::Enchants,
    /// v1.0 gap Track 8: ticks of fire besides Flame's (`remainingFireTicks`),
    /// from a perched dragon's scales (`AbstractDragonSittingPhase.onHurt`)
    /// and (Track 8b) fire and lava it passes; water and rain put it out.
    /// Not saved, like the arrow's flight.
    fire: u16,
    /// v1.0 gap Track 8b: water or rain put out its fire, Flame's too
    /// (Java's Flame is `igniteForSeconds(100)` at the shot, which
    /// `clearFire` ends like any other).
    doused: bool,
    /// v1.0 gap Track 8b: Java's `yRot` and `xRot` (degrees: yaw from
    /// `atan2(x, z)`, pitch up from level), which ease a fifth of the way
    /// toward the motion each tick in flight and turn 170 to 190° on a
    /// bounce; [`Arrow::dir`] points along them. `None` until the first
    /// tick, when they take the launch motion (`Projectile.shoot`).
    rot: Option<(f32, f32)>,
    /// Mobs (`Mob::uid`) this piercing arrow already passed through.
    pierced: Vec<u32>,
    /// Vehicles and decorations ([`ArrowTarget::key`]) it passed through.
    pierced_other: Vec<u64>,
    /// v0.15 Track A: a mob's arrow shot Java's way (a pillager's crossbow,
    /// an illusioner's bow): `AbstractArrow`'s base damage times its speed
    /// in blocks a tick, rounded up, instead of the older mob arrow's
    /// estimate.
    pub java_base: Option<f64>,
}

/// Something other than a mob an arrow can hit: a minecart, boat or
/// cushion (by id) or a decoration (by index this tick).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Struck {
    Cart(u32),
    Decor(usize),
}

/// A non-living entity's box for arrows this tick. `mob_proof`: it takes no
/// harm from a mob's arrow (paintings, item frames and armor stands with
/// mobGriefing off), which bounces back off it.
#[derive(Clone, Copy, Debug)]
pub struct ArrowTarget {
    pub struck: Struck,
    /// Stays the same from tick to tick, for piercing arrows.
    pub key: u64,
    /// Players riding it: their own arrows pass it (`Projectile.canHitEntity`).
    pub riders: [Option<PlayerId>; 2],
    pub min: DVec3,
    pub max: DVec3,
    pub mob_proof: bool,
}

impl Arrow {
    /// An arrow from `from` aimed to arc onto `target`, with a little spread.
    pub fn aimed(
        from: DVec3,
        target: DVec3,
        cause: impl Into<crate::simulation::damage::Cause>,
        rng: &mut Rng,
    ) -> Self {
        let d = target - from;
        // Lead the drop: over flight time t the arrow sinks g t² / 2.
        let t = d.length() / SPEED;
        let aim = d + DVec3::Y * (0.5 * GRAVITY * t * t);
        let spread = DVec3::new(rng.range(-1.0, 1.0) as f64, rng.range(-1.0, 1.0) as f64, rng.range(-1.0, 1.0) as f64);
        let vel = (aim.normalize_or(DVec3::X) + spread * 0.03) * SPEED;
        Self {
            tipped: false,
            potion: None,
            custom_effect: None,
            spectral: false,
            pos: from,
            previous_pos: from,
            vel,
            dir: vel.normalize().as_vec3(),
            age: 0.0,
            stuck: false,
            from_player: false,
            owner: None,
            shooter: None,
            deflector: None,
            pvp: false,
            cause: cause.into(),
            pickup: false,
            critical: false,
            enchants: Default::default(),
            fire: 0,
            doused: false,
            rot: None,
            pierced: Vec::new(),
            pierced_other: Vec::new(),
            java_base: None,
        }
    }

    /// An arrow loosed by the player from `eye` along `dir` with bow
    /// `power` 0..1.
    pub fn shot(eye: DVec3, dir: DVec3, power: f32, pickup: bool) -> Self {
        let dir = dir.normalize_or(DVec3::X);
        let vel = dir * BOW_SPEED * power as f64;
        Self {
            tipped: false,
            potion: None,
            custom_effect: None,
            spectral: false,
            pos: eye + dir * 0.3,
            previous_pos: eye + dir * 0.3,
            vel,
            dir: dir.as_vec3(),
            age: 0.0,
            stuck: false,
            from_player: true,
            owner: Some(PlayerId::HOST),
            shooter: None,
            deflector: None,
            pvp: false,
            cause: crate::simulation::damage::Cause::ARROW,
            pickup,
            critical: power >= 1.0,
            enchants: Default::default(),
            fire: 0,
            doused: false,
            rot: None,
            pierced: Vec::new(),
            pierced_other: Vec::new(),
            java_base: None,
        }
    }

    /// A recoverable dispenser arrow has no player owner. `vel` is its launch velocity in
    /// blocks per second (Java's 1.1 blocks per game tick plus the dispenser's inaccuracy).
    pub fn dispensed(pos: DVec3, vel: DVec3) -> Self {
        let mut arrow = Self::shot(pos, vel, 0.0, true);
        arrow.pos = pos;
        arrow.previous_pos = pos;
        arrow.vel = vel;
        arrow.critical = false;
        arrow.from_player = false;
        arrow.owner = None;
        arrow
    }

    /// A mob's arrow loosed Java's way from `from` at `vel` blocks a second
    /// (`Projectile.shoot` already applied), with base damage 2.
    pub fn mob_shot(from: DVec3, vel: DVec3, cause: crate::simulation::damage::Cause, shooter: u32) -> Self {
        let mut arrow = Self::shot(from, vel, 0.0, false);
        arrow.pos = from;
        arrow.previous_pos = from;
        arrow.vel = vel;
        arrow.dir = vel.normalize_or(DVec3::X).as_vec3();
        arrow.critical = false;
        arrow.from_player = false;
        arrow.owner = None;
        arrow.shooter = Some(shooter);
        arrow.cause = cause;
        arrow.java_base = Some(2.0);
        arrow
    }

    /// A mob arrow's damage at `speed` blocks a second: Java's
    /// `ceil(speed per tick * base damage)` for [`Arrow::mob_shot`] arrows.
    fn mob_damage(&self, speed: f64) -> f32 {
        match self.java_base {
            Some(base) => (speed / crate::knockback::TICK_RATE * base * self.power_factor()).ceil() as f32,
            None => (speed / SPEED * 3.0 * self.power_factor()).ceil() as f32,
        }
    }

    /// Power raises Java's base arrow damage of 2 by 0.5 per level + 0.5.
    fn power_factor(&self) -> f64 {
        let power = self.enchants.level(crate::enchant::Enchantment::Power) as f64;
        if power > 0.0 { (2.5 + 0.5 * power) / 2.0 } else { 1.0 }
    }

    /// Moves the arrow; returns `false` once it should be removed.
    #[allow(clippy::too_many_arguments)]
    pub(super) fn update<W: super::MobWorld + ?Sized>(
        &mut self,
        dt: f64,
        world: &W,
        ctx: &Ctx,
        mobs: &mut [Mob],
        others: &[ArrowTarget],
        mut boss: Option<&mut super::dragon::Targets<'_>>,
        rng: &mut Rng,
        events: &mut Vec<EntityEvent>,
    ) -> bool {
        self.age += dt as f32;
        // `AbstractArrow.tick`: `isInWaterOrRain` puts the fire out.
        if in_water_or_rain(world, self.pos, ARROW_HALF, ARROW_HEIGHT) {
            self.douse();
        }
        if self.stuck {
            // `applyEffectsFromBlocks` each tick in the ground: the part of
            // its box out of the block can touch fire, lava or water.
            self.touch_blocks(world, self.pos, self.pos);
            // Arrows fall out when the block they're in is broken.
            if !world.block(self.pos.floor().as_ivec3()).is_some_and(|b| b.is_solid()) {
                self.stuck = false;
                self.vel = DVec3::ZERO;
                return true;
            }
            // Arrow.tick clears potion contents at 600 in-ground ticks.
            if self.age >= 30.0 - 1e-3 && (self.potion.is_some() || self.custom_effect.is_some()) {
                self.potion = None;
                self.custom_effect = None;
                self.tipped = false;
            }
            return self.age < if self.from_player || self.pickup { PLAYER_STUCK_TIME } else { STUCK_TIME };
        }
        // `AbstractArrow.tick`: the look eases toward the motion before it moves.
        self.turn_toward_motion();
        self.vel.y -= GRAVITY * dt;
        let delta = self.vel * dt;
        let steps = (delta.length() / STEP).ceil().max(1.0) as u32;
        // `Entity.baseTick` (in flight only) burns the fire down a tick at a time.
        self.fire = self.fire.saturating_sub((dt * 20.0).round() as u16);
        for _ in 0..steps {
            let from = self.pos;
            let step = delta / steps as f64;
            self.pos += step;
            // `stepMoveAndHit`: `applyEffectsFromBlocks` along the move,
            // before what it hits.
            self.touch_blocks(world, from, self.pos);
            // A block the step crosses (even at a corner) stops the arrow,
            // and hides anything behind it.
            let block = first_solid(world, from, step);
            let reach = block.unwrap_or(1.0);
            if self.from_player || self.pickup || self.shooter.is_some() {
                // Whatever the step enters first: a mob, or a crystal or
                // dragon part (fractions of the step).
                let mob = mobs
                    .iter()
                    .enumerate()
                    .filter(|(_, m)| {
                        m.alive()
                            && self.shooter != Some(m.uid)
                            && self.deflector != Some(m.uid)
                            && !self.pierced.contains(&m.uid)
                    })
                    .filter_map(|(i, m)| {
                        let (min, max) = m.aabb();
                        crate::physics::ray_aabb(from, step, min, max).filter(|&t| t <= reach).map(|t| (i, t))
                    })
                    .min_by(|a, b| a.1.total_cmp(&b.1));
                let boss_hit = boss.as_deref().and_then(|b| b.raycast(from, step, reach));
                // `AbstractArrow.onHitEntity` on a vehicle, cushion or
                // decoration nearer than any mob or dragon part: it takes the
                // arrow's damage and the arrow is gone, unless it pierces.
                let pierce = usize::from(self.enchants.level(crate::enchant::Enchantment::Piercing));
                let other = others
                    .iter()
                    .filter(|o| !self.pierced_other.contains(&o.key))
                    // `Projectile.canHitEntity`: not the vehicle carrying the shooter.
                    .filter(|o| !(self.from_player && self.owner.is_some() && o.riders.contains(&self.owner)))
                    .filter(|o| {
                        let Struck::Cart(id) = o.struck else { return true };
                        self.shooter.is_none_or(|uid| !mobs.iter().any(|m| m.uid == uid && m.riding == Some(id)))
                    })
                    .filter_map(|o| {
                        crate::physics::ray_aabb(from, step, o.min, o.max).filter(|&t| t <= reach).map(|t| (o, t))
                    })
                    .min_by(|a, b| a.1.total_cmp(&b.1));
                'hit: {
                    if let Some((o, t)) = other
                        && mob.is_none_or(|(_, m)| t < m)
                        && boss_hit.as_ref().is_none_or(|(_, b)| t < *b)
                    {
                        if o.mob_proof && self.shooter.is_some() {
                            // `hurtOrSimulate` refused: the arrow bounces back off it
                            // (`deflect(REVERSE, .., 0.2)`, a tenth of its speed), from
                            // outside its box so it doesn't hit it again.
                            self.pos = from;
                            return self.bounce(rng, events);
                        }
                        // Piercing counts every entity passed through, mobs or not.
                        if pierce > 0 {
                            if self.pierced.len() + self.pierced_other.len() > pierce {
                                return false;
                            }
                            self.pierced_other.push(o.key);
                        }
                        let damage = if self.shooter.is_some() {
                            if self.java_base.is_some() {
                                self.mob_damage(self.vel.length())
                            } else {
                                (self.vel.length() / SPEED * 3.0).ceil() as f32
                            }
                        } else {
                            let mut damage =
                                (self.vel.length() / BOW_SPEED * BOW_DAMAGE * self.power_factor()).ceil() as f32;
                            if self.critical {
                                damage += (rng.next_f32() * (damage / 2.0 + 1.0)).floor();
                            }
                            damage
                        };
                        events.push(EntityEvent::EntityShot {
                            struck: o.struck,
                            pos: from + step * t,
                            damage,
                            burning: self.burning(),
                            arrow: true,
                            owner: if self.from_player { self.owner } else { None },
                            motion: self.vel,
                        });
                        if pierce == 0 {
                            return false;
                        }
                        break 'hit;
                    }
                    if let Some((hit, _)) = boss_hit.filter(|&(_, t)| mob.is_none_or(|(_, m)| t < m))
                        && let Some(boss) = boss.as_deref_mut()
                    {
                        let mut damage =
                            (self.vel.length() / BOW_SPEED * BOW_DAMAGE * self.power_factor()).ceil() as f32;
                        if self.critical {
                            damage += (rng.next_f32() * (damage / 2.0 + 1.0)).floor();
                        }
                        // `AbstractArrow.onHitEntity`: the damage source's
                        // entity is the owner (a dispenser's arrow has
                        // none), and only a player's arrow takes health; a
                        // skeleton's lands and is gone.
                        let by = if self.from_player { self.owner } else { None };
                        // v1.0 gap Track 8b: a mob's arrow names its mob in
                        // a crystal's blast.
                        let shooter = self
                            .shooter
                            .and_then(|uid| mobs.iter().find(|m| m.uid == uid && m.alive()).map(|m| m.kind));
                        let blow = super::dragon::Blow::arrow(damage, by.is_some()).blamed_on(shooter);
                        // v1.0 gap Track 8: a perched dragon's scales set the
                        // arrow alight for a second (`igniteForSeconds(1)`).
                        let scorched = boss.scorches(hit, blow);
                        if boss.strike(hit, blow, by) {
                            return false;
                        }
                        if scorched {
                            self.fire = self.fire.max(20);
                        }
                        // Refused (a perched dragon's scales, or a dying
                        // dragon, which nothing hurts): `deflect(REVERSE,
                        // .., 0.2)` sends it back at a tenth of its speed,
                        // from outside the part (Java leaves it where it met
                        // the box) so it doesn't meet it again.
                        self.pos = from;
                        return self.bounce(rng, events);
                    }
                    if let Some((i, _)) = mob {
                        let mob = &mut mobs[i];
                        if self.from_player
                            && mob.allay.as_ref().is_some_and(|s| s.player == self.owner && self.owner.is_some())
                        {
                            return false;
                        }
                        // `Breeze.deflection`: `ProjectileDeflection.REVERSE` before any damage.
                        if mob.kind == super::MobKind::Breeze {
                            self.vel *= -0.5;
                            self.turn_back(rng);
                            self.deflector = Some(mob.uid);
                            events.push(EntityEvent::Sound {
                                sound: super::MobSound::Breeze(super::breeze::Cue::Deflect),
                                pos: mob.pos,
                            });
                            return true;
                        }
                        // Endermen dodge arrows by teleporting; the arrow flies on.
                        if mob.kind == super::MobKind::Enderman {
                            mob.teleport_pending = true;
                        } else {
                            use crate::enchant::Enchantment;
                            // `AbstractArrow.onHitEntity`: a piercing arrow passes through
                            // `pierce level + 1` mobs, and the next one discards it unhurt.
                            if pierce > 0 {
                                if self.pierced.len() + self.pierced_other.len() > pierce {
                                    return false;
                                }
                                self.pierced.push(mob.uid);
                            }
                            let speed = self.vel.length();
                            let mut damage = (speed / BOW_SPEED * BOW_DAMAGE * self.power_factor()).ceil() as f32;
                            if self.critical {
                                damage += (rng.next_f32() * (damage / 2.0 + 1.0)).floor();
                            }
                            // Punch adds 0.6 blocks/tick of shove per level.
                            let punch = 1.0 + 3.0 * self.enchants.level(Enchantment::Punch) as f64;
                            let push = DVec3::new(self.vel.x, 0.0, self.vel.z).normalize_or_zero() * 4.0 * punch
                                + DVec3::Y * 4.0;
                            // `onHitEntity` undoes Flame and skips `doPostHurtEffects`
                            // (the potion) when `hurtOrSimulate` refuses the hit.
                            let refused = mob.refuses_hit(damage);
                            if self.burning() && !refused {
                                mob.ignite(5.0);
                            }
                            if self.from_player {
                                match self.owner {
                                    Some(owner) => mob.player_hit_by(owner),
                                    None => mob.player_hit(),
                                }
                            }
                            if let Some(attacker) = self.shooter {
                                if !refused {
                                    self.effects_on_mob(mob, rng);
                                }
                                // Resolve through the shared mob-on-mob path so
                                // retaliation and loot attribution retain the shooter.
                                events.push(EntityEvent::MobHit {
                                    target: mob.uid,
                                    attacker,
                                    damage: self.mob_damage(speed),
                                    knockback: push,
                                    melee: false,
                                });
                                return false;
                            }
                            // `hurtServer`'s 0.4 push along the arrow, then Punch's shove (`doKnockback`).
                            let motion = self.vel / crate::knockback::TICK_RATE;
                            let killed =
                                mob.damage_projectile(damage, &[crate::knockback::Push::projectile(motion)], rng);
                            if !killed && !refused {
                                self.effects_on_mob(mob, rng);
                            }
                            let killed = !mob.alive();
                            mob.shove_punch(u32::from(self.enchants.level(Enchantment::Punch)), motion);
                            events.push(EntityEvent::MobShot {
                                kind: mob.kind,
                                pos: mob.pos,
                                killed,
                                burning: mob.burning,
                                player_kill: self.from_player,
                                owner: self.owner,
                            });
                            if pierce == 0 {
                                return false;
                            }
                        }
                    }
                }
            }
            if let Some(t) = block {
                // Stuck just inside the block it hit.
                self.pos = from + step * t + step.normalize_or_zero() * 0.01;
                self.stuck = true;
                let cell = self.pos.floor().as_ivec3();
                let hit = from + step * t;
                let local = hit - cell.as_dvec3();
                let axis = (0..3)
                    .min_by(|&a, &b| {
                        local[a]
                            .abs()
                            .min((1.0 - local[a]).abs())
                            .total_cmp(&local[b].abs().min((1.0 - local[b]).abs()))
                    })
                    .unwrap();
                let mut normal = glam::IVec3::ZERO;
                normal[axis] = if local[axis] < 0.5 { -1 } else { 1 };
                // `AbstractCandleBlock.onProjectileHit`: a burning arrow
                // lights a candle; (v1.0 gap Track 8b) `CampfireBlock`'s and
                // `TntBlock`'s light a campfire and prime TNT.
                let burning = self.burning();
                if world.block(cell).is_some_and(|b| {
                    matches!(
                        crate::world::redstone_blocks::component(b),
                        Some(crate::world::redstone_blocks::Component::Target(_))
                    ) || matches!(
                        b.base(),
                        crate::world::block::Block::REDSTONE_ORE | crate::world::block::Block::DEEPSLATE_REDSTONE_ORE
                    ) || burning && lights(b)
                        // v0.16 Track A: `ChorusFlowerBlock.onProjectileHit`.
                        || crate::world::end_blocks::flower_age(b).is_some()
                }) {
                    events.push(EntityEvent::ProjectileBlockHit {
                        cell,
                        pos: hit,
                        normal,
                        arrow: true,
                        burning,
                        mob_owned: self.shooter.is_some(),
                    });
                }
                self.age = 0.0;
                return true;
            }
            if self.from_player && !self.pvp {
                continue;
            }
            if let Some(hit) = ctx
                .players
                .iter()
                .find(|t| t.targetable && t.contains(self.pos) && !(self.from_player && Some(t.id) == self.owner))
            {
                let (damage, push) = if self.from_player {
                    // Another player's bow: the same damage, criticals, Punch and Flame as on mobs.
                    use crate::enchant::Enchantment;
                    let mut damage = (self.vel.length() / BOW_SPEED * BOW_DAMAGE * self.power_factor()).ceil() as f32;
                    if self.critical {
                        damage += (rng.next_f32() * (damage / 2.0 + 1.0)).floor();
                    }
                    let punch = 1.0 + 3.0 * self.enchants.level(Enchantment::Punch) as f64;
                    if self.burning() {
                        events.push(EntityEvent::Ignite { player: hit.id, secs: 5.0 });
                    }
                    (damage, DVec3::new(self.vel.x, 0.0, self.vel.z).normalize_or_zero() * 3.0 * punch + DVec3::Y * 3.0)
                } else {
                    if self.burning() {
                        events.push(EntityEvent::Ignite { player: hit.id, secs: 5. });
                    }
                    (
                        self.mob_damage(self.vel.length()),
                        DVec3::new(self.vel.x, 0.0, self.vel.z).normalize_or_zero() * 3.0 + DVec3::Y * 3.0,
                    )
                };
                let punch = u32::from(self.enchants.level(crate::enchant::Enchantment::Punch));
                events.push(EntityEvent::PlayerHit {
                    java: Some(crate::knockback::Hit::arrow(self.vel / crate::knockback::TICK_RATE, punch)),
                    player: hit.id,
                    damage,
                    knockback: push.as_vec3(),
                    cause: if self.from_player { crate::simulation::damage::Cause::PLAYER_ARROW } else { self.cause },
                });
                if self.potion.is_some() || self.custom_effect.is_some() || self.spectral {
                    events.push(EntityEvent::ArrowEffectsStart { player: hit.id });
                    self.effects_on_player(hit.id, events);
                    events.push(EntityEvent::ArrowEffectsEnd);
                }
                if let Some(attacker) = self.shooter {
                    events.push(EntityEvent::OwnerHurt { player: hit.id, attacker });
                }
                return false;
            }
        }
        self.age < MAX_FLIGHT && self.pos.y > -64.0
    }

    /// `AbstractArrow.tick`'s `lerpRotation`s: a fifth of the way from the
    /// look toward the motion (yaw by the short way round), which a
    /// first tick takes outright.
    fn turn_toward_motion(&mut self) {
        if self.vel.length_squared() < 1e-12 {
            return;
        }
        let (yaw, pitch) = rotation_of(self.vel);
        let (y0, p0) = self.rot.unwrap_or((yaw, pitch));
        self.rot = Some((lerp_rotation(y0, yaw), lerp_rotation(p0, pitch)));
        self.point_along_rotation();
    }

    /// `ProjectileDeflection.REVERSE`'s turn: the yaw by 170 to 190°, the
    /// pitch kept (the motion is the caller's).
    fn turn_back(&mut self, rng: &mut Rng) {
        let (yaw, pitch) = self.rot.unwrap_or_else(|| rotation_of(self.dir.as_dvec3()));
        self.rot = Some((yaw + reverse_turn(rng), pitch));
        self.point_along_rotation();
    }

    fn point_along_rotation(&mut self) {
        if let Some(rot) = self.rot {
            self.dir = pointing(rot);
        }
    }

    /// `AbstractArrow.onHitEntity` on a hit refused: `deflect(REVERSE, ..,
    /// 0.2)`, back at a tenth of its speed and turned about, and left all
    /// but still (under 1e-7 blocks² a tick) it drops as an item if it may
    /// be picked up, and goes. Returns whether it stays.
    fn bounce(&mut self, rng: &mut Rng, events: &mut Vec<EntityEvent>) -> bool {
        self.vel = -self.vel * 0.1;
        self.turn_back(rng);
        if (self.vel / crate::knockback::TICK_RATE).length_squared() < 1e-7 {
            if self.pickup {
                let pos = self.pos + DVec3::Y * 0.1;
                events.push(EntityEvent::DropEquipment { stack: self.pickup_stack(), pos });
            }
            return false;
        }
        true
    }

    /// `clearFire` (water, rain): Flame's fire goes too.
    fn douse(&mut self) {
        self.fire = 0;
        self.doused = true;
    }

    /// `Entity.applyEffectsFromBlocks` for the box moving `from` to `to`:
    /// fire and lava set it alight, water then puts it out, and so does rain.
    fn touch_blocks<W: super::MobWorld + ?Sized>(&mut self, world: &W, from: DVec3, to: DVec3) {
        let touched = touched_blocks(world, from, to, ARROW_HALF, ARROW_HEIGHT);
        if touched.apply(&mut self.fire) || in_rain(world, to, ARROW_HEIGHT) {
            self.douse();
        }
    }

    /// Current velocity in blocks per second.
    pub fn velocity(&self) -> DVec3 {
        self.vel
    }

    /// On fire (`isOnFire`): a Flame bow's until water or rain puts it
    /// out, or set alight since it was shot.
    pub fn burning(&self) -> bool {
        self.fire > 0 || !self.doused && self.enchants.has(crate::enchant::Enchantment::Flame)
    }

    /// A piercing arrow needs the mobs' ids to remember what it passed through.
    pub(super) fn piercing(&self) -> bool {
        self.enchants.has(crate::enchant::Enchantment::Piercing)
    }

    pub fn is_stuck(&self) -> bool {
        self.stuck
    }
}

/// v1.0 gap Track 8b: a block a burning projectile's hit lights or
/// primes: a candle or candle cake, a campfire, TNT.
pub(super) fn lights(b: crate::world::block::Block) -> bool {
    crate::world::colour_blocks::candle::can_light(b)
        || crate::world::utilities::campfire_of(b).is_some()
        || b == crate::world::block::Block::TNT
}

/// `EntityTypes.ARROW`'s box: 0.5 × 0.5.
const ARROW_HALF: f64 = 0.25;
const ARROW_HEIGHT: f64 = 0.5;

/// `Projectile.lerpRotation`: from `from` a fifth of the way to `to`, the
/// short way round (`from` moved by whole turns first).
pub(super) fn lerp_rotation(mut from: f32, to: f32) -> f32 {
    while to - from < -180.0 {
        from -= 360.0;
    }
    while to - from >= 180.0 {
        from += 360.0;
    }
    from + 0.2 * (to - from)
}

/// An `AbstractArrow`'s `yRot` and `xRot` for motion `v`: Java degrees,
/// yaw `atan2(x, z)`, pitch up from level.
pub(super) fn rotation_of(v: DVec3) -> (f32, f32) {
    (v.x.atan2(v.z).to_degrees() as f32, v.y.atan2(v.x.hypot(v.z)).to_degrees() as f32)
}

/// The way an `AbstractArrow` with `(yRot, xRot)` points.
pub(super) fn pointing((yaw, pitch): (f32, f32)) -> Vec3 {
    let (yaw, pitch) = (f64::from(yaw).to_radians(), f64::from(pitch).to_radians());
    DVec3::new(yaw.sin() * pitch.cos(), pitch.sin(), yaw.cos() * pitch.cos()).as_vec3()
}

/// `ProjectileDeflection.REVERSE`'s turn of the yaw: 170 to 190°.
pub(super) fn reverse_turn(rng: &mut Rng) -> f32 {
    170.0 + rng.next_f32() * 20.0
}

/// v1.0 gap Track 8b: the fire-related blocks a projectile's box (`half`
/// each way across, `height` tall) touched moving `from` to `to`
/// (`checkInsideBlocks`, every cell is a full block to it: fire's, lava's
/// and water's `getEntityInsideCollisionShape`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(super) struct Touched {
    pub fire: bool,
    pub lava: bool,
    pub water: bool,
}

impl Touched {
    /// The `InsideBlockEffectType`s in Java's order on `fire` ticks:
    /// `FIRE_IGNITE` (`igniteForSeconds(8)`), `LAVA_IGNITE`
    /// (`igniteForSeconds(15)`), then `EXTINGUISH`. Returns whether water
    /// put it out.
    pub fn apply(self, fire: &mut u16) -> bool {
        if self.fire {
            *fire = (*fire).max(160);
        }
        if self.lava {
            *fire = (*fire).max(300);
        }
        if self.water {
            *fire = 0;
        }
        self.water
    }
}

/// The cells the box sweeps, sampled every quarter block along the move.
pub(super) fn touched_blocks<W: BlockSource + ?Sized>(
    world: &W,
    from: DVec3,
    to: DVec3,
    half: f64,
    height: f64,
) -> Touched {
    let mut out = Touched::default();
    let samples = ((to - from).length() / 0.25).ceil().max(1.0) as u32;
    for i in 0..=samples {
        let at = from.lerp(to, f64::from(i) / f64::from(samples));
        let (min, max) = (at - DVec3::new(half, 0.0, half), at + DVec3::new(half, height, half));
        let (lo, hi) = (min.floor().as_ivec3(), (max - DVec3::splat(1e-7)).floor().as_ivec3());
        for y in lo.y..=hi.y {
            for z in lo.z..=hi.z {
                for x in lo.x..=hi.x {
                    let Some(b) = world.block(glam::IVec3::new(x, y, z)) else { continue };
                    out.fire |= b.is_fire();
                    out.lava |= b.is_lava();
                    out.water |= b.holds_water();
                }
            }
        }
    }
    out
}

/// `Entity.isInRain`: rain on its cell or the cell at the top of its box.
pub(super) fn in_rain<W: super::MobWorld + ?Sized>(world: &W, pos: DVec3, height: f64) -> bool {
    let cell = pos.floor().as_ivec3();
    world.rains_on(cell) || world.rains_on(glam::IVec3::new(cell.x, (pos.y + height).floor() as i32, cell.z))
}

/// `Entity.isInWaterOrRain` at the start of a tick: its box touches
/// water, or rain falls on it.
pub(super) fn in_water_or_rain<W: super::MobWorld + ?Sized>(world: &W, pos: DVec3, half: f64, height: f64) -> bool {
    touched_blocks(world, pos, pos, half, height).water || in_rain(world, pos, height)
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
                let Some(block) = world.block(cell).filter(|b| b.is_solid()) else { continue };
                // Candles, candle cakes and straw beds hit by their Java
                // collision shape (`getShape`), not their drawn boxes.
                let boxes = if let Some(boxes) = crate::world::colour_blocks::collision(block) {
                    boxes
                } else if block.shaped().is_some() {
                    crate::world::shape::shape(
                        block,
                        |f| world.block(cell + f.offset()).unwrap_or(crate::world::block::Block::AIR),
                        world.block(cell - glam::IVec3::Y).unwrap_or(crate::world::block::Block::AIR),
                    )
                } else {
                    crate::world::shape::Boxes::from_box(crate::world::shape::Box16::FULL)
                };
                for bx in boxes.as_slice() {
                    let min = cell.as_dvec3() + DVec3::from_array(bx.min.map(|v| v as f64 / 16.0));
                    let max = cell.as_dvec3() + DVec3::from_array(bx.max.map(|v| v as f64 / 16.0));
                    if let Some(t) = crate::physics::ray_aabb(from, step, min, max).filter(|&t| t <= 1.0) {
                        first = Some(first.map_or(t, |f: f64| f.min(t)));
                    }
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
    fn mob_arrows_hit_other_mobs_skip_the_shooter_and_keep_mob_credit() {
        let world = Grid::flat(0);
        let ctx = Ctx {
            spawn_light: crate::simulation::lighting::SpawnLight::DAY,
            players: vec![],
            daylight: 1.,
            spawning: false,
            raining: false,
            dimension: Dimension::Nether,
        };
        let mut e = crate::entity::Entities::new(3);
        e.spawn(MobKind::Piglin, DVec3::new(0.5, 1., 0.5));
        e.spawn(MobKind::Cow, DVec3::new(5.5, 1., 0.5));
        e.ensure_mob_ids();
        let (shooter, victim) = (e.mobs[0].uid, e.mobs[1].uid);
        let before = e.mobs[0].health;
        let mut arrow =
            Arrow::aimed(e.mobs[0].pos + DVec3::Y, e.mobs[1].pos + DVec3::Y, "was shot by a piglin", &mut e.rng);
        arrow.shooter = Some(shooter);
        let mut events = Vec::new();
        for _ in 0..20 {
            if !arrow.update(0.05, &world, &ctx, &mut e.mobs, &[], None, &mut e.rng, &mut events) {
                break;
            }
        }
        assert!(events.iter().any(
            |ev| matches!(ev, EntityEvent::MobHit { target, attacker, .. } if *target == victim && *attacker == shooter)
        ));
        e.resolve_mob_hits(&mut events);
        assert_eq!(e.mobs[0].health, before);
        assert!(e.mobs[1].health < MobKind::Cow.max_health());
        assert!(e.orbs.is_empty(), "mob shots award no player kill XP");
    }

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
            spawn_light: crate::simulation::lighting::SpawnLight::DAY,
            players: vec![Target::new(PlayerId::HOST, DVec3::new(50.0, 10.0, 0.0), true)],
            daylight: 1.0,
            spawning: false,
            raining: false,
            dimension: Dimension::Overworld,
        };
        let mut events = Vec::new();
        assert!(arrow.update(dt, &world, &ctx, &mut mobs, &[], None, &mut Rng::new(1), &mut events));
        assert!(arrow.is_stuck());
        assert_eq!(arrow.pos.floor().as_ivec3(), IVec3::new(0, 20, 0));
        assert!(events.is_empty(), "{events:?}");
        assert_eq!(mobs[0].health, MobKind::Cow.max_health());
    }
    #[test]
    fn only_a_flame_arrow_lights_the_candle_it_hits() {
        use crate::world::colour_blocks::candle;
        let ctx = Ctx {
            spawn_light: crate::simulation::lighting::SpawnLight::DAY,
            players: vec![Target::new(PlayerId::HOST, DVec3::new(50.0, 10.0, 0.0), true)],
            daylight: 1.0,
            spawning: false,
            raining: false,
            dimension: Dimension::Overworld,
        };
        // Two candles leave a gap at the block's centre between their bodies,
        // but Java's collision box for them covers it.
        for (flame, count) in [(false, 1), (true, 1), (true, 2)] {
            let mut world = Grid::flat(10);
            world.set(IVec3::new(0, 11, 0), candle::candle(3, count, false, false));
            let mut arrow = Arrow::shot(DVec3::new(0.5, 14.0, 0.5), DVec3::NEG_Y, 1.0, true);
            if flame {
                arrow.enchants.set(crate::enchant::Enchantment::Flame, 1);
            }
            let mut events = Vec::new();
            for _ in 0..40 {
                if !arrow.update(0.05, &world, &ctx, &mut [], &[], None, &mut Rng::new(1), &mut events)
                    || arrow.is_stuck()
                {
                    break;
                }
            }
            assert!(arrow.is_stuck());
            assert_eq!(arrow.pos.floor().as_ivec3(), IVec3::new(0, 11, 0), "stuck in the candle");
            let lit = events.iter().any(|e| {
                matches!(e, EntityEvent::ProjectileBlockHit { cell, burning: true, .. } if *cell == IVec3::new(0, 11, 0))
            });
            assert_eq!(lit, flame, "{events:?}");
        }
    }

    #[test]
    fn dispenser_arrows_hurt_mobs_and_players_without_player_kill_credit() {
        let world = Grid::flat(10);
        for player in [false, true] {
            let mut arrow = Arrow::dispensed(DVec3::new(0.5, 11.0, 0.5), DVec3::X * 22.0);
            assert!((arrow.vel.length() - 22.0).abs() < 0.001);
            let mut mobs =
                if player { Vec::new() } else { vec![Mob::new(MobKind::Cow, DVec3::new(2.0, 10.0, 0.5), 0.0)] };
            let ctx = Ctx {
                spawn_light: crate::simulation::lighting::SpawnLight::DAY,
                players: vec![Target::new(
                    PlayerId::HOST,
                    DVec3::new(if player { 2.0 } else { 50.0 }, 10.0, 0.5),
                    true,
                )],
                daylight: 1.0,
                spawning: false,
                raining: false,
                dimension: Dimension::Overworld,
            };
            let mut events = Vec::new();
            let mut rng = Rng::new(7);
            for _ in 0..20 {
                if !arrow.update(0.05, &world, &ctx, &mut mobs, &[], None, &mut rng, &mut events) {
                    break;
                }
            }
            if player {
                assert!(events.iter().any(|e| matches!(
                    e,
                    EntityEvent::PlayerHit { cause: crate::simulation::damage::Cause::ARROW, .. }
                )));
            } else {
                assert!(events.iter().any(|e| matches!(e, EntityEvent::MobShot { player_kill: false, .. })));
                assert!(mobs[0].health < MobKind::Cow.max_health());
            }
        }
    }
    /// v0.15 integration review: a raider killed only by a player's arrow
    /// still names that player, so a ranged raid win crowns the hero.
    #[test]
    fn a_player_arrow_marks_the_raider_it_hits_with_its_owner() {
        let world = Grid::flat(10);
        let mut arrow = Arrow::shot(DVec3::new(0.5, 11.5, 0.5), DVec3::X, 1.0, true);
        arrow.owner = Some(PlayerId::HOST);
        let mut mobs = vec![Mob::new(MobKind::Pillager, DVec3::new(3.0, 10.0, 0.5), 0.0)];
        let ctx = Ctx {
            spawn_light: crate::simulation::lighting::SpawnLight::DAY,
            players: vec![Target::new(PlayerId::HOST, DVec3::new(-50.0, 10.0, 0.5), true)],
            daylight: 1.0,
            spawning: false,
            raining: false,
            dimension: Dimension::Overworld,
        };
        let mut events = Vec::new();
        let mut rng = Rng::new(7);
        for _ in 0..20 {
            if !arrow.update(0.05, &world, &ctx, &mut mobs, &[], None, &mut rng, &mut events) {
                break;
            }
        }
        assert!(mobs[0].health < MobKind::Pillager.max_health(), "the arrow hit");
        assert!(mobs[0].player_hit_left > 0.0);
        assert_eq!(mobs[0].illager.as_ref().unwrap().raider.last_player, Some(PlayerId::HOST));
    }

    fn sky_ctx() -> Ctx {
        Ctx {
            spawn_light: crate::simulation::lighting::SpawnLight::DAY,
            players: vec![Target::new(PlayerId::HOST, DVec3::new(50.0, 10.0, 0.0), true)],
            daylight: 1.0,
            spawning: false,
            raining: false,
            dimension: Dimension::Overworld,
        }
    }

    /// A test world where rain falls everywhere.
    struct Rainy(Grid);

    impl BlockSource for Rainy {
        fn block(&self, p: IVec3) -> Option<Block> {
            self.0.block(p)
        }
    }

    impl crate::entity::MobWorld for Rainy {
        fn loaded(&self, _: IVec3) -> bool {
            true
        }
        fn surface(&self, _: i32, _: i32) -> Option<i32> {
            Some(self.0.floor_y - 1)
        }
        fn exposed(&self, p: IVec3) -> bool {
            p.y >= self.0.floor_y
        }
        fn rains_on(&self, _: IVec3) -> bool {
            true
        }
    }

    /// v1.0 gap Track 8b: `AbstractArrow.tick`'s look eases a fifth of the
    /// way toward the motion each tick in flight, so a falling arrow's
    /// nose lags its path.
    #[test]
    fn an_arrows_look_eases_toward_its_motion() {
        let world = Grid::flat(0);
        let mut a = Arrow::shot(DVec3::new(0.5, 50.0, 0.5), DVec3::X, 1.0, true);
        let (mut rng, mut events) = (Rng::new(1), Vec::new());
        assert!(a.update(0.05, &world, &sky_ctx(), &mut [], &[], None, &mut rng, &mut events));
        assert_eq!(a.rot, Some((90.0, 0.0)), "the first tick takes the launch motion");
        for _ in 0..5 {
            let (_, before) = a.rot.unwrap();
            let (_, want) = rotation_of(a.velocity());
            assert!(a.update(0.05, &world, &sky_ctx(), &mut [], &[], None, &mut rng, &mut events));
            let (_, pitch) = a.rot.unwrap();
            assert!((pitch - (before + 0.2 * (want - before))).abs() < 1e-4, "{pitch} from {before} toward {want}");
        }
        let (_, path) = rotation_of(a.velocity());
        assert!(a.rot.unwrap().1 > path, "the nose lags the falling path");
        assert!((a.dir.as_dvec3() - pointing(a.rot.unwrap()).as_dvec3()).length() < 1e-6);
    }

    /// v1.0 gap Track 8b: an arrow refused by what it hits
    /// (`deflect(REVERSE, .., 0.2)`) goes back at a tenth of its speed with
    /// its yaw turned 170 to 190°, then eases round toward its new motion.
    #[test]
    fn a_bounced_arrow_turns_about_and_eases_round() {
        let world = Grid::flat(0);
        let mut mobs = vec![Mob::new(MobKind::Skeleton, DVec3::new(-5.0, 49.0, 0.5), 0.0)];
        mobs[0].uid = 9;
        let painting = ArrowTarget {
            struck: Struck::Decor(0),
            key: 1,
            riders: [None; 2],
            min: DVec3::new(4.0, 49.0, 0.0),
            max: DVec3::new(4.1, 51.0, 1.0),
            mob_proof: true,
        };
        let cause = crate::simulation::damage::Cause::SKELETON_ARROW;
        let (mut rng, mut events) = (Rng::new(3), Vec::new());
        let mut a = Arrow::mob_shot(DVec3::new(0.5, 50.0, 0.5), DVec3::X * 30.0, cause, 9);
        let mut before = a.velocity();
        for _ in 0..10 {
            before = a.velocity();
            assert!(a.update(0.05, &world, &sky_ctx(), &mut mobs, &[painting], None, &mut rng, &mut events));
            if a.velocity().x < 0.0 {
                break;
            }
        }
        assert!((a.velocity().x + before.x * 0.1).abs() < 1e-9, "a tenth of {before} back: {}", a.velocity());
        let (yaw, _) = a.rot.unwrap();
        assert!((90.0 + 170.0..=90.0 + 190.0).contains(&yaw), "turned about: {yaw}");
        let (target, _) = rotation_of(a.velocity());
        assert!(a.update(0.05, &world, &sky_ctx(), &mut mobs, &[painting], None, &mut rng, &mut events));
        let (eased, _) = a.rot.unwrap();
        let want = lerp_rotation(yaw, target);
        assert!((eased - want).abs() < 1e-3, "eased from {yaw} toward {target}: {eased}");
    }

    /// v1.0 gap Track 8b: fire sets an arrow alight for 8 seconds and lava
    /// for 15 (`FIRE_IGNITE`, `LAVA_IGNITE`); water puts it out, Flame's
    /// fire too, and so does rain (`isInWaterOrRain`, `isInRain`).
    #[test]
    fn fire_and_lava_light_arrows_and_water_and_rain_put_them_out() {
        let fly = |world: &dyn Fn(&mut Arrow, &mut Rng, &mut Vec<EntityEvent>), flame: bool| {
            let mut a = Arrow::shot(DVec3::new(0.5, 50.2, 0.5), DVec3::X, 1.0, true);
            if flame {
                a.enchants.set(crate::enchant::Enchantment::Flame, 1);
            }
            let (mut rng, mut events) = (Rng::new(1), Vec::new());
            world(&mut a, &mut rng, &mut events);
            a
        };
        let through = |block: Block| {
            move |a: &mut Arrow, rng: &mut Rng, events: &mut Vec<EntityEvent>| {
                let mut world = Grid::flat(0);
                world.set(IVec3::new(3, 50, 0), block);
                for _ in 0..3 {
                    a.update(0.05, &world, &sky_ctx(), &mut [], &[], None, rng, events);
                }
            }
        };
        let a = fly(&through(Block::FIRE), false);
        assert!(a.burning() && (155..=160).contains(&a.fire), "fire: {}", a.fire);
        let a = fly(&through(Block::LAVA), false);
        assert!(a.burning() && (295..=300).contains(&a.fire), "lava: {}", a.fire);
        let a = fly(&through(Block::WATER), true);
        assert!(!a.burning(), "water puts out Flame");
        let a = fly(&through(Block::STONE), true);
        assert!(a.burning(), "Flame burns on otherwise");
        let a = fly(
            &|a: &mut Arrow, rng: &mut Rng, events: &mut Vec<EntityEvent>| {
                a.update(0.05, &Rainy(Grid::flat(0)), &sky_ctx(), &mut [], &[], None, rng, events);
            },
            true,
        );
        assert!(!a.burning(), "rain puts out Flame");
    }

    /// v1.0 gap Track 8b: `CampfireBlock` and `TntBlock.onProjectileHit`:
    /// a burning arrow's hit on a campfire or TNT is reported, with whether
    /// a mob shot it (`mayInteract`); a cold arrow's isn't.
    #[test]
    fn burning_arrows_report_campfires_and_tnt() {
        use crate::world::block::Facing;
        use crate::world::utilities::{Campfire, campfire};
        let fire = campfire(Campfire { soul: false, lit: false, signal: false, wet: false, facing: Facing::North });
        for block in [Block::TNT, fire] {
            for (flame, mob) in [(true, false), (true, true), (false, false)] {
                let mut world = Grid::flat(0);
                world.set(IVec3::new(4, 50, 0), block);
                let mut a = Arrow::shot(DVec3::new(0.5, 50.2, 0.5), DVec3::X, 1.0, true);
                if mob {
                    a.shooter = Some(5);
                }
                if flame {
                    a.enchants.set(crate::enchant::Enchantment::Flame, 1);
                }
                let (mut rng, mut events) = (Rng::new(1), Vec::new());
                for _ in 0..4 {
                    a.update(0.05, &world, &sky_ctx(), &mut [], &[], None, &mut rng, &mut events);
                }
                assert!(a.is_stuck());
                let hit = events.iter().find_map(|e| match *e {
                    EntityEvent::ProjectileBlockHit { cell, burning, mob_owned, .. } => {
                        Some((cell, burning, mob_owned))
                    }
                    _ => None,
                });
                let want = flame.then_some((IVec3::new(4, 50, 0), true, mob));
                assert_eq!(hit, want, "{block:?} flame {flame} mob {mob}");
            }
        }
    }

    #[test]
    fn embedded_potion_contents_expire_after_six_hundred_ticks() {
        let mut a = Arrow::shot(DVec3::new(0.5, 9.5, 0.5), DVec3::X, 1., true);
        a.stuck = true;
        a.age = 29.9;
        a.potion = crate::potion::Potion::from_id("poison");
        a.tipped = true;
        let ctx = Ctx {
            players: Vec::new(),
            spawning: false,
            daylight: 0.,
            raining: false,
            dimension: crate::world::terrain::Dimension::Overworld,
            spawn_light: crate::simulation::lighting::SpawnLight::NIGHT,
        };
        let w = Grid::flat(10);
        let mut rng = Rng::new(5);
        let mut events = Vec::new();
        a.update(0.05, &w, &ctx, &mut [], &[], None, &mut rng, &mut events);
        assert!(a.potion.is_some());
        a.update(0.05, &w, &ctx, &mut [], &[], None, &mut rng, &mut events);
        assert!(a.potion.is_none());
        assert_eq!(a.pickup_stack().item, crate::item::Item::ARROW);
    }
}
