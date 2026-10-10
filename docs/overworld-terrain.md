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

## Shape

`world::climate` builds the surface height from the same parameters, like
Java's terrain splines. Continentalness lifts land out of 30-block-deep
oceans; low erosion with high peaks-and-valleys raises mountains up to
about y = 250, with jagged ridges where weirdness is negative; mid
erosion inland makes flat-topped plateaus; valleys cut rivers down to
just under the sea; very high erosion flattens swamps to sea level; and
windswept ground is broken and lumpy. Badlands terrace their cliffs and
eroded badlands grow hoodoos. Terrain is a heightmap, so there are no
overhangs on the surface; caves provide the 3D structure.

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

## Gaps

Surface terrain is a heightmap rather than Java's 3D density, so there
are no floating islands or overhanging cliffs. There is no deep dark,
no glow lichen, and no water or lava springs. Cave and climate noise are
original, so a Java seed does not reproduce Java terrain.
