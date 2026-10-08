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
<name>` (32-block ring search like Java, keeping the caller's height), the
F3 screen (`Biome: minecraft:crimson_forest (nether)`) and per-biome music
(`Situation::nether`). Bastion remnants skip regions whose start chunk
centre is in a basalt delta (`#has_structure/bastion_remnant`).

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
| 1614–1617 | crimson, warped and stripped hyphae |
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
cached until the camera changes quart. Particles follow Java's providers
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

Headless `--bench --rd 8` on the development container (4 cores, noisy;
medians of several runs): Overworld generation is unchanged (0.65–0.70 ms
per chunk there), light+mesh unchanged, and the Nether line the benchmark
now prints stays at about 0.6 ms per chunk around the bench origin, where
the shared column cache pays for the surface rules. Feature-dense biomes
cost more: about 1.0 ms per chunk in crimson forests and 1.2 ms in basalt
deltas (single thread, cold caches), against 0.7 ms in the wastes.

## Known gaps

- Biome layouts are statistically Java's but not seed-for-seed copies;
  Java's fuzzy biome zoom is not applied, so borders follow the 4×4 quart
  grid.
- Hyphae store no axis (their bark texture is the same on every face here,
  as textures are never rotated).
- Features see only undecorated terrain, so later features don't react to
  earlier ones as in Java (painting order keeps fungi and vines whole).
- Fossils are procedural skeletons, not Java's fourteen templates.
  Missing Nether features: lava springs, fire patches in the wastes and
  deltas, soul sand/gravel/blackstone/magma ore blobs, nether gold ore (no
  block yet), and the gravel and lava edges of soul sand valley and delta
  floors near the lava sea.
- Soul lanterns (no lanterns yet), signs, Soul Speed, the soul fire HUD
  overlay and wall torches are absent; vines are climbable for players
  only.
- `/locate biome` keeps the caller's height rather than searching in 3D.
- Java's spawn costs (soul sand valley's charge per mob) aren't modelled.
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
