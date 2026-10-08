# Nether mobs: piglins, brutes, hoglins, zoglins and striders

[← Back to VoxelCraft](../README.md) · [Mob parity](mob-parity.md) · [Bastions](bastions.md)

VoxelCraft v0.5 adds the Nether's living mobs. They follow Java Edition 1.21's
brains (`PiglinAi`, `PiglinBruteAi`, `HoglinAi`, `Zoglin`, `Strider`) as closely as
the engine allows. The engine has no pathfinding, riding or breeding, so those parts
are approximated or left out (see [Known gaps](#known-gaps)).

| Mob | Health | Box | Damage | XP | Drops |
|---|---|---|---|---|---|
| Piglin | 16 | 0.6 × 1.95 | 5 unarmed, 8 with golden sword (Normal), crossbow arrows | 5 | what it carries; gear 8.5% (+1% per Looting) on a player kill |
| Piglin brute | 50 | 0.6 × 1.95 | 13 with golden axe | 20 | golden axe 8.5% (+1% per Looting) |
| Hoglin | 40 | 1.4 × 1.4 | 3–8 and a toss; babies 0.5 | 5 (baby 3) | 2–4 porkchops (cooked if burning), 0–1 leather; babies nothing |
| Zoglin | 40 | 1.4 × 1.4 | 3–8 and a toss | 5 | 1–3 rotten flesh |
| Strider | 20 | 0.9 × 1.7 | — | 1–3 (baby 0) | 2–5 string; babies nothing |

Babies are half size. Looting adds to each drop as for other mobs.

## Code map

| File | What |
|---|---|
| `src/entity/nether.rs` | `NetherMob` state, sensing (players, mobs, dropped items), item pickup, bartering, anger broadcasts, mob-on-mob hits, zombification, bastion residents, spawn table, saving, tests |
| `src/entity/mob/nether_ai.rs` | per-tick movement and attacks, strider lava physics, knockback resistance |
| `src/entity/model/nether.rs` | box models |
| `src/audio/voices.rs` | piglin, hoglin and strider voices (brutes and zoglins use them pitched down) |
| `src/app/bow.rs` | the player's crossbow |
| `examples/nether_mobs_bench.rs` | entity-update cost against zombies |

Mobs now carry a `uid`, so mobs can target other mobs (`EntityEvent::MobHit`).
They also carry a `persistent` flag for mobs Java would never despawn. The look-around
(sensing) runs about twice a second per mob, at staggered times, over one shared
snapshot of the living mobs. Each mob's work is linear in the number of entities.
Sensing scans the shared snapshot for each Nether mob, so its worst-case work
is quadratic in the mob count; staggering keeps most scans off the same tick.

## Piglins

**Spawning.** Piglins spawn in groups of 4 at nether-wastes weight 15 (3–4 at weight 5 in crimson forests), against
zombified piglins' 100 (cap 4 per player). Java's `finalizeSpawn` rolls apply:
- 20% are babies, which carry nothing.
- Adults hold a golden sword or a crossbow, 50/50.
- Each golden armor piece has a 10% chance.

**Hostility.** An adult attacks the nearest visible, targetable player within 16
blocks who wears no golden armor. One golden piece is enough, as in `isWearingGold`.
- Swords swing once a second (Java's 20-tick cooldown).
- Crossbow piglins load for 25 ticks, then wait a pause of 1–2 s drawn once per shot.
  They shoot when they can see
  the target within 8 blocks and back away inside 5 (`BackUpIfTooClose(5, 0.75)`).

**Anger** lasts 600 ticks (30 s) and is not lifted by putting gold armor on.
- Opening a chest or barrel angers *idle* piglins within 16 blocks that can see the
  player.
- Breaking a block in `guarded_by_piglins` angers idle piglins within 16 blocks even
  unseen. The tagged blocks that exist are: chests, barrels, gold blocks, gilded
  blackstone, gold ore, deepslate gold ore and raw gold blocks.
- Idle means not admiring, fighting or fleeing.
- A hit adult fights back against the actual attacker, including melee, bow,
  crossbow and splash-potion hits. It and the target are broadcast to adult piglins
  and brutes within 16 blocks (`broadcastAngerTarget`).
- A hit baby runs for 100 ticks and alerts the adults.
- A player's hit stops admiring and puts the piglin off gold for 400 ticks.
- Piglins never retaliate against other piglins.

**Admiring and bartering.** A piglin notices wanted items within 9 blocks and walks
to them. Gold ingots and items in `piglin_loved` take priority even mid-fight, unless
a player's hit disabled admiring. The loved items that exist are: golden tools and
armor, gold blocks, gilded blackstone, gold ores, raw gold, clocks and glistering
melon slices. Gold items other branches add are matched by name.

It picks one ingot off a stack and holds it for 119 ticks (`ADMIRE_DURATION`):
- An adult then throws one roll of the bartering table toward the nearest player
  within 16 blocks, at Java's 0.3 blocks a tick, or in a random direction.
- Babies and loved non-currency items keep the item in their pocket.
- A hit while admiring loses the ingot (Java's `stopHoldingOffHandItem(false)`).
- Gold nuggets go straight into the pocket.
- Porkchops are eaten, with a 200-tick cooldown.
- Picking anything up makes the piglin persistent. `mobGriefing=false` disables
  pickup and bartering of dropped gold.
- Carried stacks keep wear, enchantments and names. A full pocket throws any
  overflow back into the world, keeping the items already stored.

**Zombification.** Outside the Nether, after 300 ticks, a piglin becomes a zombified
piglin. It keeps its age, armor, persistence and uid. Immune piglins
(`IsImmuneToZombification`) stay. The existing zombified piglin carries on: it is
neutral, angers its whole pack within 32 blocks for 30 s when one is hit, and spawns
in Nether packs. Conversion drops the pocket contents and the item being admired.

**Other behaviour:**
- Piglins attack wither skeletons on sight (their nemesis).
- They keep 6 blocks from zombified piglins and zoglins.
- Adults hunt adult hoglins (below).

### Bartering table (Java 1.21 `gameplay/piglin_bartering`)

Total weight 459. Counts are uniform and inclusive.

| Output | Weight | Chance | Count | Here |
|---|---|---|---|---|
| Enchanted book (Soul Speed) | 5 | 1.09% | 1 | empty roll (no Soul Speed) |
| Iron boots (Soul Speed) | 8 | 1.74% | 1 | iron boots, unenchanted |
| Potion of fire resistance | 8 | 1.74% | 1 | ✓ |
| Splash potion of fire resistance | 8 | 1.74% | 1 | ✓ |
| Water bottle | 10 | 2.18% | 1 | ✓ |
| Iron nugget | 10 | 2.18% | 10–36 | ✓ |
| Ender pearl | 10 | 2.18% | 2–4 | ✓ |
| String | 20 | 4.36% | 3–9 | ✓ |
| Nether quartz | 20 | 4.36% | 5–12 | ✓ |
| Obsidian | 40 | 8.71% | 1 | ✓ |
| Crying obsidian | 40 | 8.71% | 1–3 | ✓ (block 1800) |
| Fire charge | 40 | 8.71% | 1 | ✓ |
| Leather | 40 | 8.71% | 2–4 | ✓ |
| Soul sand | 40 | 8.71% | 2–8 | ✓ |
| Nether brick | 40 | 8.71% | 2–8 | ✓ |
| Spectral arrow | 40 | 8.71% | 6–12 | empty roll (item missing) |
| Gravel | 40 | 8.71% | 8–16 | ✓ |
| Blackstone | 40 | 8.71% | 8–16 | ✓ |

Missing outputs keep their weights as empty rolls, as bastion loot does, so the other
odds stay Java's. About 9.8% of trades give nothing until those items exist. Rolls
use the entity RNG (seeded per world), not Java's loot random sequence. The same seed
gives the same trades.

## Piglin brutes

Brutes have Java's 50 health and always carry a golden axe: 7 attack + 6 = 13 on
Normal.
- They attack any visible player within 16 blocks, gold armor or not, and wither
  skeletons.
- They take part in piglin anger broadcasts both ways and ignore gold on the floor.
- They zombify like piglins.
- They never spawn naturally: only as bastion residents.

## Bastion residents

Java places bastion mobs while generating the structure. Here a bastion within 48
blocks of a player gets its residents once its corners and centre have loaded:
- Each piece draws from a small pool with empty slots, Java's mob pools reduced to
  this engine's four hand-made layouts.
- Housing: 2 piglins, 1 brute and an empty slot. Courtyard: 2 piglins and an empty
  slot. Tower: a piglin and a brute. Treasure hall: 2 brutes and 2 piglins. Bridge: a
  piglin or nothing. Rampart face: 2 piglins and a brute. Stables: 2 piglins and 4
  hoglins.
- Bastion piglins are adults and roll Java's pool weapons: unarmed 1, sword 4,
  crossbow 4.
- They start with Java's 30–120 s pause before hunting.
- Stable hoglins can't be hunted (`CannotBeHunted`).
- Residents are persistent. They remember their spot and walk back once 12 blocks
  away, a stand-in for Java's brute `HOME` memory and the lack of pathfinding.
- Each bastion rolls from its own seed, so every copy of a world gets the same
  residents.
- Populated bastions are saved, so residents are neither duplicated nor lost.
- Nothing is placed on Peaceful or outside the Nether.

## Hoglins and zoglins

**Hoglins:**
- They charge the nearest visible, targetable player within 16 blocks, gold or not.
  Adults attack every 40 ticks, babies every 15.
- An adult deals half its 6 attack plus a random part of it (3–8). It then throws the
  target (`HoglinBase.throwTarget`): 0.2–0.7 blocks a tick horizontally, up to 10°
  off straight away, and 0–0.5 blocks a tick upward, on top of the usual knockback.
  Babies nip for 0.5 and don't throw.
- They have 0.6 knockback resistance.
- **Repellents**: any `hoglin_repellents` block within 8 blocks across and 4 up or
  down pacifies a hoglin for 200 ticks, and it backs away. Nether portals and warped
  fungus count; potted warped fungus and respawn anchors are matched by name once
  they exist. The box is scanned once a second.
- A hit adult attacks back for 200 ticks and rallies adult hoglins within 16 blocks.
  A hit baby retreats for 5–20 s.
- Adults back off when adult piglins within 16 outnumber them.
- Babies trail the nearest adult when it's 5–16 blocks away.
- Outside the Nether, after 300 ticks, a hoglin becomes a zoglin.

**Hunting.** An adult piglin that may hunt, and hasn't for a while, picks a huntable
adult hoglin within 16 blocks (`StartHuntingHoglin`). The piglins around it join the
attack, and none of them hunt again for 30–120 s. A piglin hit by a hoglin whose
herd outnumbers the piglins retreats instead.

**Zoglins** are fire immune and undead, so Smite works on them.
- They attack the nearest mob or player in reach and sight, except creepers and
  other zoglins (`Zoglin.isTargetable`).
- They use the hoglin's tusk attack and throw.

**Spawning.** Hoglins spawn only from the crimson-forest entry (weight 9, groups of
3–4); see the spawn table. 20% spawn as babies. Bastion stables also contain
persistent hoglins.

**Breeding** needs animal breeding support, which is not implemented. Crimson
fungus exists; `nether::breeding_item` identifies it for future breeding support.

## Striders

Striders are passive, fire immune and hurt by water and rain.
- They walk on lava: a lava surface counts as ground, and a strider sunk into lava
  rises (Java's `floatStrider`).
- They treat lava as floor when stepping. Once on lava they won't wander onto dry
  land, matching Java's walk-target scores of 10 for lava and unwalkable for land
  from lava.
- **Cold**: when neither the feet nor the block below are lava (Java's
  `strider_warm_blocks` rule), a strider turns purple-grey and shivers. It moves 34%
  slower (`SUFFOCATING_MODIFIER`) and heads for the nearest open lava within 8 blocks
  across and 2 up or down (`StriderGoToLavaGoal`).
- **Spawning**: on lava seas, where the lava has open air above (Java's
  `checkStriderSpawnRules`), at weight 60 in groups of 1–2 in every Nether biome.

## The spawn table and biomes

Spawns follow the biome under each attempt: `world::nether_biome` holds Java's
per-biome lists (see [Nether biomes](nether-biomes.md#mob-spawning)) and
`entity::nether_mob` maps them to these mobs. The Java 1.21 lists are:

| Biome | Monsters | Creatures |
|---|---|---|
| Nether wastes | ghast 50 (4), zombified piglin 100 (4), magma cube 2 (4), enderman 1 (4), piglin 15 (4) | strider 60 (1–2) |
| Crimson forest | zombified piglin 1 (2–4), hoglin 9 (3–4), piglin 5 (3–4) | strider 60 (1–2) |
| Warped forest | enderman 1 (4) | strider 60 (1–2) |
| Soul sand valley | skeleton 20 (5), ghast 50 (4), enderman 1 (4) | strider 60 (1–2) |
| Basalt deltas | ghast 40 (1), magma cube 100 (2–5) | strider 60 (1–2) |

Natural Nether attempts use these biome weights through `nether_spawn`;
`MobKind::spawn_chance` continues to govern other dimensions.

## Crossbow

Items 840 (crossbow) and 841 (charged crossbow) are the first of the nether-mob item
range.
- A crossbow has Java's 465 uses.
- Holding use for 25 ticks loads it. That spends an arrow and turns the held stack
  into the charged form, keeping wear, enchantments and name.
- Using a charged crossbow shoots at 3.15 blocks a tick and wears it by one.
- Piglins drop crossbows as gear, bastion chests that list one now hold it
  (with the table's wear and enchantment roll), and fletcher trades naming the
  crossbow now resolve.

## Crying obsidian

Block 1800 (texture layer 1400) is Java's crying obsidian, added for bartering:
- Hardness 50, needs a diamond pickaxe, light level 10.
- Immune to explosions and pistons, like obsidian.
- Its texture is obsidian streaked with glowing violet tears.
- Bastion chests that list it (treasure, bridge, hoglin stable, other) now give
  it instead of an empty roll.
- It drops no tear particles and doesn't make respawn anchors (which don't exist).

## Saves

New state lives in the dimension's `nether_mobs` level property, as JSON:
`{"mobs":[…], "bastions":[[x,y,z],…]}`.
- Each mob records kind, position, yaw, health, baby flag, golden armor slots, weapon,
  admired item and time left, pocket stacks, zombification timer, immunity, hunting
  flag and home.
- Only persistent mobs are saved. Ordinary spawns despawn like other mobs.
- Levels without the key load as before, and bad entries are skipped. Round trips are
  unit tested.
- Mob uids are not saved; they are handed out again on load.

## Sounds

Piglins, hoglins and striders have synthesized voices (ambient, hurt, death) through
the existing mob-voice path. Brutes and zoglins use the piglin and hoglin voices
pitched down (0.72–0.82). Admiring plays the piglin's ambient call, and zombifying
plays the zombified piglin's. Crossbow shots and loading use the bow sound at
different pitches.

## Known gaps

- **Riding**: there is no riding, so no saddles, warped fungus on a stick, strider
  steering, strider jockeys, baby striders riding adults or piglins riding hoglins.
- **Breeding**: none for hoglins or striders (there is no animal breeding yet,
  though the fungi exist); striders can't be tempted.
- **Bartering**: spectral arrows and Soul Speed don't exist (see the table).
- **Piglin details**:
  - No celebrating dance after a kill.
  - Crossbow piglins melee mobs instead of shooting them, because mob arrows only
    hit players.
  - Piglins don't pick up and equip better weapons or armor, or golden axes for
    brutes.
  - Their pocket holds 8 stacks.
  - Zombification retains armor but replaces the main-hand weapon with the existing
    zombified piglin sword model; crossbows and brute axes are not retained.
- **Zoglins** only pick mob targets within 4 blocks of height, so they don't chase
  ghasts they could never reach.
- **Peaceful**: Java keeps piglins and hoglins on Peaceful; here every hostile mob
  is removed, as before.
- **Bastion residents** are spread over each piece's floor rather than placed at
  Java's exact jigsaw spots. Treasure-room magma cube spawners are still absent.
- **Crossbow**: crafted with Java's recipe and supports existing durability
  enchantments, but no Multishot, Piercing or Quick Charge, and no first-person
  loading pose. Device-free agents have no bow or crossbow use command.
- **Line of sight** uses the existing block raycast, which treats every solid block
  as opaque.
- **Visibility**: anger and hunting counts check distance, not sight lines.
- **Split-screen fog**: all views share the host camera's blended biome fog.

## What to check visually

Visual checks for the integrator:

- **Piglins**:
  - Proportions: wide 10-pixel head, snout, tusks, flopping ears.
  - Leather outfit with gold buckle; golden armor overlays.
  - Sword arm chopping; crossbow held level while fighting.
  - The gold ingot raised in the off hand with the head bowed while admiring.
- **Brutes**: black, gold-trimmed tunic and golden axe swing.
- **Hoglins and zoglins**:
  - Body and crest height against the 1.4 box; head tilt and toss.
  - Tusk placement; zoglin colours and raw patches.
  - Babies at half size.
- **Striders**:
  - Red on lava, purple-grey and shivering on land.
  - Feet resting exactly on the lava surface, with no sinking or bobbing.
  - Leg stride.
- **Crossbow icons**: unloaded and charged.
- **Gameplay**:
  - Bartering items arcing toward the player.
  - Bastion residents standing on floors, not in walls or lava, across all four
    layouts.
  - Stable hoglins inside the pens.
  - Sound balance of the new voices.

## Performance

`cargo run --release --example nether_mobs_bench` steps 100 or 300 mobs for 400
fixed 50 ms updates around one player in gold armor, and compares zombies with a
mix of the five new kinds. Medians of five seeds, from two runs on the 3-core Linux
agent container:

| Mobs | Zombies | Nether mobs |
|---|---|---|
| 100 | 0.11–0.13 ms/update | 0.12–0.13 ms/update |
| 300 | 0.58 ms/update | 0.60–0.68 ms/update |

Sensing costs about as much as a zombie's per-tick player search. The superlinear
growth from 100 to 300 mobs comes from the existing pairwise mob separation, which
both kinds pay.

`cargo run --release -- --bench --rd 8` (headless terrain) is unaffected: none of
this runs during generation or meshing, and the quad output is identical. Medians
of five interleaved runs on the same container:

| | Base (`t3/roadmap-release-next`) | This branch |
|---|---|---|
| generate | 0.694 ms/chunk | 0.642 ms/chunk |
| light+mesh | 1.832 ms/chunk | 1.816 ms/chunk |
| stream rd=8 | 1.05 s | 1.05 s |
| quads | 148903 | 148903 |

These machines are slower and noisier than the Apple M5 figures in
[architecture](architecture.md#recorded-benchmarks-apple-m5-release-build).

## Sources

- [PiglinAi](https://raw.githubusercontent.com/mahtomedi/minecraft/main/src/main/java/net/minecraft/world/entity/monster/piglin/PiglinAi.java),
  [Piglin](https://raw.githubusercontent.com/mahtomedi/minecraft/main/src/main/java/net/minecraft/world/entity/monster/piglin/Piglin.java),
  [AbstractPiglin](https://raw.githubusercontent.com/mahtomedi/minecraft/main/src/main/java/net/minecraft/world/entity/monster/piglin/AbstractPiglin.java),
  [PiglinBrute](https://raw.githubusercontent.com/mahtomedi/minecraft/main/src/main/java/net/minecraft/world/entity/monster/piglin/PiglinBrute.java),
  [PiglinBruteAi](https://raw.githubusercontent.com/mahtomedi/minecraft/main/src/main/java/net/minecraft/world/entity/monster/piglin/PiglinBruteAi.java)
- [Hoglin](https://raw.githubusercontent.com/mahtomedi/minecraft/main/src/main/java/net/minecraft/world/entity/monster/hoglin/Hoglin.java),
  [HoglinAi](https://raw.githubusercontent.com/mahtomedi/minecraft/main/src/main/java/net/minecraft/world/entity/monster/hoglin/HoglinAi.java),
  [HoglinBase](https://raw.githubusercontent.com/mahtomedi/minecraft/main/src/main/java/net/minecraft/world/entity/monster/hoglin/HoglinBase.java),
  [Zoglin](https://raw.githubusercontent.com/mahtomedi/minecraft/main/src/main/java/net/minecraft/world/entity/monster/Zoglin.java),
  [ZombifiedPiglin](https://raw.githubusercontent.com/mahtomedi/minecraft/main/src/main/java/net/minecraft/world/entity/monster/ZombifiedPiglin.java)
- [PiglinSpecificSensor](https://raw.githubusercontent.com/mahtomedi/minecraft/main/src/main/java/net/minecraft/world/entity/ai/sensing/PiglinSpecificSensor.java)
- [Strider](https://raw.githubusercontent.com/mahtomedi/minecraft/main/src/main/java/net/minecraft/world/entity/monster/Strider.java)
- [Bartering loot table](https://raw.githubusercontent.com/misode/mcmeta/1.21.5-data/data/minecraft/loot_table/gameplay/piglin_bartering.json)
- Biome spawn lists: `data/minecraft/worldgen/biome/*.json` in the same
  [mcmeta data branch](https://github.com/misode/mcmeta/tree/1.21.5-data/data/minecraft/worldgen/biome)
