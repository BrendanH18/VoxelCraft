# Nether biomes

[← Back to VoxelCraft](../README.md) · [Bastions and Nether materials](bastions.md) · [Gameplay](gameplay.md)

VoxelCraft v0.5 gives the Nether Java 1.21's five biomes: nether wastes,
crimson forest, warped forest, soul sand valley and basalt deltas. Each has
its own surface, features, fog colour, ambient particles, music and spawn
list, and brings the crimson, warped and soul blocks with it.

## Biome source

`world::nether_biome` reproduces Java 1.21's Nether multi-noise biome
source. Nether biomes have no vertical variation (the Nether router uses
`shiftedNoise2d`), so each 4×4-block quart column has one biome:

| Biome | Temperature | Vegetation | Offset |
|---|---|---|---|
| Nether wastes | 0.0 | 0.0 | 0.0 |
| Soul sand valley | 0.0 | -0.5 | 0.0 |
| Crimson forest | 0.4 | 0.0 | 0.0 |
| Warped forest | 0.0 | 0.5 | 0.375 |
| Basalt deltas | -0.5 | 0.0 | 0.175 |

The selected biome minimises `Δt² + Δh² + offset²`
(`Climate.ParameterPoint.fitness`). Temperature and vegetation are Java's
`NormalNoise` layouts (temperature octave -10 with amplitudes 1.5, 0, 1;
vegetation octave -8 with 1, 1), sampled at a quarter of the block scale
after the `offset` shift noise (octave -3, amplitudes 1, 1, 1) warps the
coordinates by up to a few quarts. Octave input and value factors,
`NormalNoise`'s second octave set (input factor 1.0181268882175227) and its
expected-deviation normalisation follow `PerlinNoise` and `NormalNoise`.
The gradient noise is the same Improved Perlin noise, so climate values have
Java's scale and spread; permutation tables come from VoxelCraft's seed
hashing, so layouts are not copies of Java seeds.

Because temperature starts at octave -10 on a quarter scale, Nether biome
regions are large: in tests the mean run along a line is roughly a
kilometre, wastes are the most common biome and warped forests the rarest.

Lookups: `Generator::nether_biome(x, z)` (gameplay), `/locate biome
<name>` (32-block ring search like Java; the result keeps the caller's height, which is what Java's 3D search returns here because the Nether noise router sets `y_scale` 0 for temperature and vegetation, pinned in the lighting fixture), the
F3 screen (`Biome: minecraft:crimson_forest (nether)`) and per-biome music
(`Situation::nether`). Bastion remnants skip regions whose start chunk
centre is in a basalt delta (`#has_structure/bastion_remnant`).

## Spawn costs

Java biomes carry `spawn_costs` (the `natural_mob_spawns` attribute): soul sand
valleys charge 0.7 with a budget of 0.15 for endermen, ghasts, skeletons and
striders, and warped forests charge endermen 1.0 against 0.12. `Entities` applies
`NaturalSpawner.SpawnState.canSpawn` to natural Nether spawns (pack leaders, pack
members and fortress spawns): each non-persistent mob with a cost in its own biome
adds `charge / distance`, and a new mob needs `charge x potential <= budget`.
A skeleton therefore keeps other charged mobs about 3.3 blocks away in a soul sand
valley, and warped forest endermen need nine blocks. Java rebuilds the field once
per spawn pass from every loaded mob; VoxelCraft sums the live mobs per attempt.

## Surfaces

`NetherGen::classify` applies the biome's surface rules (after Java's
`SurfaceRuleData.nether`) to each block, using how many solid blocks lie
between it and the open space above (floor depth) and below (ceiling
depth), and a surface depth of 3–5 blocks that varies by column. The five
layers under the bedrock roof stay netherrack everywhere.

| Biome | Floors | Ceilings | Rock |
|---|---|---|---|
| Nether wastes | soul sand patches and gravel shores (unchanged) | netherrack | netherrack, quartz |
| Crimson forest | crimson nylium on the top block above y = 31, nether wart block patches, bare netherrack patches | netherrack | netherrack, quartz |
| Warped forest | warped nylium, warped wart block patches, bare patches | netherrack | netherrack, quartz |
| Soul sand valley | soul sand or soul soil (state-selector noise) | soul sand or soul soil | netherrack, quartz |
| Basalt deltas | blackstone or basalt | basalt | basalt and blackstone blobs in netherrack, twice the quartz |

Basalt-delta blobs (Java's `ReplaceBlobsFeature` for basalt and blackstone)
are value noise hashed on the density grid and interpolated, so they cost a
few lerps per block instead of hundreds of blob tests.

Generation 7 (v1.0 gap Track 5) replaces this table with Java 26.3's own
rules; see [below](#generation-7-javas-surface-rules-and-decoration-order-v10).

## Features

`world::nether_features` ports the feature classes and
`world::nether_decoration` places them per Java 16×16 cell:

| Biome | Feature (placement) | Java class |
|---|---|---|
| Crimson forest | huge crimson fungi (8 on every floor layer) | `HugeFungusFeature`: stems 4–13 (doubled 1 in 12), wart hats with shroomlight, weeping vines under 1 in 10 hat blocks, 6% giant 3×3 stems |
| | vegetation (6 per layer): crimson roots 87, crimson fungus 11, warped fungus 1 | `NetherForestVegetationFeature` (8 wide, 4 high) |
| | weeping vines (10 at uniform heights) | `WeepingVinesFeature`: a wart-block patch on the ceiling with vines 1–8 (×2, or 1) long |
| Warped forest | huge warped fungi (8 per layer) | `HugeFungusFeature` with warped wart and shroomlight |
| | vegetation (5 per layer): warped roots 85, crimson roots 1, warped fungus 13, crimson fungus 1 | `NetherForestVegetationFeature` |
| | nether sprouts (4 per layer) | `NetherForestVegetationFeature` |
| | twisting vines (10 at uniform heights) | `TwistingVinesFeature` (8, 4, max 8) |
| Soul sand valley | basalt pillars (10 at heights 10–117) | `BasaltPillarFeature` |
| | soul fire patches (0–5 at heights 4–123) | `RandomPatchFeature` of soul fire on soul soil |
| | fossils (one start per 2×2 cells) | the `nether_fossil` structure |
| Basalt deltas | deltas (40 per layer) | `DeltaFeature`: lava pools 3–7 wide with 0–2 magma rims set into the floor |
| | small and large basalt columns (4 and 2 per layer) | `BasaltColumnsFeature`, also rising out of the lava sea |

Placement follows `CountOnEveryLayerPlacement` (a new random column per
attempt and floor layer, from the top down, until a layer finds no floor)
or a count at uniform heights. Fungus age, vine ages and all shapes use the
Java constants.

Chunks are generated independently, so every feature start and its
randomness are pure functions of the seed and its cell, and every feature
reads only the undecorated terrain (a `Probe` over the shared grid
columns). Each chunk replays the features that can reach it — culled by a
per-feature bounding box, with each attempt seeded separately so culling
never shifts the others — and paints its own blocks in a fixed global order
where the first write wins. Neighbouring chunks therefore agree at their
borders. Structures (fortresses, bastions) are painted afterwards and cut
through features.

Full-height grid columns (densities every 4 blocks, blob values and the
biome) live in a bounded cache shared by all chunks, together with the
solidity of their block columns as 128-bit masks. The mask is the same
trilinear interpolation as before, evaluated exactly only in the grid cells
whose ends straddle zero, so terrain is unchanged and the four chunks of a
column and every feature reaching it scan each column once.

## Blocks (1600..=1793)

| Ids | Blocks |
|---|---|
| 1600–1601 | crimson and warped nylium |
| 1602–1613 | crimson, warped and stripped stems (Y, X, Z axes) |
| 1614–1617 | crimson, warped and stripped hyphae (Y axis) |
| 2500–2507 | the same four hyphae on the X then Z axis (reserved range; bark on every face, so they look alike) |
| 1618–1622 | crimson and warped planks, nether and warped wart blocks, shroomlight |
| 1623–1627 | crimson and warped fungus and roots, nether sprouts |
| 1628–1633 | soul soil, soul fire, soul torch, bone block (3 axes) |
| 1634–1687 | weeping vines (ages 0–25, then the plant), twisting vines likewise |
| 1688–1733 | crimson and warped stairs, slab, fence, fence gate, door (`world::forms` woods 7 and 8) |
| 1734–1793 | crimson then warped trapdoor, button and pressure plate |

Texture layers are 1300–1342 (soul fire's seven frames animate on the GPU
like fire). No new item ids were needed: every addition is a block item.

Rules, all from Java's block properties, tags and loot tables:

- Nylium (0.4, pickaxe) drops netherrack unless mined with silk touch, and
  turns back into netherrack under a light-blocking block.
- Stems, hyphae, planks and shapes (2.0, axe; doors and trapdoors 3.0)
  never burn and are not furnace fuel. Axes strip stems and hyphae.
- Wart blocks and shroomlight (1.0) mine fastest with a hoe. Shroomlight
  emits 15; soul fire and soul torches 10.
- Fungi, roots and sprouts stand on nylium, soul soil or dirt-like blocks;
  roots and sprouts are replaceable; sprouts need shears.
- Vines are climbable. Weeping vines hang from a sturdy block or more
  vines and fall all the way down without it; twisting vines stand on
  one. Vines drop a third of the time (55/77/100% with Fortune), always
  with shears or silk touch. Heads and plant blocks swap as vines grow or
  are cut.
- Soul fire only stays lit on soul sand or soul soil, never ages or
  spreads, and burns for 2 damage. Flint and steel, lava and spreading fire
  light soul fire over soul blocks.
- Crimson/warped trapdoors, buttons and plates mirror the oak redstone
  states (`switch_oak`/`keep_wood`), so every redstone rule applies.

Recipes: stems, stripped stems and hyphae → 4 planks; 4 stems → 3 hyphae
(also stripped); stairs, slabs, fences, gates, doors, trapdoors, buttons
and plates from each wood's planks; Nether planks also count as planks
(sticks, crafting tables, chests, tools); 9 nether wart → nether wart
block; coal or charcoal + stick + soul sand or soil → 4 soul torches;
9 bone meal ↔ bone block.

## Growth and bone meal

- Weeping and twisting vine heads grow one block on 10% of random ticks
  until age 25. Bone meal grows 1 block plus more with 82.6% each, ageing
  as they go (`NetherVines`).
- Bone meal on nylium sprouts its forest floor (`*_forest_vegetation_bonemeal`
  3 wide; warped adds sprouts and, 1 time in 8, twisting vines); on
  netherrack beside nylium it turns the netherrack into that nylium (a coin
  toss when both touch it).
- Bone meal on a fungus over its own nylium grows a planted huge fungus 40%
  of the time.

## Atmosphere

| Biome | Fog / sky | Ambient particle (chance per sample) | Music |
|---|---|---|---|
| Nether wastes | `#330808` | — | nether_wastes |
| Crimson forest | `#330303` | crimson spore, 0.025 | crimson_forest |
| Warped forest | `#1A051A` | warped spore, 0.01428 | warped_forest |
| Soul sand valley | `#1B4745` | ash, 0.00625 | soul_sand_valley |
| Basalt deltas | `#685F70` | white ash, 0.118 | basalt_deltas |

The fog colour is blended like Java's `FogRenderer`: a cubic Gaussian
(`CubicSampler`, kernel 1-4-6-4-1) over the 6×6 quarts around the camera,
cached until the camera changes quart. Every split-screen view keeps its own cache and
samples around its own eye. Particles follow Java's providers
(see [particles](particles.md)); soul torches burn with the blue soul fire
flame. Music uses the existing procedural Nether palettes.

## Mob spawning

Nether spawning uses each biome's Java spawn list. An attempt for a mob
goes ahead with its weight against the heaviest entry in the biome's list,
and groups follow the list's sizes:

| Biome | Monsters (weight, group) |
|---|---|
| Nether wastes | zombified piglin 100 (4), ghast 50 (4), piglin 15 (4)*, magma cube 2 (4), enderman 1 (4) |
| Soul sand valley | ghast 50 (4), skeleton 20 (5), enderman 1 (4) |
| Crimson forest | hoglin 9 (3–4)*, piglin 5 (3–4)*, zombified piglin 1 (2–4) |
| Warped forest | enderman 1 (4) |
| Basalt deltas | magma cube 100 (2–5), ghast 40 (1) |

Every biome also lists striders 60 (1–2)* as creatures. Entries marked *
are the [Nether mobs](nether-mobs.md); `entity::nether_mob` maps each list
entry to its `MobKind`, and striders spawn on the lava sea at weight 60
against zombified piglins' 100. Fortress spawning is unchanged.

## Performance

Measured on 2026-10-08 on an Apple M5, release build, seed 12345:
`--bench --rd 8` gave 0.259 ms per Overworld chunk, 0.216 ms per Nether
chunk at the benchmark origin, 0.684 ms per dense chunk for light+mesh,
and 0.18 s for streaming. Origin throughput remains close to v0.4.

Feature-heavy biomes cost more. Three fresh generators each generated
100 chunks (5×5 columns, all four vertical Nether chunks), with a median
of 0.234 ms per chunk in the wastes, 0.275 in soul sand valleys, 0.417 in
crimson forests, 0.370 in warped forests and 2.360 in basalt deltas.
The patches were centered at (-2560, -4480), (-4096, -4480),
(-4480, -4480), (1216, -3520) and (-1024, -4480), respectively, for seed
12345. Biome lookup was outside the timer. Basalt feature generation is
a performance hotspot; the ordinary benchmark origin understates its cost.

v0.7.0 fix: a profile (macOS `sample`) of the delta patch put nearly all decoration time in
`Probe::terrain` reads: 117k reads and 214 feature runs per chunk, 71% of the reads from
large basalt columns. Features still replay for every chunk they can reach, but now skip
work that only decides blocks outside the chunk being painted: a column cell outside the
chunk's x/z range is skipped (it reads and writes only its own column) and a delta cell
is skipped unless the chain `p, p + r, p + 2r, …` of its rim/pool writes reaches the chunk
(a cell's outcome depends on earlier cells having written those blocks). A test
(`culling_features_to_the_chunk_changes_nothing`) paints 56 chunks in the delta patches and
other biomes with and without the culling and compares every block. Reads per delta chunk fell
from 11.7M to under 5M per 100 chunks, and `cargo run --release --example nether_patch_bench`
(same patches as the table above, seven fresh generators each) measured the delta patch 1.7–1.9×
faster than the previous build, interleaved on the same loaded machine (best of seven: 4.7 → 2.6,
4.8 → 2.9 and 5.4 → 3.2 ms/chunk at load average ~20; the unloaded figure was 2.16 ms before
and ≈1.2 ms after on an earlier quiet run). The other four biomes are unchanged. Basalt deltas
remain the most expensive biome because Java's feature counts (40 deltas and 6 column
clusters per 16×16 cell and layer) are fixed.

## Generation 6: Java 26.3's placed features (v1.0)

Worlds made from v1.0 on (generation 6) decorate the Nether with every
placed feature Java 26.3's five biomes list, as data: `world::nether_placed`
copies each one's placement modifiers (`count`, `count_on_every_layer`,
`in_square`, height ranges, `heightmap`, `biome`, `rarity_filter`,
`offset`, block predicate filters and `environment_scan`) and its configured
feature, and `nether_decoration` runs them as `PlacedFeature` does. Over
generation 5 this adds or changes:

- ores replacing netherrack (`OreFeature`): magma (4 at 27–36, size 33),
  gravel (2 at 5–41) and blackstone (2 at 5–31), soul sand in soul sand
  valleys (12 at 0–31, size 12), nether gold (10, or 20 in basalt deltas,
  size 10) and quartz (16, or 32 in deltas, size 14) at 10–117. They replace
  the noise quartz and gold of earlier generations;
- glowstone as Java's neighbour spread (1,500 tries below a ceiling, with
  `glowstone_extra`'s 0–9 and `glowstone`'s 10 a chunk), replacing the
  noise glowstone;
- lava springs: open (8 at 4–123), closed (16, or 32 in deltas, at
  10–117) and the deltas' own (16); open springs start flowing when their
  chunk loads;
- fire on netherrack and soul fire on soul soil in every biome that lists
  them, brown and red mushrooms (1 in 2 chunks in the wastes and deltas, and
  Java's 1 in 256 and 1 in 512 on the bedrock roof of the wastes and
  forests), crimson roots in soul sand valleys;
- basalt and blackstone blobs in basalt deltas (`ReplaceBlobsFeature`, 75
  and 25 a chunk), replacing the noise blobs;
- 26.3's vegetation, sprouts and vines: plants scattered 64 times around a
  nylium spot (trapezoid offsets), weeping vines as a wart spread and a
  hundred vine columns, twisting vines as columns found by a downward scan,
  each a plant layer of weighted length and a head aged 17–25.

The overworld lava spring Java lists for four Nether biomes never places
(none of its valid blocks is Nether rock). Ancient debris keeps its own
pass, which already followed Java. Features still read the undecorated
terrain and paint first-write-wins; the huge fungi and vines paint before
the rest so they stay whole (Java's stems replace vines, and Java's vines
skip a spot a plant or fire took). `nether_generation_tests` checks the
table, the biome source and the Nether structure sets against
`tests/fixtures/java/26.3/nether_generation.json` both ways
([record](verification/2026-11-25-v1.0-track-08-generation.md)).

## Generation 7: Java's surface rules and decoration order (v1.0)

Worlds made from the v1.0 gap-closing build on (generation 7,
`worldgen=7`) keep generation 6's noise, biomes and placed features and
change two things ([record](verification/2026-11-27-v1.0-gap-track-05-nether-portals.md);
structures and ancient debris in their steps:
[Track 5b's record](verification/2026-11-27-v1.0-gap-track-05b-nether-structures-portals.md)).

**Surfaces** follow 26.3's `worldgen/material_rule/nether.json`
(`world::nether_surface`), applied as `MaterialSystem.buildSurface` does to
every rock block. A block's stone depth above counts the rock up to the
first air (lava doesn't end it, so the floor under the lava sea is a
floor); its depth below counts down to the first air or lava. Each column
has a surface depth, `surface` noise × 2.75 + 3 plus a random quarter,
truncated, and is a hole where that is 0 or less. Then, in order:

| Where | Block |
|---|---|
| bedrock floor and roof (y 0–4 and 123–127, thinning) | bedrock |
| y 122 and up | netherrack |
| basalt deltas, under a ceiling (depth below ≤ 1 + surface depth) | basalt |
| basalt deltas, under a floor (depth above ≤ 1 + surface depth) | gravel where `patch` ≥ -0.012 and the floor's surface is at y 30–34, else basalt or blackstone by `nether_state_selector` |
| soul sand valleys, under a ceiling | soul sand or soul soil by the state selector |
| soul sand valleys, under a floor | gravel as in the deltas, else soul sand or soul soil |
| a floor's top block below y 32 in a hole | **lava**: the ragged edges of the lava sea |
| a crimson or warped floor's top block from y 31 where `netherrack` < 0.54 | wart block where `nether_wart` ≥ 1.17, else nylium |
| nether wastes, under a floor where `soul_sand_layer` ≥ -0.012 | soul sand where the surface is at y 30–34 and not a hole, else netherrack |
| nether wastes, a floor's top block from y 31 with its surface below 35 where `gravel_layer` ≥ -0.012 | gravel (at y 31 only outside holes) |
| anything else | netherrack |

So the wastes' soul sand and gravel now lie only at the lava sea's level,
as in Java, and the forests' bare patches come from the `netherrack`
noise. The noises keep Java's octaves and amplitudes (26.3 stores them as
the 1.21 layouts' parity parameters) with VoxelCraft's permutations
(FE-01); the biome is still the plain quart's, as for every other surface
and feature.

**Features** are placed one 16 × 16 Java chunk (a cell) at a time, as
`ChunkGenerator.applyBiomeDecoration` does (`world::nether_order`): over
the 3 × 3 cells around it, which it may write (writes beyond are dropped,
as `WorldGenRegion` drops them), step by step, every placed feature a biome
of those nine cells lists, in the order `FeatureSorter` gives the five
biomes (pillars; deltas and columns; blobs, springs, fire, glowstone,
mushrooms, ores; then the overworld spring and mushrooms, the warped
forest's fungi, vegetation, sprouts and twisting vines, and the crimson
forest's weeping vines, fungi and vegetation). Each placed feature runs
its modifiers as `FeaturePlacer` does, and every modifier and feature
reads the cell's region as the earlier ones left it. So glowstone spreads
from glowstone of either glowstone feature, ores replace only the
netherrack the blobs left, fire needs the floor deltas left, twisting vines
skip spots the warped vegetation took (they are rarer than in generation
6), and huge fungi grow among the vines that came before them.

**Structures** go in their own steps (v1.0 gap Track 5b), before that
step's features, as `applyBiomeDecoration` places them: bastions and the
Nether's ruined portals in step 4 (`surface_structures`, before deltas and
columns), fortresses and Nether fossils in step 7 (`underground_decoration`,
before the blobs, fire, glowstone and ores), each step's in the structure
registry's order (sorted by id). Each structure's pieces are clipped to
the cell being decorated (`getWritableArea`: its 16 × 16 columns from y 1);
a ruined portal goes whole into the cell holding its middle
(`RuinedPortalPiece.postProcess`), its processors, netherrack spread and
drips reading that cell's region. Structures read the region as earlier
steps left it (fortress pillars stop on what is under them then) and later
features read them: glowstone hangs from bastion blackstone, fire lies on
a portal's netherrack, ores and blobs pass the bricks by. Where two cells'
regions overlap, a block a structure changed wins over the other cell's
change there, Java's result when the structure's chunk was decorated
after the other (its pieces replace what they cover).

**Ancient debris** is the step 7 `scattered_ore` it is in Java
(`ore_ancient_debris_large`, then `ore_debris_small`, after the other
ores): 0 to 3 (or 2) tries, each up to `min(i, 7)` blocks from the
origin on every axis, replacing netherrack, basalt or blackstone
(`#base_stone_nether`) with no air beside it as the cell's region stands
then, so it can sit in basalt and blackstone blobs and in bastion walls,
and never beside a fortress corridor placed before it.

Each cell is decorated once into a bounded cache of what it changed (at
most 4 million blocks, 32 MB; a basalt delta cell changes about 17,000,
others a few hundred to 2,000, plus what its structures change), and a
32³ chunk takes the changes of the sixteen cells whose regions reach it:
first those structures made, then the rest. Where two regions overlap,
Java's result depends on which chunk the server decorated first (the
later one sees the earlier one's blocks); here neither sees the other's,
and the cell first by z, then x, keeps its block (a structure's block
first, as above).

Generation 7 costs about 0.45 ms more per Nether chunk than generation 6
(`nether_patch_bench`, 100 chunks a patch on Boat: wastes 0.45 → 0.90 ms,
soul sand valley 0.44 → 0.89, crimson forest 0.53 → 0.76–0.94, warped
forest 0.58–0.67 → 1.00–1.24, basalt deltas 2.13–2.21 → 2.67–2.76), on the
generation threads.

## Known gaps

- Biome layouts are statistically Java's but not seed-for-seed copies.
  Java's fuzzy biome zoom (`BiomeManager.getBiome`: the nearest of the eight
  surrounding quarts after a seed-hashed jitter) is applied to gameplay
  queries (mob spawn lists and costs, the F3 biome line, music). Terrain
  surfaces, feature placement, fog (Java's fog reads plain quarts) and the
  ambient-particle window still use the plain 4×4 quart grid, so a nylium
  border and the spawn-list border can differ by a few blocks.
- Generations 1 to 6: features see only undecorated terrain, so later
  features don't react to earlier ones as in Java (painting order keeps
  fungi and vines whole), and structures paint after them. Generation 7
  places features and structures in Java's order on decorated terrain;
  where two Java chunks' regions overlap, neither sees the other's
  features or structures (Java's outcome there depends on load order).
- Fossils are procedural skeletons, not Java's fourteen templates.
  Generations 1 to 5 lack lava springs, fire patches in the wastes and
  deltas and the soul sand/gravel/blackstone/magma ore blobs (generation 6
  places them); generations 1 to 6 lack the gravel and lava edges of
  floors near the lava sea and use the original surface table above
  (generation 7 follows Java's surface rules).
- Soul lanterns (no lanterns yet), signs, the soul fire HUD
  overlay (the Java client code is not in the reference jar to check) and wall
  torches are absent. Every mob inside a ladder or vine slides at most 0.15
  blocks per tick and climbs at 0.2 while pressing a wall (Java
  `LivingEntity.handleOnClimbable`); ground paths don't route up vines.
- Worlds saved before v0.5 keep their generated Nether chunks; chunks
  generated next to them use the biomes, so old areas meet new terrain at a
  seam.

## What to check visually

Textures of every new block (nylium side fringe, stems and stripped stems,
fungi, roots, sprouts, vines, wart blocks, shroomlight, soul soil, soul
torch, bone block, doors and trapdoors in hand and inventory); soul fire's
animation and brightness; the five fog colours and their blending at
biome borders; ambient particle density and colour; huge fungi, vine
clusters, basalt pillars, fossils, deltas and columns in each biome; and
the F3 biome line.
