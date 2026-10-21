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
//! Slimes (`Slime.checkSlimeSpawnRules`) and drowned (`IN_WATER` placement and
//! `Drowned.checkDrownedSpawnRules`) walk the same packs with their own
//! spawn rules.
//!
//! Caps follow Java's monster category (see [`super::category`]): 70 per
//! 289 spawnable chunks globally and 70 per player locally, over chunks
//! within 128 blocks.
//!
//! Remaining VoxelCraft limits, recorded in the lighting/spawning report:
//! structure spawn overrides (swamp huts, monuments, outposts) are not
//! applied, and list entries for mobs VoxelCraft lacks (strays, bogged,
//! parched, zombie horses, ocelots) end their pack.

use super::category::{self, MobCategory};
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
    Known(MobKind),
    Missing(&'static str),
}

impl JavaMonster {
    /// Java's entity id (without `minecraft:`).
    #[cfg(test)]
    pub(super) fn id(self) -> &'static str {
        match self {
            JavaMonster::Known(k) => k.name(),
            JavaMonster::Missing(id) => id,
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

    /// The kind this cycle spawns for the entry; zombie horses, strays,
    /// bogged, parched and ocelots are not in VoxelCraft yet.
    pub(super) fn kind(self) -> Option<MobKind> {
        Some(match self {
            JavaMonster::Known(k) => k,
            JavaMonster::Spider => MobKind::Spider,
            JavaMonster::Zombie => MobKind::Zombie,
            JavaMonster::ZombieVillager => MobKind::ZombieVillager,
            JavaMonster::Skeleton => MobKind::Skeleton,
            JavaMonster::Creeper => MobKind::Creeper,
            JavaMonster::Enderman => MobKind::Enderman,
            JavaMonster::Witch => MobKind::Witch,
            JavaMonster::Husk => MobKind::Husk,
            JavaMonster::Slime => MobKind::Slime,
            JavaMonster::Drowned => MobKind::Drowned,
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

pub(super) const fn e(mob: JavaMonster, weight: u32, min: u32, max: u32) -> MonsterSpawn {
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
pub(super) fn pick(list: &'static [MonsterSpawn], rng: &mut Rng) -> Option<&'static MonsterSpawn> {
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

/// A bottom slab: a low cutout block whose box `shaped()` doesn't describe.
/// Top slabs are shaped and double slabs full cubes.
fn bottom_slab(b: Block) -> bool {
    use crate::world::slab_stair_states as ss;
    ss::slab_type(b) == Some(ss::SlabType::Bottom)
}

/// Whether a block fills its whole cell for collision (Java's `isCollisionShapeFullBlock`).
fn full_collision(b: Block) -> bool {
    crate::world::mount_states::collision_is_full(b)
}

/// Java's `isRedstoneConductor`. The default is `isCollisionShapeFullBlock`;
/// `Blocks` overrides it with `never` for glass, ice, leaves, pistons and a few
/// others, and `always` for soul sand and mud (fixture `redstone_conductor`).
pub(super) fn is_redstone_conductor(b: Block) -> bool {
    conductor_override(b.name()).unwrap_or_else(|| full_collision(b))
}

/// The registered override for a block name; the list covers the fixture's
/// `never`/`always` fields plus its stained glass, copper grate/bulb and leaves families.
fn conductor_override(name: &str) -> Option<bool> {
    match name {
        "mud" | "soul sand" => Some(true),
        "glass" | "tinted glass" | "ice" | "frosted ice" | "glowstone" | "sea lantern" | "redstone block" | "tnt"
        | "beacon" | "observer" | "piston" | "sticky piston" | "moving piston" | "bamboo" | "scaffolding"
        | "powder snow" | "chorus flower" | "pointed dripstone" | "sulfur spike" => Some(false),
        n if n.ends_with("stained glass")
            || n.ends_with("leaves")
            || n.contains("copper grate")
            || n.contains("copper bulb") =>
        {
            Some(false)
        }
        _ => None,
    }
}

/// Java's default `isValidSpawn`: a sturdy top face (a collision box
/// covering the whole top of the cell) and emission below 14.
fn sturdy_top(b: Block) -> bool {
    use crate::world::shape;
    if !b.is_solid() || bottom_slab(b) {
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
        || (b.is_leaves() && kind != MobKind::Parrot)
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

/// The light and weather inputs of the spawn rules.
#[derive(Clone, Copy)]
pub(super) struct SpawnRules {
    pub dimension: Dimension,
    /// `Level.skyDarken` and the thunder-adjusted darkening of `Monster.isDarkEnoughToSpawn`.
    pub light: crate::simulation::lighting::SpawnLight,
    /// `Level.getMoonBrightness` (`surface_slime_spawn_chance` is half of it).
    pub moon: f32,
}

/// `Slime.checkSlimeSpawnRules` for natural spawns, after the `ON_GROUND`
/// floor checks: swamps between y 50 and 70 in dim light (chance from the
/// moon phase), or a slime chunk below y 40 on one attempt in ten.
fn slime_rules<W: MobWorld + ?Sized>(world: &W, cell: IVec3, rules: SpawnRules, rng: &mut Rng) -> bool {
    if world.cave_biome(cell).is_swamp() && cell.y > 50 && cell.y < 70 && rng.next_f32() < 0.5 * rules.moon {
        let Some(sky) = world.sky_light_level(cell) else { return false };
        let light = sky.saturating_sub(rules.light.sky_darken).max(world.block_light(cell));
        if u32::from(light) <= rng.next_int(8) {
            return true;
        }
    }
    let slime_chunk = super::slime::slime_chunk(world.seed(), cell.x.div_euclid(16), cell.z.div_euclid(16));
    rng.next_int(10) == 0 && slime_chunk && cell.y < 40
}

/// `SpawnPlacementTypes.IN_WATER` and `Drowned.checkDrownedSpawnRules`: water
/// in the cell and below it, no conducting block above, dark enough, then
/// one attempt in 15 in rivers or one in 40 in deep water elsewhere.
fn drowned_spot_ok<W: MobWorld + ?Sized>(world: &W, cell: IVec3, rules: SpawnRules, rng: &mut Rng) -> bool {
    use crate::world::terrain::SEA_LEVEL;
    let (Some(below), Some(at), Some(above)) =
        (world.block(cell - IVec3::Y), world.block(cell), world.block(cell + IVec3::Y))
    else {
        return false;
    };
    if !at.holds_water() || is_redstone_conductor(above) || !below.holds_water() {
        return false;
    }
    let can_spawn = dark_enough(world, cell, rules.light.monster_darken, rng);
    if world.cave_biome(cell).is_river() {
        rng.next_int(15) == 0 && can_spawn
    } else {
        rng.next_int(40) == 0 && cell.y < SEA_LEVEL - 5 && can_spawn
    }
}

/// Every check Java makes on a natural monster spawn position (feet cell
/// `cell`) after distance: the placement type, the type's spawn rules and
/// a clear spawn box (dry unless the mob swims).
pub(super) fn monster_spot_ok<W: MobWorld + ?Sized>(
    world: &W,
    kind: MobKind,
    cell: IVec3,
    rules: SpawnRules,
    rng: &mut Rng,
) -> bool {
    let pos = cell.as_dvec3() + DVec3::new(0.5, 0.0, 0.5);
    if kind == MobKind::Drowned {
        // `Drowned.checkSpawnObstruction` is only `isUnobstructed`: water is fine.
        return drowned_spot_ok(world, cell, rules, rng) && !physics::overlaps_solid(world, pos, kind.shape());
    }
    let (Some(below), Some(at), Some(above)) =
        (world.block(cell - IVec3::Y), world.block(cell), world.block(cell + IVec3::Y))
    else {
        return false;
    };
    if !valid_spawn_floor(below, kind) || !valid_empty_cell(at, kind) || !valid_empty_cell(above, kind) {
        return false;
    }
    if kind == MobKind::Slime {
        if !slime_rules(world, cell, rules, rng) {
            return false;
        }
    } else if !dark_enough(world, cell, rules.light.monster_darken, rng) {
        return false;
    }
    // Husks use `checkSurfaceMonstersSpawnRules`: they also need to see the sky.
    if kind == MobKind::Husk && world.sky_light_level(cell) != Some(15) {
        return false;
    }
    !physics::overlaps_solid(world, pos, kind.shape())
        && !physics::touches_block(world, pos, kind.shape(), |b| b.is_fluid() || b.is_waterlogged())
}

/// Whether natural spawning of `kind` in the Overworld belongs to this cycle.
#[cfg(test)]
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
            | MobKind::Slime
            | MobKind::Drowned
    )
}

impl Entities {
    #[cfg(test)]
    pub(super) fn monster_packs<W: MobWorld + ?Sized>(
        &mut self,
        world: &W,
        ctx: &Ctx,
        start: IVec3,
        rules: SpawnRules,
        state: &mut category::SpawnState,
    ) {
        self.category_packs(world, ctx, start, rules, state, MobCategory::Monster, &[]);
    }

    /// Java's `spawnCategoryForPosition` for monsters: three packs walking
    /// from `start` in steps of `rand(6) - rand(6)`, each settling on one
    /// list entry at its first position far enough from every player.
    #[allow(clippy::too_many_arguments)]
    pub(super) fn category_packs<W: MobWorld + ?Sized>(
        &mut self,
        world: &W,
        ctx: &Ctx,
        start: IVec3,
        rules: SpawnRules,
        state: &mut category::SpawnState,
        category: MobCategory,
        structures: &[std::sync::Arc<crate::world::temples::Built>],
    ) {
        // Java's `isRedstoneConductor`: a conducting start cell ends the attempt.
        if world.block(start).is_none_or(is_redstone_conductor) {
            return;
        }
        let mut spawned = 0;
        for _ in 0..3 {
            let (mut x, mut z) = (start.x, start.z);
            let mut entry: Option<&'static MonsterSpawn> = None;
            let mut group = PackGroup::default();
            let mut steps = (self.rng.next_f32() * 4.0).ceil() as u32;
            let mut step = 0;
            while step < steps {
                step += 1;
                x += self.rng.next_int(6) as i32 - self.rng.next_int(6) as i32;
                z += self.rng.next_int(6) as i32 - self.rng.next_int(6) as i32;
                let cell = IVec3::new(x, start.y, z);
                let pos = cell.as_dvec3() + DVec3::new(0.5, 0.0, 0.5);
                let Some(d2) = ctx.nearest_player_dist2(pos) else { continue };
                // Java rejects 24 blocks or closer.
                if d2 <= SPAWN_MIN_DIST * SPAWN_MIN_DIST {
                    continue;
                }
                // Java also keeps natural spawns 24 blocks from the world spawn.
                if super::near_world_spawn(self.world_spawn, pos) {
                    continue;
                }
                // `isValidSpawnPostitionForType`: the start chunk or one that can spawn entities.
                if !world.entity_ticking(cell) {
                    continue;
                }
                // Biome lookups sample generator noise, so each step resolves its list once.
                let mut list = None;
                let e = match entry {
                    Some(e) => e,
                    None => {
                        if category == MobCategory::WaterAmbient
                            && world.cave_biome(cell).is_river()
                            && self.rng.next_f32() < 0.98
                        {
                            break;
                        }
                        let here = super::spawning::spawn_list(world, category, rules.dimension, cell, structures);
                        list = Some(here);
                        let Some(e) = pick(here, &mut self.rng) else {
                            break;
                        };
                        entry = Some(e);
                        steps = e.min + self.rng.next_int(e.max - e.min + 1);
                        e
                    }
                };
                let Some(kind) = e.mob.kind() else { break };
                // `isValidSpawnPostitionForType`: no monster spawns where it
                // would despawn at once.
                let far = kind.category().despawn_distance();
                if d2 > far * far {
                    continue;
                }
                // Placement first: it rejects most cells without a biome lookup.
                if !super::spawn_rules::spot_ok(world, kind, cell, rules, &mut self.rng)
                    || !list
                        .unwrap_or_else(|| {
                            super::spawning::spawn_list(world, category, rules.dimension, cell, structures)
                        })
                        .contains(e)
                    || (rules.dimension == Dimension::Nether && !self.nether_cost_allows(world, kind, cell))
                {
                    continue;
                }
                self.spawn_natural(kind, pos, world.cave_biome(cell), &mut group);
                state.add(category, pos);
                spawned += 1;
                // Java's default `getMaxSpawnClusterSize` is 4 for these monsters.
                if spawned
                    >= match kind {
                        MobKind::Wolf => 8,
                        MobKind::Ghast => 1,
                        _ => 4,
                    }
                {
                    return;
                }
            }
        }
    }

    /// Spawns one natural mob as the next member of `group`, Java's `SpawnGroupData`.
    pub(super) fn spawn_natural(&mut self, kind: MobKind, pos: DVec3, biome: Biome, group: &mut PackGroup) {
        if kind == MobKind::ZombieVillager {
            self.spawn(MobKind::Zombie, pos);
            self.make_zombie_villager();
        } else {
            self.spawn(kind, pos);
        }
        // Natural spawns never require persistence (axolotls and hoglins included),
        // so they count toward caps. Animal.removeWhenFarAway keeps creatures anyway.
        self.mobs.last_mut().unwrap().persistent = false;
        self.natural_animal(biome, group.size);
        group.size += 1;
        let Some(m) = self.mobs.last_mut() else { return };
        // The first member picks the horse coat, llama colour, rabbit or fox
        // type; later members copy it. Horse markings stay per horse.
        let variant = match m.kind {
            MobKind::Horse | MobKind::Llama => m.mount.as_mut().map(|s| &mut s.variant),
            MobKind::Rabbit | MobKind::Fox => m.animal.as_mut().map(|a| &mut a.variant),
            _ => None,
        };
        if let Some(variant) = variant {
            *variant = *group.variant.get_or_insert(*variant);
        }
    }
}

/// Java's `SpawnGroupData` for one pack or generation herd: the shared variant
/// and `AgeableMobGroupData`'s count of members spawned so far.
#[derive(Clone, Copy, Default)]
pub(super) struct PackGroup {
    pub variant: Option<u8>,
    pub size: u32,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::world::lighting::flat_test_world;
    use glam::IVec2;
    use serde_json::Value;

    #[test]
    fn natural_spawns_never_require_persistence() {
        let mut e = Entities::new(1);
        for kind in [MobKind::Axolotl, MobKind::Hoglin, MobKind::Cow, MobKind::Zombie] {
            e.spawn_natural(kind, DVec3::new(0.5, 64.0, 0.5), Biome::Plains, &mut PackGroup::default());
            assert!(!e.mobs.last().unwrap().persistent, "{kind:?} counts toward its category cap");
        }
        e.ensure_mob_ids();
        let mut restored = Entities::new(2);
        restored.load_nether_mobs(&e.nether_mobs_to_string());
        let kinds: Vec<_> = restored.mobs.iter().map(|m| (m.kind, m.persistent)).collect();
        assert_eq!(
            kinds,
            [(MobKind::Axolotl, false), (MobKind::Hoglin, false), (MobKind::Cow, false)],
            "breedable natural mobs still save; natural zombies never did"
        );
    }

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
    fn redstone_conductor_start_cells_follow_the_reference_predicates() {
        let f = fixture();
        let conductor = &f["redstone_conductor"];
        assert_eq!(conductor["default"], "BlockStateBase.isCollisionShapeFullBlock");
        assert_eq!(conductor["call_sites_found"], 26);
        let mut checked = 0;
        for (rule, expected) in [("never", false), ("always", true)] {
            for name in conductor[rule].as_array().unwrap() {
                let name = name.as_str().unwrap();
                // Several overrides name blocks VoxelCraft has not added yet.
                let Some(b) = Block::from_name(name) else { continue };
                assert_eq!(is_redstone_conductor(b), expected, "{rule} {name}");
                checked += 1;
            }
        }
        assert!(checked >= 12, "{checked}");
        // Registered families: every dye color and the leaves helper.
        for name in ["white stained glass", "lime stained glass", "oak leaves", "birch leaves"] {
            let b = Block::from_name(name).unwrap_or_else(|| panic!("no block {name}"));
            assert!(!is_redstone_conductor(b), "{name}");
        }
        assert!(conductor["families"].as_object().unwrap().contains_key("stained_glass"));
        // The default: a full collision shape conducts; slabs, air and water do not.
        assert!(is_redstone_conductor(Block::STONE));
        assert!(!is_redstone_conductor(Block::slab_of(Block::STONE).unwrap()));
        assert!(!is_redstone_conductor(Block::AIR) && !is_redstone_conductor(Block::WATER));
        // `always` overrides Java's 14/16 shapes (VoxelCraft's soul sand and mud
        // collision is still a full cube, so the override is checked directly).
        assert_eq!(conductor_override("soul sand"), Some(true));
        assert_eq!(conductor_override("mud"), Some(true));
    }

    #[test]
    fn a_conducting_start_cell_ends_the_spawn_attempt() {
        let mut world = flat_test_world((-3..=3).flat_map(|x| (-3..=3).map(move |z| IVec2::new(x, z))), 63);
        let player = Target::new(PlayerId::HOST, DVec3::new(0.5, 64.0, 0.5), true);
        let night = ctx(vec![player]);
        let start = IVec3::new(40, 64, 0);
        let spawn_count = |world: &World| {
            let mut e = Entities::new(3);
            let mut state = category::SpawnState::new(&[night.players[0].pos], std::iter::empty(), |_| true);
            for _ in 0..400 {
                e.monster_packs(world, &night, start, rules(night.spawn_light, 1.0), &mut state);
            }
            e.mobs.len()
        };
        assert!(spawn_count(&world) > 0, "an open start cell spawns on a dark surface");
        world.set_block(start, Block::STONE);
        assert_eq!(spawn_count(&world), 0, "a full block ends the attempt");
        world.set_block(start, Block::SOUL_SAND);
        assert_eq!(spawn_count(&world), 0, "soul sand is registered as always conducting");
        world.set_block(start, Block::GLASS);
        assert!(spawn_count(&world) > 0, "glass is registered as never conducting");
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
        assert_eq!(level(6000, 0., 0.).sky_darken, 0);
        assert_eq!(level(18000, 0., 0.).sky_darken, (15.0 - night) as u8, "clear night");
        assert_eq!(level(6000, 1., 0.).sky_darken, (15.0 - (15.0 + (4.0 - 15.0) * rain)) as u8, "rain at noon");
        assert_eq!(level(18000, 1., 0.).sky_darken, 11, "rainy night stays at the night level");
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

    fn rules(light: crate::simulation::lighting::SpawnLight, moon: f32) -> SpawnRules {
        SpawnRules { dimension: Dimension::Overworld, light, moon }
    }

    /// Accepted spawns out of `trials` for a zombie at `cell`.
    fn accepted(world: &World, cell: IVec3, darken: u8, trials: u32, rng: &mut Rng) -> u32 {
        let light = crate::simulation::lighting::SpawnLight { sky_darken: darken, monster_darken: darken };
        (0..trials).filter(|_| monster_spot_ok(world, MobKind::Zombie, cell, rules(light, 1.0), rng)).count() as u32
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
        // Cave packs are a few percent of night spawns, so pool several random
        // sequences rather than depend on one (any change in RNG use shifts it).
        let cave_by_night: usize = (1..=4)
            .map(|seed| spawned_cells(&mut Entities::new(seed), &dark, &night, 30.0))
            .map(|spawns| spawns.iter().filter(|(_, p)| in_room(*p)).count())
            .sum();
        assert!(cave_by_night > 0, "and the cave keeps spawning");

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
    fn natural_spawns_keep_clear_of_the_world_spawn_point() {
        let f = fixture();
        let reference = f["natural_spawner"]["spawn_point_min_distance"].as_f64().unwrap();
        assert_eq!(reference, super::super::SPAWN_POINT_MIN_DIST);
        // `closerToCenterThan` measures from the spawn block's centre and is strict.
        let spawn = Some(IVec3::new(0, 64, 0));
        let near = super::super::near_world_spawn;
        assert!(!near(spawn, DVec3::new(0.5 + reference, 64.5, 0.5)), "exactly 24 is allowed");
        assert!(near(spawn, DVec3::new(0.5 + reference - 1e-9, 64.5, 0.5)));
        assert!(near(spawn, DVec3::new(0.5, 64.5 - 23.9, 0.5)), "vertical distance counts");
        assert!(!near(None, DVec3::new(0.5, 64.0, 0.5)), "no spawn point in this dimension");

        // The dark cave spawns by day until the world spawn sits inside it.
        let player = Target::new(PlayerId::HOST, DVec3::new(0.5, 64.0, 0.5), true);
        let day = Ctx { daylight: 1.0, spawn_light: crate::simulation::lighting::SpawnLight::DAY, ..ctx(vec![player]) };
        let dark = cave_world(false);
        let mut free = Entities::new(5);
        assert!(spawned_cells(&mut free, &dark, &day, 30.0).iter().any(|(_, p)| in_room(*p)));
        let mut guarded = Entities::new(5);
        guarded.world_spawn = Some(IVec3::new(40, 41, 0));
        let spawns = spawned_cells(&mut guarded, &dark, &day, 30.0);
        assert!(spawns.is_empty(), "every room cell is within 24 blocks of the spawn: {spawns:?}");
    }

    #[test]
    fn spawn_mobs_and_spawn_monsters_gate_natural_monster_spawns() {
        let world = flat_test_world((-3..=3).flat_map(|x| (-3..=3).map(move |z| IVec2::new(x, z))), 63);
        let night = || ctx(vec![Target::new(PlayerId::HOST, DVec3::new(0.5, 64.0, 0.5), true)]);
        let hostile = |spawns: Vec<(MobKind, IVec3)>| spawns.iter().filter(|(kind, _)| kind.is_hostile()).count();
        // Control: the default rules spawn monsters at night.
        let mut on = Entities::new(9);
        assert!(hostile(spawned_cells(&mut on, &world, &night(), 20.0)) > 0);
        // `spawn_mobs` false: `ServerChunkCache.tickChunks` builds no categories.
        let mut off = Entities::new(9);
        let quiet = Ctx { spawning: false, ..night() };
        assert_eq!(hostile(spawned_cells(&mut off, &world, &quiet, 20.0)), 0);
        // `spawn_monsters` false: only the hostile categories are skipped.
        let mut no_monsters = Entities::new(9);
        no_monsters.monster_spawning = false;
        assert_eq!(hostile(spawned_cells(&mut no_monsters, &world, &night(), 20.0)), 0);
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

    /// A headless world whose generator has `biome` near the origin; returns
    /// it with a cell in that biome. Columns are loaded flat around the cell
    /// (stone up to `ground`), so the generator only supplies biomes and the seed.
    fn biome_world(biome: Biome, ground: i32, slime_chunk: bool) -> Option<(World, IVec3)> {
        use crate::world::chunk::CHUNK_SIZE_I;
        for seed in 1..40 {
            let generator = std::sync::Arc::new(crate::world::terrain::Generator::new(seed));
            for k in 0..8 {
                let Some(found) = generator.nearest_biome(IVec2::new(k * 900, 0), biome, 2500) else { continue };
                let (cx, cz) = (found.x.div_euclid(16), found.z.div_euclid(16));
                if super::super::slime::slime_chunk(seed, cx, cz) != slime_chunk {
                    continue;
                }
                let mut world = World::new_headless(generator.clone(), Default::default(), 2);
                let column = IVec2::new(found.x.div_euclid(CHUNK_SIZE_I), found.z.div_euclid(CHUNK_SIZE_I));
                for dz in -1..=1 {
                    for dx in -1..=1 {
                        world.load_test_column(column + IVec2::new(dx, dz), ground);
                    }
                }
                world.update_block_light();
                return Some((world, IVec3::new(found.x, ground + 1, found.z)));
            }
        }
        None
    }

    fn night_rules(moon: f32) -> SpawnRules {
        rules(crate::simulation::lighting::SpawnLight::NIGHT, moon)
    }

    fn slime_rate(world: &World, cell: IVec3, moon: f32, trials: u32) -> f64 {
        let mut rng = Rng::new(31);
        let hits = (0..trials).filter(|_| monster_spot_ok(world, MobKind::Slime, cell, night_rules(moon), &mut rng));
        hits.count() as f64 / trials as f64
    }

    #[test]
    fn swamp_slimes_follow_the_moon_phase_and_light_roll() {
        // Slime.checkSlimeSpawnRules: swamp, 50 < y < 70, nextFloat < moon / 2
        // (`surface_slime_spawn_chance` 0.5 at a full moon, 0 at a new moon),
        // then brightness 4 (sky 15 - night darkening 11) <= nextInt(8): 1/2.
        let Some((world, cell)) = biome_world(Biome::Swamp, 63, false) else { panic!("no swamp near an origin") };
        let near = |got: f64, want: f64| assert!((got - want).abs() < 0.03, "{got} vs {want}");
        near(slime_rate(&world, cell, 1.0, 8192), 0.25);
        near(slime_rate(&world, cell, 0.5, 8192), 0.125);
        assert_eq!(slime_rate(&world, cell, 0.0, 2048), 0.0, "no surface slimes at a new moon");
        // Out of the 50..70 band, or in daylight, only slime chunks can spawn them.
        let low = cell - IVec3::Y * 20;
        assert_eq!(slime_rate(&world, low, 1.0, 2048), 0.0);
        let day = rules(crate::simulation::lighting::SpawnLight::DAY, 1.0);
        let mut rng = Rng::new(5);
        assert!((0..2048).all(|_| !monster_spot_ok(&world, MobKind::Slime, cell, day, &mut rng)));
    }

    #[test]
    fn slime_chunks_spawn_slimes_below_y_40_one_try_in_ten() {
        let Some((mut world, cell)) = biome_world(Biome::Plains, 33, true) else { panic!("no slime chunk plains") };
        let cell = IVec3::new(cell.x, 34, cell.z);
        // The ground is at y 33: a slime chunk below y 40, in a non-swamp biome.
        let rate = slime_rate(&world, cell, 1.0, 20_000);
        assert!((rate - 0.1).abs() < 0.015, "{rate}");
        // Light does not matter for slime chunks.
        let day = rules(crate::simulation::lighting::SpawnLight::DAY, 1.0);
        let mut rng = Rng::new(9);
        let by_day = (0..20_000).filter(|_| monster_spot_ok(&world, MobKind::Slime, cell, day, &mut rng)).count();
        assert!((by_day as f64 / 20_000.0 - 0.1).abs() < 0.015, "{by_day}");
        // y 40 and above is excluded.
        world.set_block(cell + IVec3::Y * 6, Block::STONE);
        for y in 34..=41 {
            world.set_block(IVec3::new(cell.x, y, cell.z), Block::AIR);
        }
        world.set_block(IVec3::new(cell.x, 40, cell.z) - IVec3::Y, Block::STONE);
        assert_eq!(slime_rate(&world, IVec3::new(cell.x, 40, cell.z), 1.0, 2048), 0.0);
        // A chunk that is not a slime chunk spawns none in plains.
        let Some((plain, c)) = biome_world(Biome::Plains, 33, false) else { panic!("no ordinary chunk") };
        assert_eq!(slime_rate(&plain, IVec3::new(c.x, 34, c.z), 1.0, 2048), 0.0);
    }

    fn flood(world: &mut World, cell: IVec3, down: i32, up: i32) {
        for dz in -1..=1 {
            for dx in -1..=1 {
                for y in cell.y - down..=cell.y + up {
                    world.set_block(cell + IVec3::new(dx, y - cell.y, dz), Block::WATER);
                }
            }
        }
    }

    fn roof(world: &mut World, cell: IVec3) {
        for dz in -1..=1 {
            for dx in -1..=1 {
                world.set_block(cell + IVec3::new(dx, 0, dz), Block::STONE);
            }
        }
    }

    fn drowned_rate(world: &World, cell: IVec3, trials: u32) -> f64 {
        let mut rng = Rng::new(77);
        let hits = (0..trials).filter(|_| monster_spot_ok(world, MobKind::Drowned, cell, night_rules(1.0), &mut rng));
        hits.count() as f64 / trials as f64
    }

    #[test]
    fn drowned_spawn_in_water_one_try_in_40_when_deep_and_one_in_15_in_rivers() {
        use crate::world::terrain::SEA_LEVEL;
        // Dark, flooded cells carved into solid stone, sealed from the sky.
        let Some((mut ocean, cell)) = biome_world(Biome::Ocean, 70, false) else { panic!("no ocean") };
        let deep = IVec3::new(cell.x, SEA_LEVEL - 10, cell.z);
        flood(&mut ocean, deep, 6, 3);
        roof(&mut ocean, deep + IVec3::Y * 4);
        let near = |got: f64, want: f64| assert!((got - want).abs() < 0.012, "{got} vs {want}");
        near(drowned_rate(&ocean, deep, 20_000), 1.0 / 40.0);
        // Shallower than sea level - 5 is refused outside rivers.
        let shallow = IVec3::new(deep.x, SEA_LEVEL - 4, deep.z);
        flood(&mut ocean, shallow, 4, 1);
        roof(&mut ocean, shallow + IVec3::Y * 2);
        assert_eq!(drowned_rate(&ocean, shallow, 4096), 0.0);
        // The cell below must be water too: the bottom of the flooded column is not.
        assert_eq!(drowned_rate(&ocean, IVec3::new(deep.x, deep.y - 6, deep.z), 4096), 0.0);
        // A conducting block above refuses (the roof sits above the water column).
        ocean.set_block(deep + IVec3::Y, Block::STONE);
        assert_eq!(drowned_rate(&ocean, deep, 4096), 0.0);
        // Dry land never qualifies.
        let dry = deep + IVec3::Y * 40;
        assert_eq!(drowned_rate(&ocean, dry, 2048), 0.0);

        let Some((mut river, cell)) = biome_world(Biome::River, 70, false) else { panic!("no river") };
        let shallow = IVec3::new(cell.x, SEA_LEVEL - 1, cell.z);
        flood(&mut river, shallow, 4, 3);
        roof(&mut river, shallow + IVec3::Y * 4);
        near(drowned_rate(&river, shallow, 20_000), 1.0 / 15.0);
    }

    #[test]
    fn slimes_and_drowned_belong_to_the_biome_pack_cycle() {
        assert!(cycle_kind(MobKind::Slime) && cycle_kind(MobKind::Drowned));
        assert_eq!(J::Slime.kind(), Some(MobKind::Slime));
        assert_eq!(J::Drowned.kind(), Some(MobKind::Drowned));
        assert!(J::ZombieHorse.kind().is_none() && J::Stray.kind().is_none() && J::Ocelot.kind().is_none());
    }

    #[test]
    fn spawning_waits_for_queued_light_to_settle() {
        let player = Target::new(PlayerId::HOST, DVec3::new(0.5, 64.0, 0.5), true);
        let day = Ctx { daylight: 1.0, spawn_light: crate::simulation::lighting::SpawnLight::DAY, ..ctx(vec![player]) };
        let mut world = cave_world(false);
        let mut e = Entities::new(5);
        assert!(!spawned_cells(&mut e, &world, &day, 30.0).is_empty(), "the settled dark cave spawns");
        // A torch edit leaves queued light until it is resolved.
        world.edit(IVec3::new(41, 40, 0), Block::TORCH, true);
        assert!(!world.light_settled());
        assert!(spawned_cells(&mut e, &world, &day, 30.0).is_empty(), "no spawns on unresolved light");
        assert!(world.update_block_light_within(std::time::Duration::from_secs(1)));
        assert!(world.light_settled());
    }
}
