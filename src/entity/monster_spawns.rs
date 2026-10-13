//! Java 26.3 natural monster spawning in the Overworld: `NaturalSpawner`'s
//! per-chunk position sampling and pack walk, the biome monster lists, the
//! `ON_GROUND` placement checks and `Monster.isDarkEnoughToSpawn` on
//! authoritative sky and block light. This is what lets monsters spawn in
//! dark caves and keeps them out of torch-lit ones.
//!
//! Sources: the 26.3 server's `NaturalSpawner`, `SpawnPlacementTypes`,
//! `SpawnPlacements`, `Monster`, `Mob`, `Blocks` and the biomes'
//! `gameplay/natural_mob_spawns` attribute (see
//! `tests/fixtures/java/26.3/lighting_spawning.json`).
//!
//! Remaining VoxelCraft limits, recorded in the lighting/spawning report:
//! positions stay within the existing 24..64 block spawn ring (Java samples
//! every chunk within 128 blocks), per-kind caps replace Java's local
//! monster cap of 70, structure spawn overrides (swamp huts, monuments,
//! outposts) are not applied, and list entries VoxelCraft spawns elsewhere
//! (slimes, drowned) or lacks (strays, bogged, parched, zombie horses,
//! ocelots) end their pack.

use super::*;
use crate::world::terrain::Biome;

/// Monsters named by Java's Overworld biome lists.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum JavaMonster {
    Spider,
    Zombie,
    ZombieVillager,
    ZombieHorse,
    Skeleton,
    Creeper,
    Slime,
    Enderman,
    Witch,
    Drowned,
    Husk,
    Parched,
    Stray,
    Bogged,
    Ocelot,
}

impl JavaMonster {
    /// Java's entity id (without `minecraft:`).
    #[cfg(test)]
    pub(super) fn id(self) -> &'static str {
        match self {
            JavaMonster::Spider => "spider",
            JavaMonster::Zombie => "zombie",
            JavaMonster::ZombieVillager => "zombie_villager",
            JavaMonster::ZombieHorse => "zombie_horse",
            JavaMonster::Skeleton => "skeleton",
            JavaMonster::Creeper => "creeper",
            JavaMonster::Slime => "slime",
            JavaMonster::Enderman => "enderman",
            JavaMonster::Witch => "witch",
            JavaMonster::Drowned => "drowned",
            JavaMonster::Husk => "husk",
            JavaMonster::Parched => "parched",
            JavaMonster::Stray => "stray",
            JavaMonster::Bogged => "bogged",
            JavaMonster::Ocelot => "ocelot",
        }
    }

    /// The kind this cycle spawns for the entry. Slimes and drowned keep
    /// their own spawn paths; the rest are not in VoxelCraft yet.
    pub(super) fn kind(self) -> Option<MobKind> {
        Some(match self {
            JavaMonster::Spider => MobKind::Spider,
            JavaMonster::Zombie => MobKind::Zombie,
            JavaMonster::ZombieVillager => MobKind::ZombieVillager,
            JavaMonster::Skeleton => MobKind::Skeleton,
            JavaMonster::Creeper => MobKind::Creeper,
            JavaMonster::Enderman => MobKind::Enderman,
            JavaMonster::Witch => MobKind::Witch,
            JavaMonster::Husk => MobKind::Husk,
            _ => return None,
        })
    }
}

/// One weighted entry of a biome's monster list and its pack size range.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct MonsterSpawn {
    pub mob: JavaMonster,
    pub weight: u32,
    pub min: u32,
    pub max: u32,
}

const fn e(mob: JavaMonster, weight: u32, min: u32, max: u32) -> MonsterSpawn {
    MonsterSpawn { mob, weight, min, max }
}

use JavaMonster as J;

const DEFAULT: [MonsterSpawn; 8] = [
    e(J::Spider, 100, 4, 4),
    e(J::Zombie, 95, 4, 4),
    e(J::ZombieVillager, 5, 1, 1),
    e(J::Skeleton, 100, 4, 4),
    e(J::Creeper, 100, 4, 4),
    e(J::Slime, 100, 4, 4),
    e(J::Enderman, 10, 1, 4),
    e(J::Witch, 5, 1, 1),
];

/// `DEFAULT` with one more entry at the end.
const fn plus(extra: MonsterSpawn) -> [MonsterSpawn; 9] {
    let d = DEFAULT;
    [d[0], d[1], d[2], d[3], d[4], d[5], d[6], d[7], extra]
}

const PLAINS: [MonsterSpawn; 9] = [
    e(J::Spider, 100, 4, 4),
    e(J::Zombie, 90, 4, 4),
    e(J::ZombieVillager, 5, 1, 1),
    e(J::ZombieHorse, 5, 1, 1),
    e(J::Skeleton, 100, 4, 4),
    e(J::Creeper, 100, 4, 4),
    e(J::Slime, 100, 4, 4),
    e(J::Enderman, 10, 1, 4),
    e(J::Witch, 5, 1, 1),
];
const SNOWY_PLAINS: [MonsterSpawn; 10] = [
    e(J::Spider, 100, 4, 4),
    e(J::Zombie, 90, 4, 4),
    e(J::ZombieVillager, 5, 1, 1),
    e(J::ZombieHorse, 5, 1, 1),
    e(J::Skeleton, 20, 4, 4),
    e(J::Creeper, 100, 4, 4),
    e(J::Slime, 100, 4, 4),
    e(J::Enderman, 10, 1, 4),
    e(J::Witch, 5, 1, 1),
    e(J::Stray, 80, 4, 4),
];
const ICE_SPIKES: [MonsterSpawn; 9] = [
    e(J::Spider, 100, 4, 4),
    e(J::Zombie, 95, 4, 4),
    e(J::ZombieVillager, 5, 1, 1),
    e(J::Skeleton, 20, 4, 4),
    e(J::Creeper, 100, 4, 4),
    e(J::Slime, 100, 4, 4),
    e(J::Enderman, 10, 1, 4),
    e(J::Witch, 5, 1, 1),
    e(J::Stray, 80, 4, 4),
];
const DESERT: [MonsterSpawn; 10] = [
    e(J::Spider, 100, 4, 4),
    e(J::Zombie, 19, 4, 4),
    e(J::ZombieVillager, 1, 1, 1),
    e(J::Skeleton, 50, 4, 4),
    e(J::Creeper, 100, 4, 4),
    e(J::Slime, 100, 4, 4),
    e(J::Enderman, 10, 1, 4),
    e(J::Witch, 5, 1, 1),
    e(J::Husk, 80, 4, 4),
    e(J::Parched, 50, 4, 4),
];
const SWAMP: [MonsterSpawn; 10] = [
    e(J::Spider, 100, 4, 4),
    e(J::Zombie, 95, 4, 4),
    e(J::ZombieVillager, 5, 1, 1),
    e(J::Skeleton, 70, 4, 4),
    e(J::Creeper, 100, 4, 4),
    e(J::Slime, 100, 4, 4),
    e(J::Enderman, 10, 1, 4),
    e(J::Witch, 5, 1, 1),
    e(J::Slime, 1, 1, 1),
    e(J::Bogged, 30, 4, 4),
];
const PINE_TAIGA: [MonsterSpawn; 8] = [
    e(J::Spider, 100, 4, 4),
    e(J::Zombie, 100, 4, 4),
    e(J::ZombieVillager, 25, 1, 1),
    e(J::Skeleton, 100, 4, 4),
    e(J::Creeper, 100, 4, 4),
    e(J::Slime, 100, 4, 4),
    e(J::Enderman, 10, 1, 4),
    e(J::Witch, 5, 1, 1),
];
const WARM_OCEAN: [MonsterSpawn; 9] = {
    let d = DEFAULT;
    [e(J::Drowned, 5, 1, 1), d[0], d[1], d[2], d[3], d[4], d[5], d[6], d[7]]
};
const OCEAN: [MonsterSpawn; 9] = plus(e(J::Drowned, 5, 1, 1));
const RIVER: [MonsterSpawn; 9] = plus(e(J::Drowned, 100, 1, 1));
const FROZEN_RIVER: [MonsterSpawn; 9] = plus(e(J::Drowned, 1, 1, 1));
const DRIPSTONE: [MonsterSpawn; 9] = plus(e(J::Drowned, 95, 4, 4));
const JUNGLE: [MonsterSpawn; 9] = plus(e(J::Ocelot, 2, 1, 3));
const BAMBOO_JUNGLE: [MonsterSpawn; 9] = plus(e(J::Ocelot, 2, 1, 1));

/// A biome's 26.3 `monster` spawn list, in Java's order.
pub(super) fn monster_list(biome: Biome) -> &'static [MonsterSpawn] {
    use Biome::*;
    match biome {
        Plains | SunflowerPlains | Savanna | SavannaPlateau | WindsweptSavanna => &PLAINS,
        SnowyPlains => &SNOWY_PLAINS,
        IceSpikes => &ICE_SPIKES,
        Desert => &DESERT,
        Swamp | MangroveSwamp => &SWAMP,
        OldGrowthPineTaiga => &PINE_TAIGA,
        WarmOcean => &WARM_OCEAN,
        DeepFrozenOcean | DeepColdOcean | DeepOcean | DeepLukewarmOcean | FrozenOcean | ColdOcean | Ocean
        | LukewarmOcean => &OCEAN,
        River => &RIVER,
        FrozenRiver => &FROZEN_RIVER,
        DripstoneCaves => &DRIPSTONE,
        Jungle => &JUNGLE,
        BambooJungle => &BAMBOO_JUNGLE,
        MushroomFields => &[],
        _ => &DEFAULT,
    }
}

/// Java's `WeightedList.getRandom`: an entry with probability proportional to its weight.
fn pick(list: &'static [MonsterSpawn], rng: &mut Rng) -> Option<&'static MonsterSpawn> {
    let total: u32 = list.iter().map(|s| s.weight).sum();
    if total == 0 {
        return None;
    }
    let mut r = rng.next_int(total);
    list.iter().find(|s| {
        let hit = r < s.weight;
        r = r.wrapping_sub(s.weight);
        hit
    })
}

/// Whether a block fills its whole cell for collision (Java's `isCollisionShapeFullBlock`).
fn full_collision(b: Block) -> bool {
    use crate::world::shape;
    b.is_solid()
        && !b.is_slab()
        && (b.shaped().is_none()
            || shape::collision(b, |_| Block::AIR, Block::AIR).as_slice().contains(&shape::Box16::FULL))
}

/// Java's default `isValidSpawn`: a sturdy top face (a collision box
/// covering the whole top of the cell) and emission below 14.
fn sturdy_top(b: Block) -> bool {
    use crate::world::shape;
    if !b.is_solid() || b.is_slab() {
        return false;
    }
    // Java's 15/16-high farmland and paths have no full top face.
    if matches!(b.name(), "farmland" | "dirt path") {
        return false;
    }
    b.shaped().is_none()
        || shape::collision(b, |_| Block::AIR, Block::AIR)
            .as_slice()
            .iter()
            .any(|x| x.max[1] == 16 && x.min[0] == 0 && x.min[2] == 0 && x.max[0] == 16 && x.max[2] == 16)
}

/// Java's `BlockState.isValidSpawn` for the block a monster would stand on,
/// including `Blocks`' explicit overrides.
pub(super) fn valid_spawn_floor(b: Block, kind: MobKind) -> bool {
    use crate::world::{overworld_blocks, pumpkin_blocks, redstone_blocks};
    // Blocks::always: soul sand, mud, carved pumpkins, jack o'lanterns, redstone lamps.
    if b == Block::SOUL_SAND
        || b == overworld_blocks::MUD
        || pumpkin_blocks::is_head(b)
        || matches!(redstone_blocks::component(b), Some(redstone_blocks::Component::Lamp(_)))
    {
        return true;
    }
    match b {
        // Ice is for polar bears; magma for the fire immune.
        Block::ICE => return false,
        Block::MAGMA => return kind.fire_immune(),
        _ => {}
    }
    // Blocks::never, and leaves (ocelots and parrots only).
    let name = b.name();
    if b == Block::BEDROCK
        || b == Block::GLASS
        || b == redstone_blocks::MOVING
        || b.is_leaves()
        || name.ends_with("stained glass")
        || matches!(name, "tinted glass" | "barrier" | "scaffolding" | "chorus flower" | "frosted ice")
        || name.contains("trapdoor")
    {
        return false;
    }
    sturdy_top(b) && b.emission() < 14
}

/// Java's `isValidEmptySpawnBlock` for the cells a ground mob occupies:
/// not a full collision block, not a redstone power source, no fluid,
/// no rails, and nothing that would hurt it.
pub(super) fn valid_empty_cell(b: Block, kind: MobKind) -> bool {
    use crate::world::redstone_blocks::{Component, component};
    let signal = matches!(
        component(b),
        Some(
            Component::Wire(_)
                | Component::Lever { .. }
                | Component::Button { .. }
                | Component::Torch { .. }
                | Component::Repeater { .. }
                | Component::Comparator { .. }
                | Component::Source
                | Component::Plate { .. }
                | Component::Daylight { .. }
                | Component::Target(_)
                | Component::Observer { .. }
        )
    );
    // Java's `isBlockDangerous`: burning blocks (unless fire immune), wither
    // roses, sweet berry bushes, cactus and powder snow.
    let burning = b.is_fire() || b.is_lava() || b == Block::MAGMA || b.name() == "campfire";
    let dangerous = (burning && !kind.fire_immune())
        || matches!(b.name(), "wither rose" | "sweet berry bush" | "cactus" | "powder snow");
    !full_collision(b) && !signal && !b.is_fluid() && !b.is_waterlogged() && !b.is_rail() && !dangerous
}

/// Java's `Monster.isDarkEnoughToSpawn` in the Overworld (`monster_spawn_block_light_limit`
/// 0, `monster_spawn_light_level` uniform 0..=7): skylight must not beat a
/// roll out of 32, any block light forbids the spawn, and the darkened
/// brightness must not beat a roll of 0..=7. Unknown skylight never spawns.
pub(super) fn dark_enough<W: MobWorld + ?Sized>(world: &W, cell: IVec3, sky_darken: u8, rng: &mut Rng) -> bool {
    let Some(sky) = world.sky_light_level(cell) else { return false };
    if u32::from(sky) > rng.next_int(32) {
        return false;
    }
    let block = world.block_light(cell);
    if block > 0 {
        return false;
    }
    let raw = sky.saturating_sub(sky_darken).max(block);
    u32::from(raw) <= rng.next_int(8)
}

/// Every check Java makes on a natural monster spawn position (feet cell
/// `cell`) after distance: `ON_GROUND` placement, the type's spawn rules and
/// a clear, dry spawn box.
pub(super) fn monster_spot_ok<W: MobWorld + ?Sized>(
    world: &W,
    kind: MobKind,
    cell: IVec3,
    sky_darken: u8,
    rng: &mut Rng,
) -> bool {
    let (Some(below), Some(at), Some(above)) =
        (world.block(cell - IVec3::Y), world.block(cell), world.block(cell + IVec3::Y))
    else {
        return false;
    };
    if !valid_spawn_floor(below, kind) || !valid_empty_cell(at, kind) || !valid_empty_cell(above, kind) {
        return false;
    }
    if !dark_enough(world, cell, sky_darken, rng) {
        return false;
    }
    // Husks use `checkSurfaceMonstersSpawnRules`: they also need to see the sky.
    if kind == MobKind::Husk && world.sky_light_level(cell) != Some(15) {
        return false;
    }
    let pos = cell.as_dvec3() + DVec3::new(0.5, 0.0, 0.5);
    !physics::overlaps_solid(world, pos, kind.shape())
        && !physics::touches_block(world, pos, kind.shape(), |b| b.is_fluid() || b.is_waterlogged())
}

/// Whether natural spawning of `kind` in the Overworld belongs to this cycle.
pub(super) fn cycle_kind(kind: MobKind) -> bool {
    matches!(
        kind,
        MobKind::Zombie
            | MobKind::ZombieVillager
            | MobKind::Husk
            | MobKind::Skeleton
            | MobKind::Creeper
            | MobKind::Spider
            | MobKind::Enderman
            | MobKind::Witch
    )
}

/// Java ticks per natural spawn round.
const TICKS_PER_ROUND: u32 = (SPAWN_INTERVAL * 20.0 + 0.5) as u32;
/// Java's chunk edge for spawn sampling.
const SPAWN_CHUNK: i32 = 16;

impl Entities {
    /// One natural spawn round of Overworld monsters: for every 16x16
    /// chunk that reaches the spawn ring of a player, one start position
    /// per Java tick, uniform in the chunk and in height from the bottom of
    /// the world to just above its `WORLD_SURFACE` block.
    pub(super) fn overworld_monsters<W: MobWorld + ?Sized>(&mut self, world: &W, ctx: &Ctx) {
        let mut chunks: Vec<(i32, i32)> = Vec::new();
        let reach = SPAWN_MAX_DIST;
        for t in &ctx.players {
            let lo = ((t.pos.x - reach).floor() as i32).div_euclid(SPAWN_CHUNK);
            let hi = ((t.pos.x + reach).floor() as i32).div_euclid(SPAWN_CHUNK);
            let zlo = ((t.pos.z - reach).floor() as i32).div_euclid(SPAWN_CHUNK);
            let zhi = ((t.pos.z + reach).floor() as i32).div_euclid(SPAWN_CHUNK);
            for cz in zlo..=zhi {
                for cx in lo..=hi {
                    // Nearest point of the chunk to the player, horizontally.
                    let nx = t.pos.x.clamp((cx * SPAWN_CHUNK) as f64, (cx * SPAWN_CHUNK + SPAWN_CHUNK) as f64);
                    let nz = t.pos.z.clamp((cz * SPAWN_CHUNK) as f64, (cz * SPAWN_CHUNK + SPAWN_CHUNK) as f64);
                    if (nx - t.pos.x).powi(2) + (nz - t.pos.z).powi(2) <= reach * reach {
                        chunks.push((cx, cz));
                    }
                }
            }
        }
        chunks.sort_unstable();
        chunks.dedup();
        let darken = ctx.spawn_light.monster_darken;
        for _ in 0..TICKS_PER_ROUND {
            for &(cx, cz) in &chunks {
                let x = cx * SPAWN_CHUNK + self.rng.next_int(SPAWN_CHUNK as u32) as i32;
                let z = cz * SPAWN_CHUNK + self.rng.next_int(SPAWN_CHUNK as u32) as i32;
                let Some(surface) = world.world_surface(x, z) else { continue };
                let min_y = Dimension::Overworld.min_y();
                let top = surface + 1;
                if top < min_y {
                    continue;
                }
                let y = min_y + self.rng.next_int((top - min_y + 1) as u32) as i32;
                if y < min_y + 1 {
                    continue;
                }
                self.monster_packs(world, ctx, IVec3::new(x, y, z), darken);
            }
        }
    }

    /// Java's `spawnCategoryForPosition` for monsters: three packs walking
    /// from `start` in steps of `rand(6) - rand(6)`, each settling on one
    /// list entry at its first position far enough from every player.
    pub(super) fn monster_packs<W: MobWorld + ?Sized>(&mut self, world: &W, ctx: &Ctx, start: IVec3, darken: u8) {
        // Java's `isRedstoneConductor`: a full opaque start cell ends the attempt.
        if world.block(start).is_none_or(|b| b.is_opaque() && full_collision(b)) {
            return;
        }
        let mut spawned = 0;
        for _ in 0..3 {
            let (mut x, mut z) = (start.x, start.z);
            let mut entry: Option<&'static MonsterSpawn> = None;
            let mut steps = (self.rng.next_f32() * 4.0).ceil() as u32;
            let mut step = 0;
            while step < steps {
                step += 1;
                x += self.rng.next_int(6) as i32 - self.rng.next_int(6) as i32;
                z += self.rng.next_int(6) as i32 - self.rng.next_int(6) as i32;
                let cell = IVec3::new(x, start.y, z);
                let pos = cell.as_dvec3() + DVec3::new(0.5, 0.0, 0.5);
                let Some(d2) = ctx.nearest_player_dist2(pos) else { continue };
                // Java rejects 24 blocks or closer; VoxelCraft's ring also ends at 64.
                if d2 <= SPAWN_MIN_DIST * SPAWN_MIN_DIST || d2 > SPAWN_MAX_DIST * SPAWN_MAX_DIST {
                    continue;
                }
                if !world.loaded(cell) {
                    continue;
                }
                let e = match entry {
                    Some(e) => e,
                    None => {
                        let Some(e) = pick(monster_list(world.cave_biome(cell)), &mut self.rng) else { break };
                        entry = Some(e);
                        steps = e.min + self.rng.next_int(e.max - e.min + 1);
                        e
                    }
                };
                let Some(kind) = e.mob.kind() else { break };
                let near = ctx
                    .players
                    .iter()
                    .min_by(|a, b| a.pos.distance_squared(pos).total_cmp(&b.pos.distance_squared(pos)))
                    .map_or(pos, |t| t.pos);
                if self.count_near(kind, near) >= kind.spawn_cap(Dimension::Overworld) {
                    break;
                }
                if !monster_spot_ok(world, kind, cell, darken, &mut self.rng) {
                    continue;
                }
                self.spawn_natural_monster(kind, pos);
                spawned += 1;
                // Java's default `getMaxSpawnClusterSize` is 4 for these monsters.
                if spawned >= 4 {
                    return;
                }
            }
        }
    }

    fn spawn_natural_monster(&mut self, kind: MobKind, pos: DVec3) {
        if kind == MobKind::ZombieVillager {
            self.spawn(MobKind::Zombie, pos);
            self.make_zombie_villager();
        } else {
            self.spawn(kind, pos);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::world::lighting::flat_test_world;
    use glam::IVec2;
    use serde_json::Value;

    fn fixture() -> Value {
        serde_json::from_str(include_str!("../../tests/fixtures/java/26.3/lighting_spawning.json")).unwrap()
    }

    fn block(name: &str) -> Block {
        match name {
            "air" => Block::AIR,
            "oak_leaves" => Block::LEAVES,
            "magma_block" => Block::MAGMA,
            "stone_slab" => Block::slab_of(Block::STONE).unwrap(),
            "stone_brick_stairs" => Block::stairs_of(Block::STONE_BRICKS).unwrap(),
            "jack_o_lantern" => crate::world::pumpkin_blocks::JACK_O_LANTERN,
            "redstone_lamp" => crate::world::redstone_blocks::LAMP,
            "lit_redstone_lamp" => Block(crate::world::redstone_blocks::LAMP.0 + 1),
            other => Block::from_name(other).unwrap_or_else(|| panic!("no block {other}")),
        }
    }

    fn cell(v: &Value) -> IVec3 {
        let a = v.as_array().unwrap();
        IVec3::new(a[0].as_i64().unwrap() as i32, a[1].as_i64().unwrap() as i32, a[2].as_i64().unwrap() as i32)
    }

    #[test]
    fn biome_monster_lists_match_the_reference_attributes() {
        let f = fixture();
        for biome in Biome::ALL {
            let reference = f["monster_spawns"][biome.name()].as_array().unwrap_or_else(|| panic!("{biome:?}"));
            let ours: Vec<_> = monster_list(biome).iter().map(|s| (s.mob.id(), s.weight, s.min, s.max)).collect();
            let theirs: Vec<_> = reference
                .iter()
                .map(|e| {
                    let n = |k: &str| e[k].as_u64().unwrap() as u32;
                    (e["type"].as_str().unwrap(), n("weight"), n("min"), n("max"))
                })
                .collect();
            assert_eq!(ours, theirs, "{biome:?}");
        }
    }

    #[test]
    fn spawn_floor_overrides_follow_the_reference() {
        let f = fixture();
        let floors = &f["spawn_floor"];
        let mut checked = 0;
        for (rule, ok) in [("never", false), ("always", true), ("polar_bear", false), ("fire_immune", false)] {
            for name in floors[rule].as_array().unwrap() {
                let name = name.as_str().unwrap();
                let Some(b) = Block::from_name(name).filter(|_| name != "air") else { continue };
                assert_eq!(valid_spawn_floor(b, MobKind::Zombie), ok, "{rule} {name}");
                if rule == "fire_immune" {
                    assert!(valid_spawn_floor(b, MobKind::MagmaCube), "{name}");
                }
                checked += 1;
            }
        }
        assert!(checked >= 8, "{checked}");
        // `leavesProperties()` covers every leaves block.
        assert!(floors["ocelot_or_parrot"].as_array().unwrap().iter().any(|v| v == "leavesProperties()"));
        for b in [Block::LEAVES, Block(23), Block(73), Block(74), Block(75)] {
            assert!(b.is_leaves() && !valid_spawn_floor(b, MobKind::Zombie), "{b:?}");
        }
        assert!(floors["default"].as_str().unwrap().contains("below 14"));
        assert!(valid_spawn_floor(Block::STONE, MobKind::Zombie));
        assert!(!valid_spawn_floor(Block::GLOWSTONE, MobKind::Zombie), "emission 15");
        assert!(!valid_spawn_floor(Block::slab_of(Block::STONE).unwrap(), MobKind::Zombie), "bottom slab");
        assert!(!valid_spawn_floor(Block::FARMLAND, MobKind::Zombie), "15/16 high");
        assert!(!valid_empty_cell(Block::RAIL, MobKind::Zombie), "rails prevent spawning inside");
        assert!(!valid_empty_cell(Block::WATER, MobKind::Zombie));
        assert!(valid_empty_cell(Block::TORCH, MobKind::Zombie), "a torch is not a collision block");
    }

    #[test]
    fn sky_darkening_matches_the_reference_ends() {
        let f = fixture();
        let darken = &f["sky_darken"];
        assert_eq!(darken["default_level"].as_f64(), Some(15.0));
        // Java multiplies Float attributes in float precision before f2i.
        // Using f64 here incorrectly truncates the night darkening to ten.
        let night = darken["day_multiplier_keyframes"][2][1].as_f64().unwrap() as f32 * 15.0;
        let rain = darken["rain_blend"]["alpha"].as_f64().unwrap() as f32;
        let level = crate::simulation::lighting::SpawnLight::at;
        assert_eq!(level(6000, 0., 0., false).sky_darken, 0);
        assert_eq!(level(18000, 0., 0., false).sky_darken, (15.0 - night) as u8, "clear night");
        assert_eq!(level(6000, 1., 0., false).sky_darken, (15.0 - (15.0 + (4.0 - 15.0) * rain)) as u8, "rain at noon");
        assert_eq!(level(18000, 1., 0., false).sky_darken, 11, "rainy night stays at the night level");
    }

    fn ctx(players: Vec<Target>) -> Ctx {
        Ctx {
            players,
            daylight: 0.12,
            spawn_light: crate::simulation::lighting::SpawnLight::NIGHT,
            spawning: true,
            raining: false,
            dimension: Dimension::Overworld,
        }
    }

    /// Accepted spawns out of `trials` for a zombie at `cell`.
    fn accepted(world: &World, cell: IVec3, darken: u8, trials: u32, rng: &mut Rng) -> u32 {
        (0..trials).filter(|_| monster_spot_ok(world, MobKind::Zombie, cell, darken, rng)).count() as u32
    }

    fn check_probes(world: &World, probes: &Value, scene: &str, rng: &mut Rng) {
        const TRIALS: u32 = 4096;
        for probe in probes.as_array().unwrap() {
            let p = cell(&probe["cell"]);
            assert_eq!(world.sky_light(p).map(u64::from), probe["sky"].as_u64(), "{scene} sky {p}");
            assert_eq!(u64::from(world.block_light(p)), probe["block"].as_u64().unwrap(), "{scene} block {p}");
            for (key, darken) in [("zombie_night_of_256", 11), ("zombie_noon_of_256", 0)] {
                let expected = probe[key].as_u64().unwrap() as f64 / 256.0;
                let got = accepted(world, p, darken, TRIALS, rng) as f64 / TRIALS as f64;
                if expected == 0.0 || expected == 1.0 {
                    assert_eq!(got, expected, "{scene} {key} {p}");
                } else {
                    assert!((got - expected).abs() < 0.04, "{scene} {key} {p}: {got} vs {expected}");
                }
            }
        }
    }

    #[test]
    fn reference_scenes_light_and_spawn_odds_match_incrementally() {
        let f = fixture();
        let mut rng = Rng::new(263);
        let columns = || (-1..=2).flat_map(|x| (-1..=1).map(move |z| IVec2::new(x, z)));
        for scene in f["scenes"].as_array().unwrap() {
            let name = scene["name"].as_str().unwrap();
            let mut world = flat_test_world(columns(), 63);
            for step in scene["steps"].as_array().unwrap() {
                for edit in step["edits"].as_array().unwrap() {
                    let b = block(edit[0].as_str().unwrap());
                    let (lo, hi) = (cell(&edit[1]), cell(&edit[2]));
                    for y in lo.y..=hi.y {
                        for z in lo.z..=hi.z {
                            for x in lo.x..=hi.x {
                                world.set_block(IVec3::new(x, y, z), b);
                            }
                        }
                    }
                }
                check_probes(&world, &step["probes"], name, &mut rng);
            }
            // Unloading the column that lights a border tunnel makes its
            // light unknown, so nothing spawns there; reloading restores both.
            if let Some(c) = scene["unload_column"].as_array() {
                let column = IVec2::new(c[0].as_i64().unwrap() as i32, c[1].as_i64().unwrap() as i32);
                world.unload_test_column(column);
                world.update_block_light();
                let probes = scene["steps"].as_array().unwrap().last().unwrap()["probes"].as_array().unwrap();
                for probe in probes {
                    let p = cell(&probe["cell"]);
                    assert_eq!(world.sky_light(p), None, "{name} {p}");
                    assert_eq!(accepted(&world, p, 11, 64, &mut rng), 0, "{name} {p}");
                }
                world.load_test_column(column, 63);
                world.update_block_light();
                check_probes(&world, &scene["steps"].as_array().unwrap().last().unwrap()["probes"], name, &mut rng);
            }
        }
    }

    /// Feet cells of monsters that appeared during `seconds` of simulation.
    fn spawned_cells(entities: &mut Entities, world: &World, ctx: &Ctx, seconds: f64) -> Vec<(MobKind, IVec3)> {
        let mut out = Vec::new();
        for _ in 0..(seconds * 20.0) as u32 {
            let before = entities.mobs.len();
            entities.update_difficulty(0.05, world, ctx, crate::simulation::difficulty::Difficulty::Normal);
            out.extend(entities.mobs.iter().skip(before).map(|m| (m.kind, m.pos.floor().as_ivec3())));
            // Keep caps open: count the spawns, then clear them away.
            entities.mobs.clear();
        }
        out
    }

    /// A sealed room 40-48 blocks from a surface player, 20-24 blocks down.
    fn cave_world(torches: bool) -> World {
        let mut world = flat_test_world((-3..=3).flat_map(|x| (-3..=3).map(move |z| IVec2::new(x, z))), 63);
        for y in 40..=43 {
            for z in -6..=6 {
                for x in 34..=46 {
                    world.set_block(IVec3::new(x, y, z), Block::AIR);
                }
            }
        }
        if torches {
            for z in [-5, 0, 5] {
                for x in [35, 40, 45] {
                    world.set_block(IVec3::new(x, 40, z), Block::TORCH);
                }
            }
        }
        world
    }

    fn in_room(p: IVec3) -> bool {
        (34..=46).contains(&p.x) && (40..=43).contains(&p.y) && (-6..=6).contains(&p.z)
    }

    #[test]
    fn monsters_spawn_in_dark_caves_by_day_and_never_in_torch_light() {
        let player = Target::new(PlayerId::HOST, DVec3::new(0.5, 64.0, 0.5), true);
        let day = Ctx { daylight: 1.0, spawn_light: crate::simulation::lighting::SpawnLight::DAY, ..ctx(vec![player]) };
        let night = ctx(vec![player]);
        let dark = cave_world(false);
        let mut e = Entities::new(5);
        let spawns = spawned_cells(&mut e, &dark, &day, 30.0);
        assert!(spawns.iter().all(|(_, p)| in_room(*p)), "daylight keeps the surface clear: {spawns:?}");
        assert!(!spawns.is_empty(), "the dark cave spawns by day");
        assert!(spawns.iter().all(|(k, p)| cycle_kind(*k) && p.y == 40), "monsters stand on the cave floor");
        let night_spawns = spawned_cells(&mut e, &dark, &night, 30.0);
        assert!(night_spawns.iter().any(|(_, p)| p.y == 64), "night opens the surface");
        assert!(night_spawns.iter().any(|(_, p)| in_room(*p)), "and the cave keeps spawning");

        let lit = cave_world(true);
        assert!((34..=46).all(|x| (-6..=6).all(|z| lit.block_light(IVec3::new(x, 40, z)) > 0)));
        for c in [&day, &night] {
            let spawns = spawned_cells(&mut e, &lit, c, 30.0);
            assert!(!spawns.iter().any(|(_, p)| in_room(*p)), "torch light suppresses the cave: {spawns:?}");
        }
        // Removing the torches lets the cave spawn again.
        let mut relit = lit;
        for z in [-5, 0, 5] {
            for x in [35, 40, 45] {
                relit.set_block(IVec3::new(x, 40, z), Block::AIR);
            }
        }
        assert!(spawned_cells(&mut e, &relit, &day, 30.0).iter().any(|(_, p)| in_room(*p)));
    }

    #[test]
    fn starts_sample_the_full_column_height_and_respect_peaceful() {
        let world = flat_test_world((-3..=3).flat_map(|x| (-3..=3).map(move |z| IVec2::new(x, z))), 63);
        let player = Target::new(PlayerId::HOST, DVec3::new(0.5, 64.0, 0.5), true);
        let night = ctx(vec![player]);
        let mut e = Entities::new(9);
        let spawns = spawned_cells(&mut e, &world, &night, 20.0);
        assert!(!spawns.is_empty() && spawns.iter().all(|(_, p)| p.y == 64), "only the open surface: {spawns:?}");
        let mut quiet = Entities::new(9);
        for _ in 0..200 {
            quiet.update_difficulty(0.05, &world, &night, crate::simulation::difficulty::Difficulty::Peaceful);
        }
        assert!(quiet.mobs.iter().all(|m| !m.kind.is_hostile()));
    }
}
