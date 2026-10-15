//! Existing friendly mobs' Java 26.3 SpawnPlacements predicates. Biome
//! membership is checked separately by the category pack walk.
use super::{MobKind, MobWorld, Rng, category::MobCategory, monster_spawns};
use crate::{
    physics,
    world::{
        block::Block,
        terrain::{Biome, Dimension, SEA_LEVEL},
    },
};
use glam::{DVec3, IVec3};

impl MobKind {
    /// Animal.removeWhenFarAway, independently of Mob.PersistenceRequired.
    pub(super) fn never_distance_despawns(self) -> bool {
        self.category() == MobCategory::Creature
    }
}

pub(super) fn spot_ok<W: MobWorld + ?Sized>(
    world: &W,
    kind: MobKind,
    cell: IVec3,
    rules: monster_spawns::SpawnRules,
    rng: &mut Rng,
) -> bool {
    if !world.loaded(cell) {
        return false;
    }
    let pos = cell.as_dvec3() + DVec3::new(0.5, 0.0, 0.5);
    let Some(at) = world.block(cell) else { return false };
    let Some(below) = world.block(cell - IVec3::Y) else { return false };
    let Some(above) = world.block(cell + IVec3::Y) else { return false };
    if kind == MobKind::Strider {
        if !at.is_lava() {
            return false;
        }
        let mut y = cell.y + 1;
        while y < rules.dimension.max_y() && world.block(IVec3::new(cell.x, y, cell.z)).is_some_and(|b| b.is_lava()) {
            y += 1;
        }
        return world.block(IVec3::new(cell.x, y, cell.z)) == Some(Block::AIR)
            && !physics::overlaps_solid(world, pos, kind.shape());
    }
    if kind.is_aquatic() && kind != MobKind::Turtle {
        // IN_WATER permits a solid clay floor (axolotls), and checks the
        // conductor above; it does not require water below for every species.
        if !at.holds_water() || monster_spawns::is_redstone_conductor(above) {
            return false;
        }
        let rules_ok = match kind {
            MobKind::Axolotl => below == Block::CLAY,
            MobKind::GlowSquid => {
                cell.y <= SEA_LEVEL - 33
                    && at.is_water()
                    && world.sky_light_level(cell) == Some(0)
                    && world.block_light(cell) == 0
            }
            MobKind::Guardian => below.holds_water() && (rng.next_int(20) == 0 || !world.exposed(cell)),
            MobKind::Drowned => return monster_spawns::monster_spot_ok(world, kind, cell, rules, rng),
            _ => {
                below.holds_water()
                    && above.is_water()
                    && ((kind == MobKind::TropicalFish && world.cave_biome(cell) == Biome::LushCaves)
                        || (SEA_LEVEL - 13..=SEA_LEVEL).contains(&cell.y))
            }
        };
        return rules_ok && !physics::overlaps_solid(world, pos, kind.shape());
    }
    if kind.category() == MobCategory::Monster {
        if rules.dimension != Dimension::Overworld {
            let light_ok = match kind {
                MobKind::Blaze | MobKind::MagmaCube => true,
                MobKind::Piglin | MobKind::Hoglin | MobKind::ZombifiedPiglin => below.name() != "nether wart block",
                MobKind::Ghast => rng.next_int(20) == 0,
                _ if rules.dimension == Dimension::Nether => world.block_light(cell) <= 7,
                // End's monster_spawn_light_level is a constant 15.
                _ => {
                    world.block_light(cell) == 0
                        && world.sky_light_level(cell).is_some_and(|sky| u32::from(sky) <= rng.next_int(32))
                }
            };
            return light_ok && ground_ok(world, kind, cell);
        }
        if kind == MobKind::Pillager {
            return world.block_light(cell) <= 8 && ground_ok(world, kind, cell);
        }
        return monster_spawns::monster_spot_ok(world, kind, cell, rules, rng);
    }
    let bright = world.sky_light_level(cell).is_some_and(|sky| sky.max(world.block_light(cell)) > 8);
    let name = below.name();
    let ground = match kind {
        MobKind::Wolf | MobKind::Fox => {
            below == Block::GRASS || matches!(name, "snow" | "snow block" | "podzol" | "coarse dirt")
        }
        MobKind::Rabbit => below == Block::GRASS || below == Block::SAND || matches!(name, "snow" | "snow block"),
        MobKind::Goat => {
            below == Block::GRASS
                || below == Block::STONE
                || below == Block::GRAVEL
                || matches!(name, "snow" | "snow block" | "packed ice")
        }
        MobKind::Parrot => below == Block::GRASS || below == Block::AIR || below.is_leaves() || below.is_log(),
        MobKind::Turtle => cell.y < SEA_LEVEL + 4 && matches!(name, "sand" | "red sand"),
        _ => below == Block::GRASS,
    };
    ground
        && bright
        && (kind == MobKind::Fox || ground_placement(below, at, above, kind))
        && !physics::overlaps_solid(world, pos, kind.shape())
        && !physics::touches_block(world, pos, kind.shape(), |b| b.is_fluid() || b.is_waterlogged())
}

fn ground_placement(below: Block, at: Block, above: Block, kind: MobKind) -> bool {
    monster_spawns::valid_spawn_floor(below, kind)
        && monster_spawns::valid_empty_cell(at, kind)
        && monster_spawns::valid_empty_cell(above, kind)
}

fn ground_ok<W: MobWorld + ?Sized>(world: &W, kind: MobKind, cell: IVec3) -> bool {
    let Some(below) = world.block(cell - IVec3::Y) else { return false };
    let Some(at) = world.block(cell) else { return false };
    let Some(above) = world.block(cell + IVec3::Y) else { return false };
    let pos = cell.as_dvec3() + DVec3::new(0.5, 0.0, 0.5);
    ground_placement(below, at, above, kind)
        && !physics::overlaps_solid(world, pos, kind.shape())
        && !physics::touches_block(world, pos, kind.shape(), |b| b.is_fluid() || b.is_waterlogged())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{physics::BlockSource, simulation::lighting::SpawnLight};
    struct Habitat {
        y: i32,
        floor: Block,
        at: Block,
        above: Block,
        light: u8,
        biome: Biome,
    }
    impl BlockSource for Habitat {
        fn block(&self, p: IVec3) -> Option<Block> {
            Some(if p.y < self.y {
                self.floor
            } else if p.y == self.y {
                self.at
            } else {
                self.above
            })
        }
    }
    impl MobWorld for Habitat {
        fn loaded(&self, _: IVec3) -> bool {
            true
        }
        fn surface(&self, _: i32, _: i32) -> Option<i32> {
            Some(self.y - 1)
        }
        fn exposed(&self, _: IVec3) -> bool {
            self.light == 15
        }
        fn sky_light_level(&self, _: IVec3) -> Option<u8> {
            Some(self.light)
        }
        fn biome(&self, _: i32, _: i32) -> Biome {
            self.biome
        }
    }
    fn ok(w: &Habitat, k: MobKind) -> bool {
        spot_ok(
            w,
            k,
            IVec3::new(0, w.y, 0),
            monster_spawns::SpawnRules { dimension: Dimension::Overworld, light: SpawnLight::NIGHT, moon: 1.0 },
            &mut Rng::new(3),
        )
    }
    #[test]
    fn animals_need_tagged_ground_and_raw_brightness_above_eight() {
        let mut w =
            Habitat { y: 64, floor: Block::GRASS, at: Block::AIR, above: Block::AIR, light: 8, biome: Biome::Plains };
        assert!(!ok(&w, MobKind::Cow));
        w.light = 9;
        assert!(ok(&w, MobKind::Cow)); // Raw sky is used even at night.
        w.floor = Block::STONE;
        assert!(!ok(&w, MobKind::Cow));
        assert!(ok(&w, MobKind::Goat));
        w.floor = Block::LEAVES;
        assert!(ok(&w, MobKind::Parrot), "parrots spawn on canopies");
        assert!(!ok(&w, MobKind::Cow));
        w.floor = Block::SAND;
        assert!(ok(&w, MobKind::Rabbit));
        assert!(!ok(&w, MobKind::Wolf));
    }
    #[test]
    fn water_placements_and_species_height_light_and_clay_rules() {
        let mut w = Habitat {
            y: 50,
            floor: Block::WATER,
            at: Block::WATER,
            above: Block::WATER,
            light: 15,
            biome: Biome::WarmOcean,
        };
        for k in [MobKind::Squid, MobKind::Dolphin, MobKind::Cod, MobKind::Salmon, MobKind::TropicalFish] {
            assert!(ok(&w, k), "{k:?}");
        }
        w.at = Block::AIR;
        assert!(!ok(&w, MobKind::Squid));
        w.at = Block::WATER;
        w.y = 30;
        assert!(!ok(&w, MobKind::Squid));
        assert!(!ok(&w, MobKind::GlowSquid));
        w.light = 0;
        assert!(ok(&w, MobKind::GlowSquid));
        w.y = 31;
        assert!(!ok(&w, MobKind::GlowSquid));
        assert!(!ok(&w, MobKind::TropicalFish));
        w.biome = Biome::LushCaves;
        assert!(ok(&w, MobKind::TropicalFish));
        assert!(!ok(&w, MobKind::Axolotl));
        w.floor = Block::CLAY;
        assert!(ok(&w, MobKind::Axolotl));
        w.above = Block::STONE;
        assert!(!ok(&w, MobKind::Axolotl));
    }
}
