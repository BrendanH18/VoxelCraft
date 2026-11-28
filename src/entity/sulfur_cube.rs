//! v0.16 Track D: the sulfur cube (Java 26.3 `SulfurCube` on
//! `AbstractCubeMob`, with the `sulfur_cube_archetype` registry).
//!
//! A sulfur cube is an ageable cube mob of the sulfur caves: babies are
//! size 1, adults size 2, with `4 × size` health. It never attacks. Without
//! a block it hops about like a slime (`CubeMobRandomDirectionGoal`,
//! `CubeMobKeepOnJumpingGoal`, `CubeMobFloatGoal`), follows players holding
//! what it wants (`TemptGoal.ForNonPathfinders` within the 8-block tempt
//! range: slime balls for babies, swallowable blocks for adults) and hops to
//! swallowable item entities within 8 blocks (`SulfurCubeSearchForItemsGoal`).
//!
//! An adult swallows a block from a player's hand or from the ground into
//! its body slot. Then it drops every goal and becomes a ball with the
//! block's archetype: knockback resistance, bounciness, friction and air
//! drag modifiers, buoyancy, Java's angled knockback when hit, pushes from
//! players walking into it, contact damage (magma) or an explosion (TNT,
//! primed by fire, flint and steel, fire charges, explosions or redstone).
//! With a block it shrugs off the damage types in
//! `#sulfur_cube_with_block_immune_to` (taking only their knockback).
//! Shears eject the block (the cube waits 100 ticks before taking another);
//! an empty bucket scoops an adult into a `sulfur_cube_bucket`.
//!
//! The goals and the ball physics live in `mob::sulfur_cube`; this module
//! holds the numbers, the archetypes, the saved state and the entity-level
//! passes. Every number is checked against
//! `tests/fixtures/java/26.3/sulfur_cube.json`.
use super::equipment::Slot;
use super::{Ctx, Entities, EntityEvent, Mob, MobKind, MobSound, MobWorld, PlayerId};
use crate::inventory::Stack;
use crate::item::Item;
use glam::DVec3;
use serde_json::{Value, json};

/// v0.16 Track D's item 1707: `sulfur_cube_bucket` (a `MobBucketItem` with
/// no fluid, stacking to 1).
pub const BUCKET: Item = Item(1707);

/// `EntityTypes.SULFUR_CUBE`: `sized(0.49, 0.49)` scaled by the size,
/// `eyeHeight(0.175)`.
pub const BASE_SIZE: f64 = 0.49_f32 as f64;
pub const EYE_HEIGHT: f64 = 0.175_f32 as f64;
/// `SulfurCube.MIN_SIZE` (babies) and `MAX_SIZE` (adults).
pub const BABY_SIZE: u8 = 1;
pub const ADULT_SIZE: u8 = 2;
/// `setCubeMobHealth`: `4 × size`.
pub const HEALTH_PER_SIZE: f32 = 4.0;
/// `createSulfurCubeAttributes`: `TEMPT_RANGE` 8; the search goal inflates
/// the cube's box by 8 too.
pub const TEMPT_RANGE: f64 = 8.0;
pub const SEARCH_RANGE: f64 = 8.0;
/// `PICKUP_TIMER_DURATION`: ticks after shearing before it takes a block.
pub const PICKUP_TIMER: i32 = 100;
/// `PUSH_DISTANCE_THRESHOLD`, `MAX_PLAYER_PUSH_SPEED` and the push scales.
pub const PUSH_DISTANCE: f64 = 1.3_f32 as f64;
pub const MAX_PLAYER_PUSH_SPEED: f64 = 0.5;
pub const PLAYER_PUSH_SCALE: f32 = 0.3;
pub const VEHICLE_PUSH_SCALE: f32 = 0.16;
pub const VERTICAL_PUSH: f32 = 0.3;
/// `knockback`'s hit-angle scales.
pub const HORIZONTAL_HIT_ANGLE_SCALE: f32 = 1.6;
pub const VERTICAL_HIT_ANGLE_SCALE: f32 = 0.5;
pub const VERTICAL_POSITION_ANGLE_SCALE: f32 = 0.8;
/// `AgeableMob.getBabyStartAge`.
pub const BABY_AGE: i32 = -24000;
/// `AgeableMobGroupData`'s chance that a later pack member is a baby.
pub const PACK_BABY_CHANCE: f32 = 0.05;
/// `SulfurCubeArchetype.DEFAULT_KNOCKBACK_MODIFIERS` and the default sound
/// settings' push threshold and cooldown (seconds).
pub const DEFAULT_KNOCKBACK: (f32, f32) = (0.33, 0.06);
pub const DEFAULT_PUSH_SOUND: (f32, f32) = (0.2, 0.5);
/// `Attributes.GRAVITY`.
pub const GRAVITY: f64 = 0.08;
/// Seconds in one Java tick.
pub const TICK: f64 = 0.05;

/// The twelve `sulfur_cube_archetype` entries, in the registry's bootstrap
/// order.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Hash)]
pub enum Archetype {
    Regular,
    Bouncy,
    SlowBouncy,
    SlowFlat,
    FastFlat,
    Light,
    FastSliding,
    SlowSliding,
    HighResistance,
    Sticky,
    Explosive,
    Hot,
}

/// `SulfurCubeArchetype.ExplosionData`.
#[derive(Clone, Copy, PartialEq, Debug)]
pub struct Explosion {
    pub power: f32,
    pub causes_fire: bool,
    pub fuse: i32,
}

/// What an archetype does to the cube: the attribute values its modifiers
/// leave (from the base `knockback_resistance` 0, `bounciness` 0,
/// `friction_modifier` 1 and `air_drag_modifier` 1, clamped to each
/// attribute's range), and its other fields.
#[derive(Clone, Copy, PartialEq, Debug)]
pub struct Physics {
    pub knockback_resistance: f64,
    pub explosion_knockback_resistance: f64,
    pub bounciness: f64,
    pub friction: f32,
    pub air_drag: f32,
    pub buoyant: bool,
    pub explosion: Option<Explosion>,
    /// `ContactDamage` of `sulfur_cube_hot` (never attributed to the cube
    /// except against players).
    pub contact_damage: Option<f32>,
    /// `KnockbackModifiers` (horizontal, vertical power).
    pub knockback: (f32, f32),
    /// `SoundSettings`' push impulse threshold and cooldown (seconds).
    pub push_sound: (f32, f32),
}

impl Archetype {
    pub const ALL: [Archetype; 12] = [
        Archetype::Regular,
        Archetype::Bouncy,
        Archetype::SlowBouncy,
        Archetype::SlowFlat,
        Archetype::FastFlat,
        Archetype::Light,
        Archetype::FastSliding,
        Archetype::SlowSliding,
        Archetype::HighResistance,
        Archetype::Sticky,
        Archetype::Explosive,
        Archetype::Hot,
    ];

    /// The registry name.
    pub fn name(self) -> &'static str {
        match self {
            Archetype::Regular => "regular",
            Archetype::Bouncy => "bouncy",
            Archetype::SlowBouncy => "slow_bouncy",
            Archetype::SlowFlat => "slow_flat",
            Archetype::FastFlat => "fast_flat",
            Archetype::Light => "light",
            Archetype::FastSliding => "fast_sliding",
            Archetype::SlowSliding => "slow_sliding",
            Archetype::HighResistance => "high_resistance",
            Archetype::Sticky => "sticky",
            Archetype::Explosive => "explosive",
            Archetype::Hot => "hot",
        }
    }

    /// The archetype's data file (`data/minecraft/sulfur_cube_archetype`).
    pub fn physics(self) -> Physics {
        // (knockback resistance, bounciness, friction factor, drag factor,
        //  buoyant, knockback powers, push sound) as the data gives them.
        let (kb, bounce, friction, drag, buoyant, knockback, push_sound) = match self {
            Archetype::Regular => (-1.0, 0.5, 0.3, 0.1, true, (0.4125, 0.09), (0.2, 0.5)),
            Archetype::Bouncy => (-2.0, 0.9, 0.3, 0.01, true, (0.4125, 0.105), (0.3, 0.7)),
            Archetype::SlowBouncy => (0.4, 0.6, 0.3, 0.05, false, (0.4125, 0.24), (0.05, 0.5)),
            Archetype::SlowFlat => (0.5, 0.4, 0.4, 0.1, false, (0.4125, 0.105), (0.03, 0.9)),
            Archetype::FastFlat => (-1.0, 0.5, 0.2, 0.01, false, (0.9125, 0.09), (0.03, 0.9)),
            Archetype::Light => (-1.0, 1.0, 0.3, 1.8, true, (0.4125, 0.18), (0.2, 0.7)),
            Archetype::FastSliding => (0.5, 0.1, 0.05, 0.01, false, (0.6625, 0.09), (0.05, 1.0)),
            Archetype::SlowSliding => (0.8, 0.1, 0.05, 0.01, false, (0.4125, 0.09), (0.02, 1.0)),
            Archetype::HighResistance => (0.7, 0.2, 1.0, 0.01, false, (0.4125, 0.09), (0.03, 0.7)),
            Archetype::Sticky => (-2.0, 0.0, 2.0, 0.01, false, (0.4125, 0.09), (0.05, 0.5)),
            Archetype::Explosive => (-1.0, 0.5, 0.3, 0.3, true, (0.4125, 0.09), (0.1, 0.7)),
            Archetype::Hot => (-1.0, 0.5, 0.3, 0.1, true, (0.4125, 0.09), (0.2, 0.7)),
        };
        // The modifiers are stored as floats widened to doubles.
        let f = |v: f64| f64::from(v as f32);
        Physics {
            knockback_resistance: f(kb).clamp(-2.0, 1.0),
            explosion_knockback_resistance: f(kb).clamp(0.0, 1.0),
            bounciness: f(bounce).clamp(0.0, 1.0),
            friction: friction as f32,
            air_drag: drag as f32,
            buoyant,
            explosion: (self == Archetype::Explosive).then_some(Explosion {
                power: 3.0,
                causes_fire: false,
                fuse: 120,
            }),
            contact_damage: (self == Archetype::Hot).then_some(1.0),
            knockback: (knockback.0 as f32, knockback.1 as f32),
            push_sound: (push_sound.0 as f32, push_sound.1 as f32),
        }
    }

    /// The Java item names of `#sulfur_cube_archetype/<name>`, expanded,
    /// with their English names (`registry_names.json`) for resolution.
    pub fn item_names(self) -> &'static [(&'static str, &'static str)] {
        match self {
            Archetype::Regular => REGULAR,
            Archetype::Bouncy => BOUNCY,
            Archetype::SlowBouncy => SLOW_BOUNCY,
            Archetype::SlowFlat => SLOW_FLAT,
            Archetype::FastFlat => FAST_FLAT,
            Archetype::Light => LIGHT,
            Archetype::FastSliding => FAST_SLIDING,
            Archetype::SlowSliding => SLOW_SLIDING,
            Archetype::HighResistance => HIGH_RESISTANCE,
            Archetype::Sticky => STICKY,
            Archetype::Explosive => EXPLOSIVE,
            Archetype::Hot => HOT,
        }
    }

    /// The archetype whose tag holds `item` (`SulfurCube.matchingArchetypes`;
    /// no item is in two tags). Names this build lacks are skipped.
    pub fn of(item: Item) -> Option<Archetype> {
        static MAP: std::sync::OnceLock<rustc_hash::FxHashMap<Item, Archetype>> = std::sync::OnceLock::new();
        MAP.get_or_init(|| {
            let mut map = rustc_hash::FxHashMap::default();
            for a in Archetype::ALL {
                for (name, english) in a.item_names() {
                    if let Some(item) = resolve(name, english) {
                        map.insert(item, a);
                    }
                }
            }
            map
        })
        .get(&item)
        .copied()
    }
}

/// The item of a Java item name (a block's item when only the block's name
/// is registered).
pub fn resolve(name: &str, english: &str) -> Option<Item> {
    crate::java_registry::item(name, english)
        .or_else(|| crate::java_registry::block(name, english).map(Item::from_block))
}

/// `#sulfur_cube_swallowable`: every archetype's items.
pub fn swallowable(item: Item) -> bool {
    Archetype::of(item).is_some()
}

/// `#sulfur_cube_food`: slime balls grow babies up.
pub fn is_food(item: Item) -> bool {
    item == Item::SLIME_BALL
}

/// `#sulfur_cube_with_block_immune_to` (expanded): damage a cube with a
/// block takes only as knockback.
pub fn immune_with_block(t: crate::simulation::damage::DamageType) -> bool {
    use crate::simulation::damage::DamageType as D;
    matches!(
        t,
        D::Arrow
            | D::Cactus
            | D::DryOut
            | D::Fall
            | D::FallingAnvil
            | D::FallingBlock
            | D::FallingStalactite
            | D::Freeze
            | D::MaceSmash
            | D::HotFloor
            | D::MobAttack
            | D::MobAttackNoAggro
            | D::MobProjectile
            | D::PlayerAttack
            | D::Spear
            | D::Spit
            | D::Stalagmite
            | D::Sting
            | D::SulfurCubeHot
            | D::SweetBerryBush
            | D::Thrown
            | D::Trident
            | D::WindCharge
            | D::Fireworks
            | D::Explosion
            | D::PlayerExplosion
            | D::BadRespawnPoint
    )
}

/// `LivingEntity.computeModifiedFriction`.
pub fn modified_friction(friction: f32, modifier: f32) -> f32 {
    (1.0 - (1.0 - friction) * modifier).clamp(0.0, 1.0)
}

/// The sulfur cube's sounds beyond its hurt and death calls (original
/// stand-ins for Java's `entity.sulfur_cube.*` events).
#[derive(Clone, Copy, PartialEq, Eq, Debug, Hash)]
pub enum Cue {
    /// `SULFUR_CUBE_SQUISH` (landing without a block) and `SMALL_SQUISH`.
    Squish,
    /// `SULFUR_CUBE_BOUNCE`: landing with a block.
    Bounce,
    /// `SULFUR_CUBE_JUMP` and `SMALL_JUMP`.
    Jump,
    /// `SULFUR_CUBE_ABSORB`: a block swallowed.
    Absorb,
    /// `SULFUR_CUBE_EJECT`: a block sheared out.
    Eject,
    /// `SULFUR_CUBE_SMALL_EAT`: a baby fed a slime ball.
    Eat,
    /// An archetype's hit sound (one shared stand-in).
    Hit,
    /// An archetype's push sound (one shared stand-in).
    Push,
    /// `BUCKET_FILL_SULFUR_CUBE` and `BUCKET_EMPTY_SULFUR_CUBE`.
    BucketFill,
    BucketEmpty,
}

impl Cue {
    pub const ALL: [Cue; 10] = [
        Cue::Squish,
        Cue::Bounce,
        Cue::Jump,
        Cue::Absorb,
        Cue::Eject,
        Cue::Eat,
        Cue::Hit,
        Cue::Push,
        Cue::BucketFill,
        Cue::BucketEmpty,
    ];

    /// Java's sound event name (the regular archetype's for hits and pushes).
    pub fn key(self) -> &'static str {
        match self {
            Cue::Squish => "entity.sulfur_cube.squish",
            Cue::Bounce => "entity.sulfur_cube.bounce",
            Cue::Jump => "entity.sulfur_cube.jump",
            Cue::Absorb => "entity.sulfur_cube.absorb",
            Cue::Eject => "entity.sulfur_cube.eject",
            Cue::Eat => "entity.sulfur_cube.small_eat",
            Cue::Hit => "entity.sulfur_cube.regular.hit",
            Cue::Push => "entity.sulfur_cube.regular.push",
            Cue::BucketFill => "item.bucket.fill_sulfur_cube",
            Cue::BucketEmpty => "item.bucket.empty_sulfur_cube",
        }
    }
}

/// A sulfur cube's saved fields and goal state.
#[derive(Clone, Debug)]
pub struct State {
    /// `pickupTimer` (saved as `pickup_timer`).
    pub pickup_timer: i32,
    /// `pushSoundCooldown` (not saved, as in Java).
    pub push_cooldown: i32,
    /// `fuse`: ticks until it goes off, -1 when not primed (saved as `fuse`).
    pub fuse: i32,
    /// `FROM_BUCKET` (saved as `from_bucket`).
    pub from_bucket: bool,
    /// `CubeMobRandomDirectionGoal.chosenDegrees` (radians) and
    /// `nextRandomizeTime`.
    pub(super) heading: f32,
    pub(super) heading_wait: i32,
    /// What a goal hops toward this update: a tempting player's or an item's
    /// position, filled in by [`Entities::sulfur_cube_upkeep`].
    pub(super) lure: Option<DVec3>,
    /// `tickCount`, for the buoyancy wobble.
    pub(super) ticks: u32,
    /// Seconds not yet spent on a whole ball-physics tick.
    pub(super) carry: f64,
    /// Seconds not yet counted toward the timers.
    pub(super) timer_carry: f64,
    /// `onGround` at the end of the last tick (`AbstractCubeMob.wasOnGround`).
    pub(super) was_on_ground: bool,
}

impl Default for State {
    fn default() -> Self {
        Self {
            pickup_timer: 0,
            push_cooldown: 0,
            fuse: -1,
            from_bucket: false,
            heading: 0.0,
            heading_wait: 0,
            lure: None,
            ticks: 0,
            carry: 0.0,
            timer_carry: 0.0,
            was_on_ground: false,
        }
    }
}

impl State {
    pub fn primed(&self) -> bool {
        self.fuse >= 0
    }

    /// The fields a sulfur cube saves beside the shared mob record.
    pub fn save_into(&self, v: &mut Value) {
        v["sulfur_cube"] = json!({
            "pickup_timer": self.pickup_timer,
            "from_bucket": self.from_bucket,
            "fuse": self.fuse,
        });
    }

    /// Reads them back (`getIntOr("pickup_timer", 0)`, `getIntOr("fuse", -1)`).
    pub fn load(&mut self, v: &Value) {
        let s = &v["sulfur_cube"];
        let int = |v: &Value, default: i32| {
            v.as_i64().map_or(default, |n| n.clamp(i64::from(i32::MIN), i64::from(i32::MAX)) as i32)
        };
        self.pickup_timer = int(&s["pickup_timer"], 0);
        self.from_bucket = s["from_bucket"] == true;
        self.fuse = int(&s["fuse"], -1);
    }
}

/// The ball archetype of a cube holding `body`.
pub fn physics_of(body: Option<&Stack>) -> Option<Physics> {
    body.and_then(|s| Archetype::of(s.item)).map(Archetype::physics)
}

impl Mob {
    /// The block a sulfur cube holds (its `BODY` slot).
    pub fn sulfur_body(&self) -> Option<&Stack> {
        self.sulfur_cube.as_ref()?;
        self.equipment.get(Slot::Body)
    }

    /// Whether this is a sulfur cube holding a block.
    pub fn sulfur_has_block(&self) -> bool {
        self.sulfur_body().is_some()
    }

    /// The ball archetype in effect, if it holds a block.
    pub fn sulfur_physics(&self) -> Option<Physics> {
        physics_of(self.sulfur_body())
    }

    /// `AbstractCubeMob.setSize` with `SulfurCube.setSize`: health 4 × size,
    /// and size 1 makes a baby (an adult's age is 0).
    pub fn set_sulfur_size(&mut self, size: u8) {
        self.size = size.clamp(BABY_SIZE, ADULT_SIZE);
        self.health = HEALTH_PER_SIZE * f32::from(self.size);
        if self.size == BABY_SIZE {
            if !self.baby {
                self.baby = true;
                self.age = BABY_AGE;
            }
        } else {
            self.baby = false;
            self.age = self.age.max(0);
        }
    }

    /// Whether a sulfur cube's damage of type `t` is refused, leaving only
    /// its knockback (`SulfurCube.hurtServer`).
    pub fn sulfur_shrugs_off(&self, t: crate::simulation::damage::DamageType) -> bool {
        self.sulfur_has_block() && immune_with_block(t)
    }

    /// `SulfurCube.primeTime`: an explosive cube starts its fuse (a short
    /// random one when `imminent`) and can no longer be hurt. Returns
    /// whether it did.
    pub fn prime_sulfur_cube(&mut self, imminent: bool, tnt_explodes: bool, rng: &mut super::Rng) -> bool {
        let Some(explosion) = self.sulfur_physics().and_then(|p| p.explosion) else { return false };
        let alive = self.alive();
        let Some(s) = self.sulfur_cube.as_mut() else { return false };
        if !alive || !tnt_explodes || s.primed() {
            return false;
        }
        // `PrimedTnt.getRandomShortFuse`: nextInt(max(1, fuse / 4)) + fuse / 8.
        s.fuse = if imminent {
            rng.next_int((explosion.fuse / 4).max(1) as u32) as i32 + explosion.fuse / 8
        } else {
            explosion.fuse
        };
        true
    }
}

/// What a player's use on a sulfur cube did (`SulfurCube.mobInteract`).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Use {
    /// A baby ate a slime ball (consume one).
    Fed,
    /// Flint and steel (wear one) or a fire charge (consume one) lit an
    /// explosive cube.
    Lit { charge: bool },
    /// `tnt_explodes` is off: the lighting fails with a message.
    Disabled,
    /// Shears took its block out (wear one).
    Sheared,
    /// It swallowed a block from the hand (consume one).
    Swallowed,
    /// An empty bucket scooped it up into this filled bucket.
    Bucketed(Stack),
}

/// The filled bucket's `entity_data` (Java's `bucket_entity_data` health and
/// age lock with the `sulfur_cube_content` block). Bits: 0 set, 1 age locked, 2..=12 health × 100 (at most 800), 13..=28
/// the held item's id + 1 (0 for none).
pub fn bucket_data(m: &Mob) -> u32 {
    let health = ((m.health.clamp(0.0, 20.0) * 100.0).round() as u32).min(0x7FF);
    let item = m.sulfur_body().map_or(0, |s| u32::from(s.item.0) + 1);
    1 | (u32::from(m.age_locked) << 1) | (health << 2) | (item << 13)
}

/// The held item and health a filled bucket's payload carries.
pub fn from_bucket_data(data: u32) -> Option<(Option<Item>, Option<f32>, bool)> {
    if data & 1 == 0 {
        return None;
    }
    let item = (data >> 13) & 0xFFFF;
    let health = (data >> 2) & 0x7FF;
    Some(((item > 0).then(|| Item((item - 1) as u16)), (health > 0).then(|| health as f32 / 100.0), data & 2 != 0))
}

impl Entities {
    /// `SulfurCube.mobInteract` for a player holding `held` on mob `index`.
    /// `None` passes the use on (golden dandelions and leads included).
    pub fn use_sulfur_cube(&mut self, index: usize, held: Option<Stack>) -> Option<Use> {
        let tnt = self.tnt_explodes;
        let m = self.mobs.get_mut(index)?;
        m.sulfur_cube.as_ref()?;
        if !m.alive() {
            return None;
        }
        let held = held?;
        let at = m.pos;
        if m.baby {
            // `isFood && canAgeUp`: ageUp(getSpeedUpSecondsWhenFeeding(-age), true).
            if is_food(held.item) && !m.age_locked {
                let seconds = ((-m.age) / 20) as f32 * 0.1;
                m.age = (m.age + seconds as i32 * 20).min(0);
                if m.age == 0 {
                    m.grow_up_sulfur_cube();
                }
                self.sulfur_sound(Cue::Eat, at);
                return Some(Use::Fed);
            }
            return None;
        }
        if m.sulfur_cube.as_ref().unwrap().primed() {
            return None;
        }
        let explosive = m.sulfur_physics().is_some_and(|p| p.explosion.is_some());
        if explosive && matches!(held.item, Item::FLINT_AND_STEEL | Item::FIRE_CHARGE) {
            if !tnt {
                return Some(Use::Disabled);
            }
            if m.prime_sulfur_cube(false, tnt, &mut self.rng) {
                self.pending_sounds.push(EntityEvent::Sound { sound: MobSound::Fuse, pos: at });
            }
            return Some(Use::Lit { charge: held.item == Item::FIRE_CHARGE });
        }
        if held.item == Item::SHEARS && m.sulfur_has_block() {
            self.shear_sulfur_cube(index);
            return Some(Use::Sheared);
        }
        if swallowable(held.item) {
            return self.sulfur_swallow(index, Stack::new(held.item, 1)).then_some(Use::Swallowed);
        }
        if held.item == Item::BUCKET {
            let m = &self.mobs[index];
            let mut filled = Stack::new(BUCKET, 1);
            filled.entity_data = bucket_data(m);
            filled.name = m.name;
            self.mobs.swap_remove(index);
            self.sulfur_sound(Cue::BucketFill, at);
            return Some(Use::Bucketed(filled));
        }
        None
    }

    /// `SulfurCube.equipItem`: an adult takes `stack` into its body, spitting
    /// out a different block it held. Returns false for babies and for the
    /// block it already holds.
    pub fn sulfur_swallow(&mut self, index: usize, stack: Stack) -> bool {
        let Some(m) = self.mobs.get_mut(index) else { return false };
        if m.baby || m.sulfur_cube.is_none() {
            return false;
        }
        let old = m.equipment.get(Slot::Body).copied();
        if old.is_some_and(|o| o.item == stack.item) {
            return false;
        }
        let at = m.pos + DVec3::Y * m.shape().height;
        m.equipment.set(Slot::Body, Some(Stack { count: 1, ..stack }));
        m.equipment.set_guaranteed_drop(Slot::Body);
        m.sulfur_body_changed();
        let pos = m.pos;
        if let Some(old) = old {
            self.scatter(old, at - DVec3::Y);
        }
        self.sulfur_sound(Cue::Absorb, pos);
        true
    }

    /// v0.16 Track H: `SulfurCubeBlockDispenseItemBehavior.dispenseBlock`:
    /// the first sulfur cube whose box meets `cell` that takes the block
    /// (`equipItem`). Returns whether one did.
    pub fn feed_sulfur_cube(&mut self, cell: glam::IVec3, stack: Stack) -> bool {
        let (lo, hi) = (cell.as_dvec3(), cell.as_dvec3() + DVec3::ONE);
        let cubes: Vec<usize> = (0..self.mobs.len())
            .filter(|&i| {
                let m = &self.mobs[i];
                let (a, b) = m.aabb();
                m.sulfur_cube.is_some() && m.alive() && a.cmplt(hi).all() && b.cmpgt(lo).all()
            })
            .collect();
        cubes.into_iter().any(|i| self.sulfur_swallow(i, Stack { count: 1, ..stack }))
    }

    /// `SulfurCube.shear`: the block pops out and the cube waits
    /// [`PICKUP_TIMER`] ticks before taking another.
    pub fn shear_sulfur_cube(&mut self, index: usize) -> bool {
        let Some(m) = self.mobs.get_mut(index) else { return false };
        let Some(block) = m.sulfur_cube.as_ref().and(m.equipment.take(Slot::Body)) else { return false };
        m.sulfur_cube.as_mut().unwrap().pickup_timer = PICKUP_TIMER;
        m.sulfur_body_changed();
        let (pos, top) = (m.pos, m.pos + DVec3::Y * (m.shape().height - 1.0));
        self.scatter(block, top);
        self.sulfur_sound(Cue::Eject, pos);
        true
    }

    /// `MobBucketItem.spawn`: a released cube keeps its block, health and
    /// age lock, is marked `from_bucket` and never despawns.
    pub fn release_sulfur_cube(&mut self, stack: Stack, pos: DVec3) -> bool {
        if stack.item != BUCKET {
            return false;
        }
        self.spawn(MobKind::SulfurCube, pos);
        let m = self.mobs.last_mut().unwrap();
        m.set_sulfur_size(ADULT_SIZE);
        m.persistent = true;
        m.name = stack.name;
        m.sulfur_cube.as_mut().unwrap().from_bucket = true;
        if let Some((item, health, locked)) = from_bucket_data(stack.entity_data) {
            if let Some(item) = item {
                m.equipment.set(Slot::Body, Some(Stack::new(item, 1)));
                m.equipment.set_guaranteed_drop(Slot::Body);
                m.sulfur_body_changed();
            }
            if let Some(health) = health {
                m.health = health.min(m.max_health());
            }
            m.age_locked = locked;
        }
        self.sulfur_sound(Cue::BucketEmpty, pos);
        true
    }

    pub(super) fn sulfur_sound(&mut self, cue: Cue, pos: DVec3) {
        self.pending_sounds.push(EntityEvent::Sound { sound: MobSound::SulfurCube(cue), pos });
    }

    /// The entity-level sulfur cube pass, once per update before mobs move:
    /// fuses and explosions, redstone priming, timers, item pickups, the
    /// goals' lures, player pushes and contact damage.
    pub(super) fn sulfur_cube_upkeep<W: MobWorld + ?Sized>(
        &mut self,
        dt: f64,
        world: &W,
        ctx: &Ctx,
        events: &mut Vec<EntityEvent>,
    ) {
        if !self.mobs.iter().any(|m| m.sulfur_cube.is_some()) {
            return;
        }
        let tnt = self.tnt_explodes;
        let griefing = self.villager_griefing;
        // Player motion per tick since the last update (`getKnownSpeed`).
        let speeds: Vec<(PlayerId, DVec3)> = ctx
            .players
            .iter()
            .map(|t| {
                let last = self.player_spots.iter().find(|(id, _)| *id == t.id).map_or(t.pos, |&(_, p)| p);
                (t.id, (t.pos - last) * (TICK / dt.max(TICK)))
            })
            .collect();
        let mut i = 0;
        while i < self.mobs.len() {
            let physics = self.mobs[i].sulfur_physics();
            let Some(mut s) = self.mobs[i].sulfur_cube.take() else {
                i += 1;
                continue;
            };
            let m = &self.mobs[i];
            if !m.alive() || !world.entity_ticking(m.pos.floor().as_ivec3()) {
                self.mobs[i].sulfur_cube = Some(s);
                i += 1;
                continue;
            }
            s.timer_carry += dt;
            let ticks = (s.timer_carry / TICK).floor() as i32;
            s.timer_carry -= f64::from(ticks) * TICK;
            s.pickup_timer = (s.pickup_timer - ticks).max(0);
            s.push_cooldown = (s.push_cooldown - ticks).max(0);
            let mut exploded = false;
            if s.primed() {
                s.fuse = (s.fuse - ticks).max(0);
                if s.fuse == 0 && physics.and_then(|p| p.explosion).is_some() {
                    exploded = true;
                }
            }
            self.mobs[i].sulfur_cube = Some(s);
            if exploded {
                let explosion = physics.and_then(|p| p.explosion).unwrap();
                let m = self.mobs.swap_remove(i);
                if tnt {
                    events.push(EntityEvent::Explosion {
                        center: m.pos + DVec3::Y * 0.0625,
                        power: explosion.power,
                        cause: crate::simulation::damage::Cause::TNT,
                        credit_player: false,
                    });
                }
                continue;
            }
            // `primeWhenOnPoweredPosition`.
            if physics.is_some_and(|p| p.explosion.is_some())
                && world.redstone_signal(self.mobs[i].pos.floor().as_ivec3()) > 0
                && self.mobs[i].prime_sulfur_cube(false, tnt, &mut self.rng)
            {
                events.push(EntityEvent::Sound { sound: MobSound::Fuse, pos: self.mobs[i].pos });
            }
            // Burning primes it (`hurtServer` with an `#is_fire` source).
            if physics.is_some_and(|p| p.explosion.is_some())
                && self.mobs[i].burning
                && self.mobs[i].prime_sulfur_cube(false, tnt, &mut self.rng)
            {
                events.push(EntityEvent::Sound { sound: MobSound::Fuse, pos: self.mobs[i].pos });
            }
            self.sulfur_age_and_land(i, ticks, events);
            self.sulfur_lure(i, ctx);
            if griefing {
                self.sulfur_pickup(i);
            }
            if physics.is_some() {
                self.sulfur_player_push(i, ctx, &speeds, events);
                self.sulfur_mob_contact(i);
            }
            i += 1;
        }
    }

    /// `AgeableMob.aiStep`'s growing up (babies that aren't age locked gain
    /// a tick of age a tick) and `AbstractCubeMob.tick`'s landing: goo
    /// particles around the base and a squish (a bounce with a block), and a
    /// jump sound as a hop leaves the ground.
    fn sulfur_age_and_land(&mut self, i: usize, ticks: i32, events: &mut Vec<EntityEvent>) {
        let m = &mut self.mobs[i];
        if m.baby && !m.age_locked && ticks > 0 {
            m.age = (m.age + ticks).min(0);
            if m.age == 0 {
                m.grow_up_sulfur_cube();
            }
        }
        let landed = m.on_ground && !m.sulfur_cube.as_ref().unwrap().was_on_ground;
        let left = !m.on_ground && m.sulfur_cube.as_ref().unwrap().was_on_ground && m.vel.y > 0.0;
        m.sulfur_cube.as_mut().unwrap().was_on_ground = m.on_ground;
        let (pos, width, block) = (m.pos, m.shape().half_width, m.sulfur_has_block());
        if landed {
            let mut burst = crate::particles::Burst::new(
                crate::particles::Kind::Splash,
                pos,
                (width * 2.0 * 16.0).round().clamp(1.0, 64.0) as u16,
            );
            burst.spread = DVec3::new(width, 0.0, width);
            burst.color = Some([0.86, 0.78, 0.3, 1.0]);
            self.particles.push(crate::particles::Request::Burst(burst));
            events.push(EntityEvent::Sound {
                sound: MobSound::SulfurCube(if block { Cue::Bounce } else { Cue::Squish }),
                pos,
            });
        } else if left && !block {
            events.push(EntityEvent::Sound { sound: MobSound::SulfurCube(Cue::Jump), pos });
        }
    }

    /// The goal targets of a cube without a block: a player holding what it
    /// wants within the tempt range, else (adults whose pickup timer ran
    /// out) the nearest swallowable item within 8 blocks.
    fn sulfur_lure(&mut self, i: usize, ctx: &Ctx) {
        let m = &self.mobs[i];
        let s = m.sulfur_cube.as_ref().unwrap();
        let lure = if m.sulfur_has_block() {
            None
        } else {
            let baby = m.baby;
            let wanted = |item: Item| if baby { is_food(item) } else { swallowable(item) };
            let player = ctx
                .players
                .iter()
                .filter(|t| t.alive && !t.spectator && t.held_item.is_some_and(wanted))
                .filter(|t| t.pos.distance_squared(m.pos) <= TEMPT_RANGE * TEMPT_RANGE)
                .min_by(|a, b| a.pos.distance_squared(m.pos).total_cmp(&b.pos.distance_squared(m.pos)))
                .map(|t| t.pos);
            player.or_else(|| {
                if baby || s.pickup_timer > 0 {
                    return None;
                }
                let (min, max) = m.aabb();
                let (min, max) = (min - DVec3::splat(SEARCH_RANGE), max + DVec3::splat(SEARCH_RANGE));
                self.items
                    .iter()
                    .filter(|it| it.pickup_delay <= 0.0 && it.stack.count > 0 && swallowable(it.stack.item))
                    .filter(|it| it.pos.cmpge(min).all() && it.pos.cmple(max).all())
                    .min_by(|a, b| a.pos.distance_squared(m.pos).total_cmp(&b.pos.distance_squared(m.pos)))
                    .map(|it| it.pos)
            })
        };
        self.mobs[i].sulfur_cube.as_mut().unwrap().lure = lure;
    }

    /// `Mob.aiStep`'s loot pickup with `SulfurCube.pickUpItem`: an adult
    /// without a block whose pickup timer ran out takes one swallowable
    /// block from an item it touches (the box grown by 1, 0, 1).
    fn sulfur_pickup(&mut self, i: usize) {
        let m = &self.mobs[i];
        if m.baby || m.sulfur_has_block() || m.sulfur_cube.as_ref().unwrap().pickup_timer > 0 {
            return;
        }
        let (min, max) = m.aabb();
        let (min, max) = (min - DVec3::new(1.0, 0.0, 1.0), max + DVec3::new(1.0, 0.0, 1.0));
        let Some(j) = self.items.iter().position(|it| {
            let (imin, imax) = it.aabb();
            it.pickup_delay <= 0.0
                && it.stack.count > 0
                && swallowable(it.stack.item)
                && imin.cmplt(max).all()
                && imax.cmpgt(min).all()
        }) else {
            return;
        };
        let taken = Stack { count: 1, ..self.items[j].stack };
        self.items[j].stack.count -= 1;
        if self.items[j].stack.count == 0 {
            self.items.swap_remove(j);
        }
        let m = &mut self.mobs[i];
        m.equipment.set(Slot::Body, Some(taken));
        m.equipment.set_guaranteed_drop(Slot::Body);
        m.persistent = true;
        m.sulfur_body_changed();
        let pos = m.pos;
        self.sulfur_sound(Cue::Absorb, pos);
    }

    /// `SulfurCube.playerTouch` → `playerPush`: a player walking into a
    /// cube with a block shoves it along their own speed, and a hot cube
    /// burns them.
    fn sulfur_player_push(&mut self, i: usize, ctx: &Ctx, speeds: &[(PlayerId, DVec3)], events: &mut Vec<EntityEvent>) {
        let Some(p) = self.mobs[i].sulfur_physics() else { return };
        for t in ctx.players.iter().filter(|t| t.alive && !t.spectator) {
            let m = &self.mobs[i];
            let shape = m.shape();
            // `Player.touch` reaches entities in its box grown by (1, 0.5, 1).
            let (pmin, pmax) = t.shape.aabb(t.pos);
            let (pmin, pmax) = (pmin - DVec3::new(1.0, 0.5, 1.0), pmax + DVec3::new(1.0, 0.5, 1.0));
            let (cmin, cmax) = m.aabb();
            if !(cmin.cmplt(pmax).all() && cmax.cmpgt(pmin).all()) {
                continue;
            }
            let to_cube = m.pos - t.pos;
            let flat = DVec3::new(to_cube.x, 0.0, to_cube.z);
            let (feet, top) = (t.pos.y, t.pos.y + t.shape.height);
            if flat.length() >= PUSH_DISTANCE || feet > m.pos.y + shape.height || top <= m.pos.y {
                continue;
            }
            let knockback = (1.0 - p.knockback_resistance).max(0.0);
            let dir = crate::knockback::java_normalize(flat) * knockback;
            let scale = if t.passenger { VEHICLE_PUSH_SCALE } else { PLAYER_PUSH_SCALE };
            let speed = speeds.iter().find(|(id, _)| *id == t.id).map_or(0.0, |(_, v)| v.length());
            let speed = (speed * 2.0 * f64::from(scale)).clamp(0.0, MAX_PLAYER_PUSH_SPEED);
            let lift = if m.on_ground { knockback * f64::from(VERTICAL_PUSH) } else { 0.0 };
            let push = DVec3::new(dir.x, lift, dir.z) * speed;
            let pos = m.pos;
            let m = &mut self.mobs[i];
            m.vel += push / TICK;
            let s = m.sulfur_cube.as_mut().unwrap();
            let threshold = f64::from(p.push_sound.0);
            if push.length_squared() > threshold * threshold && s.push_cooldown <= 0 {
                s.push_cooldown = (p.push_sound.1 * 20.0) as i32;
                self.sulfur_sound(Cue::Push, pos);
            }
            if let Some(amount) = p.contact_damage {
                events.push(EntityEvent::PlayerHit {
                    player: t.id,
                    damage: amount,
                    knockback: glam::Vec3::ZERO,
                    java: None,
                    cause: HOT_CAUSE,
                });
            }
        }
    }

    /// `SulfurCube.doPush` → `applyContactDamage`: a hot cube burns the mobs
    /// it bumps into (unattributed, so they don't turn on it).
    fn sulfur_mob_contact(&mut self, i: usize) {
        let Some(amount) = self.mobs[i].sulfur_physics().and_then(|p| p.contact_damage) else { return };
        let (min, max) = self.mobs[i].aabb();
        for j in 0..self.mobs.len() {
            if j == i {
                continue;
            }
            let o = &self.mobs[j];
            let (omin, omax) = o.aabb();
            // v1.0 gap Track 8d: the hurt cooldown spaces the contact hits.
            if !o.alive() || !(omin.cmplt(max).all() && omax.cmpgt(min).all()) {
                continue;
            }
            if o.sulfur_shrugs_off(crate::simulation::damage::DamageType::SulfurCubeHot) {
                continue;
            }
            let rng = &mut self.rng;
            self.mobs[j].hit_cause = Some(HOT_CAUSE);
            self.mobs[j].damage_bypassing_wolf_armor(amount, None, rng);
        }
    }
}

/// `death.attack.sulfurCubeHot.player`: "%2$s showed %1$s that not just the
/// floor is lava".
pub const HOT_CAUSE: crate::simulation::damage::Cause = crate::simulation::damage::Cause::new(
    crate::simulation::damage::DamageType::SulfurCubeHot,
    "was shown by a sulfur cube that not just the floor is lava",
);

#[cfg(test)]
mod tests;

// The Java item names of each `#sulfur_cube_archetype/*` tag, expanded
// (`tools/extract_java_sulfur_cube_fixtures.py`).
const REGULAR: &[(&str, &str)] = &[
    ("white_concrete_powder", "White Concrete Powder"),
    ("orange_concrete_powder", "Orange Concrete Powder"),
    ("magenta_concrete_powder", "Magenta Concrete Powder"),
    ("light_blue_concrete_powder", "Light Blue Concrete Powder"),
    ("yellow_concrete_powder", "Yellow Concrete Powder"),
    ("lime_concrete_powder", "Lime Concrete Powder"),
    ("pink_concrete_powder", "Pink Concrete Powder"),
    ("gray_concrete_powder", "Gray Concrete Powder"),
    ("light_gray_concrete_powder", "Light Gray Concrete Powder"),
    ("cyan_concrete_powder", "Cyan Concrete Powder"),
    ("purple_concrete_powder", "Purple Concrete Powder"),
    ("blue_concrete_powder", "Blue Concrete Powder"),
    ("brown_concrete_powder", "Brown Concrete Powder"),
    ("green_concrete_powder", "Green Concrete Powder"),
    ("red_concrete_powder", "Red Concrete Powder"),
    ("black_concrete_powder", "Black Concrete Powder"),
    ("mud", "Mud"),
    ("muddy_mangrove_roots", "Muddy Mangrove Roots"),
    ("packed_mud", "Packed Mud"),
    ("coal_block", "Block of Coal"),
    ("dirt", "Dirt"),
    ("coarse_dirt", "Coarse Dirt"),
    ("rooted_dirt", "Rooted Dirt"),
    ("podzol", "Podzol"),
    ("grass_block", "Grass Block"),
    ("clay", "Clay"),
    ("bone_block", "Bone Block"),
];
const BOUNCY: &[(&str, &str)] = &[
    ("oak_planks", "Oak Planks"),
    ("spruce_planks", "Spruce Planks"),
    ("birch_planks", "Birch Planks"),
    ("jungle_planks", "Jungle Planks"),
    ("acacia_planks", "Acacia Planks"),
    ("dark_oak_planks", "Dark Oak Planks"),
    ("pale_oak_planks", "Pale Oak Planks"),
    ("crimson_planks", "Crimson Planks"),
    ("warped_planks", "Warped Planks"),
    ("mangrove_planks", "Mangrove Planks"),
    ("bamboo_planks", "Bamboo Planks"),
    ("cherry_planks", "Cherry Planks"),
    ("poplar_planks", "Poplar Planks"),
    ("bamboo_mosaic", "Bamboo Mosaic"),
    ("dark_oak_log", "Dark Oak Log"),
    ("dark_oak_wood", "Dark Oak Wood"),
    ("stripped_dark_oak_log", "Stripped Dark Oak Log"),
    ("stripped_dark_oak_wood", "Stripped Dark Oak Wood"),
    ("pale_oak_log", "Pale Oak Log"),
    ("pale_oak_wood", "Pale Oak Wood"),
    ("stripped_pale_oak_log", "Stripped Pale Oak Log"),
    ("stripped_pale_oak_wood", "Stripped Pale Oak Wood"),
    ("oak_log", "Oak Log"),
    ("oak_wood", "Oak Wood"),
    ("stripped_oak_log", "Stripped Oak Log"),
    ("stripped_oak_wood", "Stripped Oak Wood"),
    ("acacia_log", "Acacia Log"),
    ("acacia_wood", "Acacia Wood"),
    ("stripped_acacia_log", "Stripped Acacia Log"),
    ("stripped_acacia_wood", "Stripped Acacia Wood"),
    ("birch_log", "Birch Log"),
    ("birch_wood", "Birch Wood"),
    ("stripped_birch_log", "Stripped Birch Log"),
    ("stripped_birch_wood", "Stripped Birch Wood"),
    ("jungle_log", "Jungle Log"),
    ("jungle_wood", "Jungle Wood"),
    ("stripped_jungle_log", "Stripped Jungle Log"),
    ("stripped_jungle_wood", "Stripped Jungle Wood"),
    ("spruce_log", "Spruce Log"),
    ("spruce_wood", "Spruce Wood"),
    ("stripped_spruce_log", "Stripped Spruce Log"),
    ("stripped_spruce_wood", "Stripped Spruce Wood"),
    ("mangrove_log", "Mangrove Log"),
    ("mangrove_wood", "Mangrove Wood"),
    ("stripped_mangrove_log", "Stripped Mangrove Log"),
    ("stripped_mangrove_wood", "Stripped Mangrove Wood"),
    ("cherry_log", "Cherry Log"),
    ("cherry_wood", "Cherry Wood"),
    ("stripped_cherry_log", "Stripped Cherry Log"),
    ("stripped_cherry_wood", "Stripped Cherry Wood"),
    ("poplar_log", "Poplar Log"),
    ("poplar_wood", "Poplar Wood"),
    ("stripped_poplar_log", "Stripped Poplar Log"),
    ("stripped_poplar_wood", "Stripped Poplar Wood"),
    ("crimson_stem", "Crimson Stem"),
    ("stripped_crimson_stem", "Stripped Crimson Stem"),
    ("crimson_hyphae", "Crimson Hyphae"),
    ("stripped_crimson_hyphae", "Stripped Crimson Hyphae"),
    ("warped_stem", "Warped Stem"),
    ("stripped_warped_stem", "Stripped Warped Stem"),
    ("warped_hyphae", "Warped Hyphae"),
    ("stripped_warped_hyphae", "Stripped Warped Hyphae"),
    ("bamboo_block", "Block of Bamboo"),
    ("stripped_bamboo_block", "Block of Stripped Bamboo"),
];
const SLOW_BOUNCY: &[(&str, &str)] = &[
    ("amethyst_block", "Block of Amethyst"),
    ("andesite", "Andesite"),
    ("basalt", "Basalt"),
    ("blackstone", "Blackstone"),
    ("bricks", "Bricks"),
    ("calcite", "Calcite"),
    ("chiseled_cinnabar", "Chiseled Cinnabar"),
    ("chiseled_deepslate", "Chiseled Deepslate"),
    ("chiseled_nether_bricks", "Chiseled Nether Bricks"),
    ("chiseled_polished_blackstone", "Chiseled Polished Blackstone"),
    ("chiseled_quartz_block", "Chiseled Quartz Block"),
    ("chiseled_red_sandstone", "Chiseled Red Sandstone"),
    ("chiseled_sandstone", "Chiseled Sandstone"),
    ("chiseled_stone_bricks", "Chiseled Stone Bricks"),
    ("chiseled_sulfur", "Chiseled Sulfur"),
    ("chiseled_tuff", "Chiseled Tuff"),
    ("chiseled_tuff_bricks", "Chiseled Tuff Bricks"),
    ("cinnabar", "Cinnabar"),
    ("cinnabar_bricks", "Cinnabar Bricks"),
    ("cobbled_deepslate", "Cobbled Deepslate"),
    ("cobblestone", "Cobblestone"),
    ("cracked_deepslate_bricks", "Cracked Deepslate Bricks"),
    ("cracked_deepslate_tiles", "Cracked Deepslate Tiles"),
    ("cracked_nether_bricks", "Cracked Nether Bricks"),
    ("cracked_polished_blackstone_bricks", "Cracked Polished Blackstone Bricks"),
    ("cracked_stone_bricks", "Cracked Stone Bricks"),
    ("crimson_nylium", "Crimson Nylium"),
    ("crying_obsidian", "Crying Obsidian"),
    ("cut_red_sandstone", "Cut Red Sandstone"),
    ("cut_sandstone", "Cut Sandstone"),
    ("dark_prismarine", "Dark Prismarine"),
    ("deepslate", "Deepslate"),
    ("deepslate_bricks", "Deepslate Bricks"),
    ("deepslate_tiles", "Deepslate Tiles"),
    ("diamond_block", "Block of Diamond"),
    ("diorite", "Diorite"),
    ("dripstone_block", "Dripstone Block"),
    ("emerald_block", "Block of Emerald"),
    ("end_stone", "End Stone"),
    ("end_stone_bricks", "End Stone Bricks"),
    ("gilded_blackstone", "Gilded Blackstone"),
    ("glowstone", "Glowstone"),
    ("granite", "Granite"),
    ("lapis_block", "Block of Lapis Lazuli"),
    ("mossy_cobblestone", "Mossy Cobblestone"),
    ("mossy_stone_bricks", "Mossy Stone Bricks"),
    ("mud_bricks", "Mud Bricks"),
    ("nether_bricks", "Nether Bricks"),
    ("netherrack", "Netherrack"),
    ("observer", "Observer"),
    ("obsidian", "Obsidian"),
    ("polished_andesite", "Polished Andesite"),
    ("polished_basalt", "Polished Basalt"),
    ("polished_blackstone", "Polished Blackstone"),
    ("polished_blackstone_bricks", "Polished Blackstone Bricks"),
    ("polished_cinnabar", "Polished Cinnabar"),
    ("polished_deepslate", "Polished Deepslate"),
    ("polished_diorite", "Polished Diorite"),
    ("polished_granite", "Polished Granite"),
    ("polished_sulfur", "Polished Sulfur"),
    ("polished_tuff", "Polished Tuff"),
    ("prismarine", "Prismarine"),
    ("prismarine_bricks", "Prismarine Bricks"),
    ("purpur_block", "Purpur Block"),
    ("purpur_pillar", "Purpur Pillar"),
    ("quartz_block", "Block of Quartz"),
    ("quartz_bricks", "Quartz Bricks"),
    ("nether_quartz_ore", "Nether Quartz Ore"),
    ("quartz_pillar", "Quartz Pillar"),
    ("red_nether_bricks", "Red Nether Bricks"),
    ("red_sandstone", "Red Sandstone"),
    ("redstone_lamp", "Redstone Lamp"),
    ("sandstone", "Sandstone"),
    ("sea_lantern", "Sea Lantern"),
    ("smooth_basalt", "Smooth Basalt"),
    ("smooth_quartz", "Smooth Quartz Block"),
    ("smooth_red_sandstone", "Smooth Red Sandstone"),
    ("smooth_sandstone", "Smooth Sandstone"),
    ("smooth_stone", "Smooth Stone"),
    ("stone", "Stone"),
    ("stone_bricks", "Stone Bricks"),
    ("sulfur", "Sulfur"),
    ("sulfur_bricks", "Sulfur Bricks"),
    ("tuff", "Tuff"),
    ("tuff_bricks", "Tuff Bricks"),
    ("warped_nylium", "Warped Nylium"),
    ("white_concrete", "White Concrete"),
    ("orange_concrete", "Orange Concrete"),
    ("magenta_concrete", "Magenta Concrete"),
    ("light_blue_concrete", "Light Blue Concrete"),
    ("yellow_concrete", "Yellow Concrete"),
    ("lime_concrete", "Lime Concrete"),
    ("pink_concrete", "Pink Concrete"),
    ("gray_concrete", "Gray Concrete"),
    ("light_gray_concrete", "Light Gray Concrete"),
    ("cyan_concrete", "Cyan Concrete"),
    ("purple_concrete", "Purple Concrete"),
    ("blue_concrete", "Blue Concrete"),
    ("brown_concrete", "Brown Concrete"),
    ("green_concrete", "Green Concrete"),
    ("red_concrete", "Red Concrete"),
    ("black_concrete", "Black Concrete"),
    ("coal_ore", "Coal Ore"),
    ("deepslate_coal_ore", "Deepslate Coal Ore"),
    ("lapis_ore", "Lapis Lazuli Ore"),
    ("deepslate_lapis_ore", "Deepslate Lapis Lazuli Ore"),
    ("redstone_ore", "Redstone Ore"),
    ("deepslate_redstone_ore", "Deepslate Redstone Ore"),
    ("diamond_ore", "Diamond Ore"),
    ("deepslate_diamond_ore", "Deepslate Diamond Ore"),
    ("emerald_ore", "Emerald Ore"),
    ("deepslate_emerald_ore", "Deepslate Emerald Ore"),
    ("terracotta", "Terracotta"),
    ("white_terracotta", "White Terracotta"),
    ("orange_terracotta", "Orange Terracotta"),
    ("magenta_terracotta", "Magenta Terracotta"),
    ("light_blue_terracotta", "Light Blue Terracotta"),
    ("yellow_terracotta", "Yellow Terracotta"),
    ("lime_terracotta", "Lime Terracotta"),
    ("pink_terracotta", "Pink Terracotta"),
    ("gray_terracotta", "Gray Terracotta"),
    ("light_gray_terracotta", "Light Gray Terracotta"),
    ("cyan_terracotta", "Cyan Terracotta"),
    ("purple_terracotta", "Purple Terracotta"),
    ("blue_terracotta", "Blue Terracotta"),
    ("brown_terracotta", "Brown Terracotta"),
    ("green_terracotta", "Green Terracotta"),
    ("red_terracotta", "Red Terracotta"),
    ("black_terracotta", "Black Terracotta"),
    ("white_glazed_terracotta", "White Glazed Terracotta"),
    ("orange_glazed_terracotta", "Orange Glazed Terracotta"),
    ("magenta_glazed_terracotta", "Magenta Glazed Terracotta"),
    ("light_blue_glazed_terracotta", "Light Blue Glazed Terracotta"),
    ("yellow_glazed_terracotta", "Yellow Glazed Terracotta"),
    ("lime_glazed_terracotta", "Lime Glazed Terracotta"),
    ("pink_glazed_terracotta", "Pink Glazed Terracotta"),
    ("gray_glazed_terracotta", "Gray Glazed Terracotta"),
    ("light_gray_glazed_terracotta", "Light Gray Glazed Terracotta"),
    ("cyan_glazed_terracotta", "Cyan Glazed Terracotta"),
    ("purple_glazed_terracotta", "Purple Glazed Terracotta"),
    ("blue_glazed_terracotta", "Blue Glazed Terracotta"),
    ("brown_glazed_terracotta", "Brown Glazed Terracotta"),
    ("green_glazed_terracotta", "Green Glazed Terracotta"),
    ("red_glazed_terracotta", "Red Glazed Terracotta"),
    ("black_glazed_terracotta", "Black Glazed Terracotta"),
];
const SLOW_FLAT: &[(&str, &str)] = &[
    ("iron_block", "Block of Iron"),
    ("gold_block", "Block of Gold"),
    ("raw_copper_block", "Block of Raw Copper"),
    ("raw_gold_block", "Block of Raw Gold"),
    ("raw_iron_block", "Block of Raw Iron"),
    ("gold_ore", "Gold Ore"),
    ("nether_gold_ore", "Nether Gold Ore"),
    ("deepslate_gold_ore", "Deepslate Gold Ore"),
    ("iron_ore", "Iron Ore"),
    ("deepslate_iron_ore", "Deepslate Iron Ore"),
    ("copper_ore", "Copper Ore"),
    ("deepslate_copper_ore", "Deepslate Copper Ore"),
    ("netherite_block", "Block of Netherite"),
    ("ancient_debris", "Ancient Debris"),
    ("copper_block", "Block of Copper"),
    ("exposed_copper", "Exposed Copper"),
    ("weathered_copper", "Weathered Copper"),
    ("oxidized_copper", "Oxidized Copper"),
    ("waxed_copper_block", "Waxed Block of Copper"),
    ("waxed_exposed_copper", "Waxed Exposed Copper"),
    ("waxed_weathered_copper", "Waxed Weathered Copper"),
    ("waxed_oxidized_copper", "Waxed Oxidized Copper"),
    ("copper_bulb", "Copper Bulb"),
    ("exposed_copper_bulb", "Exposed Copper Bulb"),
    ("weathered_copper_bulb", "Weathered Copper Bulb"),
    ("oxidized_copper_bulb", "Oxidized Copper Bulb"),
    ("waxed_copper_bulb", "Waxed Copper Bulb"),
    ("waxed_exposed_copper_bulb", "Waxed Exposed Copper Bulb"),
    ("waxed_weathered_copper_bulb", "Waxed Weathered Copper Bulb"),
    ("waxed_oxidized_copper_bulb", "Waxed Oxidized Copper Bulb"),
    ("cut_copper", "Cut Copper"),
    ("exposed_cut_copper", "Exposed Cut Copper"),
    ("weathered_cut_copper", "Weathered Cut Copper"),
    ("oxidized_cut_copper", "Oxidized Cut Copper"),
    ("waxed_cut_copper", "Waxed Cut Copper"),
    ("waxed_exposed_cut_copper", "Waxed Exposed Cut Copper"),
    ("waxed_weathered_cut_copper", "Waxed Weathered Cut Copper"),
    ("waxed_oxidized_cut_copper", "Waxed Oxidized Cut Copper"),
    ("chiseled_copper", "Chiseled Copper"),
    ("exposed_chiseled_copper", "Exposed Chiseled Copper"),
    ("weathered_chiseled_copper", "Weathered Chiseled Copper"),
    ("oxidized_chiseled_copper", "Oxidized Chiseled Copper"),
    ("waxed_chiseled_copper", "Waxed Chiseled Copper"),
    ("waxed_exposed_chiseled_copper", "Waxed Exposed Chiseled Copper"),
    ("waxed_weathered_chiseled_copper", "Waxed Weathered Chiseled Copper"),
    ("waxed_oxidized_chiseled_copper", "Waxed Oxidized Chiseled Copper"),
];
const FAST_FLAT: &[(&str, &str)] = &[
    ("tube_coral_block", "Tube Coral Block"),
    ("brain_coral_block", "Brain Coral Block"),
    ("bubble_coral_block", "Bubble Coral Block"),
    ("fire_coral_block", "Fire Coral Block"),
    ("horn_coral_block", "Horn Coral Block"),
    ("dead_tube_coral_block", "Dead Tube Coral Block"),
    ("dead_brain_coral_block", "Dead Brain Coral Block"),
    ("dead_bubble_coral_block", "Dead Bubble Coral Block"),
    ("dead_fire_coral_block", "Dead Fire Coral Block"),
    ("dead_horn_coral_block", "Dead Horn Coral Block"),
    ("sponge", "Sponge"),
    ("wet_sponge", "Wet Sponge"),
    ("dried_kelp_block", "Dried Kelp Block"),
    ("moss_block", "Moss Block"),
    ("pale_moss_block", "Pale Moss Block"),
    ("resin_block", "Block of Resin"),
    ("resin_bricks", "Resin Bricks"),
    ("chiseled_resin_bricks", "Chiseled Resin Bricks"),
    ("melon", "Melon"),
    ("hay_block", "Hay Bale"),
    ("pumpkin", "Pumpkin"),
    ("carved_pumpkin", "Carved Pumpkin"),
    ("jack_o_lantern", "Jack o'Lantern"),
    ("ochre_froglight", "Ochre Froglight"),
    ("pearlescent_froglight", "Pearlescent Froglight"),
    ("verdant_froglight", "Verdant Froglight"),
];
const LIGHT: &[(&str, &str)] = &[
    ("white_wool", "White Wool"),
    ("orange_wool", "Orange Wool"),
    ("magenta_wool", "Magenta Wool"),
    ("light_blue_wool", "Light Blue Wool"),
    ("yellow_wool", "Yellow Wool"),
    ("lime_wool", "Lime Wool"),
    ("pink_wool", "Pink Wool"),
    ("gray_wool", "Gray Wool"),
    ("light_gray_wool", "Light Gray Wool"),
    ("cyan_wool", "Cyan Wool"),
    ("purple_wool", "Purple Wool"),
    ("blue_wool", "Blue Wool"),
    ("brown_wool", "Brown Wool"),
    ("green_wool", "Green Wool"),
    ("red_wool", "Red Wool"),
    ("black_wool", "Black Wool"),
];
const FAST_SLIDING: &[(&str, &str)] =
    &[("blue_ice", "Blue Ice"), ("packed_ice", "Packed Ice"), ("snow_block", "Snow Block")];
const SLOW_SLIDING: &[(&str, &str)] = &[
    ("brown_mushroom_block", "Brown Mushroom Block"),
    ("red_mushroom_block", "Red Mushroom Block"),
    ("mushroom_stem", "Mushroom Stem"),
    ("mycelium", "Mycelium"),
    ("nether_wart_block", "Nether Wart Block"),
    ("warped_wart_block", "Warped Wart Block"),
    ("shroomlight", "Shroomlight"),
];
const HIGH_RESISTANCE: &[(&str, &str)] = &[("soul_sand", "Soul Sand"), ("soul_soil", "Soul Soil")];
const STICKY: &[(&str, &str)] = &[("honeycomb_block", "Honeycomb Block")];
const EXPLOSIVE: &[(&str, &str)] = &[("tnt", "TNT")];
const HOT: &[(&str, &str)] = &[("magma_block", "Magma Block")];
