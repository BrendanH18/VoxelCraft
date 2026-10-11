//! Java animal age, temptation and courtship. Add a food arm to
//! `MobKind::breeding_food` to opt another species into the shared baby API.
use glam::{DVec3, IVec3};
use serde_json::{Value, json};

use super::{Ctx, Entities, EntityEvent, Mob, MobKind, MobWorld, PlayerId, Rng};
use crate::{color::DyeColor, inventory::Stack, item::Item, particles};

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
            Self::Chicken => matches!(food, Item::WHEAT_SEEDS | Item::PUMPKIN_SEEDS),
            Self::Cat => matches!(food, Item::COD | Item::SALMON),
            Self::Hoglin => food == Item::from_block(crate::world::nether_biome_blocks::CRIMSON_FUNGUS),
            Self::Strider => food == Item::from_block(crate::world::nether_biome_blocks::WARPED_FUNGUS),
            Self::Turtle => food == Item::from_block(crate::world::overworld_blocks::SEAGRASS),
            Self::Axolotl => food == Item::TROPICAL_FISH_BUCKET,
            _ => false,
        }
    }
    pub fn is_breedable(self) -> bool {
        matches!(
            self,
            Self::Cow
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
            && self.animal.as_ref().is_some_and(|a| a.love > 0.0 && !a.sitting && !a.pregnant)
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
        let m = self.mobs.get_mut(index)?;
        if !m.alive() {
            return None;
        }
        let food = held?.item;
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
        // World-changing animal goals use compare-and-set events, so unloaded
        // or replaced blocks cannot accidentally be overwritten by the client.
        let _ = events;
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
    json!({"love":a.love,"love_by":a.love_by.map(|p|p.0),"courtship":a.courtship,"home":a.home.to_array(),"pregnant":a.pregnant,"owner":a.owner.map(|p|p.0),"sitting":a.sitting,"collar":a.collar as u8,"growth":a.growth,"grazing":a.grazing,"nest_time":a.nest_time})
}
pub(super) fn load(a: &mut State, v: &Value) {
    let timer =
        |key: &str, max: f32| v[key].as_f64().filter(|f| f.is_finite()).unwrap_or(0.0).clamp(0.0, max as f64) as f32;
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
}
