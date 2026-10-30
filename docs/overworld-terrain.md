# Overworld terrain (v0.6)

VoxelCraft 0.6 replaced the Overworld generator with one modelled on Java
Edition 1.18 and later. The world runs from y = -64 (bedrock) to y = 319,
with the sea at y = 63. The Nether and the End keep y = 0..255.

## Climate and biomes

Five 2D noise fields stand in for Java's climate parameters:
temperature, humidity (vegetation), continentalness, erosion and
weirdness. They are scaled so their values spread over Java's `-1..1`
bands the way the vanilla noise does. `world::biome::pick` follows Java's
`OverworldBiomeBuilder`: temperature and humidity choose a row of the
middle, plateau and shattered tables; continentalness picks ocean, coast
or how far inland; erosion picks the kind of land; and weirdness picks
variants and, through peaks-and-valleys, rivers and mountain tops.

There are 53 biomes with Java's names (`/locate biome cherry_grove`):
nine oceans, mushroom fields, two rivers, three shores, plains and
their variants, forests (flower, birch, old growth birch, dark, pale
garden), taigas (snowy, old growth pine and spruce), swamps and
mangrove swamps, desert, savannas, jungles (sparse, bamboo), badlands
(wooded, eroded), mountain biomes (meadow, cherry grove, grove, snowy
slopes, jagged, frozen and stony peaks), windswept hills, and the lush
and dripstone cave biomes. In a seeded sample about 30% of the world is
ocean.

## Generations

Worlds record the generator version that made them as `worldgen=<n>` in
`level.txt`, because unmodified chunks are never saved and are regenerated
on every load. Worlds saved before v0.13 have no key and keep
**generation 1**: the climate heightmap described below, pinned by the
golden chunk hashes. New worlds use **generation 2** (v0.13): density
terrain, with caves fitted to it. Every v0.13 generation change applies to
generation 2 only.

## Shape

`world::climate` builds a base surface height from the same parameters,
like Java's terrain splines. Continentalness lifts land out of 30-block-deep
oceans; low erosion with high peaks-and-valleys raises mountains up to
about y = 250, with jagged ridges where weirdness is negative; mid
erosion inland makes flat-topped plateaus; valleys cut rivers down to
just under the sea; very high erosion flattens swamps to sea level; and
windswept ground is broken and lumpy. Badlands terrace their cliffs and
eroded badlands grow hoodoos.

In generation 1 that height is the terrain: a heightmap, with no
overhangs on the surface.

In generation 2 (`world::terrain::density`) the height is the base of a 3D
density field. A block is solid where the surface height seen at that
block's own height, `base + relief × noise(x, y, z)`, lies at or above it.
The relief (`ClimateNoise::relief`) comes from the climate too: up to about
45 blocks in low-erosion mountains and jagged peaks, around 20 on
windswept ground, at least 10 in badlands above the sea, about 10 on
rugged inland hills, and a block or two in oceans, plains, swamps, rivers
and on the ground just above the sea, so shores, river beds and sea level
keep their character. Where the relief is larger than the noise's
vertical scale the surface leans over itself, giving overhangs, cliffs,
arches, ledges and steep spires on the peaks; badlands terrace the 3D
shape, so their mesas step and overhang with the strata running through
them. Above the base height the noise reaches only 0.65 as far and is
squashed the higher it goes, so its highs don't leave rock floating clear
of the ground. The noise is original (FE-01) and sampled on a grid every 4 blocks across and
8 up, interpolated, and only within the band of heights the relief can
reach; nodes sit at world coordinates, so a chunk is the same whichever
order or thread generates it.

A column's height (`Generator::column`) is still its topmost solid block,
so structures, villages, spawn, `/locate` and mob spawning work as before.
Generation 2 works it out from the density field of the whole chunk
column, which is cached.

Surface rules run on every face open to the air above, not just the top
of the column: the biome's top block and filler (dirt, sand and sandstone,
snow, terracotta) lie under each face, including floors under
overhangs; faces steeper than the generation 1 rule allows (measured from
the density gradient, so leaning and overhanging faces count) are bare
stone in mountains and on stony shores; and the underside of an overhang
is stone, or sandstone and red sandstone under sand, so nothing hangs
that would fall. Trees, plants, snow and ice use the column's top, so
they sit on the highest surface and never inside rock; floors under
overhangs get their soil but no plants yet.

## Underground

`world::caves` carves noise caves the way Java 1.18 does, sampling noise
on a coarse grid (every 4 blocks, caverns every 8) and interpolating:

- **Cheese caves:** big caverns that grow larger with depth and stay
  under at least 10 blocks of rock.
- **Spaghetti caves:** long winding tunnels where two noises are both
  near zero; they can break the surface as cave entrances.
- **Noodle caves:** thin passages switched on in patches.
- **Ravines:** Java's canyon carver, one start in fifty 16×16 chunks.

**Aquifers** decide what fills each carved cell. The world is divided
into 16×12×16 cells, each with its own water level, a lava level when
deep, or none. Where two neighbouring cells disagree a stone barrier is
left, so underground lakes sit at different heights. Within 10 blocks of
the surface, below sea level, caves flood with the sea. Below y = -54
open space fills with lava, as in Java.

In generation 2 caves are fitted to the density terrain: a block's depth
is measured from the nearest open air above it, so caverns stay sealed
under overhang floors as well as the top; rock within 6 blocks of open air
below it (overhangs, arches, ledges) is never carved, and where the relief
is above 8 nothing above the base height is, so overhangs and spires don't
shatter into floating pieces; tunnels still break through the surface as
entrances on lower slopes and flatter land; nothing is carved within two blocks under the sea or a
river; aquifers measure their levels from the lowest the column's surface
can reach; and aquifer water and lava are never left beside open dry air.
Every open block of the density field below the sea is sea water.

Deepslate replaces stone below y = 0, blending over y = 0..8. Ores use
Java's placed-feature heights and counts directly, plus Java's dirt and
gravel blobs, granite, diorite, andesite and tuff.

**Cave biomes:** where humidity is at least 0.7, caves more than 20
blocks underground are lush caves: moss floors and ceilings, moss
carpets, azaleas, small and big dripleaves, glow berry cave vines,
spore blossoms, and clay pools. Azalea trees on the surface above them
send rooted dirt and hanging roots down. Where continentalness is at
least 0.8 they are dripstone caves, with dripstone blocks and
stalactites and stalagmites of pointed dripstone.

### Generation 2 (v0.13)

New worlds (`worldgen=2`) add the following; generation-1 worlds keep the
output above, pinned by the golden chunk hashes. `world::underground` runs
once per chunk after the structures, as Java's decoration steps do, and
reads the generated blocks rather than the terrain noise, so it follows
whatever shape the terrain has. Every decision comes from hashed world
coordinates and noise, so features continue across chunk borders whatever
order chunks generate in. Details and differences:
[the v0.13 underground record](verification/2026-10-30-v0.13-underground.md).

- **Deep dark** (`world::deep_dark`): a cave biome where the erosion
  parameter is below -0.375 (Java's two lowest erosion bands, mountainous
  ground), at least 120 blocks under the surface and no higher than y = 0.
  There it replaces lush and dripstone caves. Exposed stone turns to sculk
  in large patches from a 3D noise; sculk veins line the stone faces at the
  patch edges, with a few scattered elsewhere; patch cores hold the odd
  sculk catalyst; sensors and, one time in eleven, shriekers stand on the
  sculk floor. No other plants grow there. `/locate biome deep_dark` finds
  it. The catalyst, sensor and shrieker are inert until v0.16.
- **Glow lichen** in clusters on cave walls and ceilings (never floors), at
  least 13 blocks under the surface, outside the deep dark, on stone,
  deepslate, granite, diorite, andesite, tuff, calcite and dripstone,
  waterlogged in water.
- **Springs:** Java's 25 water tries (from the bottom up to y = 192) and 20
  lava tries (biased to the bottom) per 16×16 area. A try becomes a source
  only where it has stone above and below and stone on three sides with air
  on the fourth, so the fluid pours out of a wall once the chunk is
  simulated.
- **Ores:** dripstone caves place the large copper veins (size 20) instead
  of the ordinary ones, judged by the cave biome at the vein's origin.
  Java's large ore veins run through the stone: copper ore in granite at
  y 0..=50 and deepslate iron ore in tuff at y -60..=-8, each with a few
  raw ore blocks.

## Surface and features

Each biome has its own surface rules (sand and sandstone, red sand and
terracotta strata, mycelium, mud, podzol and coarse dirt patches, snow
and packed ice on peaks, gravel on windswept hills, stone on steep
mountain faces), trees, flowers and plants. Trees include fancy oaks,
dark oaks and huge mushrooms, pale oaks with hanging moss, mangroves with
roots, cherry trees, mega spruces and pines, tall birches, jungle trees
with vines and cocoa, swamp oaks with vines, azaleas and bamboo.

Features decided column by column agree across chunk seams: kelp forests
and seagrass in temperate seas, coral reefs with coral, fans and sea
pickles in warm oceans, icebergs of packed and blue ice in frozen oceans,
ice spikes, lily pads in swamps, snow layers on cold ground and ice on
cold still water.

**Generation 2 (v0.13).** New worlds add the features below; generation 1
worlds keep the output above unchanged (the `generated_chunk_hashes_stay_identical`
test). All of it is placed from hashed world coordinates, each chunk writing
only its own part, so a feature that crosses a chunk border is whole in
either order. Details and the per-biome audit are in
[the v0.13 trees record](verification/2026-10-30-v0.13-trees.md).

- *Giant trees.* Old growth spruce and pine taigas grow 2x2 spruces under a
  tall cone of leaves, with an irregular disc of podzol (and a little coarse
  dirt) around the trunk and mossy cobblestone boulders. Jungles grow 2x2
  jungle trees with vines climbing the trunk's sides and cocoa pods low on
  it. Four saplings of spruce or jungle in a 2x2 grow the same giant trees
  (dark oak already needed a 2x2).
- *Fallen logs* lie in forests, taigas, jungles and swamps; boulders in
  taigas.
- *New plants* (`world::biome_plants`): leaf litter under forests, wildflowers
  in meadows and birch forests, bushes in savannas, plains and wooded
  badlands, firefly bushes beside swamp water, short and tall dry grass in
  deserts and badlands, cactus flowers on cacti, melon patches in jungles.

## Saves

Saves now record `terrain=2`. Worlds made before 0.6 keep their edited
chunks, but the new generator will not line up with them; the world list
marks them as "pre-0.6 terrain". Start a new world to see the new
terrain.

## Performance

On an Apple M5 (`--bench --rd 8`, release build): Overworld generation
about 0.39 ms per 32³ chunk and light and mesh about 0.87 ms per dense
chunk, against 0.26 and 0.68 ms in 0.5.0. A column is now 12 chunks
instead of 8, and caves expose more geometry.

`--bench` also compares the two generations on fresh generators. On
2026-10-30, generation 2 took 0.53 ms per chunk around the origin of seed
12345 and 0.71 ms in its nearest jagged peaks, against 0.46 and 0.58 ms
for generation 1: 1.16× and 1.22×
([record](verification/2026-10-30-v0.13-density.md#generation-speed)).

## Gaps

Generation 1's surface is a heightmap, so its worlds have no overhanging
cliffs. Generation 2's density field is original rather than Java's
density functions (FE-01). There is no deep dark,
no glow lichen, and no water or lava springs. Cave and climate noise are
original, so a Java seed does not reproduce Java terrain.
