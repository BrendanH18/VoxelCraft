# VoxelCraft roadmap

[← Back to VoxelCraft](../README.md) · [Documentation index](README.md) · [Contributing](../CONTRIBUTING.md) · [Architecture](architecture.md) · [Development](development.md)

## Current plan — updated 2026-10-12

Build a faithful recreation of modern Minecraft Java single-player gameplay
in Rust. Version 1.0 must deliver complete core survival, building,
exploration and progression, with very high quality and performance.
Match Java's mechanics, terrain generation and structure layouts as exactly
as practical. Multiplayer follows the single-player 1.0 milestone.

Reference hardware is an M5 MacBook Pro. The performance requirement
is **240 FPS minimum at 1920 x 1080 physical pixels and a 256-block view
distance** (Java's 16 chunks; VoxelCraft's current `--rd 8`). Also measure
native-resolution performance; a native-resolution numerical gate is not yet
specified.

Original handmade textures, models, sounds and music are allowed when they
improve quality. Keep procedural generation where it is useful. All
implementation and shipped assets remain original or appropriately licensed.

A feature is complete only after correctness checks, player testing,
save/reload checks and performance verification. Dates remain flexible.
Milestones follow dependencies and acceptance evidence, not a fixed release
calendar or the number of implementation tasks completed.

Immediate order:

1. M0: establish the reference version, requirement inventory and test/benchmark fixtures.
2. M1: correct the simulation, player, lighting, spawning, navigation and save foundations.
3. M2: complete block/item families and their gameplay integrations.
4. M3: match Java terrain, features, structures and exploration.
5. M4: complete mobs, animals, villages, farming, travel and weather interactions.
6. M5: verify the entire survival and equipment progression.
7. M6: complete core automation, workstation and storage behavior.
8. M7: finish presentation, usability, stability and the 1.0 acceptance pass.

Work can overlap where dependencies permit. Bring existing working features
up to the acceptance standard; do not rebuild them merely because this
roadmap is new.

Existing multiplayer, split-screen, controller and CLI-agent capabilities
receive maintenance and integration fixes, not new feature expansion ahead
of the single-player goal.

Retain the pending v0.7.0 branch and assess its useful single-player work before
integration. The [restructuring plan](restructuring.md) records that assessment
and six concrete architecture/verification tasks. Those code and asset tasks
are planned, not implemented by the documentation migration.

This tracked document is the authoritative project roadmap. Update it with
the code as scope or verified behavior changes. Milestones and performance
figures below are acceptance targets unless explicitly recorded as measured
results; they are not promises that the current release already meets them.

## 1. Product target and scope

### 1.1 The experience version 1.0 must deliver

A player can install the game, create a world, survive the first night,
explore recognizable modern Java biomes and structures, build with complete
material families, farm and keep animals, trade with villagers, travel
through all three dimensions, brew and enchant, upgrade equipment, defeat
the bosses and continue playing in a durable world. Ordinary building,
resource gathering, transport, storage and automation work as an experienced
Java player expects.

The game feels finished in routine use: readable interfaces, responsive
controls, coherent visuals and audio, correct collision, dependable saves,
smooth terrain streaming and predictable mechanics. A feature with a name,
texture or creative-menu entry but no survival acquisition or function does
not satisfy this target.

Creative mode is required as a building mode and validation tool. Survival
and its difficulty settings are the primary acceptance path. Preserve
existing Hardcore, Adventure and Spectator behavior; deeper mode-specific
scope is assessed in the requirement inventory rather than assumed complete.

### 1.2 Reference-version policy

Research baseline as of 2026-10-12: **Minecraft Java Edition 26.3**, released
2026-09-15. The 26.4 development line is still snapshots at this review.
Use stable released behavior, not snapshot experiments, as the parity target.

Sources:

- [Java 26.3 released notes](https://feedback.minecraft.net/hc/en-us/articles/48913133328013-Minecraft-Java-Edition-26-3).
- [Java 26.4 Snapshot 3](https://www.minecraft.net/de-de/article/minecraft-26-4-snapshot-3).

Record the exact reference version in each behavioral fixture, recipe/loot
inventory and comparison report. Older source or data may help explain a
mechanic but cannot override changed behavior in the selected reference
version. Distinguish Java from Bedrock rules.

Recheck the latest stable Java version at milestone planning and before 1.0.
Record new released core features as explicit scope changes with their
dependencies and acceptance checks. Resolve those changes before calling
1.0 equivalent to the current core game. Do not silently freeze the target
at Java 1.21 or move all modern content after 1.0.

### 1.3 Fidelity policy

Java gameplay is the default contract: recipes, acquisition, drops,
durability, combat, movement, lighting, spawning, AI, fluids, growth,
redstone, containers, world generation and structures.

For terrain, pursue matching algorithms, seed behavior, biome placement,
large terrain features, structure starts and structure layouts. Matching
Java names or approximate distributions alone is insufficient. Validate
block-level reference fixtures where practical, including negative
coordinates and chunk borders. The existing original heightmap and compact
structure layouts are partial implementations under this target.

Engine internals may differ: 32-cube chunks, compact storage, GPU packing,
caches, threading and rendering techniques are implementation choices.
They must preserve observable gameplay and generation behavior.
Original art and audio can reproduce the recognizable style and feedback
without duplicating Java's asset files.

Document every intentional observable difference with its reason, affected
requirements and player impact. A fidelity exception is a scope decision
for the maintainer, not something an implementation agent may hide under
"performance," "simplification" or "practical." Optimize the matching
behavior first; do not silently remove simulation work or content to hit FPS.

### 1.4 What can follow 1.0

Multiplayer hosting/joining, dedicated servers, internet services and
independent multiplayer dimensions follow 1.0. Existing implementations are
preserved and regression-checked when shared code changes.

Candidates for later side-feature work include the full advancement/
statistics interface, archaeology-specific interactions, the complete
command/selector language, custom skins, resource/data-pack compatibility,
and mod/plugin APIs. These are proposed deferrals under the maintainer's allowance
for less-used side features, not blanket exclusions of associated core
content. M0 must record the exact boundary: for example, a loot-bearing
structure still needs a playable exploration/reward loop even if its
archaeology mechanics are deferred.

Modern biomes, building materials, mobs, equipment and travel mechanics
belong in 1.0 when they participate in the chosen core loops. Being introduced
in a recent release does not make a feature optional.

New split-screen polish, agent-specific features, graphics modes unrelated
to Java fidelity, mobile/console/web ports and vanilla network/save-format
interoperability are outside the current core milestone unless the maintainer
explicitly changes the scope. Internal save stability is required; importing
Minecraft saves is a separate compatibility project.

## 2. Status and tracking rules

Use one current record per requirement, with these fields:
`ID | scope/reference | state | dependencies | code/asset evidence |
acceptance cases/results | known differences | next action`.

States:
- **Unassessed:** no current comparison against the selected Java reference.
- **Missing:** no functional implementation.
- **Partial:** implementation exists but content, behavior or integration is incomplete.
- **Implemented, unverified:** behavior is implemented but required acceptance evidence is incomplete.
- **Verified:** all applicable completion checks passed for the recorded commit and scope.
- **Deferred:** explicitly outside the current milestone, with a reason and destination.

A shipped release is not a completion state. A passing test suite is not
proof that all requirements are implemented. Record desktop, headless,
controller and agent evidence separately when applicable.

A group below contains multiple requirements; split it into individual
rows in M0 before claiming the entire group Verified. Every missing member
of a material family, recipe, loot pool or mob behavior remains visible.
Never mark a whole group done because its first feature landed.

Evidence must identify the tested commit, Java reference, fixture or manual
steps, platform and result. Preserve failed acceptance cases until resolved.
After an integration change, recheck the affected behavior and its dependent
systems. Don't erase a defect by changing its test to match the wrong behavior.

Keep the active roadmap focused on present scope, dependencies, status and
next work. Keep implementation details and benchmark reports in the development
documentation; release notes describe only delivered behavior.

## 3. Starting inventory — evidence, not a new completion claim

Baseline for this rewrite: `main` at `022ca44`, package version 0.6.0.
The 2026-10-11 review passed formatting, strict release Clippy and existing
release tests in both desktop and headless configurations. Desktop:
670 library + 78 executable + 5 integration tests passed; three ignored.
Headless: 670 library + 5 integration tests passed; two ignored.
Three-dimension headless smoke and an rd=8 meshing/streaming benchmark passed.

Those results do not satisfy the new full fidelity or 240 FPS acceptance
gates. The review did not complete a fresh-world survival playthrough,
physical-controller testing or a representative 1080p frame-time suite.

Current broad state:

- **Partial under the new target:** simulation/player/save foundations;
  survival and End progression; enchanting/brewing/Netherite; materials and
  shaped blocks; villages and redstone; Nether content; modern Overworld
  generation/structures/aquatic mobs; presentation and options.
- **Existing implementations to retain:** fixed 20 Hz simulation, device-free
  library, gameplay block light, compact saves, crafting/containers, physics,
  survival, three dimensions, progression entities, procedural assets,
  particles/music, third-person poses, controller players and CLI agents.
- **Work outside this baseline:** assess later changes at their merged commit
  before importing completion claims. Pending work is not counted as shipped
  or verified in the 0.6 inventory.

### 3.1 Confirmed 0.6 audit follow-ups

These findings belong in the new requirement records. They were reproduced
or traced on the baseline above; recheck whether later branch work fixes them.

- **BLD-01 / BLD-02:** pale-oak and bamboo stairs/slabs/fences/doors are absent;
  bamboo mosaic has no recipe; mud-brick and prismarine shaped families are
  incomplete. Relevant sources: [world/forms.rs](../src/world/forms.rs), [crafting.rs](../src/crafting.rs).
- **ENV-04:** dry sponges do not absorb neighboring water. Wet-sponge smelting
  exists. Relevant sources: [world/mod.rs](../src/world/mod.rs), [world/overworld_blocks.rs](../src/world/overworld_blocks.rs).
  Follow-up: sponges drain waterlogged live coral and continue the search;
  wet sponges dry only on Nether placement. Sea pickle draining and dry
  live-coral states need reserved block states.
- **PRO-02:** pufferfish is not accepted for brewing Water Breathing.
  Relevant source: [world/brewing.rs](../src/world/brewing.rs).
  Follow-up: normal/splash brewing and interrupted brewing-save regressions
  are implemented on the assessment branch; ordinary-player acceptance remains.
- **BLD-02 / AUT-03:** dried kelp blocks, bamboo and bamboo mosaic lack fuel
  handling; kelp, berries, moss and new flowers lack composter integration.
  Relevant sources: [world/furnace.rs](../src/world/furnace.rs), [world/composter.rs](../src/world/composter.rs).
  Follow-up: shared fuel metadata and reference-value/save cases now cover the
  named missing fuels and additional existing wooden forms. Composter coverage
  and the full fuel catalogue remain incomplete.
- **FOO-03:** fishing pools still omit existing tropical fish, pufferfish and
  nautilus shells. Relevant source: [entity/bobber.rs](../src/entity/bobber.rs).
  Follow-up: fishing rolls Java 26.3's complete `gameplay/fishing` tables
  (fish in closed water, open-water treasure, jungle bamboo, set_damage,
  enchant_with_levels 30 over `#on_random_loot`), checked against
  `fishing.json`. Remaining: Java random sequences, the player luck attribute
  and ordinary-player acceptance.
- **GEN-02 / BLD-03:** amethyst geodes do not generate; budding amethyst
  incorrectly drops with Silk Touch. Relevant sources: [world/terrain.rs](../src/world/terrain.rs),
  [world/overworld_blocks.rs](../src/world/overworld_blocks.rs).
  Follow-up: the 26.3 `amethyst_geode` feature now generates (see the
  [geode evidence](verification/2026-10-13-amethyst-geodes.md)), with six-way
  bud facings and wall bud states 2400–2415. **Implemented, unverified**: no
  Java-seed coordinate parity, player acceptance or representative
  performance acceptance. The Silk Touch drop is already corrected and covered
  by `tests/plant_integrations.rs`.
- **FOO-02:** bone meal on mangrove leaves does not create propagules;
  one dark-oak sapling grows a tree without the Java 2x2 requirement.
  Relevant sources: [world/overworld_flora.rs](../src/world/overworld_flora.rs), [world/growth.rs](../src/world/growth.rs).
  Follow-up: dark and pale oak saplings need a 2x2 (reference search order);
  bone meal on a lone one is not consumed. Sapling `stage` and spruce/jungle
  mega trees remain open.
- **MOB-03:** baby turtle growth runs while the turtle is unloaded/distant.
  Relevant sources: [entity/aquatic.rs](../src/entity/aquatic.rs), [entity/mod.rs](../src/entity/mod.rs).
- **PLY-04:** CLI water buckets attempt creature capture without a fallback
  for ordinary pouring. Relevant sources: [agent.rs](../src/agent.rs), [app/bucket.rs](../src/app/bucket.rs).
  Follow-up: shared bucket/bottle execution now provides that fallback and covers
  pickup, creative, overflow, Nether and payload cases. Full player/session
  extraction and player acceptance remain open.
- **FOO-02:** ripe cocoa randomly yields two or three beans instead of the
  reference loot's three. Relevant source: [world/overworld_flora.rs](../src/world/overworld_flora.rs).

Broader known partial areas include real skylight and shape-aware lighting,
light-based cave spawning, navigation, full material/state families, missing
workstation functions, anvil naming, additional brewing, world-generation
fidelity, turtle reproduction, dripstone/dripleaf behavior and complete
current-version content. The inventory must inspect these rather than trust
old "done" labels.

The assessment branch now implements the named building recipes, sponge water
absorption, composting metadata, fish weights, budding-amethyst loot, cocoa
count and sapling/propagule fixes. Reference, integration and process-restart
checks are recorded in the [content/resource report](verification/2026-10-12-content-and-assets.md)
and the [content review fixes](verification/2026-10-13-review-content-fixes.md).
Armor-equip block checks use the Java block interaction range.
These are implemented slices; ordinary-player acceptance, remaining state/
waterlogging fidelity and other identified mechanics remain open. The [complete reference fuel inventory](verification/2026-10-12-fuel-inventory.md)
now checks all 176 currently registered reference fuels and 48 non-fuel
exceptions; 171 reference fuel names remain missing registrations under
M0/BLD-01/02. Turtle helmets and normal/splash
Turtle Master potions now have reference-derived recipes, equipment/effect
behavior and persistence checks; see the
[turtle equipment report](verification/2026-10-12-turtle-equipment.md).
Their ordinary-player and performance acceptance remains open. Do not interpret the historical
findings above as either erased evidence or current Verified status.

### 3.2 Pending v0.7.0 work

The former branch `t3/continue-project-build`, inspected at `f5bf0d1` on
2026-10-12, contains animals, breeding, companions, leads, mounts, boats, LAN
work and additional tests. All of its commits were merged into
`work/v0.7-assessment` in `22ad25c`; the branch label was retired on
2026-10-13 at the owner's request. It is not part of the 0.6 baseline above.
The [initial assessment](verification/2026-10-12-v0.7-assessment.md) classifies
reviewed animal/transport work as **Partial**. It found modern lead/saddle
acquisition failing independent 26.3 cases; the `22ad25c` merge corrected both
recipes, and `retained_transport_recipes_match_java_26_3_and_consume_the_grid`
passes. Interaction, persistence and performance acceptance remain. Other
behaviors remain Unassessed, and none of these groups is Verified. Historical completion
reports do not override the current acceptance gates.

Review and retain useful single-player work under MOB-03/MOB-06, FOO-02/FOO-04,
AUT-05 and applicable player/save requirements. Preserve the LAN implementation
while deferring new multiplayer expansion. Review dependencies before choosing
a merge or focused cherry-picks. See the
[pending-work assessment plan](restructuring.md#pending-v07-work-retain-and-assess).

The same assessment delivered the first reference fixtures, shared fuel metadata,
Water Breathing integration and shared bucket actions. These fixes remain
Implemented-unverified under full player/performance acceptance. Retain the
pending branch's turtle activity gating and save-lock improvements for review;
do not assume every earlier audit gap still needs a duplicate implementation.

## 4. Core requirement catalogue

Assessment follow-up (2026-10-13): [shared player-session and consumption work](verification/2026-10-13-player-session.md)
implements actor ownership for core player state and one fixed-tick food/drink
transaction path across client and CLI adapters. Dried kelp timing and CLI
harming-potion death drops were corrected. PLY-03/04 remain **Partial**: full
food/effect parity, other shared actions, controller/LAN state-swap removal and
ordinary player/performance acceptance are still open.

The [shared mining follow-up](verification/2026-10-13-shared-mining.md) adds one
actor-owned destruction path, reference block reach and float progress, corrected
Mining Fatigue and ice support, and interruption/process-restart evidence.
PLY-04 and BLD-03 remain **Partial**; other actions, full shape/tool/timing parity
and player/performance acceptance remain open.

The [shared placement follow-up](verification/2026-10-13-shared-placement.md)
adds one actor-owned block placement transaction, paired-cell validation,
low-cube targeting and process-restart evidence. PLY-04 and BLD-02/03 remain
**Partial**: complete item-use ordering, shape/state/support parity and ordinary
player/performance acceptance remain open.

The [shared combat follow-up](verification/2026-10-13-shared-combat.md)
moves player melee into the session, replaces the fixed weapon cooldown with
Java's attack-strength charge and corrects reach and tool attack attributes
against a reproducible 26.3 fixture. The desktop shows Java's crosshair attack
indicator from the session charge, and right-click use is shared. PLY-04/05
remain **Partial**: copper tools, armor and nuggets are registered (reserved
ids 1050..=1069), metal gear smelts to nuggets and monster armor rolls the 26.3
tiers; spears and other weapon families, session-owned container transactions
and player acceptance remain open.

The [bucket review fixes](verification/2026-10-13-review-bucket-fixes.md) give
entity and block use priority over bucket actions through one shared ordering
used by desktop and CLI, deny fluid placement in Adventure, pour on the hit face
as Java does and drop destroyed plants. PLY-04 remains **Partial**.

The [mining review fixes](verification/2026-10-13-review-mining-fixes.md)
share Java's five-tick destroy delay through the session for every adapter,
key progress on position and held item, restrict Creative swords, destroy
Creative dragon eggs, suppress lower door/double-plant drops without the
correct tool or in Creative, gate exhaustion on the correct tool and add Haste.
Open block screens, trading and vehicle storage now close at Java's interaction
range + 4 (eye to box) on desktop, controller, LAN and CLI paths, covered by
`tests/shared_screen_ranges.rs`; ordinary-player acceptance remains.
Right-click use is one shared session chain for the desktop and CLI
([use evidence](verification/2026-10-13-shared-use.md)); the CLI gained the
desktop's entity, door, harvest, flint and tool uses, and riders can use items.
Controller and LAN players no longer borrow the host's state (`Game::puppet` is
removed); hands and container clicks run on each player's own session.
PLY-04 remains **Partial**; effect attack-speed modifiers, Conduit Power and
beacons, server destroy tolerance, a headless desktop-adapter harness and
remaining cancel-on-item-change sites (pick-block, drop) stay open.

The [lighting and spawning follow-up](verification/2026-10-13-lighting-spawning.md)
implements propagated gameplay skylight, End skylight, full-height ground-monster
attempts, cave/torch checks and reference clock/weather darkening. ENV-05 and
MOB-01 remain **Partial**: shape occlusion, growth integration, spawn caps/ranges,
structure overrides and ordinary player/performance acceptance are still open.
The [review fixes](verification/2026-10-13-review-lighting-fixes.md) add the
24-block spawn-point exclusion, thunder (> 0.9) and rain (> 0.2) thresholds,
reference daylight-detector output, the `isRedstoneConductor` start-cell check
and incremental LAN replica relighting (measured microbenchmarks only). The End
renders sky-lit terrain at 0.65 because the shaders lack the reference ambient
term (`ambient_light` 0.25, sky factor 0); soul sand and mud still collide as
full cubes.

The [amethyst geode follow-up](verification/2026-10-13-amethyst-geodes.md)
implements the Java 26.3 geode configured/placed feature with order-independent
chunk painting, adds the missing horizontal bud/cluster states and Java's
six-way budding growth. GEN-02 remains **Partial** overall; geodes are
Implemented-unverified (RNG/noise differ from Java, wall buds are drawn as
boxes, crack fluid ticks and pre-existing structures are not modelled).

The [first M5 scene recording](verification/2026-10-13-m5-baseline.md) is
invalid: streaming run 3 was paused (0 ticks), 11 of 12 non-streaming runs
sampled an occluded window rather than rendering, and the entity scene lost
its undead to daylight. The [harness fixes](verification/2026-10-13-review-bench-fixes.md)
reject paused, tick-short, unpresented and population-drifting runs, isolate
benchmark input and disable saves. The five-scene 1080p/256-block baseline
must be re-measured; no valid baseline or frame-time result exists yet. A smoke
run presented frames at exactly 8.333 ms, so a 120 Hz display may cap on-screen
frame rate below the 240 FPS target. Native resolution, profiling, hitch budgets
and the broader encounter matrix remain open; the performance target is not achieved.

All groups are in scope unless an individual requirement is explicitly
deferred or has an maintainer-approved fidelity exception. IDs are stable tracking
keys, not release versions or block/item ID allocations.

### PLY — Player, simulation and controls

- **PLY-01:** authoritative 20 Hz gameplay, correct random/scheduled ticking,
  render interpolation, loaded-region activity and pause behavior. No
  duplicate ticks, frame-rate-dependent outcomes or render-dependent rules.
- **PLY-02:** Java movement, acceleration, collision, step-up, sneaking,
  sprinting, jumping, swimming/crawling and appropriate poses/hitboxes.
  Climbing, fluid movement, fall/void damage and environment interactions
  work with every applicable block state.
- **PLY-03:** health, hunger/saturation/exhaustion, air, regeneration, damage
  immunity, status effects, death/respawn, XP, beds, difficulty and gamerules.
- **PLY-04:** one consistent action contract for building, mining, combat,
  item use and containers. Desktop is the primary player path; relevant
  existing controller and CLI paths remain functional when shared features change.
- **PLY-05:** equipment, armor/offhand/shields, attack timing, criticals,
  sweeping, projectiles, enchantment effects and all core weapon families.
  Correct tool wear, item remainders and attribution of hits/loot/XP.
- **PLY-06:** keyboard/mouse bindings, sensitivity/FOV, first/third-person
  cameras, view bobbing, interaction reach, targeting and clear feedback.

### BLD — Blocks, items, crafting and building

- **BLD-01:** complete in-scope wood, stone, metal, color and modern material
  families. Include their Java-supported shapes, orientations and recipes;
  don't invent counterparts Java lacks or omit functional family members.
- **BLD-02:** survival acquisition and complete crafting/smelting/fuel paths,
  yields, remainders, repair materials, compost eligibility and trading uses.
  Recipes accept exactly the appropriate alternatives.
- **BLD-03:** hardness, harvest/tool rules, drops, Fortune/Silk Touch,
  gravity, burning/explosion behavior, support removal and persistence.
- **BLD-04:** top/bottom/double slabs, normal/upside-down/corner stairs,
  doors/hinges, gates, trapdoors, walls, fences, panes, bars, ladders,
  wall-mounted blocks and waterlogging. Collision, lighting, culling,
  placement and neighbor updates agree on the actual state/shape.
- **BLD-05:** functional storage and ordinary building utilities, including
  double chests, barrels, signs, beds, lights and relevant decorative items.
  Inventory icons, held/dropped models and world geometry are consistent.
- **BLD-06:** sufficient registry capacity and versioned state encoding for
  the selected modern catalogue. Preserve existing IDs/migrations; do not
  silently reuse retired IDs or let unregistered states masquerade as features.

### GEN — Terrain, dimensions and exploration

- **GEN-01:** matching climate/biome/density generation, terrain heights,
  surfaces, rivers, mountains, oceans and dimension bounds. Same seeds and
  coordinates reproduce the selected Java reference behavior in fixtures.
- **GEN-02:** Java caves/carvers/aquifers, ore distributions and exposure
  rules, deepslate, geodes, springs and lush/dripstone/deep-dark content.
- **GEN-03:** correct trees, flora, fungi, reefs, ocean vegetation, ice,
  snow and other biome features; deterministic feature interactions and
  continuation across chunk borders.
- **GEN-04:** Nether and End biome/terrain/features, portal placement and
  travel, arrival safety, dimension-specific rules and separated persistence.
- **GEN-05:** all structures needed for core exploration/progression, with
  Java placement rules, layouts, terrain alignment, traps, block entities,
  residents, spawners and appropriate loot.
  Includes villages/dungeons/mineshafts, temples/huts/igloos/outposts,
  shipwrecks/ruins/monuments/buried treasure, strongholds, fortresses/
  bastions, End cities/ships, mansions, ancient cities and trial chambers.
  Include relevant structures from the selected newer stable releases.
- **GEN-06:** maps/compasses/clocks and exploration rewards needed to find
  and use core content. Loot acquisition is meaningful without cheat commands.
- **GEN-07:** generation independent of chunk request order, unloading,
  parallelism and cache eviction. No clipped structures, border mismatches,
  floating unsupported features or respawned looted containers/residents.

### MOB — Spawning, AI and the living world

- **MOB-01:** actual sky/block light, biome/dimension/structure tables,
  category caps, spawn surfaces/volumes, activation distance, despawning,
  persistence and difficulty rules.
- **MOB-02:** navigation and line of sight that respect block shapes,
  hazards, doors, vertical routes, water and unloaded regions. Correct
  targets, cooldowns, attacks, equipment and observable Java behaviors.
- **MOB-03:** relevant passive/aquatic mobs, ages, breeding/feeding, wool
  regrowth, egg laying, taming/companions, buckets, leads, naming and persistence.
- **MOB-04:** relevant hostile/neutral mobs, illagers, guardians, boss-adjacent
  mobs and modern core roster; complete drops and associated resource loops.
- **MOB-05:** villagers, jobs/POIs, schedules, all required professions,
  workstation behavior, trading/restocking, reputation, breeding, golems,
  zombie conversion/cures, wandering traders and raids.
- **MOB-06:** matching models, animations, effects and sounds. Displayed
  attack state, movement and equipment agree with simulation; death and
  conversion preserve or drop inventory according to the reference.

### FOO — Food, farming and travel

- **FOO-01:** core crops/foods, stem crops, mushrooms, berries, cocoa,
  cooking, milk/stews/cake/pie, bees/honey and related acquisition.
- **FOO-02:** Java growth, bone meal, support/light/water conditions, soil,
  trees/propagules, harvesting, yields and renewable farming loops.
- **FOO-03:** fishing behavior, open-water rules, complete relevant loot,
  rod enchantments, wear and XP.
- **FOO-04:** boats/chest boats and rafts, carts/rails, saddles, mounts,
  steering, storage, dismount safety and matching travel physics.
  Modern core travel content is included when selected in M0.

### PRO — Survival and equipment progression

- **PRO-01:** a fresh survival world can reach the Nether, acquire blaze/
  pearl ingredients, find a stronghold, activate its portal, defeat the
  dragon, return through the exit and continue safely without cheats.
- **PRO-02:** brewable in-scope potions, ingredient acquisition, effects,
  drink/splash/lingering delivery, tipped arrows and related item use.
- **PRO-03:** XP awards/pickup/death loss, enchanting offers/bookshelves/
  lapis, all required enchantment behaviors, repair/renaming/grindstones.
- **PRO-04:** ancient debris/scrap/ingots, bastion templates, Netherite
  upgrades and correct metadata/durability/repair/fire behavior.
- **PRO-05:** End crystals/dragon respawn, gateways, outer-End rewards,
  shulkers/boxes, elytra and fireworks.
- **PRO-06:** Wither summoning/fight, Nether stars, beacons and their
  acquisition, construction, range and effect rules.
- **PRO-07:** core modern progression/equipment/encounters, including trial
  content and new weapon/material families in the selected reference.
  Each must be obtainable and useful in survival, not creative-only.

### ENV — Environmental systems

- **ENV-01:** fluid flow, source formation, waterlogging, lava/water
  reactions, buckets, bubble columns and appropriate entity/item behavior.
- **ENV-02:** fire/lava ignition, spread, extinguishing, burning,
  explosions/TNT and block/entity/item resistance.
- **ENV-03:** rain/snow/thunder/lightning, weather exposure, accumulation/
  melting/freezing and difficulty-dependent environmental interactions.
- **ENV-04:** functional special blocks: sponges, dripstone/dripping/falling,
  dripleaf tilt, mud and other in-scope cave/ocean/biome interactions.
- **ENV-05:** authoritative skylight/block light and shape-aware
  propagation/occlusion for gameplay, rendering, spawning and growth.

### AUT — Automation, workstations and storage

- **AUT-01:** Java redstone signal rules, dust/connectivity, switches/
  plates/torches, repeaters/comparators, timing, update order, lighting,
  targets/daylight/tripwire and applicable modern sensors.
- **AUT-02:** pistons/sticky pistons, observers, moving collision/entity
  displacement, block reactions and Java-relevant contraption behavior.
- **AUT-03:** dispensers/droppers/hoppers, all in-scope special dispensing,
  sided transfers, composters, automatic crafting and related devices.
- **AUT-04:** furnaces/smokers/blast furnaces, brewing, enchanting, anvils,
  smithing, grindstones, stonecutters and other core workstation interfaces.
  Job-site registration alone does not make a workstation functional.
- **AUT-05:** storage, container metadata, carts/boats and interaction
  transactions without duplication, deletion or missed comparator updates.
- **AUT-06:** representative Java farms, clocks, sorting, transport and
  manufacturing work after unload/reload and save/restart.

### UX — Presentation, usability and distribution

- **UX-01:** coherent original textures/models/icons, readable states and
  appropriate animation. Original handmade assets can replace weak
  procedural ones; document asset provenance and runtime/memory costs.
- **UX-02:** clear sound cues, music, particles, weather/lighting and
  combat/item feedback. Verify trigger, timing, visual density and audio
  quality through gameplay, not only synthetic-output tests.
- **UX-03:** title/world management, seed entry/display/copy, settings,
  key rebinding, GUI scaling/accessibility/subtitles, inventory/search,
  recipe discovery and every core container/workstation interface.
- **UX-04:** install/start/save/quit/reopen/update behavior on the existing
  supported desktop platforms. M5 macOS is the performance reference;
  Windows and Linux retain correctness/build regression coverage.
- **UX-05:** graceful errors, world backups/migration, recovery guidance
  and a clearly documented 1.0 save-compatibility policy.

## 5. Dependency-ordered milestones

Each milestone stays open until its exit checks pass. Reuse working code and
land focused changes; a milestone is not permission for a blanket rewrite.

### M0 — Reference, inventory and verification tools

Deliver the per-feature catalogue against the selected current stable
reference, classify existing main and pending-branch work, and make every
required acquisition/use/drop path explicit. Resolve proposed side-feature
deferrals and deliberate fidelity exceptions.

Create repeatable Java comparison fixtures and representative M5 benchmark
worlds/routes. Establish reliable frame-time, simulation, streaming, memory
and generation measurements. Identify registry/state capacity limits before
adding further content.

Exit: every core requirement has a state and acceptance cases; no unresolved
"modern," "basic" or "done" labels hide its scope. The performance suite runs
on the reference machine and records a reproducible baseline, whether or not
that baseline yet reaches 240 FPS.

### M1 — Correct single-player foundations

Close PLY and ENV-05 foundation gaps, particularly lighting, block-shape
interaction, spawn rules, navigation, loaded-region activity, action
consistency and save integrity. Diagnose architectural performance issues
before adding more expensive systems.

Exit: indoor/cave spawning and torch suppression match Java fixtures;
representative mobs navigate terrain and hazards; collision/light queries
agree with placed states; pause/load/unload/death/save tests preserve state;
simulation outcomes don't change with render frame rate. Existing headless,
controller and agent regression paths remain functional.

### M2 — Complete materials and interactions

Close BLD, the confirmed 0.6 integration defects and relevant ENV/FOO block
rules. Complete family members, states, acquisition, recipes, tool/loot/fuel/
compost/repair integration and ordinary building utilities.

Exit: sample every required family through survival acquisition, crafting,
placement, interaction, mining and save/reload. Sponges drain water;
supported fuels burn; compostables compost; buds/trees/cocoa drop and grow
correctly. A builder can use all required orientation/shape/waterlogging
states with consistent collision and lighting.

### M3 — Java terrain and exploration

Close GEN. Replace observable approximations that fail the selected
reference fixtures. Implement missing cave/biome/features and core structure
families with correct layouts, residents and loot. Preserve edited worlds
through an explicit generation-version migration strategy.

Exit: a documented seed/coordinate corpus matches Java biome, terrain,
feature and structure fixtures to the agreed fidelity contract.
Generate neighboring chunks in different orders/threads and across cache
evictions; compare outputs and inspect borders. Explore each core biome/
structure family without cheats, obtain its rewards, and prove cleared
containers/residents stay cleared after restart.

### M4 — A complete living world

Close MOB, FOO and relevant ENV rules: animals and companions, villagers/
raids, aquatic/hostile mobs, crops/trees/fishing, mounts/travel and weather.

Exit: play renewable farming, breeding/taming, trading/cure/raid, fishing,
travel and environmental scenarios. Validate creature category caps and
behavior in day/night, lit caves, Peaceful and all dimensions.
Persistent mobs, ages, carried items, homes/jobs and mount/storage state
survive unloading and restart without continuing unloaded-only timers.

### M5 — Complete survival progression

Close PRO using the correctly generated/acquired content from prior
milestones. Recheck existing End progression rather than presume it remains
valid after terrain, AI, combat and lighting changes.

Exit: a fresh-world, no-cheat playthrough covers first-night survival,
equipment progression, Nether access, potion/enchantment/upgrades, the
stronghold/dragon/return path, post-dragon End travel, Wither/beacons and
the selected modern progression. Separate repeatable scenarios cover
dragon respawn, elytra/fireworks, shulker storage and difficult encounters.
Record failed or impractically unreliable acquisition paths as defects.

### M6 — Dependable building and automation

Close AUT and remaining core utility UI. Match Java contraption behavior;
do not label queued propagation or approximate pistons sufficient by default.

Exit: reference clocks, doors, piston mechanisms, sorting/hopper systems,
farms, rail/storage transport and crafting/processing fixtures work with
Java timing and resource accounting. Repeat after chunk unload, save/
restart and interruption during scheduled/moving updates. No duplication,
inventory loss or dead scheduled machinery.

### M7 — Quality, performance and 1.0 acceptance

Close UX, finish all requirement records and run the full completion matrix.
Polish textures/models, movement/feedback, sound, menus and accessibility
where actual play exposes problems. Optimize feature-heavy terrain, AI,
lighting, simulation and rendering without weakening the parity contract.

Exit: all mandatory core rows are Verified; every remaining exception/
deferral is explicit; current stable Java scope has been reconciled;
fresh-world and long-session acceptance passes are recorded; save migration/
stability and distribution are checked; the reference M5 performance gate
passes across the representative scene suite. No unresolved core
correctness, acquisition, persistence or release-blocking performance defects.

## 6. Performance acceptance

### 6.1 Reference configuration and frame-time gate

- Reference M5 MacBook Pro; record CPU/GPU variant, RAM, macOS and build commit.
- Release build, 1920 x 1080 **physical render pixels**, 256-block view range.
  Record the simulation distance separately; begin with Java's 8 chunks
  (128 blocks) for the repeatable comparison setup.
- Normal intended visual/audio settings, full in-scope gameplay, no hidden
  entity/particle/content removal. Record all options.
- VSync/frame caps disabled for measurement. Native-resolution runs are
  reported separately; don't substitute logical Retina window dimensions.
- Steady-state gate: **1% low at least 240 FPS** (the reciprocal of the
  mean of the slowest 1% of frame times), with **p99 frame time at most
  4.17 ms**. Also record average, p50, p95, p99, worst frame and any distinct
  hitch. These statistics are complementary, not interchangeable. This is
  the roadmap's measurement definition of the stated minimum target,
  not a new measured claim.
- Measure cold streaming and interaction stalls separately; don't discard
  them to present steady-state numbers as whole-session performance.
  M0 establishes explicit streaming/hitch budgets before these are marked Verified.

Use a consistent power/thermal setup, a warm-up period, at least three
60-second measured runs per steady-state scene and a longer streaming route.
Publish per-scene results, not only an average across easy and hard scenes.
An offscreen origin sweep or meshing microbenchmark alone cannot pass the gate.

### 6.2 Representative scenes and systems

Cover open terrain, dense forests, mountains, underground caves, oceans/
monuments, villages/raids, Nether forests/basalt deltas, the dragon fight,
weather, busy farms/redstone/storage and normal menus/inventory overlays.
Specify reproducible mob counts, activity, weather and interaction sequences.

Measure:
- CPU/GPU frame times and render submission/upload costs.
- Fixed-step simulation latency; meet 20 Hz without dropped gameplay ticks.
- Generation, decorations, lighting and meshing per chunk, including cold
  feature-heavy biomes and structures.
- Streaming movement and rapid edits/explosions/fluid changes.
- Save/load time, memory use and bounded cache/queue growth over a long session.
- Native-resolution and longer-view-distance scaling, reported separately.

Set system budgets from the measured baseline and profile bottlenecks.
A proposed internal tick-time budget must not replace the frame-time gate
or the no-skipped-ticks requirement. Compare against the selected Java
reference at equivalent view/simulation distances and scenes when making
relative performance claims.

## 7. Feature completion and release policy

For every individual feature, all applicable checks must pass:

1. **Reference correctness:** documented selected-version behavior and meaningful
   comparison/regression cases, including edge conditions.
2. **Survival completeness:** acquire it without cheats; craft/process, use,
   repair and dispose of it correctly. No essential input is creative-only.
3. **Integration:** tool/loot/enchantment, inventory/container/automation,
   light/physics/AI and relevant existing input paths agree.
4. **Player testing:** interact through the normal UI and controls. Inspect
   appearance/animation and listen to sound; a screenshot or constructor
   test alone is insufficient.
5. **Persistence:** save/restart and unload/reload relevant state, including
   interrupted work, carried components, scheduled actions and cleared loot.
6. **Performance:** measure the feature in representative loaded gameplay;
   no unbounded caches/queues or unexplained regressions.
7. **Delivery evidence:** record the commit, scope, test results, manual
   steps/captures and remaining differences; update the requirement state.

Use the repository's required pre-commit chain for source changes:
`cargo fmt && cargo clippy --release --all-targets -- -D warnings && cargo test --release`.
Also run relevant headless checks, shader validation, cross-platform CI
and applicable benchmark/player/save checks. Tests that merely mirror the
implementation do not establish Java correctness.

Minor releases can contain completed subsets of the roadmap, but shipped
features must meet their defined contract. Label omitted members and
approved scope limits explicitly. Don't publish a half-functional item or
system as complete simply because its partial code builds.

There are no promised milestone dates or automatic version increments.
Assign release versions when a coherent, verified scope is ready.
Keep one focused source commit per feature/fix on the assigned branch,
following the [contribution workflow](../CONTRIBUTING.md). Keep roadmap updates
reviewable alongside implementation changes and release notes.

## 8. Final acceptance record for 1.0

- [ ] Current stable Java reference and exact core scope recorded.
- [ ] Every mandatory core feature individually Verified with evidence.
- [ ] Proposed side-feature deferrals resolved; fidelity exceptions explicit.
- [ ] Fresh-world survival progression and continued post-boss play passed.
- [ ] Building, exploration, farming, animals, villagers, travel and automation scenarios passed.
- [ ] Player-facing interfaces, controls, feedback and original assets reviewed in play.
- [ ] Save/restart/unload, migration, interruption and long-session stability passed.
- [ ] M5 1080p/256-block 240 FPS gate passed in every required steady-state scene.
- [ ] Streaming/hitch budgets passed; native-resolution and scaling results reported.
- [ ] Existing supported-platform checks and installer/update smoke passed.
- [ ] Existing controller/agent/multiplayer shared-code regressions addressed.
- [ ] Release documentation accurately states delivered scope and remaining limitations.

Multiplayer and additional side features receive their own roadmap after
this single-player acceptance milestone. They are not prerequisites for 1.0.

### v0.7 building defect follow-up (2026-10-13)

Implemented with automated evidence on `work/fix-w1-building`: mining input
continues through missed rays and adapter errors; a new re-aimed target starts
at zero. Ordinary-player input and visual acceptance remain open.
