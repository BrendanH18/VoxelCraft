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
    pub sitting: bool,
    pub collar: DyeColor,
    pub goal: Option<DVec3>,
    growth: f32,
    hearts: f32,
    pub grazing: f32,
    pub nest_time: f32,
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
            collar: DyeColor::Red,
            goal: None,
            growth: 0.0,
            hearts: 0.0,
            grazing: 0.0,
            nest_time: 0.0,
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
        }
    }
}
impl MobKind {
    /// Java 1.21 breeding food. A food predicate and `is_breedable` arm are
    /// the only registration needed for the generic mating/baby pipeline.
    pub fn breeding_food(self, food: Item) -> bool {
        match self {
            Self::Cow | Self::Sheep => food == Item::WHEAT,
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
            _ => false,
        }
    }
    pub fn has_animal_state(self) -> bool {
        self.is_breedable() || self == Self::Parrot
    }
    pub fn is_breedable(self) -> bool {
        matches!(
            self,
            Self::Wolf
                | Self::Fox
                | Self::Rabbit
                | Self::Goat
                | Self::Cow
                | Self::Sheep
                | Self::Pig
                | Self::Chicken
                | Self::Cat
                | Self::Hoglin
                | Self::Strider
                | Self::Turtle
                | Self::Axolotl
        )
    }
}
impl Mob {
    pub fn ready_to_breed(&self) -> bool {
        self.alive()
            && self.age == 0
            && !self.baby
            && self.animal.as_ref().is_some_and(|a| {
                a.love > 0.0
                    && !a.sitting
                    && !a.pregnant
                    && (!matches!(self.kind, MobKind::Cat | MobKind::Wolf) || a.owner.is_some())
                    && (self.kind != MobKind::Wolf || self.health >= self.max_health())
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
    pub(super) fn advance_animal_age(&mut self, dt: f32) {
        if !self.alive() || self.kind == MobKind::Turtle && self.baby {
            return;
        }
        if self.baby && self.age == 0 {
            self.age = BABY_AGE;
        }
        if let Some(a) = &mut self.animal {
            a.growth += dt * 20.0;
            let ticks = a.growth.floor() as i32;
            a.growth -= ticks as f32;
            self.age = if self.age < 0 { (self.age + ticks).min(0) } else { (self.age - ticks).max(0) };
            self.baby = self.age < 0;
        }
    }
    pub(super) fn animal_think<W: MobWorld + ?Sized>(
        &mut self,
        dt: f32,
        world: &W,
        ctx: &Ctx,
        rng: &mut Rng,
    ) -> Option<(Option<DVec3>, f64)> {
        let a = self.animal.as_ref()?;
        if a.sitting || a.grazing > 0.0 {
            return Some((None, 0.0));
        }
        let goal = a.goal.or_else(|| {
            ctx.players
                .iter()
                .filter(|t| {
                    t.alive
                        && t.pos.distance_squared(self.pos) <= 100.0
                        && t.held_item.is_some_and(|i| self.kind.breeding_food(i))
                })
                .min_by(|a, b| a.pos.distance_squared(self.pos).total_cmp(&b.pos.distance_squared(self.pos)))
                .map(|t| t.pos)
        });
        let goal = goal?;
        self.look_at(goal, 1.2);
        let flat = (goal - self.pos) * DVec3::new(1.0, 0.0, 1.0);
        if flat.length_squared() < 2.25 {
            return Some((None, 0.0));
        }
        Some((Some(self.steer(flat.normalize(), dt, world, rng)), 2.0))
    }
}
impl Entities {
    /// Shared action. `Some(true)` consumes food; `Some(false)` uses an
    /// empty hand/owner command; `None` permits ordinary item actions.
    pub fn use_animal(&mut self, index: usize, held: Option<Stack>, player: PlayerId) -> Option<bool> {
        if let Some(result) = self.use_lead_or_name(index, held, player) {
            return Some(result);
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
        let a = m.animal.as_mut()?;
        let taming = match m.kind {
            MobKind::Wolf => food == Some(Item::BONE),
            MobKind::Cat => food.is_some_and(|i| matches!(i, Item::COD | Item::SALMON)),
            MobKind::Parrot => food.is_some_and(seeds),
            _ => false,
        };
        if a.owner.is_none() && taming && m.angry_player.is_none() {
            let success = self.rng.chance(if m.kind == MobKind::Parrot { 0.1 } else { 1.0 / 3.0 });
            if success {
                a.owner = Some(player);
                a.sitting = true;
                m.health = if m.kind == MobKind::Wolf { 40.0 } else { m.kind.max_health() };
                m.persistent = true;
                self.animal_hearts(index);
            } else {
                self.particles.push(particles::Request::Burst(particles::Burst::new(
                    particles::Kind::Smoke,
                    m.pos + DVec3::Y * 0.7,
                    7,
                )));
            }
            return Some(true);
        }
        if a.owner.is_some() && matches!(m.kind, MobKind::Cat | MobKind::Wolf) {
            if let Some(c) = food.and_then(Item::dye_color)
                && c != a.collar
            {
                a.collar = c;
                return Some(true);
            }
            let max = if m.kind == MobKind::Wolf { 40.0 } else { m.kind.max_health() };
            if food.is_some_and(|i| m.kind.breeding_food(i)) && m.health < max {
                m.health = (m.health + food.and_then(Item::food).map_or(2.0, |f| f.0 as f32)).min(max);
                return Some(true);
            }
        }
        if a.owner == Some(player) && !food.is_some_and(|i| m.kind.breeding_food(i)) {
            a.sitting = !a.sitting;
            a.perched = None;
            a.foe = None;
            return Some(false);
        }
        if matches!(m.kind, MobKind::Cat | MobKind::Wolf) && a.owner.is_none() {
            return None;
        }
        let food = food?;
        if !m.kind.breeding_food(food) {
            return None;
        }
        if m.baby || m.age < 0 {
            m.feed_growth();
        } else {
            let a = m.animal.as_mut()?;
            if m.age != 0 || a.love > 0.0 || a.pregnant {
                return None;
            }
            a.love = LOVE_SECONDS;
            a.love_by = Some(player);
        }
        m.persistent = true;
        self.animal_hearts(index);
        Some(true)
    }
    fn animal_hearts(&mut self, index: usize) {
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
        let mut child = Mob::new(a.kind, a.pos, self.rng.range(0.0, std::f32::consts::TAU));
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
        child
    }
    pub(super) fn tick_animals<W: MobWorld + ?Sized>(
        &mut self,
        dt: f32,
        world: &W,
        _ctx: &Ctx,
        events: &mut Vec<EntityEvent>,
    ) {
        self.mob_index.rebuild(&self.mobs);
        let n = self.mobs.len();
        for i in 0..n {
            let m = &mut self.mobs[i];
            if !m.alive() || !world.loaded(m.pos.floor().as_ivec3()) {
                continue;
            }
            let Some(a) = m.animal.as_mut() else { continue };
            a.goal = None;
            a.love = (a.love - dt).max(0.0);
            a.hearts -= dt;
            let hearts = a.love > 0.0 && a.hearts <= 0.0;
            if hearts {
                a.hearts = 0.5;
                self.animal_hearts(i);
            }
            if !self.mobs[i].ready_to_breed() {
                self.mobs[i].animal.as_mut().unwrap().courtship = 0.0;
                continue;
            }
            let m = &self.mobs[i];
            let Some(j) = self
                .mob_index
                .nearest(&self.mobs, m.pos, 8.0, |b| b.uid != m.uid && b.kind == m.kind && b.ready_to_breed())
            else {
                self.mobs[i].animal.as_mut().unwrap().courtship = 0.0;
                continue;
            };
            let goal = self.mobs[j].pos;
            let distance = self.mobs[i].pos.distance_squared(goal);
            let a = self.mobs[i].animal.as_mut().unwrap();
            a.goal = Some(goal);
            a.courtship = if distance < 9.0 { a.courtship + dt } else { 0.0 };
            if a.courtship < COURTSHIP_SECONDS {
                continue;
            }
            let turtle = self.mobs[i].kind == MobKind::Turtle;
            let child = (!turtle).then(|| self.animal_child(i, j));
            for k in [i, j] {
                self.mobs[k].age = BREEDING_COOLDOWN;
                let a = self.mobs[k].animal.as_mut().unwrap();
                a.love = 0.0;
                a.courtship = 0.0;
                a.goal = None;
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
        self.tick_animal_lifecycle(dt, world, _ctx, events);
        self.tick_companions(dt, world, _ctx, events);
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
    json!({"love":a.love,"love_by":a.love_by.map(|p|p.0),"courtship":a.courtship,"home":a.home.to_array(),"pregnant":a.pregnant,"owner":a.owner.map(|p|p.0),"sitting":a.sitting,"collar":a.collar as u8,"growth":a.growth,"grazing":a.grazing,"nest_time":a.nest_time, "variant":a.variant, "trusted":a.trusted.map(|p|p.map(|p|p.0)), "mouth":a.mouth.map(|s|crate::inventory::stack_to_string(Some(s))), "mouth_time":a.mouth_time, "perched":a.perched, "horns":a.horns, "horn_kind":a.horn_kind, "screaming":a.screaming, "ram_cooldown":a.ram_cooldown, "jump_cooldown":a.jump_cooldown})
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
    a.pregnant = v["pregnant"].as_bool().unwrap_or(false);
    a.collar = DyeColor::ALL.get(v["collar"].as_u64().unwrap_or(14) as usize).copied().unwrap_or(DyeColor::Red);
    a.growth = timer("growth", 1.0);
    a.grazing = timer("grazing", 2.0);
    a.nest_time = timer("nest_time", 10.0);
    if let Some(p) = v["home"].as_array().filter(|p| p.len() == 3) {
        let coord = |i: usize| p[i].as_i64().and_then(|n| i32::try_from(n).ok());
        if let (Some(x), Some(y), Some(z)) = (coord(0), coord(1), coord(2)) {
            a.home = IVec3::new(x, y, z);
        }
    }
}

pub fn seeds(food: Item) -> bool {
    matches!(food, Item::WHEAT_SEEDS | Item::PUMPKIN_SEEDS | Item::MELON_SEEDS | Item::BEETROOT_SEEDS)
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
pub(super) fn on_spawn(m: &mut Mob, rng: &mut Rng) {
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
        if self.kind == MobKind::Wolf && self.animal.as_ref().is_some_and(|a| a.owner.is_some()) {
            40.0
        } else {
            self.kind.max_health()
        }
    }
    pub(super) fn companion_think<W: MobWorld + ?Sized>(
        &mut self,
        dt: f32,
        w: &W,
        ctx: &Ctx,
        rng: &mut Rng,
        events: &mut Vec<EntityEvent>,
    ) -> Option<(Option<DVec3>, f64)> {
        let a = self.animal.as_ref()?;
        if a.sitting || a.sleeping {
            return Some((None, 0.0));
        }
        if self.kind == MobKind::Wolf
            && let Some(id) = self.angry_player
        {
            if let Some(t) = ctx.players.iter().find(|t| t.id == id && t.targetable) {
                let flat = (t.pos - self.pos) * DVec3::new(1.0, 0.0, 1.0);
                if flat.length_squared() < 2.25 && self.attack_cooldown <= 0.0 {
                    self.attack_cooldown = 1.0;
                    events.push(EntityEvent::PlayerHit {
                        player: id,
                        damage: 4.0,
                        knockback: (flat.normalize_or(DVec3::X) * 4.0 + DVec3::Y * 3.0).as_vec3(),
                        cause: "was slain by a wolf",
                    });
                }
                return Some((Some(self.steer(flat.normalize_or(DVec3::X), dt, w, rng)), 3.0));
            }
            self.angry_player = None;
        }
        if let Some((target, goal)) = a.foe {
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
                });
            }
            return Some((Some(self.steer(flat.normalize_or(DVec3::X), dt, w, rng)), 3.0));
        }
        if self.kind == MobKind::Goat && a.ramming > 0.0 {
            let goal = a.ram_goal?;
            let flat = (goal - self.pos) * DVec3::new(1.0, 0.0, 1.0);
            return Some((if a.ram_prepare > 0.0 { None } else { Some(flat.normalize_or(DVec3::X)) }, 8.0));
        }
        if a.goal.is_some() || ctx.players.iter().any(|t| t.held_item.is_some_and(|i| self.kind.breeding_food(i))) {
            return None;
        }
        let owner = a.owner?;
        let t = ctx.players.iter().find(|t| t.id == owner && t.alive)?;
        let distance = t.pos.distance_squared(self.pos);
        if distance > 144.0 && self.riding.is_none() && self.leash.is_none() {
            for _ in 0..10 {
                let p = t.pos.floor().as_ivec3()
                    + IVec3::new(rng.next_int(7) as i32 - 3, rng.next_int(3) as i32 - 1, rng.next_int(7) as i32 - 3);
                let at = p.as_dvec3() + DVec3::new(0.5, 0.0, 0.5);
                if (at.x - t.pos.x).abs() < 2.0 && (at.z - t.pos.z).abs() < 2.0 {
                    continue;
                }
                if w.loaded(p)
                    && w.block(p - IVec3::Y).is_some_and(|b| b.is_solid() && !b.is_leaves())
                    && !crate::physics::overlaps_solid(w, at, self.shape())
                    && !crate::physics::is_fluid_at(w, at)
                {
                    self.pos = at;
                    self.previous_pos = at;
                    self.vel = DVec3::ZERO;
                    break;
                }
            }
        }
        if distance < 4.0 {
            return Some((None, 0.0));
        }
        self.look_at(t.pos, 1.2);
        let dir = if self.kind == MobKind::Parrot {
            (t.pos + DVec3::Y * 1.5 - self.pos).normalize_or(DVec3::X)
        } else {
            ((t.pos - self.pos) * DVec3::new(1.0, 0.0, 1.0)).normalize_or(DVec3::X)
        };
        Some((Some(self.steer(dir, dt, w, rng)), 2.5))
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
            if !self.mobs[i].alive() || !w.loaded(self.mobs[i].pos.floor().as_ivec3()) {
                continue;
            }
            let m = &self.mobs[i];
            if m.kind == MobKind::Creeper {
                if let Some(j) = self.mob_index.nearest(&self.mobs, m.pos, 6.0, |b| b.kind == MobKind::Cat && b.alive())
                {
                    let from = self.mobs[j].pos;
                    let m = &mut self.mobs[i];
                    m.ai = super::mob::Ai::Panic;
                    m.ai_timer = 1.0;
                    m.move_yaw = (m.pos.z - from.z).atan2(m.pos.x - from.x) as f32;
                    m.fuse = 0.0;
                    m.hunt = Some(m.pos + (m.pos - from).normalize_or(DVec3::X) * 8.0);
                } else {
                    self.mobs[i].hunt = None;
                }
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
                        let side = if self.mobs[..i].iter().any(|b| {
                            b.kind == MobKind::Parrot
                                && b.animal.as_ref().is_some_and(|a| a.owner == Some(t.id) && a.perched == Some(0))
                        }) {
                            1
                        } else {
                            0
                        };
                        if !self.mobs[..i].iter().any(|b| {
                            b.kind == MobKind::Parrot
                                && b.animal.as_ref().is_some_and(|a| a.owner == Some(t.id) && a.perched == Some(side))
                        }) {
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
    pub(super) fn natural_animal(&mut self, biome: crate::world::terrain::Biome) {
        use crate::world::terrain::Biome::*;
        let m = self.mobs.last_mut().unwrap();
        let Some(a) = m.animal.as_mut() else { return };
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
        if m.kind.is_breedable()
            && m.kind != MobKind::Hoglin
            && self.rng.chance(if m.kind == MobKind::Fox { 0.2 } else { 0.05 })
        {
            m.baby = true;
            m.age = BABY_AGE;
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
        for _ in 0..61 {
            e.tick_animals(0.05, &w, &c, &mut Vec::new());
        }
        assert_eq!(e.mobs.len(), 3);
        assert_eq!(e.mobs[2].age, BABY_AGE);
        for i in 0..2 {
            assert_eq!(e.mobs[i].age, BREEDING_COOLDOWN);
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
        for _ in 0..61 {
            e.tick_animals(0.05, &Flat(Grid::flat(0)), &ctx(), &mut Vec::new());
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
            assert!(e.mobs[0].animal.as_ref().unwrap().sitting);
            assert_eq!(e.use_animal(0, None, PlayerId(7)), None);
            assert_eq!(e.use_animal(0, None, PlayerId(42)), Some(false));
            assert!(!e.mobs[0].animal.as_ref().unwrap().sitting);
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
        e.mobs[0].animal.as_mut().unwrap().grazing = 0.05;
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
        e.mobs[0].animal.as_mut().unwrap().grazing = 0.05;
        events.clear();
        e.tick_animal_lifecycle(0.05, &w, &ctx(), &mut events);
        assert!(!e.mobs[0].sheared);
        assert!(events.is_empty());
    }
    #[test]
    fn turtle_nests_only_at_home_and_egg_stages_keep_count() {
        let mut e = Entities::new(4);
        e.spawn(MobKind::Turtle, DVec3::new(0.5, 1.0, 0.5));
        let a = e.mobs[0].animal.as_mut().unwrap();
        a.pregnant = true;
        let mut grid = Grid::flat(0);
        grid.set(IVec3::ZERO, Block::SAND);
        let w = Flat(grid);
        let mut events = Vec::new();
        e.tick_animal_lifecycle(10.0, &w, &ctx(), &mut events);
        assert!(!e.mobs[0].animal.as_ref().unwrap().pregnant);
        assert!(events.iter().any(
            |e| matches!(e,EntityEvent::AnimalBlock{to,..} if crate::world::overworld_blocks::egg_count(*to).is_some())
        ));
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
}
