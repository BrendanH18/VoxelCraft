//! Java 26.3 skeleton variants. Sources: AbstractSkeleton, Stray, Bogged,
//! Skeleton and ConversionTracker in the pinned server jar; entity and
//! shearing loot tables in the client jar. Original geometry is in model/hostiles.
use super::{Entities, Mob, MobKind, MobWorld};
use crate::{inventory::Stack, item::Item, physics, potion::Potion, simulation::effects::Effect};
use serde_json::{Value, json};

/// ConversionTracker uses a separate affliction clock and countdown. Leaving
/// powder snow cancels both, including an already-started conversion.
#[derive(Clone, Debug, PartialEq)]
pub struct Freezing {
    pub exposure: i32,
    pub conversion: Option<i32>,
    remainder: f32,
}
impl Default for Freezing {
    fn default() -> Self {
        Self { exposure: 0, conversion: None, remainder: 0.0 }
    }
}
impl Freezing {
    fn tick(&mut self, dt: f32, snow: bool) -> bool {
        self.remainder += dt * 20.0;
        let ticks = (self.remainder + 1e-5).floor() as u32;
        self.remainder = (self.remainder - ticks as f32).max(0.0);
        for _ in 0..ticks {
            if !snow {
                self.exposure = -1;
                self.conversion = None;
            } else if let Some(left) = &mut self.conversion {
                *left -= 1;
                if *left < 0 {
                    return true;
                }
            } else {
                self.exposure += 1;
                if self.exposure >= 140 {
                    self.conversion = Some(300);
                }
            }
        }
        false
    }
    pub(super) fn save(&self) -> Value {
        json!({"exposure": self.exposure, "conversion": self.conversion, "fraction": self.remainder})
    }
    pub(super) fn load(v: &Value) -> Self {
        Self {
            exposure: v["exposure"].as_i64().unwrap_or(0).clamp(-1, 140) as i32,
            conversion: v["conversion"].as_i64().filter(|x| (0..=300).contains(x)).map(|x| x as i32),
            remainder: v["fraction"].as_f64().filter(|x| x.is_finite() && (0.0..1.0).contains(x)).unwrap_or(0.0) as f32,
        }
    }
}
impl MobKind {
    pub fn is_bow_skeleton(self) -> bool {
        matches!(self, Self::Skeleton | Self::Stray | Self::Bogged)
    }
    pub(super) fn arrow_potion(self) -> Option<Potion> {
        match self {
            Self::Stray => Potion::from_id("slowness"),
            Self::Bogged => Potion::from_id("poison"),
            _ => None,
        }
    }
    /// Mob-fired arrows use custom effects on an ordinary arrow (duration scale
    /// 1); loot is a tipped arrow with the registered potion (scale 1/8).
    pub(super) fn arrow_effect(self) -> Option<(Effect, u8, u32)> {
        match self {
            Self::Stray => Some((Effect::Slowness, 0, 600)),
            Self::Bogged => Some((Effect::Poison, 0, 100)),
            _ => None,
        }
    }
    pub(super) fn bow_interval(self, hard: bool) -> f32 {
        let ticks = match (self, hard) {
            (Self::Bogged, true) => 50,
            (Self::Bogged, false) => 70,
            (_, true) => 20,
            (_, false) => 40,
        };
        // RangedBowAttackGoal's interval elapses before its 20-tick draw.
        (ticks + 20) as f32 / 20.0
    }
}
impl Mob {
    pub(super) fn freeze_in_powder_snow<W: MobWorld + ?Sized>(&mut self, dt: f32, world: &W) {
        if self.kind != MobKind::Skeleton || !self.alive() {
            return;
        }
        let snow =
            physics::touches_block(world, self.pos, self.shape(), |b| b == crate::world::scaffolding::POWDER_SNOW);
        if self.freezing.get_or_insert_with(Default::default).tick(dt, snow) {
            self.kind = MobKind::Stray;
            self.freezing = None;
            // Same dimensions and max health. Retain identity, equipment, health,
            // effects and persistence, as ConversionParams.SINGLE does.
        }
    }
}
impl Entities {
    pub fn shear_bogged(&mut self, index: usize, item: Item) -> bool {
        let Some(m) = self.mobs.get_mut(index) else {
            return false;
        };
        if m.kind != MobKind::Bogged || !m.alive() || m.sheared || item != Item::SHEARS {
            return false;
        }
        m.sheared = true;
        let pos = m.pos + glam::DVec3::Y * m.shape().height;
        // Two equally weighted independent rolls, not one of each mushroom.
        for _ in 0..2 {
            let mushroom = if self.rng.next_int(2) == 0 {
                crate::world::block::Block::BROWN_MUSHROOM
            } else {
                crate::world::block::Block::RED_MUSHROOM
            };
            self.scatter(Stack::new(mushroom, 1), pos);
        }
        true
    }
}

impl Entities {
    pub(super) fn skeleton_equipment_drops(&mut self, kind: MobKind, pos: glam::DVec3, looting: u8, player_kill: bool) {
        if !kind.is_bow_skeleton() || !player_kill {
            return;
        }
        let Some(m) = self.mobs.iter_mut().find(|m| m.kind == kind && !m.alive() && m.pos.distance_squared(pos) < 0.01)
        else {
            return;
        };
        let bow = m.skeleton_bow.take();
        let mut equipment: Vec<Stack> = bow.into_iter().map(|s| *s).collect();
        for (i, piece) in crate::item::ArmorPiece::ALL.into_iter().enumerate() {
            use super::armor::ArmorKind as A;
            use crate::item::ArmorMaterial as M;
            let material = match m.armor[i] {
                Some(A::Leather) => Some(M::Leather),
                Some(A::Gold) => Some(M::Gold),
                Some(A::Iron) => Some(M::Iron),
                Some(A::Diamond) => Some(M::Diamond),
                Some(A::Copper) => Some(M::Copper),
                _ => None,
            };
            if let Some(material) = material {
                equipment.push(Stack::new(Item::armor(piece, material), 1));
            }
        }
        for mut stack in equipment {
            if self.rng.chance(0.085 + 0.01 * f32::from(looting)) {
                if let Some(max) = stack.item.durability() {
                    let inner = self.rng.next_int(u32::from(max.saturating_sub(3).max(1)));
                    stack.damage = max - self.rng.next_int(inner + 1) as u16;
                }
                self.scatter(stack, pos);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        entity::{Ctx, PlayerId, Target},
        physics::test_util::Grid,
        world::{block::Block, terrain::Dimension},
    };
    use glam::{DVec3, IVec3};
    #[test]
    fn reference_conversion_threshold_and_interruption() {
        let mut f = Freezing::default();
        for _ in 0..139 {
            assert!(!f.tick(0.05, true));
        }
        assert_eq!(f.conversion, None);
        assert!(!f.tick(0.05, true));
        assert_eq!(f.conversion, Some(300));
        for _ in 0..300 {
            assert!(!f.tick(0.05, true));
        }
        assert!(f.tick(0.05, true));
        assert!(!f.tick(0.05, false));
        assert_eq!((f.exposure, f.conversion), (-1, None));
        // A first snow tick after being outside starts from -1, like Java.
        for _ in 0..140 {
            assert!(!f.tick(0.05, true));
        }
        assert_eq!(f.conversion, None);
        assert!(!f.tick(0.05, true));
        assert_eq!(f.conversion, Some(300));
        let saved = f.save();
        assert_eq!(Freezing::load(&saved), f);
    }
    #[test]
    fn conversion_retains_identity_equipment_and_health() {
        let mut world = Grid::flat(10);
        world.set(IVec3::new(0, 10, 0), crate::world::scaffolding::POWDER_SNOW);
        let mut mob = Mob::new(MobKind::Skeleton, DVec3::new(0.5, 10., 0.5), 0.);
        mob.uid = 71;
        mob.health = 7.;
        mob.armor[0] = Some(super::super::armor::ArmorKind::Iron);
        for _ in 0..441 {
            mob.freeze_in_powder_snow(0.05, &world);
        }
        assert_eq!(mob.kind, MobKind::Stray);
        assert_eq!((mob.uid, mob.health), (71, 7.));
        assert_eq!(mob.armor[0], Some(super::super::armor::ArmorKind::Iron));
    }
    #[test]
    fn shearing_is_once_only_and_saved_with_natural_mobs() {
        let mut e = Entities::new(8);
        e.spawn(MobKind::Bogged, DVec3::new(0.5, 10., 0.5));
        e.mobs[0].persistent = false;
        assert!(!e.shear_bogged(0, Item::STICK));
        assert!(e.shear_bogged(0, Item::SHEARS));
        assert!(!e.shear_bogged(0, Item::SHEARS));
        assert_eq!(e.items.iter().map(|s| u32::from(s.stack.count)).sum::<u32>(), 2);
        assert!(
            e.items.iter().all(|s| matches!(s.stack.item.block(), Some(Block::BROWN_MUSHROOM | Block::RED_MUSHROOM)))
        );
        e.spawn(MobKind::Stray, DVec3::new(3.5, 10., 0.5));
        e.mobs[1].persistent = false;
        e.spawn(MobKind::Skeleton, DVec3::new(6.5, 10., 0.5));
        let mut f = Freezing::default();
        f.tick(7.0, true);
        e.mobs[2].freezing = Some(f.clone());
        let text = e.nether_mobs_to_string();
        let mut loaded = Entities::new(9);
        loaded.load_nether_mobs(&text);
        assert_eq!(loaded.mobs.len(), 3);
        assert_eq!(loaded.mobs[0].kind, MobKind::Bogged);
        assert!(loaded.mobs[0].sheared);
        assert!(!loaded.mobs[0].persistent);
        assert_eq!(loaded.mobs[1].kind, MobKind::Stray);
        assert_eq!(loaded.mobs[2].freezing, Some(f));
        let look = e.mobs[0].look();
        let mut replica = Mob::new(MobKind::Bogged, DVec3::ZERO, 0.);
        replica.apply_look(&look);
        assert!(replica.sheared);
    }
    #[test]
    fn variants_shoot_at_shared_player_targets_and_burn_in_sun() {
        for (kind, effect) in [(MobKind::Stray, Effect::Slowness), (MobKind::Bogged, Effect::Poison)] {
            let mut e = Entities::new(2);
            e.spawn(kind, DVec3::new(0.5, 10., 0.5));
            let ctx = Ctx {
                players: vec![Target::new(PlayerId(7), DVec3::new(9.5, 10., 0.5), true)],
                spawning: false,
                daylight: 0.,
                raining: false,
                dimension: Dimension::Overworld,
                spawn_light: crate::simulation::lighting::SpawnLight::NIGHT,
            };
            let world = Grid::flat(10);
            let mut effects = Vec::new();
            for _ in 0..180 {
                effects.extend(e.update_difficulty(
                    0.05,
                    &world,
                    &ctx,
                    crate::simulation::difficulty::Difficulty::Normal,
                ));
            }
            assert!(effects.iter().any(|e|matches!(e, super::super::EntityEvent::PlayerEffect { player:PlayerId(7), effect:ef, .. } if *ef == effect)),"{kind:?}");
            assert!(e.arrows.iter().all(|a| !a.pickup));
            assert!(kind.burns_in_sun());
        }
        assert_eq!(MobKind::Bogged.max_health(), 16.);
        assert_eq!(MobKind::Stray.max_health(), 20.);
    }
    #[test]
    fn tipped_loot_requires_player_credit_and_never_exceeds_one() {
        for kind in [MobKind::Stray, MobKind::Bogged] {
            for player_kill in [false, true] {
                let mut tipped = 0;
                for seed in 0..200 {
                    let mut e = Entities::new(seed);
                    e.drop_loot_with_fire(kind, DVec3::new(0., 10., 0.), 3, false, player_kill);
                    for s in &e.items {
                        if s.stack.item == Item::TIPPED_ARROW {
                            assert_eq!(s.stack.count, 1);
                            assert_eq!(s.stack.arrow_potion(), kind.arrow_potion());
                            tipped += 1;
                        }
                    }
                }
                assert_eq!(tipped > 0, player_kill);
            }
        }
    }
    #[test]
    fn bow_enchantments_survive_conversion_and_reload() {
        let mut e = Entities::new(7);
        e.spawn(MobKind::Stray, DVec3::ZERO);
        e.mobs[0].skeleton_bow.as_mut().unwrap().enchants =
            crate::enchant::Enchants::NONE.with(crate::enchant::Enchantment::Power, 3);
        let mut copy = Entities::new(8);
        copy.load_nether_mobs(&e.nether_mobs_to_string());
        assert_eq!(copy.mobs[0].skeleton_bow.as_ref().unwrap().enchants.level(crate::enchant::Enchantment::Power), 3);
    }
}
