//! Java animal age, temptation and courtship. Add a food arm to
//! `MobKind::breeding_food` to opt another species into the shared baby API.
use glam::{DVec3, IVec3};
use serde_json::{Value, json};

use super::{Ctx, Entities, EntityEvent, Mob, MobKind, MobWorld, PlayerId, Rng};
use crate::{color::DyeColor, inventory::Stack, item::Item, particles};
mod lifecycle;

pub const BABY_AGE: i32 = -24000;
pub const BREEDING_COOLDOWN: i32 = 6000;
pub const LOVE_SECONDS: f32 = 30.0;
pub const COURTSHIP_SECONDS: f32 = 3.0;

#[derive(Clone, Debug)]
pub struct State {
    pub love: f32,
    pub love_by: Option<PlayerId>,
    pub courtship: f32,
    pub home: IVec3,
    pub pregnant: bool,
    pub owner: Option<PlayerId>,
    /// `TamableAnimal.orderedToSit` (saved as `sitting`, Java's `Sitting`).
    pub sitting: bool,
    /// `isInSittingPose`: set by `SitWhenOrderedToGoal` once an ordered pet
    /// is on the ground and out of water (never saved; a load sits it, as
    /// `readAdditionalSaveData` does).
    pub sit_pose: bool,
    pub collar: DyeColor,
    growth: f32,
    hearts: f32,
    pub grazing: f32,
    pub variant: u8,
    pub sleeping: bool,
    pub begging: bool,
    pub trusted: [Option<PlayerId>; 2],
    pub mouth: Option<Stack>,
    pub mouth_time: f32,
    pub perched: Option<u8>,
    pub dancing: bool,
    pub imitate: f32,
    pub foe: Option<(u32, DVec3)>,
    pub foe_time: f32,
    pub ramming: f32,
    pub ram_goal: Option<DVec3>,
    pub ram_prepare: f32,
    pub ram_cooldown: f32,
    pub stationary: f32,
    pub jump_cooldown: f32,
    pub horns: u8,
    pub horn_kind: u8,
    pub screaming: bool,
    /// `BreedGoal.partner`: its uid and latest position, kept by
    /// `tick_animals` while the partner can still mate (never saved).
    pub partner: Option<(u32, DVec3)>,
    /// `BreedGoal.tick` reached `breed()` with this partner this tick.
    pub breed_now: Option<u32>,
}
impl State {
    pub fn new(pos: DVec3) -> Self {
        Self {
            love: 0.0,
            love_by: None,
            courtship: 0.0,
            home: pos.floor().as_ivec3(),
            pregnant: false,
            owner: None,
            sitting: false,
            sit_pose: false,
            collar: DyeColor::Red,
            growth: 0.0,
            hearts: 0.0,
            grazing: 0.0,
            variant: 0,
            sleeping: false,
            begging: false,
            trusted: [None; 2],
            mouth: None,
            mouth_time: 0.0,
            perched: None,
            dancing: false,
            imitate: 10.0,
            foe: None,
            foe_time: 0.0,
            ramming: 0.0,
            ram_goal: None,
            ram_prepare: 0.0,
            ram_cooldown: 30.0,
            stationary: 0.0,
            jump_cooldown: 30.0,
            horns: 2,
            horn_kind: 0,
            screaming: false,
            partner: None,
            breed_now: None,
        }
    }
}
impl MobKind {
    /// Java 1.21 breeding food. A food predicate and `is_breedable` arm are
    /// the only registration needed for the generic mating/baby pipeline.
    pub fn breeding_food(self, food: Item) -> bool {
        match self {
            Self::Sniffer => food == Item::TORCHFLOWER_SEEDS,
            Self::Bee => food.block().is_some_and(super::bee::flower),
            Self::Cow | Self::Mooshroom | Self::Sheep => food == Item::WHEAT,
            Self::Frog | Self::Tadpole => food == Item::SLIME_BALL,
            Self::Panda => food == Item::from_block(crate::world::overworld_blocks::BAMBOO),
            Self::Armadillo => food == Item::SPIDER_EYE,
            Self::Ocelot => matches!(food, Item::COD | Item::SALMON),
            Self::Pig => matches!(food, Item::CARROT | Item::POTATO | Item::BEETROOT),
            Self::Chicken => seeds(food),
            Self::Wolf => wolf_food(food),
            Self::Fox => matches!(food, Item::SWEET_BERRIES | Item::GLOW_BERRIES),
            Self::Rabbit => {
                food == Item::CARROT
                    || food == Item::GOLDEN_CARROT
                    || food == Item::from_block(crate::world::block::Block::DANDELION)
            }
            Self::Goat => food == Item::WHEAT,
            Self::Cat => matches!(food, Item::COD | Item::SALMON),
            Self::Hoglin => food == Item::from_block(crate::world::nether_biome_blocks::CRIMSON_FUNGUS),
            Self::Strider => food == Item::from_block(crate::world::nether_biome_blocks::WARPED_FUNGUS),
            Self::Turtle => food == Item::from_block(crate::world::overworld_blocks::SEAGRASS),
            Self::Axolotl => food == Item::TROPICAL_FISH_BUCKET,
            Self::Horse | Self::Donkey => {
                matches!(food, Item::GOLDEN_CARROT | Item::GOLDEN_APPLE | Item::ENCHANTED_GOLDEN_APPLE)
            }
            Self::Llama => food == Item::from_block(crate::world::block::Block::HAY_BALE),
            Self::Camel => food == Item::from_block(crate::world::block::Block::CACTUS),
            Self::Nautilus => super::nautilus::food(food),
            _ => false,
        }
    }
    pub fn has_animal_state(self) -> bool {
        self.is_breedable() || matches!(self, Self::Parrot | Self::PolarBear)
    }
    pub fn is_breedable(self) -> bool {
        matches!(
            self,
            Self::Sniffer
                | Self::Bee
                | Self::Wolf
                | Self::Fox
                | Self::Rabbit
                | Self::Goat
                | Self::Cow
                | Self::Mooshroom
                | Self::Frog
                | Self::Panda
                | Self::Armadillo
                | Self::Ocelot
                | Self::Sheep
                | Self::Pig
                | Self::Chicken
                | Self::Cat
                | Self::Hoglin
                | Self::Strider
                | Self::Turtle
                | Self::Axolotl
                | Self::Horse
                | Self::Donkey
                | Self::Llama
                | Self::Camel
                | Self::Nautilus
        )
    }
    /// Mates must match, except that a horse and a donkey make a mule.
    pub fn can_mate_with(self, other: MobKind) -> bool {
        self == other || matches!((self, other), (Self::Horse, Self::Donkey) | (Self::Donkey, Self::Horse))
    }
}
/// `AgeableMob.getSpeedUpSecondsWhenFeeding`: `(int)(ticks / 20 * 0.1F)`.
pub(crate) fn speed_up_seconds(ticks_left: i32) -> i32 {
    ((ticks_left / 20) as f32 * 0.1) as i32
}

impl Mob {
    pub fn ready_to_breed(&self) -> bool {
        self.alive()
            && self.armadillo.as_ref().is_none_or(|s| !s.scared())
            && self.age == 0
            && !self.baby
            && self.animal.as_ref().is_some_and(|a| {
                a.love > 0.0
                    && !a.sitting
                    && !a.pregnant
                    && (!matches!(self.kind, MobKind::Cat | MobKind::Wolf) || a.owner.is_some())
                    && (!super::mounts::equine(self.kind) || self.mount.as_ref().is_some_and(|s| s.tame))
            })
    }
    /// Feeding a baby removes 10% of its remaining growth time, rounded to
    /// whole seconds as Java's ageUp does. Never starts love mode.
    pub fn feed_growth(&mut self) {
        if self.kind == MobKind::Turtle {
            self.grow = (self.grow + ((1200.0 - self.grow) * 0.1).floor()).min(1200.0);
            self.age = -((1200.0 - self.grow) * 20.0) as i32;
        } else if self.age < 0 {
            self.age = (self.age + ((-self.age / 20) as f32 * 0.1) as i32 * 20).min(0);
            self.baby = self.age < 0;
        }
    }
    /// v1.0 gap Track 3b: `AgeableMob.ageUp(seconds, forced)`: older by
    /// `seconds`, at most to adulthood; forced growth adds to
    /// [`Mob::forced_age`] and starts 40 ticks of particles; coming of age
    /// this way, the adult takes the forced age as its age (counting down).
    pub fn age_up(&mut self, seconds: i32, forced: bool) {
        let old = self.age;
        let age = (old + seconds * 20).min(0);
        self.age = age;
        if forced {
            self.forced_age += age - old;
            if self.forced_age_timer == 0 {
                self.forced_age_timer = 40;
            }
        }
        if self.age == 0 {
            self.age = self.forced_age;
        }
        self.baby = self.age < 0;
    }
    pub(super) fn advance_animal_age(&mut self, dt: f32) {
        // v1.0 gap Track 3b: `AgeableMob.aiStep` for a dolphin (no animal
        // state): the forced-growth particle every fourth tick, then a
        // baby a tick older a tick (unless age-locked) and a forced adult
        // a tick younger, down to 0.
        if self.kind == MobKind::Dolphin && self.alive() && (self.age != 0 || self.forced_age_timer > 0) {
            self.grow += dt * 20.0;
            let ticks = self.grow.floor() as i32;
            self.grow -= ticks as f32;
            for _ in 0..ticks {
                if self.forced_age_timer > 0 {
                    if self.forced_age_timer.is_multiple_of(4) {
                        self.forced_sparkles = self.forced_sparkles.saturating_add(1);
                    }
                    self.forced_age_timer -= 1;
                }
            }
            if !(self.age_locked && self.baby) {
                self.age = if self.age < 0 { (self.age + ticks).min(0) } else { (self.age - ticks).max(0) };
            }
            self.baby = self.age < 0;
            return;
        }
        // `AgeableMob.canAgeUp`: an age-locked baby stays one.
        if !self.alive() || self.kind == MobKind::Turtle && self.baby || self.age_locked && self.baby {
            return;
        }
        if self.baby && self.age == 0 {
            self.age = if self.kind == MobKind::Sniffer { -48000 } else { BABY_AGE };
        }
        if let Some(a) = &mut self.animal {
            a.growth += dt * 20.0;
            let ticks = a.growth.floor() as i32;
            a.growth -= ticks as f32;
            self.age = if self.age < 0 { (self.age + ticks).min(0) } else { (self.age - ticks).max(0) };
            self.baby = self.age < 0;
        }
    }
}
impl Entities {
    /// Java 26.3 `AgeableMob.mobInteract` with a golden dandelion (v0.16
    /// Track D): on a baby animal outside `#cannot_be_age_locked` it toggles
    /// the age lock, resets the baby to its youngest age, uses up the flower
    /// and keeps a locked baby from despawning.
    fn use_golden_dandelion(&mut self, index: usize, held: Option<Stack>) -> Option<bool> {
        if held?.item != Item::from_block(crate::world::gen5_blocks::GOLDEN_DANDELION) {
            return None;
        }
        let m = self.mobs.get_mut(index)?;
        if !m.alive()
            || !m.baby
            || (m.animal.is_none() && !matches!(m.kind, MobKind::SulfurCube | MobKind::Dolphin))
            || matches!(m.kind, MobKind::ZombieHorse | MobKind::SkeletonHorse | MobKind::Villager)
        {
            return None;
        }
        m.age_locked = !m.age_locked;
        m.age = if m.kind == MobKind::Sniffer { -48000 } else { BABY_AGE };
        if m.age_locked {
            m.persistent = true;
        }
        Some(true)
    }

    /// Shared action. `Some(true)` consumes food; `Some(false)` uses an
    /// empty hand/owner command; `None` permits ordinary item actions.
    pub fn use_animal(&mut self, index: usize, held: Option<Stack>, player: PlayerId) -> Option<bool> {
        self.use_animal_with_materials(index, held, player, false)
    }
    pub fn use_animal_with_materials(
        &mut self,
        index: usize,
        held: Option<Stack>,
        player: PlayerId,
        infinite: bool,
    ) -> Option<bool> {
        if let Some(result) = self.use_lead_or_name(index, held, player, infinite) {
            return Some(result);
        }
        if let Some(result) = self.use_golden_dandelion(index, held) {
            return Some(result);
        }
        if let Some(result) = self.feed_dolphin(index, held) {
            return Some(result);
        }
        if self.mobs.get(index).is_some_and(|m| m.kind == MobKind::Panda) {
            return self.use_panda(index, held, player, infinite);
        }
        let m = self.mobs.get_mut(index)?;
        if !m.alive() {
            return None;
        }
        let food = held.map(|s| s.item);
        if m.kind == MobKind::Parrot && food == Some(Item::COOKIE) {
            let kind = m.kind;
            let pos = m.pos;
            m.damage(f32::MAX, None, &mut self.rng);
            let mut b = particles::Burst::new(particles::Kind::Effect, pos + DVec3::Y * 0.5, 12);
            b.color = Some([0.2, 0.6, 0.15, 1.0]);
            self.particles.push(particles::Request::Burst(b));
            self.drop_loot_with_fire(kind, pos, 0, false, false);
            return Some(true);
        }
        // Untamed horses and foals take food through the mount rules (temper, healing, growth).
        if super::mounts::equine(m.kind) && (m.baby || !m.mount.as_ref().is_some_and(|s| s.tame)) {
            return None;
        }
        let (tame, owned) = {
            let a = m.animal.as_ref()?;
            (a.owner.is_some(), a.owner == Some(player))
        };
        let dye = food.and_then(Item::dye_color);
        // Java 26.3 `Wolf`, `Cat` and `Parrot.mobInteract` (v1.0 Track 6).
        match m.kind {
            // Untamed: a bone tames a wolf that isn't angry, cat food a cat,
            // parrot food a parrot; anything else is `Animal.mobInteract`.
            MobKind::Wolf if !tame && food == Some(Item::BONE) && m.angry_player.is_none() => {
                return Some(self.try_to_tame(index, player));
            }
            MobKind::Cat if !tame && food.is_some_and(cat_food) => {
                self.mobs[index].persistent = true;
                return Some(self.try_to_tame(index, player));
            }
            MobKind::Parrot if !tame && food.is_some_and(seeds) => {
                return Some(self.try_to_tame(index, player));
            }
            MobKind::Parrot if tame => {
                // `!isFlying() && isOwnedBy(player)` toggles the order to sit.
                if owned && m.on_ground {
                    self.toggle_sit(index);
                    return Some(false);
                }
                return None;
            }
            MobKind::Wolf if tame => {
                // `feed(player, hand, stack, 2, 2)`: any player heals a hurt wolf.
                if food.is_some_and(wolf_food) && m.health < m.max_health() {
                    let heal = food.and_then(Item::food).map_or(2.0, |f| 2.0 * f.0 as f32);
                    m.health = (m.health + heal).min(m.max_health());
                    return Some(true);
                }
                // Only the owner dyes the collar; the same colour does nothing.
                if let Some(c) = dye
                    && owned
                {
                    let a = m.animal.as_mut()?;
                    if c == a.collar {
                        return None;
                    }
                    a.collar = c;
                    return Some(true);
                }
                if let Some(result) = self.feed_for_love(index, food, player) {
                    return Some(result);
                }
                if owned {
                    self.toggle_sit(index);
                    return Some(false);
                }
                return None;
            }
            MobKind::Cat if tame => {
                if !owned {
                    return self.feed_for_love(index, food, player);
                }
                if let Some(c) = dye {
                    let a = m.animal.as_mut()?;
                    if c != a.collar {
                        a.collar = c;
                        m.persistent = true;
                        return Some(true);
                    }
                } else if food.is_some_and(cat_food) && m.health < m.max_health() {
                    // `feed(player, hand, stack, 1, 1)`.
                    let heal = food.and_then(Item::food).map_or(1.0, |f| f.0 as f32);
                    m.health = (m.health + heal).min(m.max_health());
                    return Some(true);
                }
                if let Some(result) = self.feed_for_love(index, food, player) {
                    return Some(result);
                }
                self.toggle_sit(index);
                return Some(false);
            }
            // An untamed cat takes nothing but its food.
            MobKind::Cat => return None,
            _ => {}
        }
        self.feed_for_love(index, food, player)
    }

    /// `TamableAnimal.setOrderedToSit(!isOrderedToSit())` from the owner,
    /// which also stops the pet (`navigation.stop`, `setTarget(null)`).
    fn toggle_sit(&mut self, index: usize) {
        let m = &mut self.mobs[index];
        m.nav.stop();
        let a = m.animal.as_mut().unwrap();
        a.sitting = !a.sitting;
        a.perched = None;
        a.foe = None;
    }

    /// `tryToTame`: one in three for wolves and cats, one in ten for
    /// parrots. A tamed wolf or cat is ordered to sit (a parrot isn't), and
    /// a wolf's `applyTamingSideEffects` raises its health to 40. Always
    /// uses the item.
    fn try_to_tame(&mut self, index: usize, player: PlayerId) -> bool {
        let kind = self.mobs[index].kind;
        let bound = if kind == MobKind::Parrot { 10 } else { 3 };
        if self.rng.next_int(bound) == 0 {
            let m = &mut self.mobs[index];
            m.nav.stop();
            let a = m.animal.as_mut().unwrap();
            a.owner = Some(player);
            a.foe = None;
            if kind != MobKind::Parrot {
                a.sitting = true;
            }
            if kind == MobKind::Wolf {
                m.health = 40.0;
            }
            m.persistent = true;
            self.animal_hearts(index);
        } else {
            let pos = self.mobs[index].pos;
            self.particles.push(particles::Request::Burst(particles::Burst::new(
                particles::Kind::Smoke,
                pos + DVec3::Y * 0.7,
                7,
            )));
        }
        true
    }

    /// `Animal.mobInteract` with food: a baby grows, a grown animal free to
    /// fall in love does. `None` passes the interaction on.
    fn feed_for_love(&mut self, index: usize, food: Option<Item>, player: PlayerId) -> Option<bool> {
        let m = self.mobs.get_mut(index)?;
        let food = food?;
        if !m.kind.breeding_food(food) {
            return None;
        }
        if m.baby || m.age < 0 {
            m.feed_growth();
        } else {
            let a = m.animal.as_mut()?;
            if m.age != 0 || a.love > 0.0 || a.pregnant || m.armadillo.as_ref().is_some_and(|s| s.scared()) {
                return None;
            }
            a.love = LOVE_SECONDS;
            a.love_by = Some(player);
            if let (Some(s), Some((heal, _, _))) = (m.mount.as_ref(), super::mounts::food(m.kind, food)) {
                m.health = (m.health + heal).min(s.max_health);
            }
        }
        m.persistent = true;
        // v1.0 Track 9: `Camel.handleEating` posts `eat`.
        if super::mounts::camel_like(m.kind) {
            self.post_mob_event(index, crate::world::vibration::GameEvent::Eat);
        }
        self.animal_hearts(index);
        Some(true)
    }
    pub(super) fn animal_hearts(&mut self, index: usize) {
        let m = &self.mobs[index];
        let mut b = particles::Burst::new(particles::Kind::Heart, m.pos + DVec3::Y * (m.shape().height + 0.25), 7);
        b.spread = DVec3::new(m.shape().half_width, 0.3, m.shape().half_width);
        b.velocity_spread = DVec3::splat(0.02);
        self.particles.push(particles::Request::Burst(b));
    }
    /// Creates a species-appropriate baby; mounts can reuse this and then
    /// apply their own attribute/variant inheritance. Does not feed parents.
    pub fn animal_child(&mut self, first: usize, second: usize) -> Mob {
        let (a, b) = (&self.mobs[first], &self.mobs[second]);
        let kind = if a.kind == b.kind { a.kind } else { MobKind::Mule };
        let mut child = Mob::new(kind, a.pos, self.rng.range(0.0, std::f32::consts::TAU));
        super::mounts::inherit(&mut child, a, b, &mut self.rng);
        child.age = BABY_AGE;
        child.baby = true;
        child.persistent = true;
        child.wool_color = inherit_color(a.wool_color, b.wool_color, &mut self.rng);
        if let Some(c) = child.animal.as_mut() {
            let chosen = if self.rng.chance(0.5) { a } else { b };
            c.owner = chosen.animal.as_ref().and_then(|a| a.owner);
            c.variant = chosen.animal.as_ref().map_or(0, |a| a.variant);
            if child.kind == MobKind::Fox {
                c.trusted = [a.animal.as_ref().and_then(|a| a.love_by), b.animal.as_ref().and_then(|a| a.love_by)];
            }
            if child.kind == MobKind::Goat {
                c.screaming = chosen.animal.as_ref().is_some_and(|a| a.screaming) || self.rng.chance(0.02);
                c.horn_kind = self.rng.next_int(4) as u8 + if c.screaming { 4 } else { 0 };
            }
            if child.kind == MobKind::Wolf && c.owner.is_some() {
                child.health = 40.0;
            }
            c.collar = inherit_color(
                a.animal.as_ref().map_or(DyeColor::Red, |a| a.collar),
                b.animal.as_ref().map_or(DyeColor::Red, |a| a.collar),
                &mut self.rng,
            );
        }
        if let Some(c) = child.aquatic.as_mut() {
            c.variant = if self.rng.chance(1.0 / 1200.0) {
                4
            } else if self.rng.chance(0.5) {
                a.aquatic.as_ref().unwrap().variant
            } else {
                b.aquatic.as_ref().unwrap().variant
            };
        }
        // `Nautilus.getBreedOffspring`: a tame parent's baby is tame, with its owner.
        if let (Some(c), Some(p)) = (child.mount.as_mut(), a.mount.as_ref())
            && super::nautilus::is_nautilus(child.kind)
            && p.tame
        {
            c.tame = true;
            c.owner = p.owner;
        }
        super::mooshroom::inherit(&mut child, a, b, &mut self.rng);
        super::panda::inherit(&mut child, a, b, &mut self.rng);
        super::armadillo::fresh(&mut child, &mut self.rng);
        child
    }
    /// Love timers and hearts, then `BreedGoal`'s partner: an animal not yet
    /// courting looks for the nearest mate within 8 blocks
    /// (`getFreePartner`: in love, not panicking); one courting keeps its
    /// partner while that partner is alive, in love and not panicking. The
    /// goal itself runs in the mob's goal selector.
    pub(super) fn tick_animals<W: MobWorld + ?Sized>(
        &mut self,
        dt: f32,
        world: &W,
        _ctx: &Ctx,
        events: &mut Vec<EntityEvent>,
    ) {
        // Partners, parents and avoided mobs are tracked by uid.
        self.ensure_mob_ids();
        self.mob_index.rebuild(&self.mobs);
        let n = self.mobs.len();
        for i in 0..n {
            let m = &mut self.mobs[i];
            if !m.alive() || !world.entity_ticking(m.pos.floor().as_ivec3()) {
                continue;
            }
            let Some(a) = m.animal.as_mut() else { continue };
            a.love = (a.love - dt).max(0.0);
            a.hearts -= dt;
            let hearts = a.love > 0.0 && a.hearts <= 0.0;
            if hearts {
                a.hearts = 0.5;
                self.animal_hearts(i);
            }
            let courting = self.mobs[i].breeding();
            let m = &self.mobs[i];
            let mate = |b: &Mob| {
                b.simulation_active
                    && b.uid != m.uid
                    && b.kind.can_mate_with(m.kind)
                    && b.ready_to_breed()
                    && b.ai != super::mob::Ai::Panic
            };
            let partner = if !m.ready_to_breed() {
                None
            } else if let Some((uid, _)) = m.animal.as_ref().unwrap().partner.filter(|_| courting) {
                self.mobs.iter().find(|b| b.uid == uid && b.alive() && mate(b)).map(|b| (uid, b.pos))
            } else {
                self.mob_index.nearest(&self.mobs, m.pos, 8.0, mate).map(|j| (self.mobs[j].uid, self.mobs[j].pos))
            };
            self.mobs[i].animal.as_mut().unwrap().partner = partner;
        }
        self.tick_animal_lifecycle(dt, world, _ctx, events);
        self.tick_companions(dt, world, _ctx, events);
    }

    /// `BreedGoal.breed` / `Animal.spawnChildFromBreeding` for the pairs whose
    /// goal finished courting this tick: both parents cool down for 6000
    /// ticks and lose their love, a baby appears (turtles get pregnant
    /// instead; sniffers drop an egg item with `SNIFFER_EGG_PLOP`), and 1–7
    /// experience drops.
    pub(super) fn resolve_breeding(&mut self) {
        for i in 0..self.mobs.len() {
            let Some(uid) = self.mobs[i].animal.as_mut().and_then(|a| a.breed_now.take()) else { continue };
            let Some(j) = self.mobs.iter().position(|b| b.uid == uid) else { continue };
            if i == j || !self.mobs[i].ready_to_breed() || !self.mobs[j].ready_to_breed() {
                continue;
            }
            let turtle = matches!(self.mobs[i].kind, MobKind::Turtle | MobKind::Frog);
            let sniffer = self.mobs[i].kind == MobKind::Sniffer;
            let child = (!turtle && !sniffer).then(|| self.animal_child(i, j));
            if sniffer {
                self.items.push(super::ItemEntity::new(
                    Stack::new(Item::SNIFFER_EGG, 1),
                    self.mobs[i].pos,
                    DVec3::ZERO,
                    0.5,
                    &mut self.rng,
                ));
                let pos = self.mobs[i].pos;
                let sound = super::MobSound::Sniffer(super::sniffer::Cue::EggPlop);
                self.pending_sounds.push(EntityEvent::Sound { sound, pos });
            }
            for k in [i, j] {
                self.mobs[k].age = BREEDING_COOLDOWN;
                let a = self.mobs[k].animal.as_mut().unwrap();
                a.love = 0.0;
                a.partner = None;
                a.breed_now = None;
                self.animal_hearts(k);
            }
            if turtle {
                self.mobs[i].animal.as_mut().unwrap().pregnant = true;
            }
            if let Some(child) = child {
                self.mobs.push(child);
            }
            if self.mob_loot {
                let points = 1 + self.rng.next_int(7);
                self.spawn_xp(self.mobs[i].pos, points);
            }
        }
    }
}
/// Sheep and collars use a two-dye crafting result when one exists.
pub fn inherit_color(a: DyeColor, b: DyeColor, rng: &mut Rng) -> DyeColor {
    if a == b {
        return a;
    }
    let mut grid = crate::crafting::Grid::new(2);
    grid.cells[0] = Some(Stack::new(a.dye(), 1));
    grid.cells[1] = Some(Stack::new(b.dye(), 1));
    if let Some(c) = grid.result().and_then(|s| s.item.dye_color()) {
        c
    } else if rng.chance(0.5) {
        a
    } else {
        b
    }
}
pub(super) fn save(a: &State) -> Value {
    json!({"love":a.love,"love_by":a.love_by.map(|p|p.0),"courtship":a.courtship,"home":a.home.to_array(),"pregnant":a.pregnant,"owner":a.owner.map(|p|p.0),"sitting":a.sitting,"collar":a.collar as u8,"growth":a.growth,"grazing":a.grazing,"variant":a.variant, "trusted":a.trusted.map(|p|p.map(|p|p.0)), "mouth":a.mouth.map(|s|crate::inventory::stack_to_string(Some(s))), "mouth_time":a.mouth_time, "perched":a.perched, "horns":a.horns, "horn_kind":a.horn_kind, "screaming":a.screaming, "ram_cooldown":a.ram_cooldown, "jump_cooldown":a.jump_cooldown})
}
pub(super) fn load(a: &mut State, v: &Value) {
    let timer =
        |key: &str, max: f32| v[key].as_f64().filter(|f| f.is_finite()).unwrap_or(0.0).clamp(0.0, max as f64) as f32;
    a.variant = v["variant"].as_u64().unwrap_or(0).min(8) as u8;
    if let Some(t) = v["trusted"].as_array() {
        for (slot, id) in a.trusted.iter_mut().zip(t) {
            *slot = id.as_u64().and_then(|n| u32::try_from(n).ok()).map(PlayerId);
        }
    }
    a.mouth = v["mouth"].as_str().and_then(|s| crate::inventory::stack_from_str(s).flatten());
    a.mouth_time = timer("mouth_time", 30.0);
    a.perched = v["perched"].as_u64().filter(|n| *n < 2).map(|n| n as u8);
    a.horns = v["horns"].as_u64().unwrap_or(2).min(2) as u8;
    a.horn_kind = v["horn_kind"].as_u64().unwrap_or(0).min(7) as u8;
    a.screaming = v["screaming"].as_bool().unwrap_or(false);
    a.ram_cooldown = v["ram_cooldown"].as_f64().filter(|f| f.is_finite()).unwrap_or(30.0).clamp(0.0, 300.0) as f32;
    a.jump_cooldown = v["jump_cooldown"].as_f64().filter(|f| f.is_finite()).unwrap_or(30.0).clamp(0.0, 60.0) as f32;
    a.love = timer("love", LOVE_SECONDS);
    a.courtship = timer("courtship", COURTSHIP_SECONDS);
    a.love_by = v["love_by"].as_u64().and_then(|n| u32::try_from(n).ok()).map(PlayerId);
    a.owner = v["owner"].as_u64().and_then(|n| u32::try_from(n).ok()).map(PlayerId);
    a.sitting = v["sitting"].as_bool().unwrap_or(false);
    a.sit_pose = a.sitting;
    a.pregnant = v["pregnant"].as_bool().unwrap_or(false);
    a.collar = DyeColor::ALL.get(v["collar"].as_u64().unwrap_or(14) as usize).copied().unwrap_or(DyeColor::Red);
    a.growth = timer("growth", 1.0);
    a.grazing = timer("grazing", 2.0);
    if let Some(p) = v["home"].as_array().filter(|p| p.len() == 3) {
        let coord = |i: usize| p[i].as_i64().and_then(|n| i32::try_from(n).ok());
        if let (Some(x), Some(y), Some(z)) = (coord(0), coord(1), coord(2)) {
            a.home = IVec3::new(x, y, z);
        }
    }
}

pub fn seeds(food: Item) -> bool {
    matches!(
        food,
        Item::WHEAT_SEEDS
            | Item::PUMPKIN_SEEDS
            | Item::MELON_SEEDS
            | Item::BEETROOT_SEEDS
            | Item::TORCHFLOWER_SEEDS
            | Item::PITCHER_POD
    )
}
/// `#cat_food`: what tames, heals and breeds cats.
pub fn cat_food(food: Item) -> bool {
    matches!(food, Item::COD | Item::SALMON)
}
pub fn wolf_food(food: Item) -> bool {
    matches!(
        food,
        Item::RAW_BEEF
            | Item::STEAK
            | Item::RAW_PORKCHOP
            | Item::COOKED_PORKCHOP
            | Item::RAW_CHICKEN
            | Item::COOKED_CHICKEN
            | Item::RAW_RABBIT
            | Item::COOKED_RABBIT
            | Item::RAW_MUTTON
            | Item::COOKED_MUTTON
            | Item::ROTTEN_FLESH
            | Item::COD
            | Item::SALMON
            | Item::COOKED_COD
            | Item::COOKED_SALMON
            | Item::TROPICAL_FISH
            | Item::PUFFERFISH
            | Item::RABBIT_STEW
    )
}
/// v0.15 Track D: the mobs with Java's climate variants.
pub fn has_farm_variant(kind: MobKind) -> bool {
    matches!(kind, MobKind::Chicken | MobKind::Cow | MobKind::Pig)
}

impl Mob {
    /// The climate variant of a chicken, cow or pig (temperate otherwise).
    pub fn farm_variant(&self) -> crate::content::farm_variant::FarmVariant {
        match &self.animal {
            Some(a) if has_farm_variant(self.kind) => crate::content::farm_variant::FarmVariant::from_index(a.variant),
            _ => Default::default(),
        }
    }

    /// Sets a chicken's, cow's or pig's climate variant; other mobs keep theirs.
    pub fn set_farm_variant(&mut self, variant: crate::content::farm_variant::FarmVariant) {
        if has_farm_variant(self.kind)
            && let Some(a) = &mut self.animal
        {
            a.variant = variant as u8;
        }
    }
}

pub(super) fn on_spawn(m: &mut Mob, rng: &mut Rng) {
    super::armadillo::fresh(m, rng);
    let Some(a) = m.animal.as_mut() else { return };
    a.variant = if m.kind == MobKind::Parrot {
        rng.next_int(5) as u8
    } else if m.kind == MobKind::Rabbit {
        rng.next_int(6) as u8
    } else {
        0
    };
    if m.kind == MobKind::Goat {
        a.screaming = rng.chance(0.02);
        a.ram_cooldown = rng.range(30.0, 300.0);
        a.horn_kind = rng.next_int(4) as u8 + if a.screaming { 4 } else { 0 };
    }
}
impl Mob {
    /// Java 1.21 tamed wolves have 40 health; wild wolves have eight.
    pub fn max_health(&self) -> f32 {
        let base = if self.panda.as_ref().is_some_and(|s| s.variant() == super::panda::Gene::Weak) {
            10.0
        } else if self.kind == MobKind::Wolf && self.animal.as_ref().is_some_and(|a| a.owner.is_some()) {
            40.0
        } else if self.kind == MobKind::SulfurCube {
            super::sulfur_cube::HEALTH_PER_SIZE * f32::from(self.size)
        } else {
            self.leader_max_health(self.kind.max_health())
        };
        // v0.15 Track D: Health Boost's `max_health` modifier.
        base + self.effects.max_health_bonus()
    }
    pub(super) fn companion_think<W: MobWorld + ?Sized>(
        &mut self,
        _dt: f32,
        w: &W,
        ctx: &Ctx,
        rng: &mut Rng,
        events: &mut Vec<EntityEvent>,
    ) -> Option<(Option<DVec3>, f64)> {
        // `SitWhenOrderedToGoal` (2) and the wolf's `MeleeAttackGoal` (5)
        // outrank following the owner, breeding and strolling. The goal
        // starts only on the ground and out of water (`canUse`) and runs
        // while the order stands (`canContinueToUse`).
        let (on_ground, in_water) = (self.on_ground, self.in_water);
        let a = self.animal.as_mut()?;
        if !a.sitting {
            a.sit_pose = false;
        } else if !a.sit_pose && on_ground && !in_water {
            a.sit_pose = true;
        }
        let a = self.animal.as_ref()?;
        if a.sit_pose || a.sleeping {
            self.stop_goal(w);
            return Some((None, 0.0));
        }
        if self.kind == MobKind::Wolf
            && let Some(id) = self.angry_player
        {
            if let Some(t) = ctx.players.iter().find(|t| t.id == id && t.targetable) {
                self.stop_goal(w);
                let flat = (t.pos - self.pos) * DVec3::new(1.0, 0.0, 1.0);
                if flat.length_squared() < 2.25 && self.attack_cooldown <= 0.0 {
                    self.attack_cooldown = 1.0;
                    // Wolves inherit `Mob.doHurtTarget`: the default push away
                    // from the wolf, then its attack knockback (none) along its facing.
                    let extra = crate::knockback::attack_strength(self.attack_knockback(), 0, false);
                    let yaw = (-flat.x).atan2(flat.z).to_degrees() as f32;
                    events.push(EntityEvent::PlayerHit {
                        java: Some(crate::knockback::Hit::melee(self.pos, t.pos, yaw, extra)),
                        player: id,
                        damage: 4.0,
                        knockback: (flat.normalize_or(DVec3::X) * 4.0 + DVec3::Y * 3.0).as_vec3(),
                        cause: crate::simulation::damage::Cause::mob("was slain by a wolf"),
                    });
                }
                return Some((self.pursue(w, t.pos, 3.0, true, true, rng), 3.0));
            }
            // `updatePersistentAnger` forgives a creative or spectating
            // player; a dead one is left to `forgive_dead_players`.
            if ctx.players.iter().any(|t| t.id == id && t.alive && !t.targetable) {
                self.angry_player = None;
            }
        }
        if let Some((target, goal)) = a.foe {
            self.stop_goal(w);
            let flat = (goal - self.pos) * DVec3::new(1.0, 0.0, 1.0);
            if flat.length_squared() < 2.25
                && self.attack_cooldown <= 0.0
                && super::mob::line_of_sight(w, self.pos + DVec3::Y * 0.7, goal + DVec3::Y * 0.5)
            {
                self.attack_cooldown = 1.0;
                self.attack_anim = 0.3;
                events.push(EntityEvent::MobHit {
                    target,
                    attacker: self.uid,
                    damage: 4.0,
                    knockback: flat.normalize_or(DVec3::X) * 4.0 + DVec3::Y * 2.0,
                    melee: true,
                });
            }
            return Some((self.pursue(w, goal, 3.0, true, true, rng), 3.0));
        }
        if self.kind == MobKind::Goat && a.ramming > 0.0 {
            let goal = a.ram_goal?;
            let flat = (goal - self.pos) * DVec3::new(1.0, 0.0, 1.0);
            return Some((if a.ram_prepare > 0.0 { None } else { Some(flat.normalize_or(DVec3::X)) }, 8.0));
        }
        // Following the owner is `FollowOwnerGoal` in the goal selector.
        None
    }
}
impl Entities {
    /// Wolves defend the actual attacking profile, never an active pad slot.
    pub(super) fn animals_owner_attack(&mut self, index: usize, owner: PlayerId) {
        let Some(target) = self.mobs.get(index) else { return };
        if matches!(target.kind, MobKind::Creeper | MobKind::Ghast)
            || target.animal.as_ref().is_some_and(|a| a.owner == Some(owner))
        {
            return;
        }
        let foe = (target.uid, target.pos);
        for m in &mut self.mobs {
            if m.kind == MobKind::Wolf
                && m.alive()
                && m.pos.distance_squared(foe.1) < 256.0
                && let Some(a) = m.animal.as_mut()
                && a.owner == Some(owner)
                && !a.sitting
            {
                a.foe = Some(foe);
                a.foe_time = 30.0;
            }
        }
    }
    pub(super) fn animals_defend_owner(&mut self, attacker: u32, owner: PlayerId) {
        if let Some(index) = self.mobs.iter().position(|m| m.uid == attacker) {
            self.animals_owner_attack(index, owner);
        }
    }
    pub fn cat_morning_gifts(&mut self, owner: PlayerId, pos: DVec3) {
        let eligible: Vec<_> = self
            .mobs
            .iter()
            .filter(|m| {
                m.kind == MobKind::Cat
                    && m.alive()
                    && m.pos.distance_squared(pos) < 100.0
                    && m.animal.as_ref().is_some_and(|a| a.owner == Some(owner) && !a.sitting)
            })
            .map(|m| m.pos)
            .collect();
        for at in eligible {
            if self.rng.chance(0.7) {
                let roll = self.rng.next_int(62);
                let item = match roll / 10 {
                    0 => Item::RABBIT_FOOT,
                    1 => Item::RABBIT_HIDE,
                    2 => Item::STRING,
                    3 => Item::ROTTEN_FLESH,
                    4 => Item::FEATHER,
                    5 => Item::RAW_CHICKEN,
                    _ => Item::PHANTOM_MEMBRANE,
                };
                self.scatter(Stack::new(item, 1), at);
            }
        }
    }
    pub(super) fn tick_companions<W: MobWorld + ?Sized>(
        &mut self,
        dt: f32,
        w: &W,
        ctx: &Ctx,
        events: &mut Vec<EntityEvent>,
    ) {
        self.mob_index.rebuild(&self.mobs);
        let n = self.mobs.len();
        for i in 0..n {
            if !self.mobs[i].alive() || !w.entity_ticking(self.mobs[i].pos.floor().as_ivec3()) {
                continue;
            }
            let Some(a) = self.mobs[i].animal.as_ref() else { continue };
            let foe =
                a.foe.and_then(|(id, _)| self.mobs.iter().find(|m| m.uid == id && m.alive()).map(|m| (id, m.pos)));
            let imitation = if self.mobs[i].kind == MobKind::Parrot && a.imitate <= dt {
                self.mob_index
                    .nearest(&self.mobs, self.mobs[i].pos, 20.0, |b| b.kind.is_hostile() && b.alive())
                    .map(|j| self.mobs[j].kind)
            } else {
                None
            };
            let m = &mut self.mobs[i];
            let a = m.animal.as_mut().unwrap();
            a.foe = foe;
            a.foe_time = (a.foe_time - dt).max(0.0);
            if a.foe_time == 0.0 {
                a.foe = None;
            }
            let m = &mut self.mobs[i];
            let a = m.animal.as_mut().unwrap();
            a.begging = m.kind == MobKind::Wolf
                && ctx.players.iter().any(|t| {
                    t.pos.distance_squared(m.pos) < 64.0 && t.held_item.is_some_and(|i| i == Item::BONE || wolf_food(i))
                });
            if m.kind == MobKind::Fox {
                a.sleeping = ctx.daylight > 0.5
                    && a.love == 0.0
                    && !m.in_water
                    && !ctx
                        .players
                        .iter()
                        .any(|t| t.pos.distance_squared(m.pos) < 64.0 && !a.trusted.contains(&Some(t.id)));
                a.mouth_time += dt;
                if self.villager_griefing
                    && a.mouth.is_none()
                    && let Some(j) =
                        self.items.iter().position(|it| it.pickup_delay <= 0.0 && it.pos.distance_squared(m.pos) < 2.25)
                {
                    let s = self.items[j].stack;
                    a.mouth = Some(Stack { count: 1, ..s });
                    a.mouth_time = 0.0;
                    m.persistent = true;
                    self.items[j].stack.count -= 1;
                }
                if a.mouth_time >= 30.0 && a.mouth.is_some_and(|s| s.item.food().is_some()) {
                    a.mouth = None;
                    a.mouth_time = 0.0;
                }
            }
            if m.kind == MobKind::Parrot {
                a.imitate -= dt;
                if a.imitate <= 0.0 {
                    a.imitate = self.rng.range(10.0, 30.0);
                    if let Some(kind) = imitation {
                        events.push(EntityEvent::Sound { sound: super::MobSound::Ambient(kind), pos: m.pos });
                    }
                }
                let m = &mut self.mobs[i];
                let a = m.animal.as_mut().unwrap();
                if let Some(t) = ctx.players.iter().find(|t| Some(t.id) == a.owner && t.alive) {
                    if a.perched.is_none()
                        && !a.sitting
                        && !t.in_water
                        && t.on_ground
                        && m.pos.distance_squared(t.pos) < 1.0
                    {
                        // Two shoulders per player: check every parrot already sitting on one.
                        let taken = |side: u8| {
                            self.mobs.iter().enumerate().any(|(j, b)| {
                                j != i
                                    && b.kind == MobKind::Parrot
                                    && b.alive()
                                    && b.animal
                                        .as_ref()
                                        .is_some_and(|a| a.owner == Some(t.id) && a.perched == Some(side))
                            })
                        };
                        if let Some(side) = [0, 1].into_iter().find(|&side| !taken(side)) {
                            self.mobs[i].animal.as_mut().unwrap().perched = Some(side);
                        }
                    }
                    let m = &mut self.mobs[i];
                    let a = m.animal.as_mut().unwrap();
                    if let Some(side) = a.perched {
                        if t.in_water || !t.on_ground || !t.alive {
                            a.perched = None;
                            m.pos = t.pos + DVec3::Y * 1.5;
                            m.vel = DVec3::Y * 2.0;
                        } else {
                            let right = DVec3::new(-t.look.z, 0.0, t.look.x).normalize_or(DVec3::Z);
                            m.pos = t.pos + DVec3::Y * 1.45 + right * if side == 0 { -0.35 } else { 0.35 };
                            m.vel = DVec3::ZERO;
                        }
                    }
                } else {
                    a.perched = None;
                }
            }
        }
        self.items.retain(|i| i.stack.count > 0);
    }
}

impl Entities {
    /// Biome variants and babies for the next natural pack member, after
    /// `pack_size` others (Java's `AgeableMobGroupData`).
    pub(super) fn natural_animal(&mut self, biome: crate::world::terrain::Biome, pack_size: u32) {
        use crate::world::terrain::Biome::*;
        let m = self.mobs.last_mut().unwrap();
        if m.animal.is_none() {
            return;
        }
        if let Some(s) = &mut m.panda {
            s.main = super::panda::Gene::random(&mut self.rng);
            s.hidden = super::panda::Gene::random(&mut self.rng);
            m.health = m.max_health();
        }
        // v0.15 Track D: `VariantUtils.selectVariantToSpawn` by biome.
        m.set_farm_variant(crate::content::farm_variant::for_biome(biome.name()));
        let a = m.animal.as_mut().unwrap();
        if m.kind == MobKind::Fox {
            a.variant = matches!(biome, SnowyTaiga | Grove) as u8;
        }
        if m.kind == MobKind::Rabbit {
            a.variant = if matches!(biome, SnowyPlains | SnowyTaiga | Grove) {
                if self.rng.chance(0.8) { 1 } else { 5 }
            } else if biome == Desert {
                4
            } else {
                [0, 2, 3, 5][self.rng.next_int(4) as usize]
            };
        }
        if m.kind == MobKind::Wolf {
            a.variant = match biome {
                Forest => 1,
                SnowyTaiga | Grove => 2,
                OldGrowthBirchForest => 3,
                OldGrowthPineTaiga => 4,
                OldGrowthSpruceTaiga => 5,
                SparseJungle => 6,
                SavannaPlateau => 7,
                WoodedBadlands => 8,
                _ => 0,
            };
        }
        if (m.kind.is_breedable() || m.kind == MobKind::PolarBear) && m.kind != MobKind::Hoglin {
            let baby = match m.kind {
                // FoxGroupData never rolls babies; the third fox on is one.
                MobKind::Fox => pack_size >= 2,
                // The first member of a pack is always an adult.
                _ if pack_size == 0 => false,
                // RabbitGroupData's baby chance is 1.
                MobKind::Rabbit | MobKind::PolarBear | MobKind::Ocelot => true,
                MobKind::Panda => self.rng.chance(0.2),
                _ => self.rng.chance(0.05),
            };
            if baby {
                m.baby = true;
                m.age = BABY_AGE;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        physics::{BlockSource, test_util::Grid},
        world::{block::Block, terrain::Dimension},
    };
    struct Flat(Grid);
    impl BlockSource for Flat {
        fn block(&self, p: IVec3) -> Option<Block> {
            self.0.block(p)
        }
    }
    impl MobWorld for Flat {
        fn loaded(&self, _: IVec3) -> bool {
            true
        }
        fn surface(&self, _: i32, _: i32) -> Option<i32> {
            Some(0)
        }
        fn exposed(&self, _: IVec3) -> bool {
            true
        }
    }
    fn ctx() -> Ctx {
        Ctx {
            spawn_light: crate::simulation::lighting::SpawnLight::DAY,
            players: vec![super::super::Target::new(PlayerId::HOST, DVec3::new(0.0, 1.0, 0.0), false)],
            daylight: 1.0,
            spawning: false,
            raining: false,
            dimension: Dimension::Overworld,
        }
    }
    #[test]
    fn food_and_baby_growth_match_java() {
        for (kind, food) in [
            (MobKind::Cow, Item::WHEAT),
            (MobKind::Sheep, Item::WHEAT),
            (MobKind::Pig, Item::CARROT),
            (MobKind::Chicken, Item::WHEAT_SEEDS),
            (MobKind::Cat, Item::COD),
            (MobKind::Axolotl, Item::TROPICAL_FISH_BUCKET),
        ] {
            assert!(kind.breeding_food(food));
            assert!(!kind.breeding_food(Item::STRING));
            let mut e = Entities::new(1);
            e.spawn(kind, DVec3::Y);
            if kind == MobKind::Cat {
                e.mobs[0].animal.as_mut().unwrap().owner = Some(PlayerId(19));
            }
            e.mobs[0].age = BABY_AGE;
            e.mobs[0].baby = true;
            assert_eq!(e.use_animal(0, Some(Stack::new(food, 1)), PlayerId(19)), Some(true));
            assert_eq!(e.mobs[0].age, -21600);
            assert_eq!(e.mobs[0].animal.as_ref().unwrap().love, 0.0);
            e.mobs[0].advance_animal_age(1080.0);
            assert_eq!(e.mobs[0].age, 0);
            assert!(!e.mobs[0].baby);
        }
    }
    #[test]
    fn a_golden_dandelion_locks_and_unlocks_a_babys_age_and_saves() {
        let dandelion = Stack::new(crate::world::gen5_blocks::GOLDEN_DANDELION, 1);
        let mut e = Entities::new(7);
        e.spawn(MobKind::Cow, DVec3::new(0.0, 1.0, 0.0));
        // Grown animals ignore it.
        assert_eq!(e.use_animal(0, Some(dandelion), PlayerId(4)), None);
        e.mobs[0].age = -1200;
        e.mobs[0].baby = true;
        assert_eq!(e.use_animal(0, Some(dandelion), PlayerId(4)), Some(true));
        assert!(e.mobs[0].age_locked && e.mobs[0].persistent);
        assert_eq!(e.mobs[0].age, BABY_AGE, "`setAge(getBabyStartAge())`");
        e.mobs[0].advance_animal_age(10_000.0);
        assert_eq!((e.mobs[0].age, e.mobs[0].baby), (BABY_AGE, true), "a locked baby stays one");
        let saved = super::super::nether::save_mob(&e.mobs[0]);
        assert_eq!(saved["age_locked"], true);
        assert_eq!(e.use_animal(0, Some(dandelion), PlayerId(4)), Some(true));
        assert!(!e.mobs[0].age_locked);
        e.mobs[0].advance_animal_age(1200.0);
        assert!(!e.mobs[0].baby, "unlocked, it grows up");
    }
    #[test]
    fn tiny_steps_do_not_accelerate_growth() {
        let mut m = Mob::new(MobKind::Chicken, DVec3::Y, 0.0);
        m.age = BABY_AGE;
        m.baby = true;
        for _ in 0..1000 {
            m.advance_animal_age(0.001);
        }
        assert!((m.age - (BABY_AGE + 20)).abs() <= 1);
    }
    #[test]
    fn parents_court_then_cool_down_and_do_not_breed_twice() {
        let mut e = Entities::new(7);
        for x in [0.0, 2.0] {
            e.spawn(MobKind::Cow, DVec3::new(x, 1.0, 0.0));
        }
        for i in 0..2 {
            e.mobs[i].uid = i as u32 + 1;
            assert_eq!(e.use_animal(i, Some(Stack::new(Item::WHEAT, 1)), PlayerId(8)), Some(true));
        }
        let w = Flat(Grid::flat(0));
        let c = ctx();
        // `BreedGoal`: `loveTime` reaches `adjustedTickDelay(60)` = 30 goal
        // updates, every other tick, within 3 blocks.
        let mut bred = None;
        for tick in 0..200 {
            e.update(0.05, &w, &c);
            if bred.is_none() && e.mobs.len() == 3 {
                bred = Some(tick);
            }
        }
        let bred = bred.expect("the pair breeds");
        assert!((56..=62).contains(&bred), "courting takes about 60 ticks, not {bred}");
        assert_eq!(e.mobs.len(), 3);
        assert_eq!(e.mobs[2].age, BABY_AGE + (199 - bred));
        for i in 0..2 {
            assert_eq!(e.mobs[i].age, BREEDING_COOLDOWN - (199 - bred));
            assert_eq!(e.use_animal(i, Some(Stack::new(Item::WHEAT, 1)), PlayerId(8)), None);
        }
        let xp: u32 = e.orbs.iter().map(|o| o.value).sum();
        assert!((1..=7).contains(&xp));
    }
    #[test]
    fn animal_save_roundtrip_and_old_saves() {
        let mut e = Entities::new(1);
        e.spawn(MobKind::Sheep, DVec3::Y);
        let m = &mut e.mobs[0];
        m.age = -1200;
        m.baby = true;
        m.sheared = true;
        m.wool_color = DyeColor::Blue;
        let a = m.animal.as_mut().unwrap();
        a.owner = Some(PlayerId(55));
        a.sitting = true;
        a.love = 18.0;
        a.collar = DyeColor::Pink;
        let mut restored = Entities::new(2);
        restored.load_nether_mobs(&e.nether_mobs_to_string());
        let m = &restored.mobs[0];
        assert_eq!(m.age, -1200);
        assert!(m.sheared);
        assert_eq!(m.wool_color, DyeColor::Blue);
        let a = m.animal.as_ref().unwrap();
        assert_eq!(a.owner, Some(PlayerId(55)));
        assert!(a.sitting);
        assert_eq!(a.love, 18.0);
        restored.load_nether_mobs(r#"{"mobs":[{"kind":"cow","pos":[1,2,3]}]}"#);
        assert_eq!(restored.mobs[1].age, 0);
    }
    #[test]
    fn sheep_inherit_craftable_colours_or_a_parent() {
        let mut rng = Rng(9);
        assert_eq!(inherit_color(DyeColor::Red, DyeColor::Yellow, &mut rng), DyeColor::Orange);
        for _ in 0..20 {
            assert!([DyeColor::Green, DyeColor::Black].contains(&inherit_color(
                DyeColor::Green,
                DyeColor::Black,
                &mut rng
            )));
        }
    }
    #[test]
    fn turtle_breeding_produces_pregnancy_and_growth_uses_scute_clock() {
        let mut e = Entities::new(9);
        for x in [0.0, 2.0] {
            e.spawn(MobKind::Turtle, DVec3::new(x, 1.0, 0.0));
        }
        for i in 0..2 {
            e.mobs[i].uid = i as u32 + 1;
            e.use_animal(i, Some(Stack::new(crate::world::overworld_blocks::SEAGRASS, 1)), PlayerId::HOST);
        }
        for _ in 0..80 {
            e.update(0.05, &Flat(Grid::flat(0)), &ctx());
        }
        assert_eq!(e.mobs.len(), 2);
        assert!(e.mobs.iter().any(|m| m.animal.as_ref().unwrap().pregnant));
        e.hatch_turtles(IVec3::new(4, 1, 0), 1);
        e.mobs[2].feed_growth();
        assert_eq!(e.mobs[2].grow, 120.0);
        e.grow_turtles(1080.0);
        assert!(!e.mobs[2].baby);
        assert_eq!(e.items.iter().filter(|i| i.stack.item == Item::TURTLE_SCUTE).count(), 1);
    }
    #[test]
    fn tameable_animals_keep_owner_and_only_owner_can_order_them() {
        for (kind, food) in
            [(MobKind::Wolf, Item::BONE), (MobKind::Cat, Item::COD), (MobKind::Parrot, Item::WHEAT_SEEDS)]
        {
            let mut e = Entities::new(91);
            e.spawn(kind, DVec3::Y);
            for _ in 0..1000 {
                e.use_animal(0, Some(Stack::new(food, 1)), PlayerId(42));
                if e.mobs[0].animal.as_ref().unwrap().owner.is_some() {
                    break;
                }
            }
            assert_eq!(e.mobs[0].animal.as_ref().unwrap().owner, Some(PlayerId(42)));
            // Parrots aren't ordered to sit when tamed (v1.0 Track 6).
            let sits = kind != MobKind::Parrot;
            assert_eq!(e.mobs[0].animal.as_ref().unwrap().sitting, sits);
            assert_eq!(e.use_animal(0, None, PlayerId(7)), None);
            // A parrot takes the order only while it isn't flying.
            e.mobs[0].on_ground = true;
            assert_eq!(e.use_animal(0, None, PlayerId(42)), Some(false));
            assert_eq!(e.mobs[0].animal.as_ref().unwrap().sitting, !sits);
            let mut restored = Entities::new(1);
            restored.load_nether_mobs(&e.nether_mobs_to_string());
            assert_eq!(restored.mobs[0].animal.as_ref().unwrap().owner, Some(PlayerId(42)));
            assert_eq!(restored.mobs[0].health, e.mobs[0].health);
            if kind == MobKind::Wolf {
                assert_eq!(e.mobs[0].health, 40.0);
            }
        }
    }
    #[test]
    fn wolves_heal_before_breeding_and_defend_their_profile() {
        let mut e = Entities::new(6);
        e.spawn(MobKind::Wolf, DVec3::Y);
        e.spawn(MobKind::Zombie, DVec3::new(2.0, 1.0, 0.0));
        e.mobs[0].uid = 1;
        e.mobs[1].uid = 2;
        e.mobs[0].health = 36.0;
        e.mobs[0].animal.as_mut().unwrap().owner = Some(PlayerId(71));
        assert_eq!(e.use_animal(0, Some(Stack::new(Item::ROTTEN_FLESH, 1)), PlayerId(71)), Some(true));
        assert_eq!(e.mobs[0].health, 40.0);
        assert_eq!(e.mobs[0].animal.as_ref().unwrap().love, 0.0);
        e.animals_owner_attack(1, PlayerId(72));
        assert!(e.mobs[0].animal.as_ref().unwrap().foe.is_none());
        e.animals_owner_attack(1, PlayerId(71));
        assert_eq!(e.mobs[0].animal.as_ref().unwrap().foe.unwrap().0, 2);
    }
    #[test]
    fn fox_babies_trust_both_feeders_without_becoming_tame() {
        let mut e = Entities::new(1);
        for _ in 0..2 {
            e.spawn(MobKind::Fox, DVec3::Y);
        }
        for (i, p) in [(0, 12), (1, 37)] {
            e.use_animal(i, Some(Stack::new(Item::SWEET_BERRIES, 1)), PlayerId(p));
        }
        let child = e.animal_child(0, 1);
        let a = child.animal.as_ref().unwrap();
        assert_eq!(a.trusted, [Some(PlayerId(12)), Some(PlayerId(37))]);
        assert_eq!(a.owner, None);
        let trusted = a.trusted;
        e.mobs.push(child);
        let mut restored = Entities::new(2);
        restored.load_nether_mobs(&e.nether_mobs_to_string());
        assert_eq!(restored.mobs[2].animal.as_ref().unwrap().trusted, trusted);
    }
    #[test]
    fn parrots_perch_and_release_when_owner_jumps_and_cookies_are_lethal() {
        let mut e = Entities::new(11);
        e.spawn(MobKind::Parrot, DVec3::Y);
        e.mobs[0].animal.as_mut().unwrap().owner = Some(PlayerId::HOST);
        let mut c = ctx();
        let w = Flat(Grid::flat(0));
        e.tick_companions(0.05, &w, &c, &mut Vec::new());
        assert!(e.mobs[0].animal.as_ref().unwrap().perched.is_some());
        c.players[0].on_ground = false;
        e.tick_companions(0.05, &w, &c, &mut Vec::new());
        assert!(e.mobs[0].animal.as_ref().unwrap().perched.is_none());
        assert_eq!(e.use_animal(0, Some(Stack::new(Item::COOKIE, 1)), PlayerId::HOST), Some(true));
        assert!(!e.mobs[0].alive());
        assert!(e.items.iter().any(|i| i.stack.item == Item::FEATHER));
    }
    #[test]
    fn sheep_grazing_regrows_wool_and_honours_griefing() {
        let mut e = Entities::new(2);
        e.spawn(MobKind::Sheep, DVec3::Y);
        e.mobs[0].sheared = true;
        e.mobs[0].animal.as_mut().unwrap().grazing = 0.25;
        let mut grid = Grid::flat(0);
        grid.set(IVec3::ZERO, Block::GRASS);
        let w = Flat(grid);
        let mut events = Vec::new();
        e.tick_animal_lifecycle(0.05, &w, &ctx(), &mut events);
        assert!(!e.mobs[0].sheared);
        assert!(
            events.iter().any(|e| matches!(e, EntityEvent::AnimalBlock { from: Block::GRASS, to: Block::DIRT, .. }))
        );
        e.villager_griefing = false;
        e.mobs[0].sheared = true;
        e.mobs[0].animal.as_mut().unwrap().grazing = 0.25;
        events.clear();
        e.tick_animal_lifecycle(0.05, &w, &ctx(), &mut events);
        assert!(!e.mobs[0].sheared);
        // No block change, but `Mob.ate` still posts `eat` (v0.17 Track E1).
        assert!(!events.iter().any(|e| matches!(e, EntityEvent::AnimalBlock { .. })));
        assert!(
            events
                .iter()
                .any(|e| matches!(e, EntityEvent::Vibration { event: crate::world::vibration::GameEvent::Eat, .. }))
        );
    }
    #[test]
    fn turtle_egg_stages_keep_count() {
        for count in 1..=4 {
            for stage in 0..=2 {
                let b = crate::world::overworld_blocks::turtle_eggs_stage(count, stage);
                assert_eq!(crate::world::overworld_blocks::egg_count(b), Some(count));
                assert_eq!(crate::world::overworld_blocks::egg_stage(b), Some(stage));
                assert_eq!(b.name(), "turtle egg");
                assert_eq!(crate::world::shape::item_shape(b).as_slice().len(), count as usize);
            }
        }
    }
    #[test]
    fn goat_ram_breaks_a_horn_on_natural_stone() {
        let mut e = Entities::new(19);
        e.spawn(MobKind::Goat, DVec3::new(0.5, 1.0, 0.5));
        let a = e.mobs[0].animal.as_mut().unwrap();
        a.ramming = 5.0;
        a.ram_prepare = 0.0;
        a.ram_goal = Some(DVec3::new(5.0, 1.0, 0.5));
        let mut grid = Grid::flat(0);
        grid.set(IVec3::new(1, 1, 0), Block::STONE);
        e.tick_animal_lifecycle(0.05, &Flat(grid), &ctx(), &mut Vec::new());
        assert_eq!(e.mobs[0].animal.as_ref().unwrap().horns, 1);
        assert!(e.items.iter().any(|i| (Item::GOAT_HORN.0..Item::GOAT_HORN.0 + 8).contains(&i.stack.item.0)));
        let mut restored = Entities::new(8);
        restored.load_nether_mobs(&e.nether_mobs_to_string());
        assert_eq!(restored.mobs[0].animal.as_ref().unwrap().horns, 1);
    }
    // ------------------------------------------------------- v1.0 Track 6

    fn tame(e: &mut Entities, kind: MobKind, at: DVec3, owner: u32) -> usize {
        e.spawn(kind, at);
        e.ensure_mob_ids();
        let i = e.mobs.len() - 1;
        e.mobs[i].animal.as_mut().unwrap().owner = Some(PlayerId(owner));
        if kind == MobKind::Wolf {
            e.mobs[i].health = 40.0;
        }
        i
    }

    /// `tryToTame`: wolves and cats are ordered to sit, parrots aren't;
    /// only a wolf's health changes (`applyTamingSideEffects`).
    #[test]
    fn taming_orders_wolves_and_cats_to_sit_but_not_parrots() {
        for (kind, food, sits) in [
            (MobKind::Wolf, Item::BONE, true),
            (MobKind::Cat, Item::SALMON, true),
            (MobKind::Parrot, Item::PUMPKIN_SEEDS, false),
        ] {
            let mut e = Entities::new(5);
            e.spawn(kind, DVec3::Y);
            e.mobs[0].health = 3.0;
            let mut tries = 0;
            while e.mobs[0].animal.as_ref().unwrap().owner.is_none() {
                assert_eq!(e.use_animal(0, Some(Stack::new(food, 1)), PlayerId(4)), Some(true), "{kind:?}");
                tries += 1;
                assert!(tries < 200);
            }
            assert_eq!(e.mobs[0].animal.as_ref().unwrap().sitting, sits, "{kind:?}");
            let health = if kind == MobKind::Wolf { 40.0 } else { 3.0 };
            assert_eq!(e.mobs[0].health, health, "{kind:?}");
        }
        // An angry wolf takes no bone.
        let mut e = Entities::new(5);
        e.spawn(MobKind::Wolf, DVec3::Y);
        e.mobs[0].angry_player = Some(PlayerId(4));
        assert_ne!(e.use_animal(0, Some(Stack::new(Item::BONE, 1)), PlayerId(4)), Some(true));
    }

    /// The tame rolls are `nextInt(3)` and `nextInt(10)`.
    #[test]
    fn taming_takes_about_three_tries_for_wolves_and_ten_for_parrots() {
        for (kind, food, expected) in [(MobKind::Wolf, Item::BONE, 3.0), (MobKind::Parrot, Item::WHEAT_SEEDS, 10.0)] {
            let mut total = 0;
            for seed in 0..400 {
                let mut e = Entities::new(seed);
                e.spawn(kind, DVec3::Y);
                while e.mobs[0].animal.as_ref().unwrap().owner.is_none() {
                    e.use_animal(0, Some(Stack::new(food, 1)), PlayerId(4));
                    total += 1;
                }
            }
            let mean = total as f64 / 400.0;
            assert!((mean - expected).abs() < expected * 0.2, "{kind:?}: {mean}");
        }
    }

    /// `Wolf.mobInteract`: food heals twice its nutrition, for anyone; only
    /// the owner dyes the collar, and the same colour doesn't toggle sitting.
    /// `Cat.mobInteract`: the owner heals it by the food's nutrition.
    #[test]
    fn pets_heal_by_their_java_factor_and_only_owners_dye_them() {
        let mut e = Entities::new(8);
        let wolf = tame(&mut e, MobKind::Wolf, DVec3::Y, 1);
        e.mobs[wolf].health = 20.0;
        assert_eq!(e.use_animal(wolf, Some(Stack::new(Item::STEAK, 1)), PlayerId(2)), Some(true));
        assert_eq!(e.mobs[wolf].health, 36.0, "steak's 8 nutrition heals 16");
        let blue = Stack::new(crate::color::DyeColor::Blue.dye(), 1);
        assert_eq!(e.use_animal(wolf, Some(blue), PlayerId(2)), None, "a stranger can't dye it");
        assert_eq!(e.mobs[wolf].animal.as_ref().unwrap().collar, DyeColor::Red);
        let red = Stack::new(DyeColor::Red.dye(), 1);
        let sitting = e.mobs[wolf].animal.as_ref().unwrap().sitting;
        assert_eq!(e.use_animal(wolf, Some(red), PlayerId(1)), None, "the same colour does nothing");
        assert_eq!(e.mobs[wolf].animal.as_ref().unwrap().sitting, sitting);
        assert_eq!(e.use_animal(wolf, Some(blue), PlayerId(1)), Some(true));
        assert_eq!(e.mobs[wolf].animal.as_ref().unwrap().collar, DyeColor::Blue);
        // Full health: food is breeding food, and a wolf in love passes it
        // on to the owner's order to sit.
        e.mobs[wolf].health = 40.0;
        assert_eq!(e.use_animal(wolf, Some(Stack::new(Item::STEAK, 1)), PlayerId(1)), Some(true));
        assert!(e.mobs[wolf].animal.as_ref().unwrap().love > 0.0);
        assert_eq!(e.use_animal(wolf, Some(Stack::new(Item::STEAK, 1)), PlayerId(1)), Some(false));
        assert_eq!(e.mobs[wolf].animal.as_ref().unwrap().sitting, !sitting);

        let cat = tame(&mut e, MobKind::Cat, DVec3::new(4.0, 1.0, 0.0), 1);
        e.mobs[cat].health = 5.0;
        assert_eq!(e.use_animal(cat, Some(Stack::new(Item::COD, 1)), PlayerId(1)), Some(true));
        assert_eq!(e.mobs[cat].health, 7.0, "cod's 2 nutrition heals 2");
        // A stranger's cod breeds it instead of healing it.
        assert_eq!(e.use_animal(cat, Some(Stack::new(Item::COD, 1)), PlayerId(2)), Some(true));
        assert_eq!(e.mobs[cat].health, 7.0);
        assert!(e.mobs[cat].animal.as_ref().unwrap().love > 0.0);
        // A wild wolf eats meat and falls in love, but can't breed untamed.
        let mut e = Entities::new(9);
        e.spawn(MobKind::Wolf, DVec3::Y);
        assert_eq!(e.use_animal(0, Some(Stack::new(Item::RAW_BEEF, 1)), PlayerId(2)), Some(true));
        assert!(e.mobs[0].animal.as_ref().unwrap().love > 0.0);
        assert!(!e.mobs[0].ready_to_breed());
    }

    /// `SitWhenOrderedToGoal`: an ordered pet sits only on the ground and
    /// out of water; the order survives a save and the pose comes back with
    /// it. Wolves and parrots stand when hurt, cats don't.
    #[test]
    fn ordered_pets_sit_on_dry_ground_and_wolves_stand_when_hurt() {
        let w = Flat(Grid::flat(0));
        let c = ctx();
        let mut e = Entities::new(3);
        let wolf = tame(&mut e, MobKind::Wolf, DVec3::new(0.5, 1.0, 0.5), 0);
        e.mobs[wolf].animal.as_mut().unwrap().owner = Some(PlayerId::HOST);
        e.mobs[wolf].animal.as_mut().unwrap().sitting = true;
        e.mobs[wolf].in_water = true;
        e.mobs[wolf].on_ground = true;
        let mut rng = Rng(1);
        let mut events = Vec::new();
        e.mobs[wolf].companion_think(0.05, &w, &c, &mut rng, &mut events);
        assert!(!e.mobs[wolf].animal.as_ref().unwrap().sit_pose, "no sitting in water");
        e.mobs[wolf].in_water = false;
        e.mobs[wolf].companion_think(0.05, &w, &c, &mut rng, &mut events);
        assert!(e.mobs[wolf].animal.as_ref().unwrap().sit_pose);
        let mut back = Entities::new(4);
        back.load_nether_mobs(&e.nether_mobs_to_string());
        let a = back.mobs[0].animal.as_ref().unwrap();
        assert!(a.sitting && a.sit_pose, "`readAdditionalSaveData` sits a loaded pet");
        for (kind, stands) in [(MobKind::Wolf, true), (MobKind::Parrot, true), (MobKind::Cat, false)] {
            let mut e = Entities::new(5);
            let i = tame(&mut e, kind, DVec3::Y, 1);
            e.mobs[i].animal.as_mut().unwrap().sitting = true;
            e.mobs[i].damage(1.0, None, &mut rng);
            assert_eq!(!e.mobs[i].animal.as_ref().unwrap().sitting, stands, "{kind:?}");
        }
    }

    /// `FollowOwnerGoal` with `tryToTeleportToOwner`: an owner 12 or more
    /// blocks away pulls the pet to a free spot 2–3 blocks from them; a
    /// sitting or leashed pet stays (`unableToMoveToOwner`).
    #[test]
    fn pets_teleport_to_a_far_owner_unless_sitting_or_leashed() {
        let mut grid = Grid::flat(0);
        grid.set(IVec3::new(3, 1, 3), Block::OAK_FENCE);
        let w = Flat(grid);
        for (kind, sitting, leashed, follows) in [
            (MobKind::Wolf, false, false, true),
            (MobKind::Cat, false, false, true),
            (MobKind::Wolf, true, false, false),
            (MobKind::Wolf, false, true, false),
        ] {
            let mut e = Entities::new(17);
            let i = tame(&mut e, kind, DVec3::new(0.5, 1.0, 0.5), 0);
            e.mobs[i].animal.as_mut().unwrap().owner = Some(PlayerId::HOST);
            e.mobs[i].animal.as_mut().unwrap().sitting = sitting;
            if leashed {
                e.mobs[i].leash = Some(super::super::leash::Leash::Fence(IVec3::new(3, 1, 3)));
            }
            let owner = DVec3::new(30.5, 1.0, 0.5);
            let mut c = ctx();
            c.players[0].pos = owner;
            for _ in 0..40 {
                e.update(0.05, &w, &c);
            }
            let d = (e.mobs[i].pos - owner).with_y(0.0).length();
            if follows {
                assert!(d <= 4.5, "{kind:?} is {d} from its owner");
                assert!(d >= 1.5, "{kind:?} landed {d} from its owner");
            } else {
                assert!(d > 25.0, "{kind:?} (sitting {sitting}, leashed {leashed}) moved to {d}");
            }
        }
    }

    /// `TamableAnimalPanicGoal.tick`: a panicking cat far from its owner
    /// teleports to it.
    #[test]
    fn a_panicking_cat_teleports_to_a_far_owner() {
        let w = Flat(Grid::flat(0));
        let mut e = Entities::new(23);
        let i = tame(&mut e, MobKind::Cat, DVec3::new(0.5, 1.0, 0.5), 0);
        e.mobs[i].animal.as_mut().unwrap().owner = Some(PlayerId::HOST);
        let mut rng = Rng(3);
        e.mobs[i].damage(1.0, Some(DVec3::X), &mut rng);
        assert_eq!(e.mobs[i].ai, crate::entity::mob::Ai::Panic);
        let owner = DVec3::new(20.5, 1.0, 0.5);
        let mut c = ctx();
        c.players[0].pos = owner;
        e.update(0.05, &w, &c);
        assert!((e.mobs[i].pos - owner).with_y(0.0).length() <= 4.5);
    }

    /// v1.0 Track 6: the 26.3 jar's food tags and farm-animal loot tables
    /// (`tests/fixtures/java/26.3/companions_villages.json`, from
    /// `tools/extract_java_companion_fixtures.py`), both ways.
    #[test]
    fn foods_temptations_and_drops_match_the_jar() {
        let v: Value =
            serde_json::from_str(include_str!("../../tests/fixtures/java/26.3/companions_villages.json")).unwrap();
        let names: Value =
            serde_json::from_str(include_str!("../../tests/fixtures/java/26.3/registry_names.json")).unwrap();
        let item = |id: &str| {
            let english = names["items"][id].as_str().or_else(|| names["blocks"][id].as_str()).unwrap_or_default();
            crate::java_registry::item(id, english).unwrap_or_else(|| panic!("{id}"))
        };
        let tag = |name: &str| -> Vec<Item> {
            v["tags"][name].as_array().unwrap().iter().map(|n| item(n.as_str().unwrap())).collect()
        };
        let every: Vec<Item> = v["tags"].as_object().unwrap().keys().flat_map(|k| tag(k)).collect();
        let check = |what: &str, wanted: &[Item], has: &dyn Fn(Item) -> bool| {
            for &food in &every {
                assert_eq!(has(food), wanted.contains(&food), "{what}: {}", food.name());
            }
        };
        use MobKind as K;
        for (kind, name) in [
            (K::Cow, "cow_food"),
            (K::Mooshroom, "cow_food"),
            (K::Pig, "pig_food"),
            (K::Sheep, "sheep_food"),
            (K::Chicken, "chicken_food"),
            (K::Rabbit, "rabbit_food"),
            (K::Goat, "goat_food"),
            (K::Wolf, "wolf_food"),
            (K::Cat, "cat_food"),
            (K::Camel, "camel_food"),
        ] {
            check(&format!("{kind:?} breeds with {name}"), &tag(name), &|f| kind.breeding_food(f));
        }
        // `TemptGoal`s follow the same tags (the pig also its carrot on a
        // stick); horses and llamas follow their own tempt tags.
        for (kind, name) in [
            (K::Cow, "cow_food"),
            (K::Sheep, "sheep_food"),
            (K::Chicken, "chicken_food"),
            (K::Rabbit, "rabbit_food"),
            (K::Goat, "goat_food"),
            (K::Horse, "horse_tempt_items"),
            (K::Llama, "llama_tempt_items"),
        ] {
            check(&format!("{kind:?} follows {name}"), &tag(name), &|f| super::super::mob::goals::tempted_by(kind, f));
        }
        check("parrots tame with parrot_food", &tag("parrot_food"), &|f| seeds(f));
        check("cats tame with cat_food", &tag("cat_food"), &|f| cat_food(f));
        assert_eq!(tag("parrot_poisonous_food"), vec![Item::COOKIE]);
        assert_eq!(tag("llama_food"), vec![Item::WHEAT, Item::from_block(crate::world::block::Block::HAY_BALE)]);
        for (kind, name) in
            [(K::Cow, "cow"), (K::Pig, "pig"), (K::Sheep, "sheep"), (K::Chicken, "chicken"), (K::Rabbit, "rabbit")]
        {
            for entry in v["loot"][name].as_array().unwrap() {
                let id = entry["item"].as_str().unwrap();
                // The rabbit's foot is the killed-by-player pool, rolled in `drops`.
                if id == "rabbit_foot" {
                    continue;
                }
                let drop = item(id);
                let count = (entry["count"][0].as_i64().unwrap(), entry["count"][1].as_i64().unwrap());
                let ours = kind.loot().iter().find(|(i, ..)| *i == drop).unwrap_or_else(|| panic!("{kind:?} {id}"));
                assert_eq!((i64::from(ours.1), i64::from(ours.2)), count, "{kind:?} {id}");
            }
        }
    }

    /// v1.0 gap Track 3b: `AgeableMob.ageUp(seconds, forced)` and
    /// `forcedAge`: a fed calf keeps the time it was forced through (and
    /// saves it); coming of age through `ageUp` makes that its age; 40
    /// ticks of particles, one every fourth.
    #[test]
    fn forced_growth_is_kept_saved_and_given_back_on_coming_of_age() {
        let mut m = Mob::new(MobKind::Dolphin, DVec3::ZERO, 0.0);
        m.baby = true;
        m.age = -24000;
        m.age_up(speed_up_seconds(24000), true);
        assert_eq!((m.age, m.forced_age, m.forced_age_timer), (-21600, 2400, 40));
        // A feeding is a tenth of the time left, so it never reaches 0
        // itself; a bigger, unforced `ageUp` does, and gives the forced age.
        let saved = super::super::nether::save_mob(&m);
        assert_eq!(saved["forced_age"], 2400);
        let loaded = super::super::nether::load_mob(&saved).unwrap();
        assert_eq!((loaded.forced_age, loaded.age), (2400, -21600));
        m.age_up(1200, false);
        assert_eq!(m.age, 2400);
        assert!(!m.baby);
        for _ in 0..40 {
            m.advance_animal_age(0.05);
        }
        assert_eq!(m.forced_sparkles, 10);
        assert_eq!(m.age, 2360, "an adult's positive age counts down");
        // Unforced, untouched mobs still save as before.
        let plain = Mob::new(MobKind::Dolphin, DVec3::ZERO, 0.0);
        assert!(super::super::nether::save_mob(&plain).get("forced_age").is_none());
    }
}
