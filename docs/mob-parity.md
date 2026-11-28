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

10 health, 4 × 4 hitbox, follow range 100, flying speed 0.06, fire immune. It
floats to random spots up to 16 blocks away, pushed 0.1 a tick every 2–6 ticks
against 0.91 drag while the way is clear. Spawns follow the biome's own list (pinned to the 26.3 biome
data in `tests/fixtures/java/26.3/mob_integrations.json`): nether wastes weight 50
(chance 0.5 against zombified piglins' 100) and soul sand valleys weight 50, both in
groups of 4; basalt deltas weight 40 in groups of 1; crimson and warped forests none. It targets a visible survival player within 100 blocks and 4 vertically, charges 20 ticks (warning at 10) while it sees the target within 64 blocks, fires one large fireball from 4 blocks in front, and waits 40 ticks.
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
No trident drowned (rare iron/carrot/potato and copper pools are implemented; see below).

V0.15 track 1 adds eye-submerged conversion (implemented/unverified): 600
ticks of exposure, then 300 ticks counting down through zero with shaking;
husks become zombies and zombies become drowned. Java 26.3 cancels the
countdown on leaving the water. Timers, name, identity, persistence, age,
effects, equipment and rider links survive saves. See [lightning and undead
mount evidence](verification/2026-11-04-v0.15-lightning.md), which also records
charged creepers, lightning transformations and the new horsemen.


## Witch and splash potions

26 health, spawns at night (weight 5/100, boosted in swamps). Throws a splash potion
every 3 s from up to 10 blocks, chosen as Java's `performRangedAttack` does (slowness from
8+ blocks unless slowed, poison at 8+ health unless poisoned, 25% weakness within 3 blocks,
else harming). Each tick it may drink water breathing (eye in water), fire resistance
(burning) or healing (hurt), and swiftness against a target beyond 11 blocks; drinks apply
the potion's effects. 85% magic resistance.
Splash potions are items 436..=471, made by adding gunpowder to any potion; players
throw them at 10 b/s aimed 20° high. A splash affects entities within 4 × 2 × 4 with
`1 - distance / 4` intensity, 75% duration, and Java's instant damage/heal amounts (the
undead are inverted). Mobs also receive every other splash effect (the undead ignore poison
and regeneration); water splashes do nothing.

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
use the shared mount and animal paths; every bartering output exists (Soul
Speed since v1.0 Track 5). Details, the bartering table and sources are in [Nether mobs](nether-mobs.md).

## Java 26.3 undead rare-pool correction — 2026-10-13

Player kills of zombies, husks and zombie villagers roll one rare pool at
2.5% + 1% per Looting level, choosing exactly one equally weighted iron ingot,
carrot or potato. Burning potatoes smelt to baked potatoes. Drowned roll one
copper ingot at 11% + 2% per Looting level. Environment and mob kills receive
neither rare pool. `undead_loot.json` is extracted from the pinned 26.3 server
loot tables; tests cover exclusivity, probabilities, credit and smelting.
Tridents, drowned trident equipment and projectile behavior require new content
and remain outside this defect-only change. Java loot RNG sequence parity remains open.

## Stray and bogged (v0.15, implemented, unverified)

The Java 26.3 biome entries now spawn strays in snowy plains/ice spikes
(weight 80, skeletons 20) and bogged in swamp/mangrove swamp (weight 30,
skeletons 70), packs of four. Strays require sky visibility. Skeletons convert
in powder snow with the reference exposure/countdown and cancellation rules.
Bogged have 16 health and shear once for two independent mushroom rolls.
Both use original procedural skeleton-variant models, burn in daylight, drop
bones/arrows and can drop their potion-bearing tipped arrow on a player kill.
Natural variants and conversion/shearing state save through the mob store.

Mob-fired stray arrows apply 600 ticks of Slowness I; bogged apply 100 ticks
of Poison I. Player loot arrows use the registered potion at one-eighth
duration. Skeleton AI/equipment gaps and remaining verification
are listed in [track evidence](verification/2026-11-04-v0.15-hostiles.md).

## Phantom (v0.15, implemented, unverified)

Phantoms use a custom insomnia spawner, saved rest ticks per player, and the
`spawn_phantoms` rule. The pinned Java random rest predicate first becomes
possible at 72,001 ticks. Attempts require sky darkness, sea level/sky
visibility and a local-difficulty roll; groups contain 1 through difficulty
id + 1 mobs. They circle an anchor, swoop at players, retreat from cats/hurt,
burn in daylight, and drop the existing phantom membrane item on player kills.
Membranes now brew Slow Falling. Natural size is zero; size/anchor persist.
Original blue ray geometry has animated wings. See the
[hostiles record](verification/2026-11-04-v0.15-hostiles.md) for source classes,
AI scheduling/equipment differences, checks and missing graphical evidence.

## Hostile goal scheduling, door breaking and spears (v1.0 gap Track 2, implemented, unverified)

The [gap-closing record](verification/2026-11-27-v1.0-gap-track-02-hostile-ai.md)
has the Java sources and checks. Code: `src/entity/mob/chase.rs` and
`src/entity/mob/door_break.rs`.

- **Targets** in the shared hostile chase (zombies, spiders, creepers,
  slimes, silverfish, witches and the rest of `think`'s chase) follow
  `NearestAttackableTargetGoal` and `TargetGoal`: a search only on the goal
  selector's full update (every other tick, staggered by entity id) and
  then one time in five (`reducedTickDelay(10)`), for the nearest player
  in sight within the follow range; the target is kept, even when a nearer
  player turns up, until it dies, leaves the follow range, stops being
  targetable or has been out of sight for 30 full updates (about 62
  ticks). Reinforcements and hits set the target at once, as `setTarget`
  and `HurtByTargetGoal` do. Spiders let go one full update in 100 in
  daylight (`SpiderAttackGoal`).
- **`MeleeAttackGoal`** (zombies, drowned without a trident, spiders, cave
  spiders, silverfish and wither skeletons in the chase):
  `canUse` at most every 20 ticks with a path to the target; the goal ends
  when the path runs out (spiders keep following); a start may hit at
  once, then one hit per 20 ticks when the attack box (the mob's box
  widened by `sqrt(2.04) − 0.6`) meets the target's and it is in sight.
- **Zombies' goal table**: the door goal (flagless), `SpearUseGoal`
  (priority 2, zombified piglins 1) and `ZombieAttackGoal` (3, or 2) run
  through the shared `GoalSelector` port (`mob::selector`), so a held
  spear takes the move and look flags from the melee goal.
- **Door breaking** (`BreakDoorGoal`): zombies, husks, drowned, zombie
  villagers and zombified piglins with `CanBreakDoors`, on Hard only and
  with `mobGriefing`, path through closed wooden doors
  (`setCanOpenDoors`) and, pressed against one, hammer it for 240 ticks:
  `zombie.attack_wooden_door` and an arm swing one tick in 20, the block
  crack stage `breakTime / 240 × 10` (drawn like a player's), then the
  door goes: the half above the path node silently, the other half with
  its break particles and sound and, for a lower half, the door item;
  then `zombie.break_wooden_door`. Vindicators in raids share the goal
  (Normal or Hard) and now drop the door too. The two zombie door sounds
  are original procedural stand-ins.
- **Spears** (`SpearUseGoal`): a zombie, husk, zombie villager or
  zombified piglin holding a spear walks at its target until within 10
  blocks, lowers the spear for `reducedTickDelay(delay + damage window)`
  full updates (119 for iron), charges, runs on 6–7 blocks past the
  target within 2 blocks of it, charges again, and when the time is up
  retreats 9–11 blocks (`LandRandomPos.getPosAway`). A lowered spear stabs
  players along the mob's look with the mob range (1–2.25 blocks): the
  base attack damage plus `floor(relative speed × multiplier)`, the 0.2
  mob factor on every speed threshold, one contact per player per 10
  ticks. Zombie horsemen and husks on camel husks steer their mounts there
  at the mount's `chargeSpeedModifier` (1.4 and 4).

## Mob spears: stabs, look and piglins (v1.0 gap Track 2b, implemented, unverified)

The [Track 2b record](verification/2026-11-27-v1.0-gap-track-02b-spears.md)
has the Java sources and checks. Code: `src/entity/mob/spear.rs`.

- **Stabs** (`KineticWeapon.damageEntities`, `LivingEntity.stabAttack`) run
  for every lowered mob spear each tick, before the mobs move: the ray from
  the eye along the head's look stabs every player and mob whose box
  (grown by 0.125) it crosses between 1 and 2.25 blocks plus the mob's own
  forward motion, short of the first block, never the spear's own mount or
  its fellow passengers. Each target is remembered for 10 ticks. Speeds are
  taken along the body's look, the mob's from its root vehicle. A contact
  deals the base attack damage plus `floor(relative speed × multiplier)`
  and Sharpness (Smite and Bane against mobs sensitive to them); one that
  knocks back pushes 0.4 along the mob's facing, plus its attack knockback,
  with or without damage, and slows the mob; one that dismounts takes the
  target off its vehicle (players too). Mob stabs never wear the spear, as
  in Java, where only the player's attacks call `postHurtEnemy`.
- **Look** (`LookControl`): while aiming, a mob keeps Java's head yaw and
  pitch: `SpearUseGoal` turns the body 30° per update (`Mob.lookAt`) and the
  head 30° a tick, a piglin's head turns 10° a tick with 40° of pitch, the
  pitch is held to its limit from level each tick, and the head stays
  within 75° of the body while walking.
- **Piglins** spawn with a golden spear in one melee roll in ten
  (`createSpawnWeapon`) and zombified piglins in one in twenty; the spear is
  drawn in the hand (an original stand-in), saved as the `golden_spear`
  weapon and dropped at the default 8.5% on a player kill (a zombified
  piglin's sword now drops the same way). Piglins fight with it through
  `SpearApproach`, `SpearAttack` and `SpearRetreat` (approach to 10 blocks,
  lower it for 289 ticks while charging and running on past the target,
  retreat 9–11 blocks for up to 100 ticks) and never swing it in melee;
  zombified piglins use `SpearUseGoal`.

## Heads, bodies and idle looks (v1.0 gap Track 2c, implemented, unverified)

The [Track 2c record](verification/2026-11-27-v1.0-gap-track-02c-looks.md)
has the Java sources, checks and remaining differences. Code:
`src/entity/mob/head.rs`.

- **Every mob keeps Java's `yHeadRot` and `xRot`** and turns them with its
  `LookControl` a tick after its goals ran: a request turns the head by
  the asking goal's speed for two ticks (attack goals 30°, Breed and
  FollowOwner 10°, Tempt the head limit plus 20°, the brain's sink the
  mob's own speed, 10° unless Java overrides it: breeze 25°, frog 35°),
  the pitch from level by its limit; without one the head turns back 10°
  a tick, and it stays within `getMaxHeadYRot` of the body while a path
  runs (ravager 45°, breeze and camels 30°, goat 15°, frog 5°, sniffer
  50°, armadillo 32° or 0 rolled up, axolotl and dolphin 1°). Dolphins,
  tadpoles, nautiluses and axolotls use `SmoothSwimmingLookControl`.
  The game's own eased head code is gone; since Track 2d only the
  phantom keeps its own head (below).
- **`BodyRotationControl`** turns a separate drawn body (`Mob::body_yaw`,
  Java's `yBodyRot`) apart from the heading movement follows (`yRot`): a
  moving body faces its travel with the head held within the limit; a
  still one follows a head that moved over 15°, and after ten still ticks
  turns to face it over ten more. A mob carrying a mob passenger doesn't
  turn while still. The move control turns the heading 90° a tick.
- **Idle look goals** in each kind's `registerGoals` priorities and flags:
  `LookAtPlayerGoal` (players, `Mob.class`, striders, guardians; 2% a full
  update, 20 to 39 full updates), `InteractGoal` and
  `LookAtTradingPlayerGoal` (wandering trader) and `RandomLookAroundGoal`
  (2% a full update, 20 to 39 ticks, only while nothing better holds
  MOVE). Attack, tempt, breed and follow-owner goals hold LOOK; a mob
  steered by a mob passenger loses MOVE and LOOK (`updateControlFlags`).
- **Brain looks**: `LookAtTargetSink(45, 90)` with each activity's
  writers: villagers' full and minimal look behaviours by their schedule
  and the trading player within 4 blocks; piglins' idle, avoid, celebrate
  and ride looks; brutes', hoglins', zoglins', creakings', allays', copper
  golems', armadillos', goats', frogs', axolotls', tadpoles' and sniffers'.
  `NEAREST_VISIBLE_LIVING_ENTITIES` is sensed every 20 ticks for the kinds
  whose looks pick mobs.
- **Spears**: the margin hit's block check toward the box's centre (for
  players' stabs too); mob stabs on minecarts and boats, decorations and
  End crystals; the body turns at the move control's 90° a tick.

## Eyes, look priorities and own look controls (v1.0 gap Track 2d, implemented, unverified)

The [Track 2d record](verification/2026-11-27-v1.0-gap-track-02d-look-details.md)
has the Java sources, checks and remaining differences. Code:
`src/entity/mob/eyes.rs` and `src/entity/mob/head.rs`.

- **Eyes**: every mob looks, sees and is looked at from its own
  `getEyeHeight` (`EntityTypes`' `eyeHeight`, each kind's
  `BABY_DIMENSIONS`, sleeping villagers, sitting camels, a digging sniffer
  or warden, phantoms and cubes by size, a floor shulker's lid), and
  players from their pose's (1.62, 1.27 crouching, 0.4 swimming).
- **Goal priorities**: an idle look goal takes `MOVE` and `LOOK` only from
  goals of a strictly worse priority and stops them (`GoalSelector`), so
  the parrot's `LookAtPlayerGoal` (1) holds off `FollowOwnerGoal` (2), the
  witch's random look (3) its patrol and village goals (4), the panda's
  (10) its stroll (14), and the shulker's (1) its attack (4).
- **Brain writers**: merchants follow their actual customer; villagers'
  `SetLookAndInteract`, `ShowTradesToPlayer` (40 ticks, 900 while the
  player holds an offer's cost) and `WorkAtPoi` looks; the piglin's
  `SetLookAndInteract`; brain `AnimalMakeLove`, `FollowTemptation` and
  `BabyFollowAdult` set `LOOK_TARGET`; brain attacks go through the sink;
  a look `RunOne` starts its `DoNothing` while a target is set.
- **Kinds' own rules**: the pollinating bee keeps its pitch and looks at
  its flower; the panda's `setTarget` after a failed breed and its
  unhappy turn; the llama's `RangedAttackGoal` aims 40 ticks at its target
  before it spits; the warden's `LOOK_TARGET` (`SetWardenLookTarget`,
  roar, fight, disturbances) and the sonic boom's look; the shulker's
  `ShulkerLookControl` in its shell's frame with no pitch and a still body;
  the bat's random head and plain controls; the phantom's head a tick
  behind its body; the ghast's and happy ghast's controls.
- **Spears** reach the dragon's parts: a mob's stab lands and does nothing
  (only players and explosions hurt the dragon); a player's jab or charge
  deals the part damage.

## Brain idle movement, the fox's hunting, baby boxes and shown trades (v1.0 gap Track 2e, implemented, unverified)

The [Track 2e record](verification/2026-11-27-v1.0-gap-track-02e-behaviours.md)
has the Java sources, checks and remaining differences. Code:
`src/entity/mob/brain_walk.rs`, `src/entity/fox.rs`,
`src/entity/mob/fox.rs`, `src/entity/mob/eyes.rs` and
`src/entity/mob/head.rs`.

- **Brains walk `WALK_TARGET`** with `MoveToTargetSink` (150 to 250 ticks,
  babies at play 80 to 120, sniffers 500 to 700) instead of wandering.
  Villagers run the idle package's movement `RunOne` (`InteractWith` a
  villager, a breedable villager or a cat within 8, `VillageBoundRandomStroll`,
  `SetWalkTargetFromLookTarget`, `JumpOnBed`, `DoNothing`), the babies' play
  `RunOne`, the meet gate at the bell (`StrollAroundPoi`, `SocializeAtBell`)
  and, without a home at night, the rest `RunOne`
  (`SetClosestHomeAsWalkTarget`, `InsideBrownianWalk`, `GoToClosestVillage`);
  work without a job site and meeting without a bell fall back to idle.
  Piglins run their idle movement `RunOne` (stroll, `InteractWith` a
  piglin, follow the look target unless a loved item is in sight) and,
  celebrating, a look, a 2-block stroll or nothing. The sniffer's idle
  `RunOne` starts its scent (also during the sniff cooldown) and sniff and
  walks and looks.
- **Baby boxes** are each kind's `BABY_DIMENSIONS` (villagers and zombie
  kinds and piglins 0.49 by 0.98, chickens 0.3 by 0.4, rabbits 0.24 by
  0.4, hoglins and zoglins 0.75 by 0.85, squid 0.5 by 0.5, foxes and
  armadillos 0.6 of the adult, horses 0.7, turtles 0.3), for collision,
  hits and navigation.
- **Shown trades**: a villager showing its trades holds up the results of
  the offers the player's item pays for, cycling every 40 ticks, and keeps
  looking for 900 ticks.
- **Foxes** hunt, sleep, perch and eat berries by Java's goals; see
  [Animals](animals.md#foxes-v10-gap-tracks-2e-and-2f-implemented-unverified).

## Work strolls, play tag, trading walks, the brute's idle, the fox's defence and the goat's long-jump pose (v1.0 gap Track 2f, implemented, unverified)

The [Track 2f record](verification/2026-11-28-v1.0-gap-track-02f-behaviours.md)
has the Java sources, checks and remaining differences. Code:
`src/entity/mob/brain_walk.rs`, `src/entity/mob/head.rs`,
`src/entity/villager.rs`, `src/entity/fox.rs`, `src/entity/mob/fox.rs`,
`src/audio/fox.rs` and `src/entity/animals/lifecycle.rs`.

- **Working villagers** no longer walk onto their job site and stand
  there. `SetWalkTargetFromBlockMemory(JOB_SITE, 0.5, 9, 100, 1200)` walks
  one more than 9 blocks (`distManhattan`) away back within 9, heads 15
  blocks toward it from beyond 100 and gives up a job site it can't reach
  for 1200 ticks; then the work package's `RunOne` picks `WorkAtPoi` (or
  the farmer's `WorkAtComposter`: within 1.73 of the job site, every 300
  ticks on a coin flip, it looks at it for 60 ticks and restocks),
  `StrollAroundPoi` (within 4, every 180 ticks), `StrollToPoi` (within
  10, every 80), `StrollToPoiList` (a farmer's farmland within 4 across
  and 2 up or down, every 100), `HarvestFarmland` (a farmer breaks a ripe
  crop within a block of it, with `mobGriefing`) and `UseBonemeal`, by
  Java's weights.
- **Playing babies** play tag (`PlayTagWithOtherKids`): with other babies
  in sight they chase the least-chased one (or the closest), and a chased
  one runs off to a spot in the village; the play `RunOne` waits while
  babies are in sight.
- **Trading villagers** walk to each other: `TradeWithVillager` locks both
  villagers' looks and walks on each other (0.5, 2 short) when it starts
  and while they're within `sqrt(5)`, for its 60 ticks, then the gate
  erases the interaction target.
- **Piglin brutes** run their idle movement `RunOne` (strolls,
  `InteractWith` a piglin or a brute, `StrollToPoi` and `StrollAroundPoi`
  on the bastion spot they spawned at, `DoNothing`) instead of wandering
  and the 12-block walk home.
- **Foxes** defend players they trust (`DefendTrustedTargetGoal`), stroll
  through villages at night, wake and take cover in thunderstorms, have
  their own sounds and kick up snow when faceplanted; see
  [Animals](animals.md#foxes-v10-gap-tracks-2e-and-2f-implemented-unverified).
- **Goats** take Java's `LONG_JUMPING` pose for the flight of their jump:
  the box at 0.7 of its size (`Mob::goat_long_jumping`).

