# VoxelCraft roadmap

[← Back to VoxelCraft](../README.md) · [Documentation index](README.md) · [Contributing](../CONTRIBUTING.md) · [Architecture](architecture.md) · [Development](development.md)

## Current plan — updated 2026-10-17

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

Releases follow the [release plan](#release-plan): small, themed versions from
v0.8 to v1.0, taken in order with no dates. Work on the earliest unreleased
version; v1.0 is everything in this roadmap.

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
and mod/plugin APIs. They are deferred under the maintainer's allowance
for less-used side features, not blanket exclusions of associated core
content. M0 must record the exact boundary: for example, a loot-bearing
structure still needs a playable exploration/reward loop even if its
archaeology mechanics are deferred.

**M0 boundary — approved by the owner on 2026-10-19** (proposed 2026-10-17).
These deferrals are the `Deferred` rows DEF-01..06 of the
[requirement inventory](requirements.md#deferred-side-features); everything
in the right-hand column stays mandatory for 1.0.

| Side feature | Deferred past 1.0 | Stays in 1.0 core |
|---|---|---|
| Advancements and statistics | The advancement and statistics screens, toasts and criteria | Recipe unlocking and the recipe book (UX, v0.18); the dragon/Wither progression itself (v0.16) |
| Archaeology | Brushes, suspicious sand and gravel, pottery sherds and decorated-pot patterns | Trail ruins, desert wells and warm ocean ruins as explorable structures with their non-archaeology loot (v0.12); sniffers obtainable some other Java way, or recorded as a fidelity exception when v0.13 plans them |
| Commands | The full command and selector language | The cheat console's testing commands (game mode, rules, time, weather, teleport, give, kill, difficulty, spawn point) |
| Custom skins | Skin selection and loading | The default player models |
| Resource and data packs | Loading packs | The built-in Java 26.3 recipes, loot tables and tags they would replace |
| Mod and plugin APIs | All of it | — |

There are no approved fidelity exceptions. The terrain-noise decision
(port Java's noise or record original noise as an exception) is due before
v0.11; until then original noise is a known difference.

Modern biomes, building materials, mobs, equipment and travel mechanics
belong in 1.0 when they participate in the chosen core loops. Being introduced
in a recent release does not make a feature optional.

New split-screen polish, agent-specific features, graphics modes unrelated
to Java fidelity, mobile/console/web ports and vanilla network/save-format
interoperability are outside the current core milestone unless the maintainer
explicitly changes the scope. Internal save stability is required; importing
Minecraft saves is a separate compatibility project.

## 2. Status and tracking rules

Use one current record per requirement, kept in the
[requirement inventory](requirements.md), with these fields:
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

Shipped in v0.7.0; see the [delivery record](#33-v070-delivery-record--2026-10-14).

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

### 3.3 v0.7.0 delivery record — 2026-10-14

v0.7.0 is [PR #33](https://github.com/BrendanH18/VoxelCraft/pull/33) from
`work/v0.7-assessment`: release 1111 and headless 1024 tests pass, the desktop
LAN harness passes 13/13, and the owner played the release build and reported
it good (no itemised play-test record). That is integration and play evidence,
not per-feature acceptance: no requirement below moves to **Verified**, because
each still lacks its recorded acceptance cases and the M5 performance baseline
is invalid.

What v0.7.0 moved, by requirement group (details, commits and skip reasons in
the [defect inventory](verification/2026-10-13-v0.7-defect-inventory.md)):

- **PLY-01/04/05 (Partial):** one shared action contract in `PlayerSession`
  for every input path (mining, placement, melee, use chain, bows, consumption,
  container screens); `Game::puppet` removed; Java attack-strength charge,
  criticals by fall distance, minimum attack charge, Haste/Mining Fatigue attack
  speed, interaction and screen ranges, crosshair indicator.
- **ENV-05 / MOB-01 (Partial):** propagated gameplay skylight, cave spawning and
  torch suppression against fixtures, redstone-conductor start cells, 24-block
  spawn-point exclusion, Peaceful `notInPeaceful` list, Nether spawn costs and
  fuzzy biome zoom, light work budgets, End/Nether ambient light (visual review
  pending). Shape-aware occlusion and Java spawn ring/caps remain.
- **MOB-02 (Partial):** line of sight clips collision boxes; navigation was still
  straight-line steering in v0.7.0 (ground paths followed on 2026-10-14).
- **BLD / PRO (Partial):** Java tool rules and shear loot, snow layers, sapling
  stage, hyphae axis, vertical barrels, copper tools/armor/nuggets, nugget
  smelting, crossbow Multishot/Piercing/Quick Charge, Creative armor equip.
- **MOB-03..06 / FOO (Partial):** villager beds, doors, farming, promotion delay,
  gossip and golem hostility; piglin celebration and crossbows; witches; camels;
  herd coats; fishing tables, luck and hooking; food effects.
- **AUT (Partial):** hopper cooldown, unloaded scheduled ticks, pressure plates,
  dispensers, minecart water/fuse/dismount, note and tripwire feedback.
- **GEN (Partial):** amethyst geodes, jungle temple piece and puzzle, iron-door
  strongholds, 26.3 structure loot, igloo depth.
- **M0 baseline recorded:** the [2026-10-16 M5 baseline](verification/2026-10-16-m5-baseline.md)
  has all 15 runs valid; the 240 FPS gate fails in streaming and one survival run.

#### v0.7 mob worker corrections — 2026-10-13

Dragon-egg teleport attempts now match Java 26.3's XYZ draw order and require
non-air support, dimension build height and the default world-border bounds.
Bytecode fixture and automated End-world coverage are implemented; player,
particles and Java RNG sequence parity remain unverified. See the
[mining review](verification/2026-10-13-review-mining-fixes.md).

#### v0.7 redstone defect follow-up (2026-10-13)

Wooden pressure-plate contact regressions now cover XP orbs, primed TNT,
splash potions, fireballs and bobbers with the Java 26.3 touch column.
The missing bobber contact and orb/potion box errors are corrected.
Automated evidence is in the redstone tests; ordinary-player and performance
acceptance remain open. See [redstone](redstone.md) for scope and reference.

Player defect follow-up (2026-10-13): Creative armor use now follows Java
26.3 copying/swapping and Binding rules, covered through the shared action.
Implemented, unverified; no ordinary-player acceptance.

Player combat defect follow-up (2026-10-13): critical eligibility reads tracked
fall distance in shared melee and LAN PvP, with Java 26.3 bytecode fixtures and
regression cases. Implemented, unverified; ordinary-player acceptance remains.

Combat charge and its indicator now include Java 26.3 Haste/Mining Fatigue
attack-speed modifiers. No reference enchantment changes attack speed.

The Java minimum-attack-charge gate is present; all registered items have zero
minimum. Its only nonzero reference users are the unregistered spears.

Blindness now persists by Java ID and blocks sprinting/criticals across input
paths, with an appended icon and per-view fog. Ordinary visual acceptance is open.

Player and combat defects (2026-10-13, w2): all implemented and unverified; no
ordinary-player acceptance. Monster spawn gear scales by local difficulty;
splash potions add every effect to mobs and effects keep hidden lower instances;
witches pick potions from the target and drink as Java does; golden apples are
always edible, Creative eats without using food up, and pufferfish, rotten flesh,
raw chicken and golden apples give Java's effects (new Nausea, Luck and Bad Luck
effects have icons only); world-spawn respawns use Java's `PlayerSpawnFinder`;
the death fall counts from the fatal tick; the fishing hook snags mobs and reads
luck and biome at the reel; bow and crossbow arm poses exist. Gaps found: no
Absorption (golden apple), no held spawn weapons for mobs, no abandoned camp,
copper horse armor, spears or armor equip sounds (so copper gear has no loot or
equip sound), no `@a`/`@r`/`@e`/`@n` selectors, no LAN jitter buffer, no
Adventure-world default or world border in the spawn search, hooked item
entities, and no automated controller-screen/bed/respawn harness.

#### v0.7 building defect follow-up (2026-10-13)

Implemented with automated evidence on `work/fix-w1-building`: mining input
continues through missed rays and adapter errors; a new re-aimed target starts
at zero. Ordinary-player input and visual acceptance remain open.

Shared item and bow actions defer to usable blocks (including hunger-dependent
cake), and Adventure redstone/composter use is implemented with automated
evidence. Normal input-path acceptance remains open.

Mud collision now uses the pinned Java 14/16 height, matching the existing
soul-sand shape. Physics landing regression checks pass; ordinary-player and
performance acceptance remain open.

Hay bales now apply Java 26.3's 0.2f fall multiplier before flooring, through
the shared player environment for all three bale axes. Reference threshold
and shared landing regressions cover the calculation; player acceptance is open.

## 4. Core requirement catalogue

Assessment follow-up (2026-10-13): [shared player-session and consumption work](verification/2026-10-13-player-session.md)
implements actor ownership for core player state and one fixed-tick food/drink
transaction path across client and CLI adapters. Dried kelp timing and CLI
harming-potion death drops were corrected. PLY-03/04 remain **Partial**: full
food/effect parity, other shared actions, controller/LAN state-swap removal and
ordinary player/performance acceptance were still open at the time; the shared
actions and the state-swap removal later landed in v0.7.0 (see the
[delivery record](#33-v070-delivery-record--2026-10-14)).

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
PLY-04 remains **Partial**; Conduit Power and
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
and incremental LAN replica relighting (measured microbenchmarks only). The
chunk, entity, block-model and particle shaders now take the dimension's
`ambient_light` (Nether 0.1, End 0.25) and `visual/ambient_light_color`, and the
End and Nether use `visual/sky_light_factor` 0 instead of the old 0.65/0.42
stand-ins; this is **unverified visually** (see the
[world fixes](verification/2026-10-13-world-fixes.md#ambient-light-in-the-nether-and-end)).
Soul sand and mud use Java's 14/16 collision height (regression evidence,
player acceptance open).

Entities, held items and particles now sample propagated gameplay skylight,
with the old heightmap estimate restricted to unknown light cells. Wide-roof
and partly loaded border regressions cover the query; visual/player acceptance
remains open. See the [world fixes](verification/2026-10-13-world-fixes.md).

Natural Nether spawns now apply Java 26.3 biome spawn costs, including pack
members and fortress attempts: soul sand valley charges and warped forest
enderman spacing are covered by extracted data and potential-field tests.
Ordinary-player and representative scene acceptance remain open.

The day now lasts 24000 ticks (20 minutes), and rain/thunder use independent
Java 26.3 timer ranges and five-second fades. Saved day fractions and older
weather entries still load. Reference range, fade and reload regressions pass;
ordinary-player weather acceptance remains open.

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

The [second recording](verification/2026-10-15-m5-baseline.md) (2026-10-15,
`793d3e0`) has 12 of 15 valid runs. Presentation is uncapped (470–634 FPS
average, about 1.55 ms median), with occasional 120 Hz-paced stretches. The
gate fails: streaming stalls about 450 frames over 50 ms per run (1% low
17 FPS), and only automation passes every run. Two harness defects (the
entity scene counting chunk-generation herds, and an unchecked simulation
distance) were fixed in `903884e`.

The [third recording](verification/2026-10-16-m5-baseline.md) (2026-10-16,
`903884e`) is the first valid baseline: all 15 runs valid, so PERF-01 is
Verified. Eleven runs pass the 240 FPS gate. Building, entities and automation
pass every run; survival fails one run through stretches of 8.33 ms display
pacing. Streaming fails every run: a burst of about eight 50–150 ms frames
every 32 blocks of travel once the camera leaves the preloaded area (1% low
16–18 FPS). Profiling that per-chunk-row work is the first performance task.

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

### Release plan

Adopted by the owner on 2026-10-15. Each release is a small, themed slice of
the milestones below, taken in this order. There are no dates: a version
ships when its own scope passes the checks in [§7](#7-feature-completion-and-release-policy).
Unfinished work moves to a later version instead of holding a release open or
shipping half-done. Patch releases (0.8.1 and so on) carry fixes only.
There are no release candidates or pre-releases (owner, 2026-10-17): a version
is tagged only once its scope is Verified, and testers get that release.

Work on the earliest unreleased version. Start a later one early only where
its dependencies allow and it doesn't stall the current one. Version 1.0 is
every mandatory requirement in this roadmap, Verified; nothing in it is
optional for 1.0 unless the owner records a deferral or fidelity exception.

| Version | Theme | Milestones | Main content | Status |
|---|---|---|---|---|
| v0.8 | Foundations | Finish M0, M1 | Valid M5 baseline and the per-requirement inventory; mob pathfinding, Java spawn categories, shape-aware light, simulation distance; pause/death/save scenarios on every input path | In progress: most implementation merged (#34, #35); strider lava paths, pause/death/save scenarios and the [requirement inventory](requirements.md) on `t3/roadmap-next-stage-v0-8`; the [M5 baseline](verification/2026-10-16-m5-baseline.md) is recorded (15/15 valid; gate fails in streaming); 2026-10-17/18: sweeping and knockback, absorption, game rules and difficulty, per-chunk entity storage, bed stand-up positions, fire tick containers and mob goals merged there; 2026-10-19: swimmer, ghast and airborne blaze movement (blazes on the ground keep VoxelCraft's ground movement), then the dolphin and turtle goal tables, the elder guardian's home and blaze retaliation; 2026-10-20: streaming re-run (bursts gone; two isolated frames over 50 ms remain); player acceptance remains |
| v0.9 | Block states | M2: BLD-04, BLD-06 | Top and double slabs, waterlogging, door hinges, floor/ceiling buttons and levers, wall grindstones, corner-stair light; reserved state ranges for later content | In progress: 2026-10-20 `STATE_CAPACITY` raised to 16,384 with ranges reserved for v0.9–v0.17 ([record](verification/2026-10-20-state-capacity.md)); slab types and stairs halves and corners ([record](verification/2026-10-20-slabs-stairs.md)) door hinges, floor/ceiling switches, wall/ceiling grindstones and lanterns ([record](verification/2026-10-20-doors-mounts.md)), and waterlogging with Java fence, wall and pane connections ([record](verification/2026-10-20-waterlogging.md)) implemented, unverified in play |
| v0.10 | Building materials | Close M2 | Missing wood, stone, copper and colour family members (pale oak, bamboo, mud brick, prismarine), the unregistered reference fuel items, signs, banners, item frames, cauldrons, soul lanterns and campfires; sponges, coral and sea pickles; falling and dripping dripstone, tilting dripleaf | Planned |
| v0.11 | Terrain | M3: GEN-01..03 | Java climate and density terrain with overhangs; Java caves, aquifers and ores; deep-dark content, springs, glow lichen; giant spruce and jungle trees and the remaining biome features | Planned |
| v0.12 | Structures | M3: GEN-05..07 | A Java structure-template (jigsaw) system; villages, outposts, bastions, fossils, shipwrecks, ocean ruins and monuments built from it; buried treasure and explorer maps; Java loot random sequences | Planned |
| v0.13 | Wildlife | M4: MOB-03/04, FOO, ENV-03 | Bats, frogs, pandas, polar bears, armadillos, mooshrooms, strays, bogged, phantoms and zombie horses (with lightning); drowned conversion and the cat spawner; melon, pumpkin and beetroot crops; animal and mount acceptance | Planned |
| v0.14 | Deep dark and trials | M3 + M4 + PRO-07 | Ancient cities with sculk and the warden; trial chambers with breezes, trial spawners, vaults and the mace; tridents, spears and the other modern weapon families | Planned |
| v0.15 | Villages and raids | Close M4 | Working lecterns, looms, cartography tables and stonecutters; illagers, woodland mansions, raids; remaining villager behaviour; the M4 exit checks | Planned |
| v0.16 | The End and the Wither | Close M3; M5 | End cities and ships (the last GEN-05 structures, then the M3 exit checks), shulkers and shulker boxes, elytra and fireworks, dragon respawn; Wither summoning, Nether stars and beacons; the no-cheat progression playthrough | Planned |
| v0.17 | Redstone and automation | M6 | Java update order, pistons with entity push and slime/honey, the crafter, remaining sensors and workstations; reference farms, clocks and sorters across save/reload | Planned |
| v0.18 | Polish and performance | M7: UX, §6 | Settings, key rebinding, accessibility and subtitles, recipe discovery, asset and sound review in play; the 240 FPS gate in every scene | Planned |
| v1.0 | Acceptance | M7 exit, §8 | Every mandatory row Verified; fresh-world and long-session playthroughs; save-compatibility policy; installer and update checks; current stable Java scope reconciled | Planned |

Notes:

- v0.13 to v0.15 interleave M3 and M4 because structures need their residents:
  ancient cities need the warden, trial chambers breezes and bogged, mansions
  and raids the illagers. M4's exit checks run at v0.15; M3's run at v0.16,
  once End cities and ships (GEN-05) exist alongside shulkers and elytra.
- M2 comes before structures because Java templates use the full set of block
  states (waterlogged stairs, top slabs, hinged doors).
- v0.11 needs an owner decision first: port Java's terrain noise so seeds
  reproduce Java's worlds (§1.3, GEN-01), or record original noise as an
  approved fidelity exception. Until then, original noise is a known difference.
- Recheck the latest stable Java release when planning each version (§1.2) and
  slot new core content into an unreleased version as an explicit scope change.
- When a version ships, set its status here, add a delivery record like
  [§3.3](#33-v070-delivery-record--2026-10-14) and list its unfinished members
  in the release notes. Moving an item to another version is an edit to this
  table, recorded with the reason.
- After 1.0: multiplayer (dedicated servers, per-player LAN and split-screen
  dimensions) and the side features in [§1.4](#14-what-can-follow-10) get their
  own v1.x roadmap.

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

#### v0.8 scope: finish M0 and M1

v0.8 ships when these exit checks pass. Work items, each with Java 26.3
fixtures and acceptance cases:

1. **M0 close-out:** a valid five-scene M5 baseline on the reference machine
   (idle, window visible) and the owner's decision on measuring 240 FPS against
   a 120 Hz display. *Baseline recorded 2026-10-16*
   ([record](verification/2026-10-16-m5-baseline.md), `903884e`): 15/15 runs
   valid, presentation uncapped apart from intermittent 8.33 ms stretches.
   The owner decided on 2026-10-17 (§6.1): simulation distance stays at 160
   blocks, the 8.33 ms stretches are display pacing and don't count as
   failures, and streaming's budget is no frame over 50 ms with p99 at most
   8.33 ms. Every later change is measured against it. Its periodic streaming stalls came from
   dungeon layout on the main thread; workers now do it, and a valid re-run
   ran on 2026-10-20 ([record](verification/2026-10-17-streaming-stalls.md),
   [re-run](verification/2026-10-20-streaming-rerun.md)): the bursts are gone,
   but one valid run had two isolated frames of 56 and 65 ms, so the budget is
   not yet met. The
   per-requirement inventory (one row per requirement with a state and
   acceptance cases, §2) is also an M0 exit check. *First version written*
   ([requirements.md](requirements.md), 2026-10-15): 119 rows; only PERF-01 Verified (2026-10-16);
   keep it current with each change. The side-feature boundary (§1.4) was
   approved on 2026-10-19 and added as the Deferred rows DEF-01..06.
2. **MOB-02 navigation:** Java's `PathNavigation`/node evaluators for walking,
   swimming and flying mobs, respecting block shapes, hazards, doors and
   fluids; villagers reach beds and job sites, bastion and structure residents
   path home. Replaces straight-line steering.
   *Ground navigation implemented, unverified* ([evidence](verification/2026-10-14-mob-navigation.md)):
   `WalkNodeEvaluator`, `PathFinder` and `PathNavigation` ported from 26.3 with
   per-mob costs; chasers, villagers, strolls, panics and following animals walk
   paths, and non-swimmers sink in water. Swim, amphibious and fly evaluators,
   wall climbing and block-update recomputation followed
   ([evidence](verification/2026-10-14-mob-navigation-swim-fly.md)), then
   `StriderPathNavigation`, `StriderGoToLavaGoal` and Java's half-block lava
   standing height ([evidence](verification/2026-10-15-strider-lava-paths.md)).
   Java's 26.3 goal selector for the shared movement goals (priority tables,
   panic, breed, tempt, follow parent and owner, avoid) and `Entity.push`
   separation followed ([evidence](verification/2026-10-17-mob-goals-movement.md));
   Java paths ignore entity boxes, so that former gap was inaccurate.
   Swimmer, ghast and airborne blaze movement followed: each kind's 26.3
   `MoveControl` and `travel` one tick at a time, fish schools,
   `BreathAirGoal`, squid strokes, the ghast's float and fire goals, the
   blaze's hover and attack goal, and `canMoveDirectly` for water, flying and
   amphibious paths ([evidence](verification/2026-10-19-swimmer-flyer-movement.md)).
   Blazes on the ground keep VoxelCraft's ground movement. Dolphins and
   turtles then got their 26.3 movement goals on a port of Java's flagged
   `GoalSelector`: swimming with players, treasure, item play, leaps, boats,
   guardian avoidance and pod retaliation; travel, going home, going to water,
   egg laying and panic. Left out are the dolphin's `TryFindLiquidGoal`
   (which has no effect) and nautilus following (no nautiluses), and the look
   goals only hold their flags; they don't turn heads. Baby dolphins are
   missing. The elder guardian's home and blaze retaliation came with them
   ([evidence](verification/2026-10-19-dolphin-turtle-goals.md)). Player
   acceptance remains.
3. **MOB-01 spawn rules:** Java's `NaturalSpawner` placement, 24/128-block
   ring, per-category caps scaled by loaded chunks, despawn distances and
   persistence, structure spawn overrides, and the slime/drowned paths folded in.
   *Monster category and despawning implemented, unverified*
   ([evidence](verification/2026-10-14-natural-spawning.md)): spawnable chunks,
   global and per-player caps, the 128-block disc and `checkDespawn`. Creature,
   water, underground-water and axolotl categories, Java biome spawn lists,
   chunk-generation herds and structure overrides followed
   ([evidence](verification/2026-10-15-spawn-categories.md)). Missing mobs (bats,
   polar bears, frogs and others) and an in-game check remain.
4. **ENV-05 / PLY-02 block shapes:** shape-aware light occlusion (slabs,
   stairs, snow layers), meshes lit from the authoritative gameplay light
   arrays, and raycasts/placement against full collision and outline shapes
   (top/double slabs, cross plants).
   *Light shapes and outline targeting implemented, unverified*
   ([evidence](verification/2026-10-14-block-shapes-light-interaction.md)):
   Java face-mask occlusion for slabs, stairs, snow and soil; meshes use the
   gameplay light arrays; targeting uses outline shapes on every input path.
   Corner-stair light shapes remain; top/double slab states need new IDs (M2).
5. **PLY-01 loaded-region activity and save integrity:** tick only simulated
   chunks as Java does, frame-rate-independent outcomes, and
   pause/load/unload/death/save regression scenarios across desktop, controller
   and agent paths.
   *Simulation distance implemented, unverified*
   ([evidence](verification/2026-10-14-simulation-activity-save-integrity.md)):
   Java's entity/block-ticking tickets in 16-block chunks gate random ticks,
   fluids, fire, containers, redstone, entities and mobs; unloaded scheduled
   ticks keep their delay; falling blocks save. One shared death and respawn
   step now serves the desktop, controller, LAN and CLI paths, and the pause
   rule and cross-process piston/repeater mid-motion saves are tested
   ([evidence](verification/2026-10-15-death-pause-save-scenarios.md)).
   Entities in unloaded chunks now leave the level into per-chunk storage and
   return with their chunk ([evidence](verification/2026-10-17-entity-storage.md));
   fire ticks follow `LevelTicks` containers and bed respawns and wake-ups use
   `findStandUpPosition` ([evidence](verification/2026-10-17-bed-standup-fire-ticks.md));
   all 59 game rules are inventoried and difficulty effects follow 26.3
   ([evidence](verification/2026-10-17-game-rules-difficulty.md)). Player
   acceptance remains.

Player acceptance for every v0.8 row follows the
[v0.8 acceptance checklist](verification/v0.8-acceptance-checklist.md): one
desktop pass, a physical-controller pass, a scripted CLI session and the
streaming benchmark, all on one recorded commit.

Carried known issues that belong to later releases (v0.9: waterlogging, door
hinges, wall/ceiling switches, top/double slabs; v0.11: 3D terrain, mega trees;
v0.12: structure templates; v0.13: the missing mobs; v0.17: Java redstone update
order) stay listed in the release notes until their release.

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
  Simulation distance is the game's default 10 chunks (160 blocks), recorded
  with each run (owner, 2026-10-17). Java comparisons state the distance
  each side used.
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
  Streaming budget (owner, 2026-10-17): on the streaming route, **no frame over
  50 ms and p99 at most 8.33 ms**. The full steady-state gate on streaming is a
  v0.18 target, not a v0.8 requirement.
- Display pacing (owner, 2026-10-17): stretches of consecutive frames at exactly
  8.33 ms, the built-in display's 120 Hz pacing despite `Immediate`
  presentation, are not counted as game frame-time failures. The 2026-10-16
  baseline stands without a re-run for them; record such stretches with each
  run.

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

There are no promised dates. Version numbers follow the [release plan](#release-plan);
a version ships when its scope is verified, and unfinished work moves to a
later version rather than shipping incomplete.
Keep one focused source commit per feature/fix on the assigned branch,
following the [contribution workflow](../CONTRIBUTING.md). Keep roadmap updates
reviewable alongside implementation changes and release notes.

## 8. Final acceptance record for 1.0

- [ ] Current stable Java reference and exact core scope recorded.
- [ ] Every mandatory core feature individually Verified with evidence.
- [ ] Side-feature deferrals resolved (boundary approved 2026-10-19, §1.4); fidelity exceptions explicit.
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
