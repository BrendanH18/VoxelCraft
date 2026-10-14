# Survival mob parity

Implementation notes for the survival roster. Entity names and item IDs are append-only.

## Cave spider

12 health, 0.7 × 0.5 collision box, spider melee/climbing/neutral-in-daylight AI,
blue spider model at 0.55 scale, spider voice, spider loot. Normal applies Poison I
for 140 ticks; Hard for 300; Easy applies none. Effects address the struck player's
ID, including agents and controller players.

Each mineshaft spider corridor holds one cave spider spawner on its centre line
near the middle, as in Java, registered like dungeon and fortress spawners.
`/summon cave_spider` also spawns one.

Sources: [CaveSpider](https://raw.githubusercontent.com/mahtomedi/minecraft/main/src/main/java/net/minecraft/world/entity/monster/CaveSpider.java),
[Spider](https://raw.githubusercontent.com/mahtomedi/minecraft/main/src/main/java/net/minecraft/world/entity/monster/Spider.java).

## Slime

Sizes 1/2/4 have health 1/4/16, width/height 0.51 × size, attack 0/2/4,
Java movement attributes 0.3/0.4/0.6, follow range 16, and hopping movement.
Killed large/medium slimes split into 2–4 children regardless of doMobLoot;
only tiny slimes drop 0–2 slimeballs (item 610, shared with bastions), with Looting. XP equals size.
The chunk formula uses the world seed and Java's 16-block chunks, signed
integer overflow and 48-bit RNG. Underground Y < 40 follows the world's
compressed Java height map. Swamps check mapped Y 50–70, random light 0–7,
50% chance and saved day-count moon brightness. The model is an opaque core with
its face inside a translucent shell (alpha 0.62, as Java's `SlimeOuterLayer`), drawn in a
second blended entity pass that is depth-tested but does not write depth; the shell
alpha and colours are original approximations, and the blended pass is not
GPU-verified in this change. Other entities stay opaque.

Sources: [Slime](https://raw.githubusercontent.com/mahtomedi/minecraft/main/src/main/java/net/minecraft/world/entity/monster/Slime.java),
[WorldgenRandom](https://raw.githubusercontent.com/mahtomedi/minecraft/main/src/main/java/net/minecraft/world/level/levelgen/WorldgenRandom.java).

## Magma cube and fire resistance

Nether magma cubes share slime sizes and splitting. Damage is size + 2 (including
tiny cubes); armor is 3 × size, jumps are higher and delayed four times longer,
fire/lava immunity, flame bursts on landing, dark segmented model and squish voice.
Only sizes 2/4 drop magma cream using Java's -2..1 roll plus Looting (item 608, shared with bastions).
Slimeball + blaze powder crafts cream; awkward + cream brews 3:00 fire resistance;
redstone extends eligible potions, including fire resistance to 8:00. Water plus
either ingredient becomes mundane. Natural attempts follow each Nether biome's own
list: nether wastes weight 2 (chance 0.02) in groups of 4, basalt deltas weight 100
in groups of 2–5; fortress pieces use Java's weight 3, group 4.

Sources: [MagmaCube](https://raw.githubusercontent.com/mahtomedi/minecraft/main/src/main/java/net/minecraft/world/entity/monster/MagmaCube.java),
[loot table](https://raw.githubusercontent.com/misode/mcmeta/1.21.5-data/data/minecraft/loot_table/entities/magma_cube.json).

## Ghast

10 health, 4 × 4 hitbox, follow range 100, flying speed 14 blocks/s (Java's 0.7
per tick), fire immune. Spawns follow the biome's own list (pinned to the 26.3 biome
data in `tests/fixtures/java/26.3/mob_integrations.json`): nether wastes weight 50
(chance 0.5 against zombified piglins' 100) and soul sand valleys weight 50, both in
groups of 4; basalt deltas weight 40 in groups of 1; crimson and warped forests none. Charges one second, then shoots one large fireball and rests two seconds.
The fireball explodes at power 1, can be punched back by the host or an agent,
and a deflected blast credits the player so the kill drops loot. Block breaking
follows `mobGriefing`, same as creepers. Drops 0–2 gunpowder and, on a player
kill, 0–1 ghast tears (item 640). Awkward + tear brews regeneration; redstone
and glowstone make the long and strong variants.

Sources: [Ghast](https://raw.githubusercontent.com/mahtomedi/minecraft/main/src/main/java/net/minecraft/world/entity/monster/Ghast.java).

## Wither skeleton

20 health, 0.7 × 2.4 box (Java's 1.2 scale), stone sword melee for 8, fire immune.
Spawns only inside fortress pieces (weight 8, groups of 5). Every hit applies Wither
for 10 s; Wither deals 1 damage every 40 >> level ticks, ignores armor and can kill.
Drops 0–1 coal, 0–2 bones and, on a player kill, a skull item (item 641) 2.5% of the
time (+1% per Looting level). The skull is item-only: there is no placeable skull
block yet. Hunger (0.005 exhaustion per level per tick) was added alongside.

## Zombie variants

Husks (desert, replacing most zombies: weight 80 vs 19) don't burn and apply Hunger
for 140 ticks × difficulty (1/2/3, instead of regional difficulty). Drowned spawn in
water in rivers (1 in 15 attempts) and oceans (1 in 40, deeper than 5 below sea level)
and still burn on land. All three have a 5% baby chance (half size, +50% speed, 12 xp).
No trident drowned, no rare zombie drops.

## Witch and splash potions

26 health, spawns at night (weight 5/100, boosted in swamps). Throws a splash potion
every 3 s from up to 10 blocks (slowness from 8+, then poison, else harming; the target's
health and effects are not inspected), drinks healing when hurt, 85% magic resistance.
Splash potions are items 436..=471, made by adding gunpowder to any potion; players
throw them at 10 b/s aimed 20° high. A splash affects entities within 4 × 2 × 4 with
`1 - distance / 4` intensity, 75% duration, and Java's instant damage/heal amounts (the
undead are inverted). Mobs only react to instant effects; water splashes do nothing.

## Piglins, brutes, hoglins, zoglins and striders

Piglins (16 health) barter for gold with Java 1.21's table: 459 weight, about 6 s
of admiring. They attack players without golden armor and are angered by chests
and broken gold. Hit piglins alert the adults nearby, and piglins zombify after 15 s
outside the Nether. Brutes (50 health, golden axe for 13) guard bastions, which get persistent
residents.

Hoglins (40 health) charge and toss players, avoid portals and outnumbering piglins,
and become zoglins outside the Nether. Zoglins attack everything but creepers and
other zoglins. Piglins hunt adult hoglins.

Striders walk on lava, shiver on land and drop string. A crossbow item (840/841)
arms piglins and the player.

The Nether spawn table is keyed by biome for the biome work. Persistent mobs and
populated bastions are saved under `nether_mobs`. Crying obsidian (block 1800)
exists for bartering and bastion loot. Strider riding and hoglin/strider breeding
use the shared mount and animal paths; spectral arrows and Soul Speed do not
exist. Details, the bartering table and sources are in [Nether mobs](nether-mobs.md).

## Java 26.3 undead rare-pool correction — 2026-10-13

Player kills of zombies, husks and zombie villagers roll one rare pool at
2.5% + 1% per Looting level, choosing exactly one equally weighted iron ingot,
carrot or potato. Burning potatoes smelt to baked potatoes. Drowned roll one
copper ingot at 11% + 2% per Looting level. Environment and mob kills receive
neither rare pool. `undead_loot.json` is extracted from the pinned 26.3 server
loot tables; tests cover exclusivity, probabilities, credit and smelting.
Tridents, drowned trident equipment and projectile behavior require new content
and remain outside this defect-only change. Java loot RNG sequence parity remains open.
