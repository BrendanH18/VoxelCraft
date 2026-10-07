# Survival mob parity

Implementation notes for the survival roster. Entity names and item IDs are append-only.

## Cave spider

12 health, 0.7 × 0.5 collision box, spider melee/climbing/neutral-in-daylight AI,
blue spider model at 0.55 scale, spider voice, spider loot. Normal applies Poison I
for 140 ticks; Hard for 300; Easy applies none. Effects address the struck player's
ID, including agents and controller players.

Mineshaft cave spider cages remain a follow-up: the existing generator has spider
corridors but no spawner placement or feature record, so this is not a one-line
mob substitution. Spawn with `/summon cave_spider` or configure a spawner.

Sources: [CaveSpider](https://raw.githubusercontent.com/mahtomedi/minecraft/main/src/main/java/net/minecraft/world/entity/monster/CaveSpider.java),
[Spider](https://raw.githubusercontent.com/mahtomedi/minecraft/main/src/main/java/net/minecraft/world/entity/monster/Spider.java).

## Slime

Sizes 1/2/4 have health 1/4/16, width/height 0.51 × size, attack 0/2/4,
Java movement attributes 0.3/0.4/0.6, follow range 16, and hopping movement.
Killed large/medium slimes split into 2–4 children regardless of doMobLoot;
only tiny slimes drop 0–2 slimeballs (item 640), with Looting. XP equals size.
The chunk formula uses the world seed and Java's 16-block chunks, signed
integer overflow and 48-bit RNG. Underground Y < 40 follows the world's
compressed Java height map. Swamps check mapped Y 50–70, random light 0–7,
50% chance and saved day-count moon brightness. The green cube model is
opaque because the current entity pass has no translucency.

Sources: [Slime](https://raw.githubusercontent.com/mahtomedi/minecraft/main/src/main/java/net/minecraft/world/entity/monster/Slime.java),
[WorldgenRandom](https://raw.githubusercontent.com/mahtomedi/minecraft/main/src/main/java/net/minecraft/world/level/levelgen/WorldgenRandom.java).

## Magma cube and fire resistance

Nether magma cubes share slime sizes and splitting. Damage is size + 2 (including
tiny cubes); armor is 3 × size, jumps are higher and delayed four times longer,
fire/lava immunity, flame bursts on landing, dark segmented model and squish voice.
Only sizes 2/4 drop magma cream using Java's -2..1 roll plus Looting (item 641).
Slimeball + blaze powder crafts cream; awkward + cream brews 3:00 fire resistance;
redstone extends eligible potions, including fire resistance to 8:00. Water plus
either ingredient becomes mundane. Natural attempts use nether-wastes weight 2
(chance 0.02) in groups of 4; fortress pieces use Java's weight 3, group 4.
There is no basalt-deltas biome, so that weight-100 pool is absent.

Sources: [MagmaCube](https://raw.githubusercontent.com/mahtomedi/minecraft/main/src/main/java/net/minecraft/world/entity/monster/MagmaCube.java),
[loot table](https://raw.githubusercontent.com/misode/mcmeta/1.21.5-data/data/minecraft/loot_table/entities/magma_cube.json).
