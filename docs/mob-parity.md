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
