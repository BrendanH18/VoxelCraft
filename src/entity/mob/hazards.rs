//! v1.0 Track 5: what the environment does to an ordinary mob's body, as
//! Java 26.3's `LivingEntity` and `Mob` do it.
//!
//! - Fall damage (`Entity.checkFallDamage`, `LivingEntity.causeFallDamage`
//!   and `calculateFallDamage`): the distance fallen out of water, reset in
//!   water, on climbable blocks, under Slow Falling and in a cobweb, is
//!   `floor((distance + 1e-6 - safe_fall_distance) * modifier)` on landing,
//!   where the safe distance is 3 (5 for foxes) plus the Jump Boost level
//!   and the landing block's `fallOn` sets the modifier (hay and honey 0.2,
//!   slime none, powder snow no call at all). Goats take 10 less, frogs 5.
//!   Mobs in `#fall_damage_immune` take none. It lands as `minecraft:fall`
//!   damage, which bypasses armor points but not Protection or Feather
//!   Falling (`getDamageAfterMagicAbsorb`).
//! - v1.0 gap Track 1: an upward pointed dripstone tip adds 2.5 blocks and
//!   doubles it as `minecraft:stalagmite` damage
//!   (`PointedDripstoneBlock.fallOn`); a creeper's landing adds to its fuse
//!   (`Creeper.causeFallDamage`); the fall distance is saved
//!   (`fall_distance`).
//! - Fire and lava hurt as `in_fire`, `lava` and `on_fire`, so worn armor
//!   reduces the first two and Protection and Fire Protection all three.
//! - Cobwebs (`WebBlock.entityInside`, `Entity.makeStuckInBlock`): a mob
//!   inside one moves (0.25, 0.05, 0.25) of its motion, or (0.5, 0.25, 0.5)
//!   under Weaving, its motion stops and its fall is forgotten. Spiders
//!   (`Spider.makeStuckInBlock`) and the Wither aren't held.
//! - Sun protection (`Mob.burnUndead`): an undead mob with anything on its
//!   head doesn't catch fire in daylight; on each sun-burn tick a damageable
//!   helmet takes 0 or 1 points instead, through Unbreaking, and breaks at
//!   full damage.
//!
//! Java's `Mob` doesn't override `hurtArmor`, so worn humanoid armor never
//! wears from damage (only players', horse armor and wolf armor do).

use glam::DVec3;

use super::{Mob, MobKind};
use crate::physics::{self, BlockSource};
use crate::simulation::damage::Cause;
use crate::world::block::Block;

/// `#minecraft:fall_damage_immune`.
pub fn fall_damage_immune(kind: MobKind) -> bool {
    matches!(
        kind,
        MobKind::CopperGolem
            | MobKind::IronGolem
            | MobKind::SnowGolem
            | MobKind::Shulker
            | MobKind::Allay
            | MobKind::Bat
            | MobKind::Bee
            | MobKind::Blaze
            | MobKind::Cat
            | MobKind::Chicken
            | MobKind::Ghast
            | MobKind::HappyGhast
            | MobKind::Phantom
            | MobKind::MagmaCube
            | MobKind::Ocelot
            | MobKind::Parrot
            | MobKind::Wither
            | MobKind::Breeze
    )
}

/// The `SAFE_FALL_DISTANCE` attribute's base: 3, and `Fox`'s 5.
pub fn safe_fall_distance(kind: MobKind) -> f64 {
    if kind == MobKind::Fox { 5.0 } else { 3.0 }
}

/// What the landing block's `fallOn` hands `causeFallDamage`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Landing {
    /// Blocks added to the distance fallen.
    pub extra: f64,
    /// The `damageModifier`.
    pub modifier: f32,
    /// The damage type: `fall`, or `stalagmite` on a dripstone tip.
    pub cause: Cause,
}

/// `Block.fallOn` for a mob landing on `below`: `causeFallDamage(distance,
/// 1, fall)`, with `HayBlock`'s and `HoneyBlock`'s 0.2, `SlimeBlock`'s 0
/// (a mob never sneaks), `PointedDripstoneBlock`'s `distance + 2.5`, 2 and
/// `stalagmite` on an upward tip; `PowderSnowBlock` calls nothing (only a
/// sound), which is `None`. `BedBlock` has no `fallOn` in 26.3: a bed
/// bounces but doesn't soften the landing.
pub fn landing(below: Option<Block>) -> Option<Landing> {
    let fall = |modifier| Some(Landing { extra: 0.0, modifier, cause: Cause::FALL });
    match below {
        Some(b) if b == crate::world::scaffolding::POWDER_SNOW => None,
        Some(b) if b == crate::world::stone_blocks::SLIME_BLOCK => fall(0.0),
        Some(b) if b.base() == Block::HAY_BALE || b == Block::HONEY_BLOCK => fall(0.2),
        Some(b) if crate::world::speleothem::stalagmite_tip(b) => Some(Landing {
            extra: crate::world::speleothem::STALAGMITE_FALL_EXTRA,
            modifier: crate::world::speleothem::STALAGMITE_FALL_MULTIPLIER,
            cause: Cause::STALAGMITE,
        }),
        _ => fall(1.0),
    }
}

/// `Creeper.causeFallDamage`: a landing creeper's swell grows by
/// `(int) (fallDistance * 1.5)` ticks (the distance `fallOn` passed, so a
/// dripstone's extra 2.5 counts), then is held to `maxSwell - 5`, which
/// also cuts back a swell already past it. Returns the fuse in seconds.
pub fn creeper_fall_fuse(fuse: f32, distance: f64) -> f32 {
    const TPS: f32 = 20.0;
    let swell = fuse + ((distance * 1.5) as i32) as f32 / TPS;
    swell.min(super::FUSE_TIME - 5.0 / TPS)
}

/// `LivingEntity.calculateFallDamage` (with `Goat`'s 10 less and `Frog`'s
/// 5 less).
pub fn fall_damage(kind: MobKind, distance: f64, modifier: f32, jump_boost: u32) -> f32 {
    if fall_damage_immune(kind) {
        return 0.0;
    }
    let power = distance + 1.0e-6 - safe_fall_distance(kind) - f64::from(jump_boost);
    let mut damage = (power * f64::from(modifier)).floor() as i32;
    match kind {
        MobKind::Goat => damage -= 10,
        MobKind::Frog => damage -= 5,
        _ => {}
    }
    damage.max(0) as f32
}

/// `WebBlock.entityInside`'s speed multiplier.
pub fn web_multiplier(weaving: bool) -> DVec3 {
    if weaving {
        DVec3::from_array(crate::simulation::effects::WEAVING_WEB_SPEED)
    } else {
        DVec3::new(0.25, 0.05_f32 as f64, 0.25)
    }
}

impl Mob {
    /// Damage of a known type: armor, Protection and the type's own
    /// protection enchantments see it.
    pub(super) fn damage_from(&mut self, amount: f32, cause: Cause, rng: &mut super::Rng) -> bool {
        self.hit_cause = Some(cause);
        let killed = self.damage(amount, None, rng);
        self.hit_cause = None;
        killed
    }

    /// The cobweb hold on this move, if the mob is inside one.
    pub(super) fn web_hold<W: BlockSource + ?Sized>(&self, world: &W) -> Option<DVec3> {
        if matches!(self.kind, MobKind::Spider | MobKind::CaveSpider | MobKind::Wither) {
            return None;
        }
        physics::touches_block(world, self.pos, self.shape(), |b| b == Block::COBWEB)
            .then(|| web_multiplier(self.effects.has(crate::simulation::effects::Effect::Weaving)))
    }

    /// `checkFallDamage` after a move from `from_y`: the fall grows, resets
    /// where Java resets it, and a landing queues its damage for the tick
    /// (see [`Mob::take_fall`]).
    pub(super) fn track_fall<W: BlockSource + ?Sized>(&mut self, world: &W, from_y: f64, landed: bool) {
        // Walking mobs neither fall nor carry a fall: no block lookups.
        if self.fall_distance == 0.0 && from_y <= self.pos.y {
            return;
        }
        let climbing = world.block(self.pos.floor().as_ivec3()).is_some_and(Block::is_climbable);
        let slow_falling = self.effects.has(crate::simulation::effects::Effect::SlowFalling) && self.vel.y < 0.0;
        // `LivingEntity.checkFallDamage` updates the fluid contact first, and
        // `Entity.move` clips a fast move against water: a fall through or
        // into water is forgotten.
        let column = self.pos.floor().as_ivec3();
        let through_water =
            (column.y..=from_y.floor() as i32).any(|y| world.block(column.with_y(y)).is_some_and(Block::holds_water));
        if self.in_water || through_water || climbing || slow_falling {
            self.fall_distance = 0.0;
        } else {
            self.fall_distance += (from_y - self.pos.y).max(0.0);
        }
        if landed {
            let distance = std::mem::take(&mut self.fall_distance);
            let below = world.block(super::super::item::below_pos(self.pos));
            // `Entity.checkFallDamage` calls `fallOn` only after a fall.
            if distance > 0.0
                && let Some(l) = landing(below)
            {
                let fallen = distance + l.extra;
                let damage = fall_damage(self.kind, fallen, l.modifier, self.effects.jump_boost());
                if damage > 0.0 {
                    self.pending_fall += damage;
                    self.pending_fall_cause = l.cause;
                }
                if self.kind == MobKind::Creeper {
                    self.fuse = creeper_fall_fuse(self.fuse, fallen);
                }
            }
        }
    }

    /// The fall damage the last moves queued, as `minecraft:fall` (or
    /// `minecraft:stalagmite`) damage.
    pub(super) fn take_fall(&mut self, rng: &mut super::Rng) -> bool {
        let damage = std::mem::take(&mut self.pending_fall);
        let cause = std::mem::replace(&mut self.pending_fall_cause, Cause::FALL);
        damage > 0.0 && self.damage_from(damage, cause, rng)
    }

    /// The fall distance Java saves as `fall_distance`: the one of whichever
    /// tracker moves this mob (a mount's, a wolf's, a frog's or the
    /// ordinary one).
    pub fn saved_fall_distance(&self) -> f64 {
        if let Some(s) = &self.mount {
            s.fall_distance
        } else if let Some(s) = &self.wolf_armor {
            s.fall_distance()
        } else if let Some(s) = &self.frog {
            s.fall_distance
        } else {
            self.fall_distance
        }
    }

    /// Restores [`Mob::saved_fall_distance`]: a mob saved mid-fall lands
    /// with the whole fall after a reload.
    pub fn restore_fall_distance(&mut self, distance: f64) {
        let distance = if distance.is_finite() { distance.max(0.0) } else { 0.0 };
        if let Some(s) = &mut self.mount {
            s.fall_distance = distance;
        } else if let Some(s) = &mut self.wolf_armor {
            s.set_fall_distance(distance);
        } else if let Some(s) = &mut self.frog {
            s.fall_distance = distance;
        } else {
            self.fall_distance = distance;
        }
    }

    /// `Mob.burnUndead`'s sun blocker: on a sun-burn tick (`isSunBurnTick`'s
    /// chance at `brightness`, drawn here), the head item takes
    /// `nextInt(2)` points (through Unbreaking) and breaks at full damage.
    /// Returns whether something on the head kept the sun off.
    pub(super) fn sun_blocked(&mut self, brightness: f32, rng: &mut super::Rng) -> bool {
        use super::super::equipment::Slot;
        let Some(head) = self.equipment.get(Slot::Head).copied() else { return false };
        if rng.next_f32() * 30.0 < (brightness - 0.4) * 2.0
            && let Some(max) = head.item.durability()
        {
            let points = rng.next_int(2) as u16;
            let worn = (0..points).filter(|_| crate::enchant::wears(head.enchants, true, rng.next_f32())).count();
            let mut head = head;
            head.damage = head.damage.saturating_add(worn as u16);
            self.equipment.set(Slot::Head, (head.damage < max).then_some(head));
        }
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn javas_fall_damage_formula_and_tag() {
        // 3 blocks are safe; each further block is one point.
        assert_eq!(fall_damage(MobKind::Zombie, 3.0, 1.0, 0), 0.0);
        assert_eq!(fall_damage(MobKind::Zombie, 3.999, 1.0, 0), 0.0);
        assert_eq!(fall_damage(MobKind::Zombie, 4.0, 1.0, 0), 1.0);
        assert_eq!(fall_damage(MobKind::Zombie, 23.5, 1.0, 0), 20.0);
        assert_eq!(fall_damage(MobKind::Zombie, 23.5, 1.0, 2), 18.0);
        assert_eq!(fall_damage(MobKind::Zombie, 23.0, 0.2, 0), 4.0);
        assert_eq!(fall_damage(MobKind::Zombie, 23.0, 0.5, 0), 10.0);
        assert_eq!(fall_damage(MobKind::Zombie, 23.0, 0.0, 0), 0.0);
        assert_eq!(fall_damage(MobKind::Fox, 6.0, 1.0, 0), 1.0);
        assert_eq!(fall_damage(MobKind::Goat, 14.0, 1.0, 0), 1.0);
        for kind in [MobKind::IronGolem, MobKind::Cat, MobKind::Chicken, MobKind::Breeze, MobKind::Wither] {
            assert_eq!(fall_damage(kind, 100.0, 1.0, 0), 0.0, "{kind:?}");
        }
        let modifier = |b| landing(Some(b)).unwrap().modifier;
        assert_eq!(modifier(Block::HAY_BALE), 0.2);
        assert_eq!(modifier(Block::STONE), 1.0);
        assert_eq!(modifier(crate::world::stone_blocks::SLIME_BLOCK), 0.0);
    }

    use crate::entity::{Rng, equipment::Slot};
    use crate::inventory::Stack;
    use crate::item::{ArmorMaterial, ArmorPiece, Item};
    use crate::physics::test_util::Grid;

    fn armored(pieces: &[(ArmorPiece, Option<(crate::enchant::Enchantment, u8)>)]) -> Mob {
        let mut m = Mob::new(MobKind::Zombie, DVec3::new(0.5, 1.0, 0.5), 0.0);
        for &(piece, enchant) in pieces {
            let mut stack = Stack::new(Item::armor(piece, ArmorMaterial::Diamond), 1);
            if let Some((e, level)) = enchant {
                stack.enchants.set(e, level);
            }
            m.equipment.set(Slot::of_piece(piece), Some(stack));
        }
        m
    }

    fn near(a: f32, b: f32, what: &str) {
        assert!((a - b).abs() < 1e-4, "{what}: {a} vs {b}");
    }

    const FULL: [ArmorPiece; 4] = [ArmorPiece::Helmet, ArmorPiece::Chestplate, ArmorPiece::Leggings, ArmorPiece::Boots];

    /// `getDamageAfterArmorAbsorb` then `getDamageAfterMagicAbsorb` for the
    /// environment's damage types, on a zombie in diamond armor.
    #[test]
    fn worn_armor_and_protection_reduce_fire_lava_and_falls() {
        let mut rng = Rng::new(3);
        // Lava (armor applies): 20 points and 8 toughness leave 4 * 0.24.
        let mut m = armored(&FULL.map(|p| (p, None)));
        m.damage_from(4.0, Cause::LAVA, &mut rng);
        near(m.health, 20.0 - 0.96, "lava through diamond armor");
        let mut bare = Mob::new(MobKind::Zombie, DVec3::ZERO, 0.0);
        bare.damage_from(4.0, Cause::LAVA, &mut rng);
        near(bare.health, 16.0, "bare lava");
        // In fire: the same armor formula.
        let mut m = armored(&FULL.map(|p| (p, None)));
        m.damage_from(1.0, Cause::IN_FIRE, &mut rng);
        near(m.health, 20.0 - (1.0 - 19.75 / 25.0), "in fire: armor fraction 20 - 1/4");
        // Burning bypasses armor; Fire Protection IV (8) still counts.
        use crate::enchant::Enchantment as E;
        let mut m = armored(&[(ArmorPiece::Chestplate, Some((E::FireProtection, 4)))]);
        m.damage_from(1.0, Cause::ON_FIRE, &mut rng);
        near(m.health, 20.0 - 0.68, "on fire with Fire Protection IV");
        // Falls bypass armor; Feather Falling IV (12) and Protection IV (4)
        // take 16 of 25.
        let mut m = armored(&[
            (ArmorPiece::Boots, Some((E::FeatherFalling, 4))),
            (ArmorPiece::Helmet, Some((E::Protection, 4))),
        ]);
        m.pending_fall = 10.0;
        m.take_fall(&mut rng);
        near(m.health, 20.0 - 10.0 * 9.0 / 25.0, "fall with Feather Falling IV and Protection IV");
        let mut m = armored(&FULL.map(|p| (p, None)));
        m.pending_fall = 10.0;
        m.take_fall(&mut rng);
        near(m.health, 10.0, "armor points don't soften a fall");
        // Fire Protection doesn't help against a fall.
        let mut m = armored(&[(ArmorPiece::Boots, Some((E::FireProtection, 4)))]);
        m.pending_fall = 10.0;
        m.take_fall(&mut rng);
        near(m.health, 10.0, "Fire Protection and falls");
        // Worn armor doesn't wear from any of it (`Mob` has no `hurtArmor`).
        assert!(Slot::ALL.iter().all(|&s| m.equipment.get(s).is_none_or(|s| s.damage == 0)));
    }

    #[test]
    fn falls_are_measured_from_the_physics_and_soften_on_hay() {
        let world = Grid::flat(1);
        let mut m = Mob::new(MobKind::Zombie, DVec3::new(0.5, 25.5, 0.5), 0.0);
        for _ in 0..200 {
            m.physics_step(0.05, &world, None, 0.0);
        }
        assert!(m.on_ground);
        // Mob collision rests 1e-5 above the floor, so start half a block up.
        assert_eq!(m.pending_fall, 21.0, "24.5 blocks less the safe 3");
        let mut hay = Grid::flat(1);
        hay.set(glam::IVec3::new(0, 0, 0), Block::HAY_BALE);
        let mut m = Mob::new(MobKind::Zombie, DVec3::new(0.5, 25.5, 0.5), 0.0);
        for _ in 0..200 {
            m.physics_step(0.05, &hay, None, 0.0);
        }
        assert_eq!(m.pending_fall, 4.0, "floor(21.5 * 0.2)");
        // In water the fall is forgotten.
        let mut wet = Grid::flat(1);
        for y in 1..3 {
            wet.set(glam::IVec3::new(0, y, 0), Block::WATER);
        }
        let mut m = Mob::new(MobKind::Zombie, DVec3::new(0.5, 25.5, 0.5), 0.0);
        for _ in 0..200 {
            m.physics_step(0.05, &wet, None, 0.0);
        }
        assert_eq!(m.pending_fall, 0.0);
        let mut golem = Mob::new(MobKind::IronGolem, DVec3::new(0.5, 25.5, 0.5), 0.0);
        for _ in 0..200 {
            golem.physics_step(0.05, &world, None, 0.0);
        }
        assert_eq!(golem.pending_fall, 0.0, "fall_damage_immune");
    }

    /// v1.0 gap Track 1: `PointedDripstoneBlock.fallOn`: landing on an
    /// upward tip adds 2.5 blocks and doubles the damage, as `stalagmite`
    /// damage; a downward or merged tip and a sulfur spike don't.
    #[test]
    fn upward_dripstone_tips_double_a_fall_as_stalagmite_damage() {
        use crate::world::overworld_blocks::{Thickness, pointed_dripstone};
        let tip = pointed_dripstone(false, Thickness::Tip);
        let l = landing(Some(tip)).unwrap();
        assert_eq!((l.extra, l.modifier, l.cause), (2.5, 2.0, Cause::STALAGMITE));
        assert_eq!(landing(Some(tip.with_water(true))).unwrap().cause, Cause::STALAGMITE, "waterlogged");
        for other in [
            pointed_dripstone(true, Thickness::Tip),
            pointed_dripstone(false, Thickness::TipMerge),
            pointed_dripstone(false, Thickness::Frustum),
            crate::world::gen5_blocks::SULFUR_SPIKE,
            Block::STONE,
        ] {
            assert_eq!(landing(Some(other)).unwrap(), Landing { extra: 0.0, modifier: 1.0, cause: Cause::FALL });
        }
        assert_eq!(landing(Some(crate::world::scaffolding::POWDER_SNOW)), None, "powder snow calls nothing");
        // A zombie dropped from 25.5 onto a tip whose top is 11/16 up.
        let mut world = Grid::flat(1);
        world.set(glam::IVec3::new(0, 1, 0), tip);
        let mut m = Mob::new(MobKind::Zombie, DVec3::new(0.5, 25.5, 0.5), 0.0);
        for _ in 0..200 {
            m.physics_step(0.05, &world, None, 0.0);
        }
        assert!(m.on_ground && (m.pos.y - (1.0 + 11.0 / 16.0)).abs() < 1e-3, "{}", m.pos.y);
        assert_eq!(m.pending_fall, ((25.5 - 1.6875 + 2.5 - 3.0) * 2.0f64).floor() as f32);
        assert_eq!(m.pending_fall_cause, Cause::STALAGMITE);
        let mut rng = Rng::new(1);
        assert!(m.take_fall(&mut rng), "46 damage kills a zombie");
        assert_eq!(m.pending_fall_cause, Cause::FALL, "the cause is spent with the damage");
    }

    /// `Creeper.causeFallDamage`: `swell += (int) (fallDistance * 1.5)`,
    /// held to `maxSwell - 5`; the fuse isn't lit by it and burns down.
    #[test]
    fn a_creepers_fall_adds_to_its_fuse() {
        assert_eq!(creeper_fall_fuse(0.0, 10.0), 15.0 / 20.0);
        assert_eq!(creeper_fall_fuse(0.0, 3.3), 4.0 / 20.0, "(int) 4.95");
        assert_eq!(creeper_fall_fuse(0.0, 40.0), 25.0 / 20.0, "maxSwell - 5");
        assert_eq!(creeper_fall_fuse(1.4, 0.5), 1.25, "a swell past it is cut back");
        let world = Grid::flat(1);
        let mut m = Mob::new(MobKind::Creeper, DVec3::new(0.5, 11.5, 0.5), 0.0);
        for _ in 0..100 {
            m.physics_step(0.05, &world, None, 0.0);
        }
        // 10.5 blocks less collision's 1e-5: (int) 15.74 = 15 ticks.
        assert_eq!(m.fuse, 15.0 / 20.0);
        assert!(!m.swelling);
        assert_eq!(m.pending_fall, 7.0);
        // Onto a dripstone tip the extra 2.5 counts: 21.3125 * 1.5.
        let mut world = Grid::flat(1);
        world.set(
            glam::IVec3::new(0, 1, 0),
            crate::world::overworld_blocks::pointed_dripstone(false, crate::world::overworld_blocks::Thickness::Tip),
        );
        let mut m = Mob::new(MobKind::Creeper, DVec3::new(0.5, 20.5, 0.5), 0.0);
        for _ in 0..200 {
            m.physics_step(0.05, &world, None, 0.0);
        }
        assert_eq!(m.fuse, 25.0 / 20.0, "31 ticks, held to 25");
        // Powder snow doesn't call `causeFallDamage`: no swell.
        let mut snow = Grid::flat(1);
        snow.set(glam::IVec3::new(0, 0, 0), crate::world::scaffolding::POWDER_SNOW);
        let mut m = Mob::new(MobKind::Creeper, DVec3::new(0.5, 1.0, 0.5), 0.0);
        m.restore_fall_distance(10.0);
        m.on_ground = false;
        m.track_fall(&snow, m.pos.y, true);
        assert_eq!((m.fuse, m.pending_fall), (0.0, 0.0));
    }

    /// Java saves `fall_distance`: a mob saved mid-fall lands with the whole
    /// fall after a reload; one at rest writes nothing.
    #[test]
    fn a_mobs_fall_distance_is_saved_and_loaded() {
        use crate::entity::nether::{load_mob, save_mob};
        let mut m = Mob::new(MobKind::Zombie, DVec3::new(0.5, 30.0, 0.5), 0.0);
        assert!(save_mob(&m).get("fall_distance").is_none(), "older records stay as they were");
        m.fall_distance = 12.25;
        let saved = save_mob(&m);
        assert_eq!(saved["fall_distance"], 12.25);
        let mut back = load_mob(&saved).unwrap();
        assert_eq!(back.saved_fall_distance(), 12.25);
        // It lands with the whole fall: 12.25 + 29 blocks.
        let world = Grid::flat(1);
        for _ in 0..200 {
            back.physics_step(0.05, &world, None, 0.0);
        }
        assert_eq!(back.pending_fall, (12.25f64 + 29.0 - 3.0).floor() as f32);
        // Wolves, frogs and mounts keep their own trackers.
        for kind in [MobKind::Wolf, MobKind::Frog, MobKind::Horse] {
            let mut m = Mob::new(kind, DVec3::new(0.5, 30.0, 0.5), 0.0);
            m.restore_fall_distance(7.5);
            assert_eq!(m.fall_distance, 0.0, "{kind:?}");
            let back = load_mob(&save_mob(&m)).unwrap();
            assert_eq!(back.saved_fall_distance(), 7.5, "{kind:?}");
        }
        // Villagers and golems save through their own records.
        let mut e = crate::entity::Entities::new(1);
        e.spawn(MobKind::Villager, DVec3::new(0.5, 30.0, 0.5));
        e.mobs[0].restore_fall_distance(4.5);
        let text = e.villagers_to_string();
        let mut loaded = crate::entity::Entities::new(1);
        loaded.load_villagers(&text);
        assert_eq!(loaded.mobs[0].saved_fall_distance(), 4.5);
        // Nonsense loads as no fall.
        let mut bad = save_mob(&Mob::new(MobKind::Zombie, DVec3::ZERO, 0.0));
        bad["fall_distance"] = serde_json::json!(-3.0);
        assert_eq!(load_mob(&bad).unwrap().saved_fall_distance(), 0.0);
    }

    /// `WebBlock.entityInside`: cobwebs hold walking mobs, less under
    /// Weaving, and spiders walk through.
    #[test]
    fn cobwebs_slow_mobs_and_weaving_frees_them_halfway() {
        let mut world = Grid::flat(1);
        for x in -1..12 {
            world.set(glam::IVec3::new(x, 1, 0), Block::COBWEB);
            world.set(glam::IVec3::new(x, 2, 0), Block::COBWEB);
        }
        let walk = |kind: MobKind, weaving: bool, world: &Grid| {
            let mut m = Mob::new(kind, DVec3::new(0.5, 1.0, 0.5), 0.0);
            if weaving {
                m.effects.add(crate::simulation::effects::Effect::Weaving, 0, 600);
            }
            for _ in 0..40 {
                m.physics_step(0.05, world, Some(DVec3::X), 4.0);
            }
            m.pos.x - 0.5
        };
        let open = walk(MobKind::Zombie, false, &Grid::flat(1));
        let held = walk(MobKind::Zombie, false, &world);
        let weaving = walk(MobKind::Zombie, true, &world);
        assert!(held > 0.0 && held < open * 0.2, "held {held} of {open}");
        assert!(weaving > held * 1.5 && weaving < open * 0.5, "weaving {weaving}, held {held}");
        let spider = walk(MobKind::Spider, false, &world);
        let free = walk(MobKind::Spider, false, &Grid::flat(1));
        assert!((spider - free).abs() < 1e-9, "spiders ignore cobwebs");
        // A web breaks a fall.
        let mut m = Mob::new(MobKind::Zombie, DVec3::new(0.5, 30.0, 0.5), 0.0);
        for _ in 0..400 {
            m.physics_step(0.05, &world, None, 0.0);
        }
        assert_eq!(m.pending_fall, 0.0, "the fall ends in the web");
    }

    /// `Mob.burnUndead`: a helmet keeps the sun off and wears instead.
    #[test]
    fn a_helmet_takes_the_sun_instead_of_the_zombie() {
        let mut rng = Rng::new(9);
        let mut bare = Mob::new(MobKind::Zombie, DVec3::ZERO, 0.0);
        assert!(!bare.sun_blocked(1.0, &mut rng));
        let mut m = armored(&[(ArmorPiece::Helmet, None)]);
        let mut wears = 0;
        for _ in 0..2000 {
            assert!(m.sun_blocked(1.0, &mut rng));
            wears = m.equipment.get(Slot::Head).map_or(wears, |s| s.damage);
        }
        // About 2000 * 0.04 sun-burn ticks, half of them wearing a point.
        assert!((20..70).contains(&wears), "{wears}");
        // A helmet at its last point breaks and the zombie burns again.
        let mut m = armored(&[(ArmorPiece::Helmet, None)]);
        let max = Item::armor(ArmorPiece::Helmet, ArmorMaterial::Diamond).durability().unwrap();
        m.equipment.get_mut(Slot::Head).unwrap().damage = max - 1;
        while m.equipment.get(Slot::Head).is_some() {
            m.sun_blocked(1.0, &mut rng);
        }
        assert!(!m.sun_blocked(1.0, &mut rng));
    }

    #[test]
    fn fall_damage_immune_matches_the_java_tag() {
        let f: serde_json::Value =
            serde_json::from_str(include_str!("../../../tests/fixtures/java/26.3/new_mobs.json")).unwrap();
        let tag: Vec<&str> = f["tags"]["entity_type"]["fall_damage_immune"]
            .as_array()
            .unwrap()
            .iter()
            .map(|v| v.as_str().unwrap().trim_start_matches("minecraft:"))
            .collect();
        for kind in MobKind::ALL {
            let id = kind.name().replace(' ', "_");
            assert_eq!(fall_damage_immune(kind), tag.contains(&id.as_str()), "{id}");
        }
    }
}
