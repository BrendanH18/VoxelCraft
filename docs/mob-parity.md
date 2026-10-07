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
