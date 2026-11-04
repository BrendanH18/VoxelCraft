//! Java 26.3 MushroomCow: cow goals, variant inheritance, flower storage,
//! bowl milking and shearing. Effect codes are saved item payloads: append only.
use super::{Entities, Mob, MobKind, Rng};
use crate::{
    inventory::Stack,
    item::Item,
    simulation::{effects::Effect, survival::Vitals},
    world::block::Block,
};
use serde_json::{Value, json};

#[derive(Clone, Debug, Default)]
pub struct State {
    pub brown: bool,
    pub stew: u32,
}
impl State {
    pub fn save(&self) -> Value {
        json!({"brown": self.brown, "stew": self.stew})
    }
    pub fn load(&mut self, v: &Value) {
        self.brown = v["brown"] == true;
        self.stew =
            v["stew"].as_u64().and_then(|n| u32::try_from(n).ok()).filter(|&n| effect(n).is_some()).unwrap_or(0);
    }
}
impl Mob {
    /// One conversion per lightning bolt. Caller deduplicates a bolt's flashes,
    /// as MushroomCow.thunderHit does with lastLightningBoltUUID.
    pub fn lightning_swap(&mut self) {
        if self.alive()
            && let Some(s) = &mut self.mooshroom
        {
            s.brown = !s.brown;
        }
    }
}

pub(super) fn inherit(child: &mut Mob, a: &Mob, b: &Mob, rng: &mut Rng) {
    if let (Some(c), Some(a), Some(b)) = (&mut child.mooshroom, &a.mooshroom, &b.mooshroom) {
        c.brown = if a.brown == b.brown && rng.next_int(1024) == 0 {
            !a.brown
        } else if rng.chance(0.5) {
            a.brown
        } else {
            b.brown
        };
    }
}

/// Stored codes are independent of Effect's enum order and flower block IDs.
fn effect(code: u32) -> Option<(Effect, u32)> {
    Some(match code {
        1 => (Effect::Saturation, 7),
        2 => (Effect::NightVision, 100),
        3 => (Effect::FireResistance, 60),
        4 => (Effect::Blindness, 220),
        5 => (Effect::Weakness, 140),
        6 => (Effect::Regeneration, 140),
        7 => (Effect::JumpBoost, 100),
        8 => (Effect::Wither, 140),
        9 => (Effect::Poison, 220),
        10 => (Effect::Nausea, 140),
        11 => (Effect::Blindness, 220),
        _ => return None,
    })
}

pub fn flower_effect(flower: Item) -> Option<u32> {
    Some(match flower.block()?.name() {
        "dandelion" | "golden dandelion" | "blue orchid" => 1,
        "poppy" | "torchflower" => 2,
        "allium" => 3,
        "azure bluet" => 4,
        "red tulip" | "orange tulip" | "white tulip" | "pink tulip" => 5,
        "oxeye daisy" => 6,
        "cornflower" => 7,
        "wither rose" => 8,
        "lily of the valley" => 9,
        "closed eyeblossom" => 10,
        "open eyeblossom" => 11,
        _ => return None,
    })
}
fn stew(code: u32) -> Stack {
    Stack { entity_data: code, ..Stack::new(Item::SUSPICIOUS_STEW, 1) }
}
pub fn consume_stew(stack: Stack, vitals: &mut Vitals) -> f32 {
    if stack.item == Item::SUSPICIOUS_STEW
        && let Some((effect, ticks)) = effect(stack.entity_data)
    {
        vitals.apply_effect(effect, 0, ticks)
    } else {
        0.0
    }
}

/// Inventory consequences are applied by the shared player action adapter.
pub enum Use {
    Exchange(Stack),
    Shear,
    Feed { consume: bool },
}
impl Entities {
    pub fn use_mooshroom(&mut self, index: usize, item: Item) -> Option<Use> {
        let m = self.mobs.get_mut(index)?;
        if !m.alive() || m.baby || m.age < 0 {
            return None;
        }
        let s = m.mooshroom.as_mut()?;
        if item == Item::BOWL {
            let result =
                if s.stew != 0 { stew(std::mem::take(&mut s.stew)) } else { Stack::new(Item::MUSHROOM_STEW, 1) };
            return Some(Use::Exchange(result));
        }
        if item == Item::BUCKET {
            return Some(Use::Exchange(Stack::new(Item::MILK_BUCKET, 1)));
        }
        if item == Item::SHEARS {
            let mushroom = if s.brown { Block::BROWN_MUSHROOM } else { Block::RED_MUSHROOM };
            let pos = m.pos;
            // SINGLE conversion: preserve health, age, name, persistence, leash
            // and motion; clear species state and goals before the cow ticks.
            m.kind = MobKind::Cow;
            m.mooshroom = None;
            m.goals = Default::default();
            for _ in 0..5 {
                self.scatter(Stack::new(mushroom, 1), pos + glam::DVec3::Y);
            }
            return Some(Use::Shear);
        }
        if s.brown
            && let Some(code) = flower_effect(item)
        {
            let consume = s.stew == 0;
            if consume {
                s.stew = code;
            }
            let kind = if consume { crate::particles::Kind::Effect } else { crate::particles::Kind::Smoke };
            self.particles.push(crate::particles::Request::Burst(crate::particles::Burst::new(
                kind,
                m.pos + glam::DVec3::Y,
                if consume { 4 } else { 2 },
            )));
            return Some(Use::Feed { consume });
        }
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::entity::{Ctx, MobWorld, PlayerId, Target};
    use crate::physics::{BlockSource, test_util::Grid};
    use crate::world::terrain::Dimension;
    use glam::{DVec3, IVec2, IVec3};

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
        fn exposed(&self, _: IVec3) -> bool {
            true
        }
        fn surface(&self, _: i32, _: i32) -> Option<i32> {
            Some(0)
        }
    }
    #[test]
    fn mooshrooms_court_grow_and_mutate_without_crossbreeding_with_cows() {
        let mut e = Entities::new(42);
        for x in [0.5, 2.5] {
            e.spawn(MobKind::Mooshroom, DVec3::new(x, 1.0, 0.5));
            assert_eq!(e.use_animal(e.mobs.len() - 1, Some(Stack::new(Item::WHEAT, 1)), PlayerId::HOST), Some(true));
        }
        let w = Flat(Grid::flat(0));
        let c = Ctx {
            players: vec![Target::new(PlayerId::HOST, DVec3::new(0.5, 1.0, 12.0), false)],
            daylight: 1.0,
            spawning: false,
            raining: false,
            dimension: Dimension::Overworld,
            spawn_light: crate::simulation::lighting::SpawnLight::DAY,
        };
        for _ in 0..80 {
            e.update(0.05, &w, &c);
        }
        assert_eq!(e.mobs.len(), 3);
        assert!(e.mobs[2].baby && e.mobs[2].age < -23000);
        assert!(e.mobs[..2].iter().all(|m| m.age > 5900));
        let old = e.mobs[2].age;
        assert_eq!(e.use_animal(2, Some(Stack::new(Item::WHEAT, 1)), PlayerId::HOST), Some(true));
        assert!(e.mobs[2].age > old);
        assert!(!MobKind::Mooshroom.can_mate_with(MobKind::Cow));
        let (a, b) = (&e.mobs[0], &e.mobs[1]);
        let mut rng = Rng::new(9);
        let mut mutations = 0;
        let mut child = Mob::new(MobKind::Mooshroom, DVec3::ZERO, 0.0);
        for _ in 0..8192 {
            inherit(&mut child, a, b, &mut rng);
            mutations += u32::from(child.mooshroom.as_ref().unwrap().brown);
        }
        assert!((1..32).contains(&mutations), "1/1024 mutation: {mutations}");
    }
    #[test]
    fn java_loot_counts_fire_cooking_and_baby_exclusion() {
        let fixture: Value =
            serde_json::from_str(include_str!("../../tests/fixtures/java/26.3/passive_mobs.json")).unwrap();
        assert_eq!(
            fixture["data"]["loot_table/entities/mooshroom"]["pools"][0]["entries"][0]["name"],
            "minecraft:leather"
        );
        let mut e = Entities::new(5);
        for _ in 0..64 {
            e.items.clear();
            e.orbs.clear();
            e.mobs.clear();
            e.spawn(MobKind::Mooshroom, DVec3::ZERO);
            e.mobs[0].damage(100.0, None, &mut Rng::new(1));
            e.drop_loot_with_fire(MobKind::Mooshroom, DVec3::ZERO, 0, true, true);
            let count =
                |item| e.items.iter().filter(|i| i.stack.item == item).map(|i| u32::from(i.stack.count)).sum::<u32>();
            assert!((1..=3).contains(&count(Item::STEAK)));
            assert!(count(Item::LEATHER) <= 2);
            assert!((1..=3).contains(&e.orbs.iter().map(|o| o.value).sum::<u32>()));
        }
        e.items.clear();
        e.orbs.clear();
        e.mobs.clear();
        e.spawn_baby(MobKind::Mooshroom, DVec3::ZERO);
        e.mobs[0].damage(100.0, None, &mut Rng::new(1));
        e.drop_loot(MobKind::Mooshroom, DVec3::ZERO);
        assert!(e.items.is_empty() && e.orbs.is_empty());
    }
    #[test]
    fn milking_stores_one_flower_and_shearing_preserves_the_animal() {
        let mut e = Entities::new(7);
        e.spawn(MobKind::Mooshroom, DVec3::ZERO);
        assert!(e.use_mooshroom(0, Item::from(Block::DANDELION)).is_none());
        e.mobs[0].lightning_swap();
        assert!(matches!(e.use_mooshroom(0, Item::from(Block::DANDELION)), Some(Use::Feed { consume: true })));
        assert!(matches!(e.use_mooshroom(0, Item::from(Block::POPPY)), Some(Use::Feed { consume: false })));
        assert!(matches!(e.use_mooshroom(0, Item::BUCKET), Some(Use::Exchange(s)) if s.item == Item::MILK_BUCKET));
        let Some(Use::Exchange(s)) = e.use_mooshroom(0, Item::BOWL) else { panic!() };
        assert_eq!((s.item, s.entity_data), (Item::SUSPICIOUS_STEW, 1));
        assert!(matches!(e.use_mooshroom(0, Item::BOWL), Some(Use::Exchange(s)) if s.item == Item::MUSHROOM_STEW));
        e.mobs[0].health = 7.0;
        e.mobs[0].name = crate::inventory::StackName::new("Mushie").unwrap();
        e.mobs[0].age = 5000;
        assert!(matches!(e.use_mooshroom(0, Item::SHEARS), Some(Use::Shear)));
        assert_eq!(e.mobs[0].kind, MobKind::Cow);
        assert_eq!((e.mobs[0].health, e.mobs[0].age, e.mobs[0].name.as_str()), (7.0, 5000, Some("Mushie")));
        assert_eq!(e.items.iter().map(|i| i.stack.count as u32).sum::<u32>(), 5);
        assert!(e.items.iter().all(|i| i.stack.item == Item::from(Block::BROWN_MUSHROOM)));
        assert!(e.use_mooshroom(0, Item::SHEARS).is_none());
        e.spawn_baby(MobKind::Mooshroom, DVec3::X);
        assert!(e.use_mooshroom(1, Item::BOWL).is_none());
        assert!(e.use_mooshroom(1, Item::SHEARS).is_none());
    }
    #[test]
    fn stew_crafting_consumption_and_payload_survive_inventory_serialization() {
        let mut grid = crate::crafting::Grid::new(2);
        for (cell, item) in grid.cells.iter_mut().zip([
            Item::BOWL,
            Item::from(Block::RED_MUSHROOM),
            Item::from(Block::BROWN_MUSHROOM),
            Item::from(Block::DANDELION),
        ]) {
            *cell = Some(Stack::new(item, 2));
        }
        let result = grid.result().unwrap();
        assert_eq!((result.item, result.entity_data), (Item::SUSPICIOUS_STEW, 1));
        for mushroom in [Block::RED_MUSHROOM, Block::BROWN_MUSHROOM] {
            grid.cells[1] = Some(Stack::new(mushroom, 2));
            grid.cells[2] = Some(Stack::new(mushroom, 2));
            assert_eq!(grid.result(), Some(result), "26.3 permits two mushrooms of the same colour");
        }
        grid.consume();
        assert!(grid.cells[..4].iter().all(|s| s.unwrap().count == 1));
        let mut inv = crate::inventory::Inventory::default();
        inv.slots[0] = Some(result);
        let inv = crate::inventory::Inventory::deserialize(&inv.serialize()).unwrap();
        assert_eq!(inv.get(0), Some(result));
        let mut p = crate::session::PlayerSession::new(PlayerId::HOST, DVec3::ZERO);
        p.inventory = inv;
        let mut e = Entities::new(8);
        for _ in 0..31 {
            assert!(p.tick_consumption(0, true, &mut e).unwrap().completed.is_none());
        }
        assert_eq!(p.inventory.get(0).unwrap().item, Item::SUSPICIOUS_STEW);
        assert!(p.tick_consumption(0, true, &mut e).unwrap().completed.is_some());
        assert_eq!(p.inventory.get(0).unwrap().item, Item::BOWL);
        assert_eq!(p.vitals.effects.get(Effect::Saturation).unwrap().ticks, 7);
        p.vitals.hunger.food = 5.0;
        p.vitals.hunger.saturation = 0.0;
        for _ in 0..7 {
            p.vitals.tick(0.05, &Default::default(), false);
        }
        assert_eq!((p.vitals.hunger.food, p.vitals.hunger.saturation), (12.0, 12.0));
        assert!(!p.vitals.effects.has(Effect::Saturation));
    }
    #[test]
    fn variants_flower_and_age_survive_chunk_unload_and_restart() {
        let mut w = crate::world::World::new_headless(
            std::sync::Arc::new(crate::world::terrain::Generator::new(3)),
            Default::default(),
            2,
        );
        w.load_test_column(IVec2::ZERO, 64);
        let mut e = Entities::new(5);
        e.spawn_baby(MobKind::Mooshroom, DVec3::new(5.0, 65.0, 5.0));
        e.mobs[0].age = -12000;
        e.mobs[0].mooshroom.as_mut().unwrap().brown = true;
        e.mobs[0].mooshroom.as_mut().unwrap().stew = 9;
        e.sync_chunk_entities(&mut w);
        w.unload_test_column(IVec2::ZERO);
        e.sync_chunk_entities(&mut w);
        assert_eq!(e.stored_entities(), 1);
        let mut loaded = Entities::new(99);
        loaded.load_nether_mobs(&e.nether_mobs_to_string());
        assert_eq!(loaded.mobs.len(), 1);
        let m = &loaded.mobs[0];
        assert_eq!((m.age, m.mooshroom.as_ref().unwrap().brown, m.mooshroom.as_ref().unwrap().stew), (-12000, true, 9));
        let mut remote = Mob::new(MobKind::Mooshroom, DVec3::ZERO, 0.0);
        remote.apply_look(&m.look());
        assert!(remote.mooshroom.unwrap().brown);
    }
    #[test]
    fn official_flower_effects_match_registered_inputs_and_durations() {
        let fixture: Value =
            serde_json::from_str(include_str!("../../tests/fixtures/java/26.3/passive_mobs.json")).unwrap();
        for row in fixture["flowers"].as_array().unwrap() {
            if let Some(item) = Item::from_name(row["flower"].as_str().unwrap()) {
                let (effect, ticks) = effect(flower_effect(item).unwrap()).unwrap();
                assert_eq!(effect.id(), row["effect"].as_str().unwrap());
                assert_eq!(ticks as u64, row["ticks"].as_u64().unwrap());
            }
        }
        assert!(flower_effect(Item::from(Block::CACTUS)).is_none());
    }
}

#[cfg(test)]
mod interaction_tests {
    use super::*;
    use crate::{
        entity::PlayerId,
        rules::GameMode,
        session::{PlayerSession, interact::UseContext},
        world::{World, terrain::Generator},
    };
    use glam::{DVec3, IVec2};
    #[test]
    fn shared_session_consumes_bowls_and_wears_shears() {
        let mut w = World::new_headless(std::sync::Arc::new(Generator::new(1)), Default::default(), 2);
        w.load_test_column(IVec2::ZERO, 64);
        let mut e = Entities::new(7);
        e.spawn(MobKind::Mooshroom, DVec3::new(3.5, 65.0, 0.5));
        let mut p = PlayerSession::new(PlayerId::HOST, DVec3::new(0.5, 65.0, 0.5));
        // yaw zero faces +X; aim through the cow's body.
        p.player.pitch = -0.16;
        let ctx = UseContext { sneaking: false, others: &[] };
        p.inventory.slots[0] = Some(Stack::new(Item::BOWL, 2));
        assert!(p.use_held(0, &mut w, &mut e, &ctx).unwrap().is_some());
        assert_eq!(p.inventory.get(0).unwrap().count, 1);
        assert!(p.inventory.slots.iter().flatten().any(|s| s.item == Item::MUSHROOM_STEW));
        p.mode = GameMode::Creative;
        p.use_held(0, &mut w, &mut e, &ctx).unwrap();
        assert_eq!(p.inventory.get(0).unwrap().item, Item::BOWL);
        assert_eq!(
            p.inventory.slots.iter().flatten().filter(|s| s.item == Item::MUSHROOM_STEW).count(),
            2,
            "creative bowls keep generating stew, even if one is already present"
        );
        p.mode = GameMode::Survival;
        p.inventory.slots[0] = Some(Stack::new(Item::SHEARS, 1));
        p.use_held(0, &mut w, &mut e, &ctx).unwrap();
        assert_eq!(p.inventory.get(0).unwrap().damage, 1);
        assert_eq!(e.mobs[0].kind, MobKind::Cow);
    }
}
