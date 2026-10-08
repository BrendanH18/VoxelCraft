# Nether biomes

[← Back to VoxelCraft](../README.md) · [Bastions and Nether materials](bastions.md)

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
