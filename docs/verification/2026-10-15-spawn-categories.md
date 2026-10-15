# Creature, water and structure spawn categories — 2026-10-15

[Roadmap](../roadmap.md) · [Natural spawning and despawning (2026-10-14)](2026-10-14-natural-spawning.md) · [Reference fixtures](../../tests/fixtures/java/26.3/README.md)

Scope: the rest of MOB-01 on `t3/v0-8-release`. Reference: official Java Edition
26.3 server SHA-1 `33680f5f2ac32864d6d7cf5e56a705fdb3e05f4c`: `NaturalSpawner`
(`spawnForChunk`, `spawnCategoryForPosition`, `spawnMobsForChunkGeneration`),
`SpawnPlacements`, biome spawn attributes, `StructureSpawnOverride` and
`Animal.removeWhenFarAway`. The biome lists and block tags are extracted into
[`spawn_categories.json`](../../tests/fixtures/java/26.3/spawn_categories.json).

## Delivered behavior

- **One category pass:** [`entity/spawning.rs`](../../src/entity/spawning.rs)
  replaces the per-kind spawner (24–64 block ring, per-kind caps) with Java's
  per-tick order. Each tick it builds fresh counts, filters categories by global
  cap once, shuffles the spawn chunks, then tries each category per chunk.
  Creatures run only every 400 ticks. Peaceful allows only friendly categories.
  Each start and every position a pack walks to needs an entity-ticking chunk
  (`canSpawnEntitiesInChunk`), so spawns follow the simulation distance. Every chunk within eight still counts toward the cap. The Nether and End use
  the same pass, so the separate fortress spawner is gone.
- **Spawn lists:** [`entity/spawn_lists.rs`](../../src/entity/spawn_lists.rs)
  holds every Overworld biome's creature, ambient, water creature, water
  ambient, underground water and axolotl list, plus the Nether biome and End
  lists. Mobs VoxelCraft lacks stay in the lists with their Java weights:
  polar bear, bat, nautilus, mooshroom, frog, armadillo and panda. Picking one
  ends that pack without spawning, so the frequencies of present mobs stay
  correct. Rivers skip 98% of water-ambient packs.
- **Structure overrides:** `spawning::spawn_list` uses these lists inside
  structures:
  - Swamp huts: witches (monsters) and cats (creatures).
  - Ocean monuments: guardians, with no underground water creatures or axolotls.
  - Pillager outposts: pillagers.
  - Nether fortresses: the fortress list.

  VoxelCraft's structure spawn bounds stand in for Java's piece or structure
  bounds. This replaces the old 15% outpost and swamp-hut resident timer.
- **Placement rules:** [`entity/spawn_rules.rs`](../../src/entity/spawn_rules.rs)
  ports `SpawnPlacements` for every present mob:
  - **Ground animals** need their tagged ground block, with raw brightness above
    8. That holds even at night, as in Java.
  - **Water mobs:** fish and squid need water above and below, within 13 blocks
    under sea level. Tropical fish may also spawn in lush caves.
  - **Underground water:** glow squid need fully dark water 33 or more blocks
    below sea level. Axolotls need a clay floor and no redstone conductor above.
  - **Guardians:** water below, plus a 1-in-20 roll or no sky.
  - **Nether:** striders spawn on lava with air above it. Zombified piglins,
    piglins and hoglins avoid nether wart blocks. Ghasts get a 1-in-20 roll.
  - **End:** monsters spawn in darkness.

  Packs cap at Java's cluster size: 4 by default, 8 for wolves, 1 for ghasts.
  Members share Java's `SpawnGroupData`: one horse coat, llama colour or rabbit
  type per pack. Each horse still rolls its own markings.
- **Generation herds:** `spawn_mobs_for_chunk_generation` gives each new
  16-block Overworld column Java's one-time creature herds. Biome probability,
  group size and up to four placement tries per member come from Java. The herd
  stays inside its chunk, and horses in a herd share a coat. Each column's herd
  uses its own random stream seeded from the world seed and column, as Java's
  `setDecorationSeed` does, so it doesn't depend on exploration order. The
  `Rng` isn't Java's, so the herds themselves differ from Java's. A column is
  populated once it and its eight neighbouring storage columns are sky-lit,
  because herd placement reads skylight. The pass runs even with
  `doMobSpawning` off, since Java does it at chunk generation. VoxelCraft
  doesn't save untouched chunks; it regenerates them. A saved set of populated
  columns therefore stops herds from coming back after a reload or after
  they're killed.
- **Persistence:** no natural spawn is persistence-required, so each counts
  toward its category cap. That includes axolotls and hoglins. Creatures never
  despawn by distance, as in `Animal.removeWhenFarAway`. Breedable mobs are
  still saved as before. Saves now record `persistence_required`. Older saves
  default it to true, which keeps their mobs.
- **Parrots** may spawn on leaves, as Java's leaves `isValidSpawn` allows.
- **Spectators** neither create spawn chunks nor stop nearby mobs despawning.

### Older saves

A pre-v0.8 save has no populated-column markers. Without them, every loaded
column would gain a second herd on top of the animals the old spawner left. On
load, columns within two of each saved animal therefore count as populated, and
columns farther out still get generation herds. Explored columns with no
animals nearby can still gain a herd; nothing is removed.

## Evidence

- `spawn_lists::friendly_biome_lists_match_the_official_26_3_attributes`: every
  biome and category list matches the extracted fixture.
- `spawn_rules` unit tests cover:
  - Ground tags and the brightness-above-8 rule.
  - Water height bands.
  - Glow squid depth and darkness.
  - Lush-cave tropical fish.
  - Axolotl clay floor and conductor check.
  - Parrots on leaves.
- Unit tests also check:
  - Natural axolotls, hoglins, cows and zombies don't require persistence.
  - Breedable ones survive save and reload.
  - On a real `World`, only columns with lit neighbours populate
    (`generation_herd_columns_wait_for_lit_neighbours`).
- `tests/natural_spawning.rs`:
  - Creatures appear only every 400 ticks and stop at the cap of 10, out to
    128 blocks.
  - Generation herds exceed the runtime cap, survive save and reload, never
    distance-despawn, and don't come back to a cleared column.
  - Spectators drive no spawning.
  - Pack members stay in entity-ticking chunks. The test fails without the check.
  - Generation herds appear with `doMobSpawning` off and are identical for two
    sessions with different random states.
  - A save without markers keeps its herd without doubling it, while columns
    farther out still populate.
- The fortress list test now reads the shared `spawn_list`.
- The full `cargo fmt && cargo clippy --release --all-targets -- -D warnings &&
  cargo test --release` gate passed.
- **Headless smoke run** (`examples/headless`, default seed, 200 ticks, three
  runs each). The first column is `t3/v0-8-release` before this change:

  | Metric | Before | After |
  | --- | --- | --- |
  | Mobs | 88 (16 animals) | 155 (73 animals, 12 squid and glow squid) |
  | Whole tick | 0.57 ms | 0.76 ms |
  | Entity update | 0.08 ms | 0.27 ms |

  After the review fixes, frontier columns that had been marked before their
  skylight existed populate too. That raised the run to 186 mobs at 0.86 ms
  per whole tick.

  Temporary instrumentation (not committed) split the spawning cost:
  - **Before the fixes below:** about 30 µs for counts and about 150 µs for
    pack attempts per tick. Most of that was 200 attempts each for the ambient,
    axolotl and water-ambient categories, each needing a few uncached
    generator biome-noise lookups.
  - **Fixes:** skipping the ambient pass, which holds only bats, and resolving
    one list per pack step. Together they cut entity-update time from 0.31 to
    0.27 ms. The rest is the larger animal population's AI.
  - **Guard test:** `ambient_lists_hold_no_existing_mob_so_the_pass_can_be_skipped`
    fails once bats exist, so the skip can't outlive them.

## Remaining differences and open work

- Not played by hand: herd density across biomes, fish in oceans and rivers,
  axolotls in lush caves, and witches and cats at swamp huts all still need an
  in-game look.
- Biome lookups during spawn attempts sample generator noise. Java reads stored
  chunk biomes, which is cheaper. A per-chunk biome cache would remove most of
  the remaining spawn cost.
- Bats, polar bears, mooshrooms, frogs, armadillos, pandas and nautiluses don't
  exist yet, so their list entries spawn nothing.
- Java's `CatSpawner` isn't implemented. That spawner replaces swamp-hut and
  village cats. Natural cats use `Animal.checkAnimalSpawnRules` (grass ground)
  as in Java, so a swamp hut's cat isn't replaced on its plank floor.
- Structure overrides use VoxelCraft's structure bounds, not Java's per-piece
  boxes. Fortress checks use `in_fortress`.
- Spawn eggs, spawners and breeding are unchanged.
