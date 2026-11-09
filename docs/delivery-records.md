# VoxelCraft delivery records

[← Back to VoxelCraft](../README.md) · [Documentation index](README.md) · [Roadmap](roadmap.md) · [Requirement inventory](requirements.md)

What each shipped or merged version delivered, and the evidence and
decisions recorded along the way. This text was moved verbatim out of the
[roadmap](roadmap.md) on 2026-11-09 so that the roadmap holds only present
scope, dependencies, status and next work (its §2). Section numbers and
headings are the ones the text had in the roadmap, so older links still
find it. These are active evidence records, not the
[archive](archive/README.md); they never establish a **Verified** state by
themselves, and the [requirement inventory](requirements.md) holds each
requirement's current state.

- [Starting inventory and the v0.7.0 to v0.9 delivery records](#3-starting-inventory--evidence-not-a-new-completion-claim)
  (the roadmap's former §3).
- [Assessment follow-ups, 2026-10-13 to 2026-10-16](#assessment-follow-ups-2026-10-13-to-2026-10-16)
  (formerly at the start of the roadmap's §4).
- Scopes of tagged versions: [v0.8](#v08-scope-finish-m0-and-m1),
  [v0.10](#v010-scope-updates), [v0.11](#v011-scope-menus-and-settings),
  [v0.12](#v012-scope-building-materials) and [v0.13](#v013-scope-terrain).
- [Release plan status of v0.8 to v0.13](#release-plan-status-of-v08-to-v013),
  as the roadmap's table recorded it before the cells were shortened.

Versions v0.14 to v0.16 are merged to `main` but not tagged; their scopes
stay in the [roadmap](roadmap.md#release-plan) until they are.

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

### 3.4 v0.8.0 delivery record — 2026-10-20

The owner declared v0.8 done on 2026-10-20. `v0.8.0` is tagged at `58b547b`
on `release/v0.8.0`: #39 (`5e9f025`) plus the version bump and release notes
([PR #41](https://github.com/BrendanH18/VoxelCraft/pull/41)). It holds #34,
#35 and #37–#39 (#36 was the release plan). #40, the first v0.9 work, is not
in it. The release workflow passed formatting, Clippy, tests, packaging and the
package smoke tests on Windows and macOS. That is build evidence, not
acceptance: no row moved to **Verified**, because the
[v0.8 acceptance checklist](verification/v0.8-acceptance-checklist.md) has not
been played and the streaming budget is not met.

What v0.8.0 moved (details in [§5's v0.8 scope](#v08-scope-finish-m0-and-m1)
and the [requirement inventory](requirements.md)):

- **M0:** the [M5 baseline](verification/2026-10-16-m5-baseline.md) (PERF-01
  Verified), the per-requirement inventory and the approved side-feature
  deferrals (DEF-01..06).
- **MOB-02 (Implemented, unverified):** Java 26.3 ground, swim, amphibious,
  fly, climbing and strider lava navigation; the shared goal tables; swimmer,
  ghast and airborne blaze move controls; dolphin and turtle goals.
- **MOB-01 (Implemented, unverified):** every spawn category from Java's
  biome lists and caps, structure overrides, chunk-generation herds and
  despawning.
- **ENV-05 / PLY-02 (Implemented, unverified):** shape-aware light occlusion
  and outline targeting.
- **PLY-01/03/05 (Implemented, unverified):** simulation distance, per-chunk
  entity storage, the shared death and respawn step, pause and mid-motion
  saves, game rules and difficulty, absorption, sweeping and knockback, bed
  stand-up positions.

Moved on when v0.8 was declared done: the player acceptance pass for the
v0.8 rows goes to v0.9, and PERF-03's streaming budget (two isolated frames
over 50 ms in the [2026-10-20 re-run](verification/2026-10-20-streaming-rerun.md))
goes to v0.20. The M0 and M1 milestones stay open until those pass.

### 3.5 v0.9 delivery record — 2026-10-22

The owner declared v0.9 done on 2026-10-22 and tags it `v0.9`, the first
two-part tag ([releases](releases.md)), once the branch is merged. It holds
#40 (slab types, stair halves and corners, door hinges, floor and ceiling
switches, wall and ceiling grindstones, lanterns, waterlogging, the 16,384-state
capacity) and the v0.9 completion work on `t3/e2f55514`, reviewed by Codex
before merging:

- **PLY-02a/b (Implemented, unverified):** Java 26.3 player movement, one
  tick per step: input, the latched sprint, collision, step-up, edge back-off,
  friction, jumping, climbing, water and lava travel, currents, poses
  ([record](verification/2026-10-21-java-player-movement.md)).
- **PLY-02c (Implemented, unverified):** cobwebs, berry bushes, bed bounce,
  the honey block and bottle ([record](verification/2026-10-21-honey-block.md)),
  scaffolding and powder snow ([record](verification/2026-10-21-scaffolding-powder-snow.md)).
- **BLD-04c/d (Implemented, unverified):** stored wall posts and doors that
  open when placed beside power
  ([record](verification/2026-10-21-wall-posts-powered-doors.md)); redstone
  stands on any sturdy top face.
- **Policy (owner, 2026-10-22):** the owner's own play is the player
  evidence (no formal checklist or physical-controller session, §7), and
  release tags are two-part.

No row moved to **Verified**: the windowed M5 scene suite with the larger
state table and the new content was deferred to v0.20 with the rows
(owner, 2026-10-22), next to PERF-03's streaming budget. The known
differences listed in each record (Nether lava currents, stricter support
for diodes and plates, mobs and items treating scaffolding and powder snow
as plain blocks, wall posts in old saves, no survival honey until bees) ship
as documented limitations.

## Assessment follow-ups, 2026-10-13 to 2026-10-16

These paragraphs opened the roadmap's §4 "Core requirement catalogue".

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

## v0.8 scope: finish M0 and M1

v0.8 ships when these exit checks pass. Work items, each with Java 26.3
fixtures and acceptance cases:

*Done (owner, 2026-10-20), tagged `v0.8.0`.* Every work item below is
implemented and the M5 baseline is recorded, but the player acceptance pass
(now with v0.9) and the streaming budget (PERF-03, now with v0.20) have not
passed, so the rows stay Implemented, unverified and the M0 and M1 exit checks
remain open. See the [delivery record](#34-v080-delivery-record--2026-10-20).

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

Player acceptance for the v0.8 rows was first planned as the
[v0.8 acceptance checklist](verification/v0.8-acceptance-checklist.md); on
2026-10-22 the owner replaced it with their own play, which covers v0.8 and
v0.9 together.

Carried known issues that belong to later releases (v0.9: waterlogging, door
hinges, wall/ceiling switches, top/double slabs; v0.13: 3D terrain, mega trees;
v0.14: structure templates; v0.15: the missing mobs; v0.19: Java redstone update
order) stay listed in the release notes until their release.

## v0.10 scope: updates

v0.10 is the stepping stone (owner, 2026-10-22): every player installs it by
hand once, and it ships as soon as an installed copy can update itself. It
adds no blocks, items or states. Work items:

1. **Updater (UX-04b).** Check, offer, download, verify, back up and install
   as described in [Updating VoxelCraft](updates.md), on Windows and Apple
   Silicon Macs. Owner decisions (2026-10-22): checks on by default, ask
   before every download, signed checksums, keep three pre-update backups,
   no v0.9.1. *Implemented, unverified:* `src/update/`, the title screen's
   update screen, **Check for Updates** in Options, the installer's update
   mode and the release workflow's signing step. Unit tests cover version
   parsing, release reading, signatures, checksums, backups and the full
   download-to-prepare path against a signed fixture; the Windows smoke test
   covers the installer's update mode. A real Mac app updated itself from a
   signed fixture and rolled back from a version that crashed at launch
   ([record](verification/2026-10-22-in-game-updater.md)); that run found and
   fixed a double launch. Windows has only been compiled, and the signing
   step first runs on the `v0.10` tag (the secret is set).
2. **Safe failures (part of UX-05a).** A world whose level file can't be
   read now goes back to the title with an error instead of starting fresh
   over it, and a failed save shows a message in game.

Exit: the release workflow signs a v0.10 build's checksums, checked on the
tag's draft release (`SHA256SUMS.txt.sig` attached) before publishing; the
owner updates a real v0.10 install to the next release on Windows and on a
Mac, keeping worlds and settings; a failed or cancelled update leaves the
installed version working. v0.10 ships before the second check, which needs
that next release (owner exception above).

## v0.11 scope: menus and settings

v0.11 ships when the menus below match Java 26.3 and work on every input
path. Java 26.3 is still the latest stable release (rechecked 2026-10-23;
26.4 is in snapshots).

Owner decisions (2026-10-23):

- Java options that need renderer or engine work VoxelCraft doesn't have yet
  (Clouds, Biome Blend, Entity Distance and the like) are left off the
  screens rather than shown as switches that do nothing, and are listed below
  as known gaps. Options that are cheap to make real are added: Brightness,
  Max Framerate, Damage Tilt, FOV Effects and Distortion Effects.
- The work runs as a shared menu toolkit first (screens, focus and
  navigation, text fields, dialogs, scrolling lists, controller input), then
  three parallel tracks: title and world management; options, controls and
  GUI scale; sound categories.
- A track that isn't ready moves to v0.12 instead of holding v0.11 open
  (patch releases carry fixes only).

Progress (2026-10-23), *implemented, unverified* (awaiting the owner's play
and a frame-time check). On 2026-10-24 a failed `options.txt` write gained its
notice (UX-05a), the scene suite gained `menus` and `title` scenes for the
frame-time check, and the release was prepared (Cargo 0.11.0, release notes).
The [first menu run](verification/2026-10-24-v0.11-menu-frame-time.md)
doesn't count: every frame was held to the display's 120 Hz, and its
building control scored 95 FPS 1% low against the baseline's 261–273. The
baseline commit `903884e`, rerun that night, was held to 120 Hz the same way
with the same scores, so the cap came from the machine's state (likely
battery power), not v0.9–v0.11. Repeat the check on an idle M5 on AC power:
`caffeinate -dis python3 tools/benchmark_scenes.py --scenes menus,title,building`.
On 2026-10-25 the owner chose to ship v0.11 without its exit checks; the
frame-time run moved to v0.20.

- **Toolkit** (`src/app/gui/`): a stack of screens with the same focus and
  navigation for mouse, keyboard (Tab, arrows, Enter, Escape) and a
  controller that hasn't joined as a split-screen player (D-pad, A, B).
  Sliders follow Java's editing rule: the mouse or Tab can adjust at once;
  arrow keys and the D-pad need Enter, Space or A first. Text fields
  support selection, word jumps and the clipboard. Also Java's scrolling
  option lists and labels, tooltips, and confirm and error screens.
- **Title and worlds** (`src/app/title/`): Java's title menu; Select World
  with search and Java's three-line rows; Edit World (rename, Make Backup,
  open the world and backups folders, a Seed button that copies it);
  Re-Create; Create New World with Game and World tabs; Multiplayer, Direct
  Connection and the update screen. Saves write `version=` to `level.txt`.
  A world saved by a newer version shows Java's downgrade prompt with a
  backup. A failed open, save, backup, rename or delete shows a notice
  saying what to do.
- **Options, controls and GUI scale** (`src/app/options/`, `src/app/keys.rs`):
  Java's Options, Video Settings (its three sections), Controls, Mouse
  Settings, Key Binds and Accessibility Settings, with Credits & Attribution.
  There is a binding table for every gameplay key and mouse button: Java's
  names, categories and defaults, saved as `key_` lines in `options.txt`,
  with click-to-rebind, conflicts and both resets. New options: GUI Scale
  (Auto and 1..max, also on the title screen), Max Framerate, Brightness,
  Sneak and Sprint Hold/Toggle, Scroll Sensitivity, Discrete Scrolling,
  Invert Mouse X/Y, Distortion Effects, FOV Effects, Damage Tilt, and Java's
  difficulty lock (saved per world). Render Distance is labelled in Java's
  16-block chunks.
- **Sound categories** (`src/audio/source.rs`): every sound plays in its
  Java 26.3 category. The mixer applies master x category volume live, and
  Music & Sound Options has a slider per category.
- Older `options.txt` files and worlds load unchanged.

Known gaps and differences, recorded per the owner decision above:

- Java options with nothing behind them are left off: Online, Skin
  Customization, Language, Chat, Resource Packs, Telemetry, Inactivity FPS
  Limit, fullscreen resolution, Graphics API, Biome Blend, clouds, mipmaps,
  Entity Shadows and Distance, menu blur, the attack indicator, Directional
  Audio, the output device, and the Voice slider.
- Also left off: Attack/Use Toggle, Auto-Jump and Allow Cursor Changes;
  Create New World's More tab, Allow Commands and World Type; and Edit
  World's Reset Icon, Optimize World and Export.
- There are no world icons, and Multiplayer lists LAN games only.
- Java 26.3 moved Difficulty to the pause menu's World Options; VoxelCraft
  keeps it on the Options screen. Graphics stays Classic/Enhanced.
- Key changes: R no longer sprints, the backtick no longer opens the
  console, and `-`/`=` no longer change render distance. Java has none of
  these, and each can be rebound.
- Not yet seen working in play: Brightness, the frame limit, GUI Scale in a
  window, the hold/toggle keys, the distortion and hurt-tilt effects, real
  clipboard and folder buttons, and the update screen with a live offer.
  The title screen has no frame limit.

Work items:

1. **Title screen (UX-03a).** Java's layout: Singleplayer, Multiplayer,
   Options… and Quit Game, with the version and update status.
2. **World management (UX-03a).** Java's Select World list with each world's
   name, folder, last-played time, game mode and the version that last saved
   it, warning before opening a world saved by a newer version. Edit World
   (rename, Make Backup, open the world and backups folders), Re-Create,
   copy seed and confirmed deletion.
3. **Options (UX-03b).** Java's Options screen and its sub-screens (Video
   Settings, Music & Sounds, Controls with Mouse Settings and Key Binds,
   Accessibility), keeping every existing option. Key rebinding for every
   keyboard and mouse action, with conflict highlighting and Reset Keys; GUI
   scale (Auto and fixed steps) that applies immediately; Java's sound
   category sliders for the categories the mixer can separate. Every option
   persists in `options.txt` and
   applies without a restart where Java's does. Subtitles stay with v0.20
   (UX-03d).
4. **Consistent menus (UX-03a/b).** Escape and the controller's back button
   behave the same everywhere; keyboard focus and controller navigation reach
   every widget; hover, focus and disabled states are visible; text fields
   support cursor movement, selection and paste; destructive actions confirm.
   Option tooltips follow Java's text.
5. **Errors and backups (UX-05a).** A failed world load, save, backup or
   update shows a screen that says what happened and what the player can do,
   instead of only logging it. Make Backup reuses the updater's backup code.

Exit: every menu item above works through mouse, keyboard and controller
(controller through the adapter tests) and its settings survive a restart;
title and menu frame time stays within the M5 budget.

## v0.12 scope: building materials

v0.12 closes M2 for every family whose material can be obtained in survival
today. Java 26.3 is still the latest stable release (rechecked 2026-10-23).
The [registry audit](verification/2026-10-23-v0.12-registry-audit.md)
(2026-10-23) compared VoxelCraft's names with Java 26.3's: 669 of Java's
1,286 blocks are missing, about 420 of them in v0.12's families. That is far
more than this row first listed, so the work is split into tracks, and a
track that isn't ready moves to v0.13 instead of holding v0.12 open (as in
v0.11). `cargo run --release --no-default-features --example registry_audit`
reports what remains.

Decisions, proposed on 2026-10-23 and approved by the owner on 2026-10-25:

- **State capacity first (BLD-06).** The audit's minimum is 3,795 new
  states, likely 6,000–8,000 with waterlogged partners and multipart
  blocks, against 4,643 available (v0.12's two ranges plus the unassigned
  reserve). Raise `STATE_CAPACITY` from 16,384 to 32,768, the most the
  current block-item encoding allows, measured the way the
  [16,384 raise](verification/2026-10-20-state-capacity.md) was, and give
  each track its own range above 16,384.
- **Families follow their source.** Blocks whose material has no survival
  source yet ship with it: poplar wood (its trees, v0.13), cinnabar and
  sulfur (sulfur springs, v0.13), resin (the creaking, v0.15), copper golem
  statues (the copper golem, v0.15), honeycomb blocks, bee nests and hives
  (bees, v0.15), heads and skulls (their mobs), purpur, end rods and shulker
  boxes (v0.18), and potted plants with their plants. The audit lists each
  block's release.
- **Waxing was deferred to bees.** Honeycomb acquisition, the 56 waxed
  copper blocks, copper waxing and sign waxing are implemented, unverified,
  with bees in v0.15 ([2026-11-04 record](verification/2026-11-04-v0.15-crops-bees.md)).
  Unwaxed copper oxidises in v0.12 and an axe scrapes it back.
- **Banners are plain in v0.12.** Patterns are applied at the loom, which
  arrives in v0.17, so banner patterns and their items come with it.
- **Retire the 11 invented blocks.** Calcite stairs, slab and wall,
  deepslate stairs, slab and wall, smooth stone stairs and wall and the
  polished granite, diorite and andesite walls have no Java counterpart.
  Remove their recipes and creative entries; blocks already placed stay and
  drop themselves, and their IDs are never reused.
- **Lightning rods without lightning.** Rods are crafted, placed in every
  facing and waterlogged in v0.12. Lightning strikes don't exist yet
  (ENV-03, v0.15), so attracting a strike, the redstone pulse it gives and
  stripping oxidation come with them. *Implemented, unverified (2026-11-04):*
  these lightning behaviors now exist in [v0.15 track 1](verification/2026-11-04-v0.15-lightning.md).
- **Jukeboxes and music discs** are unassigned; propose v0.20 with the
  music review, since every disc needs an original track.

Work items (all Missing unless noted):

0. **Capacity and allocation (BLD-06).** The raise above, chunk-format and
   LAN-protocol handling as in the 16,384 raise, and an ID range, item range
   and texture-layer range reserved per track in the
   [architecture document](architecture.md#v012-track-reservations).
   Done before the tracks start. *Implemented, unverified (2026-10-25):*
   32,768 states, `VXC4` chunk files and LAN protocol 3, plus a texture
   capacity raise from 2048 to 4096 layers, since only 273 were left; see
   the [record](verification/2026-10-25-state-capacity-32768.md). Owed: the
   cost of texture paging on the M5 once v0.12 passes 2048 layers.
1. **Wood (BLD-01a, BLD-04).** Axis states for overworld logs and the
   bamboo block (they have one state today); stripped logs and wood with axe
   stripping; `*_wood` blocks; buttons, pressure plates and trapdoors for
   spruce, birch, jungle, acacia, dark oak, mangrove and cherry, and the
   polished blackstone button and plate; shelves for every wood; standing,
   wall and hanging signs with Java's edit screen (built on v0.11's text
   fields), dye and glow ink (waxing comes with bees); the chiseled
   bookshelf. *Implemented, unverified (2026-10-25):* every block of the
   audit's wood list is registered (ids 16384–19583, `world::wood`): X and
   Z states for the nine logs and the bamboo block placed by the clicked
   face, the nine `*_wood` blocks, stripped logs, wood and bamboo with
   axe stripping, the seven trapdoor, button and plate families and the
   polished blackstone button and plate (oak and stone twins, so redstone
   is unchanged), 12 woods of standing, wall, hanging and wall hanging
   signs (Java's placement and support rules, both sides, an edit screen on
   the v0.11 text fields, dyes, ink and glow ink, text drawn in the world),
   12 shelves (three slots, redstone power, chains of three and hotbar
   swaps, comparator output) and the chiseled bookshelf (six books,
   comparator output, silk touch). See the
   [record](verification/2026-10-25-v0.12-wood.md); owed: play by hand,
   original sounds (v0.20), and the windowed performance check with the
   track's textures (v0.20). LAN, controller and CLI sign editing:
   *implemented, unverified (2026-10-29,
   [record](verification/2026-10-29-v0.12-finish.md))*.
2. **Stone and minerals (BLD-01b).** Polished tuff and tuff bricks;
   deepslate bricks and tiles with cracked and chiseled forms; quartz
   blocks; smooth, cut, chiseled and red sandstone; red nether bricks and the
   nether brick wall; end stone bricks; mossy stone brick, sandstone and
   brick shapes; blocks of coal and diamond, tinted glass and the slime
   block; retiring the invented blocks. The stonecutter's recipes are data
   here; its screen stays with v0.17.
   *Implemented, unverified (2026-10-25):* all 67 blocks of the audit's
   stone list (`src/world/stone_blocks.rs`), with every stairs, slab and
   wall state, the quartz pillar's axes, Java hardness, tools and drops,
   crafting, smelting and 99 stonecutter recipes as data, tinted glass
   blocking light and the slime block's bounce, friction, slowdown and
   fall-damage cancel; the 11 invented blocks left the recipes and palette
   ([record](verification/2026-10-25-v0.12-stone.md)). Owed: owner play,
   slime stickiness for pistons (pistons push no block structures yet,
   v0.19), and red sandstone in badlands terrain (v0.13). Block-of-coal fuel
   came with the data pass; mobs and items bouncing on slime: *implemented,
   unverified (2026-10-29)*.
3. **Copper (BLD-01c).** Every unwaxed oxidation stage of the copper block,
   cut copper and its stairs and slab, chiseled copper, grate, bulb, door,
   trapdoor, bars, chain, lantern, chest and lightning rod, plus the copper
   torch. Random-tick oxidation and axe scraping. The waxed forms and
   honeycomb waxing are implemented, unverified, with bees in v0.15
   ([2026-11-04 record](verification/2026-11-04-v0.15-crops-bees.md)). *Implemented, unverified
   (2026-10-25; checks rerun on the merge with stone 2026-10-26):* all 57
   blocks (732 registered states laid out in 22528–23372), with Java's
   `ChangeOverTimeBlock` odds, scraping, bulb toggling and comparator
   output, recipes and stonecutting data; see the
   [record](verification/2026-10-25-v0.12-copper.md). Not done: copper
   sounds. Lightning: *implemented, unverified (2026-11-04)* in
   [v0.15 track 1](verification/2026-11-04-v0.15-lightning.md). Double copper chests: *implemented, unverified
   (2026-10-28)* with the other double chests, the pair taking the younger
   stage and aging together ([record](verification/2026-10-28-v0.12-double-chests.md)).
   Copper golem statues and their construction, sorting and revival loop:
   *implemented, unverified (2026-11-05)*; see the
   [integration evidence](verification/2026-11-05-v0.15-copper-golems.md).
4. **Colour (BLD-01d).** Plain banners, standing and on walls, in 16
   colours; candles in 17 colours (one to four per block, lit, waterlogged)
   and candle cakes; 26.3's wool and concrete stairs and slabs; cushions and
   the straw bed, from the 26.3 source.
   *Implemented, unverified (2026-10-25):* all 131 blocks of the audit's
   colour list (`src/world/colour_blocks.rs`): every wool and concrete
   stairs and slab state with recipes and concrete stonecutter data
   (`slab_stair_states`' in-memory family field widened to 7 bits; no saved
   ID changed), candles with one to four candles, lighting, putting out,
   waterlogging and dyeing, candle cakes, plain standing and wall banners
   drawn as Java-shaped models, the straw bed with `straw_bed_rule` (no
   respawn point, falls apart on waking and instead of exploding outside
   the Overworld), and the 16 cushions as stationary seats on the vehicle
   system, crafted from three wool slabs
   ([record](verification/2026-10-25-v0.12-colour.md)). The plain candle's
   survival recipe was deferred here in 2026-10-26 to retain the finished
   blocks while waiting for honeycomb. It is now implemented, unverified,
   with bees (2026-11-04, [record](verification/2026-11-04-v0.15-crops-bees.md)).
   Owed: owner play and banner patterns (loom,
   v0.17). Arrows hitting cushions, boats, minecarts and decorations:
   *implemented, unverified (2026-10-29,
   [record](verification/2026-10-29-v0.12-finish.md))*.
5. **Utilities and special blocks (BLD-05b, BLD-02b, ENV-04).** Item frames
   and glow item frames, paintings and armor stands; empty, water, lava and
   powder snow cauldrons (filling, bottles, dye washing, dripstone filling);
   campfires and soul campfires (cooking, damage, smoke signals, putting
   out and lighting); flower pots and the potted forms of plants that
   exist; coral wall fans and coral dying out of water; falling and
   dripping pointed dripstone; big dripleaf tilting; trapped chests and
   ender chests (moved from v0.18, since obsidian and eyes of ender exist
   now); decorated pots without sherds; lodestones and respawn anchors.
   Sponges already drain water
   (`src/world/sponge.rs`); they need only the M2 exit sample.
   *Implemented, unverified (2026-10-26):* all 55 blocks of the audit's
   utilities list (`src/world/utilities.rs`, 254 states) and the item
   frame, glow item frame, painting and armor stand as entities
   (`src/entity/decor.rs`), with Java's states, shapes, light, mining and
   drops, recipes, cauldron buckets and bottles with rain, snow and
   stalactite filling, campfire cooking, damage, smoke signals, lighting and
   dousing, coral wall fans and their dying, falling stalactites, big
   dripleaf tilting, trapped chest signals, the per-player ender chest,
   decorated pots that keep a stack, and anchors charged with glowstone that
   set a Nether respawn point or explode elsewhere
   ([record](verification/2026-10-25-v0.12-utilities.md)). Owed: owner play,
   dye washing (dyed leather armour and banner patterns, v0.17), and
   stalactites broken by tridents (v0.16). The frame-time scene is
   `materials`. Lodestone compasses, and cauldrons putting out a burning
   player with a level: *implemented, unverified (2026-10-29)*.
   *Implemented, unverified (2026-10-27):* item frame comparator output
   through a conductor, LAN snapshots of nearby frames, paintings and armor
   stands, and ender chests for controller players (their own screen), LAN
   players and CLI agents (`chest take/put` on an aimed ender chest), whose
   ender inventories are now saved with their profiles
   ([record](verification/2026-10-27-v0.12-explosions.md)).
6. **Data pass (BLD-01e, BLD-02, BLD-03).** Register the fuel values of
   every reference fuel item that now exists; compare every crafting,
   smelting, stonecutting and campfire recipe with the 26.3 data; compost
   chances, repair materials, hardness, tools, drops, Silk Touch and
   Fortune for each new block.
   *Implemented, unverified (2026-10-27):* each area now has an extractor
   that reads the pinned 26.3 server jar, a fixture and a test over every
   registered block or item. Fixed values must match exactly; random drops
   are sampled with a seeded RNG and must match Java's chance and mean
   within five standard errors. Drops are checked only with tools that
   can harvest the state, since the harvest rule has its own test.
   - **Recipes:** 1,335 crafting, cooking, stonecutting and smithing recipes
     match in both directions; 178 fixed and 2 invented ones removed
     (melon to slices, a second packed ice). 406 wait on unregistered items;
     22 are allow-listed (dyed leather, grid repair, banner copying).
     ([record](verification/2026-10-27-v0.12-data-recipes.md))
   - **Drops:** all 1,046 registered blocks drop what their loot table gives
     for each state and harvesting tool, including Silk Touch, Fortune and
     shears; 46 fixed, such as sticks from leaves, beds dropping from the
     head and the sea lantern's crystals. ([record](verification/2026-10-27-v0.12-data-drops.md))
   - **Mining, fuel, compost, repair:** hardness, harvest rule and dig speed
     per tool for every state (29 blocks fixed; 26.3 lets the ender chest,
     bell, brewing stand, iron door, weighted plates and pointed dripstone
     drop by hand), 323 fuels (block of coal 16,000 ticks), 101 compostables
     and 60 repairable items. ([record](verification/2026-10-27-v0.12-data-blockdata.md))

   - **Explosions:** *implemented, unverified (2026-10-27).* Java 26.3's
     `ServerExplosion`: 1,352 rays weakened by each block's blast
     resistance (every state checked against the fixture), entities hurt
     and pushed by how much of them the blast sees before blocks break,
     each block's loot table rolled under the explosion's rules (a new
     `explosion` scenario in the loot fixture), drops merged into piles of
     16, and fires from beds, respawn anchors and ghast fireballs.
     ([record](verification/2026-10-27-v0.12-explosions.md))

   Owed: owner play; wind charges and the Wither's block rules (with their
   sources). Explosions destroying dropped items and experience orbs:
   *implemented, unverified (2026-10-28,
   [record](verification/2026-10-28-v0.12-double-chests.md)).*

Tracks 1–5 can run in parallel once item 0 is done, at most four at a time;
item 6 runs last, over the integrated result.

Moved out of v0.12 (2026-10-29). Under the policy approved on 2026-10-25, a
row that isn't ready moves on rather than holding v0.12 open:

- To v0.13: bubble columns (ENV-01a), the fire-tick and flammability
  comparison with 26.3 (ENV-02, BLD-03b), and the offhand and shield
  assessment (PLY-05b). None was started.
- With their dependencies: firework rockets in crossbows (PLY-05c, with
  fireworks in v0.18), stalactites broken by tridents (ENV-04, v0.16),
  splash water bottles putting out fire, candles and campfires (with the
  potion work, v0.20), item frames holding maps (cartography, v0.17), and
  copper chests waiting to age while open (container viewers, with chest
  lids, v0.20).
- To v0.20: the blocks' own sounds, the three-run `materials` frame-time
  set, and the small model and lighting differences in the track records.

Exit: the [M2 exit](roadmap.md#m2--complete-materials-and-interactions) for each
family in v0.12. A sample of every family is obtained in survival, crafted,
placed in each state, used, mined and saved and reloaded, with collision,
light and culling matching its state; `registry_audit` lists no v0.12 block;
frame time in a scene full of the new blocks stays within the M5 budget.

## v0.13 scope: terrain

Approved by the owner on 2026-10-30, with fidelity exception FE-01
([§1.4](roadmap.md#14-what-can-follow-10)): the terrain keeps VoxelCraft's own noise,
and its rows are checked against the behaviours they describe, not Java
seeds.

0. **Decision and IDs.** FE-01 recorded; v0.13's state, item and texture
   ranges split per track in the
   [architecture reservations](architecture.md#v013-track-reservations).
1. **World-generation version.** Unmodified chunks are never saved, so
   changing the generator would change every existing world's untouched
   terrain. Worlds record `worldgen=<n>` in `level.txt`; worlds saved
   before v0.13 have none and keep generation 1, whose output is pinned by
   the golden chunk hashes. New worlds use generation 2, and every v0.13
   generation change applies to generation 2 only. LAN protocol 4 sends the
   generation in `Welcome`.
2. **Density terrain** (GEN-01a/b). A 3D density field built from the
   existing climate values replaces the heightmap: overhangs, cliffs,
   arches and jagged peaks, sampled on a coarse grid and interpolated.
   Surface rules work from the density surface (steep faces, floating
   undersides), and column heights stay available to structures, spawning
   and `/locate`.
3. **Underground** (GEN-02). Caves and aquifers fitted to the density
   terrain; water and lava springs; glow lichen; an ore-distribution pass;
   a deep-dark cave biome deep under low-erosion (mountainous) ground, with
   sculk, sculk veins, catalysts, sensors and shriekers as world content
   (shriekers summon nothing until the warden, v0.16).
4. **Features** (GEN-03). Giant spruce (with podzol) and giant jungle
   trees (trunk vines and cocoa) checked and corrected, the remaining
   biome features, all continuous across chunk borders.
5. **Carried from v0.12.** Bubble columns (ENV-01a), a fire spread and
   flammability review (ENV-02, BLD-03b) and an offhand and shield review
   (PLY-05b).
6. **Checks.** Chunks identical whatever order and thread generates them
   (GEN-07); save/reload of generation-1 and generation-2 worlds; chunk
   generation throughput before and after in the streaming benchmark.

Tracks 2–5 run in parallel after items 0 and 1, at most four agents at a
time; item 6 runs over the integrated result.

Status (2026-10-30): items 0–6 implemented, unverified, integrated on one
branch with the full check chain passing (1,866 tests). Records:
[density](verification/2026-10-30-v0.13-density.md),
[underground](verification/2026-10-30-v0.13-underground.md),
[trees and features](verification/2026-10-30-v0.13-trees.md),
[carried items](verification/2026-10-30-v0.13-carried.md) and the
[integration record](verification/2026-10-30-v0.13-integration.md)
(save/reload of both generations, screenshots, chunk generation speed).
Known gaps: a few small rocks float above some generation 2 peaks; plant
and feature densities are tuned by hand; the sensor, shrieker and catalyst
are inert until v0.16; the changed fire odds need a check against the 26.3
artifact. Done (owner, 2026-10-31): the owner played it and declared v0.13
done; Cargo 0.13.0 and the release notes are prepared, and `v0.13` is to
be tagged from the merge that carries them.

## Release plan status of v0.8 to v0.13

The rows of the roadmap's [release plan](roadmap.md#release-plan) for the
tagged versions v0.8 to v0.13 as they stood on 2026-11-08, before their
Status cells were shortened.

| Version | Theme | Milestones | Main content | Status |
|---|---|---|---|---|
| v0.8 | Foundations | Finish M0, M1 | Valid M5 baseline and the per-requirement inventory; mob pathfinding, Java spawn categories, shape-aware light, simulation distance; pause/death/save scenarios on every input path | Done (owner, 2026-10-20): tagged `v0.8.0` at `58b547b` (#34, #35, #37–#39 plus release prep, [#41](https://github.com/BrendanH18/VoxelCraft/pull/41)); [delivery record](#34-v080-delivery-record--2026-10-20). Implementation complete and the [M5 baseline](verification/2026-10-16-m5-baseline.md) recorded; rows stay Implemented, unverified. Player acceptance ([checklist](verification/v0.8-acceptance-checklist.md)) moved to v0.9, the streaming budget (PERF-03; two isolated frames over 50 ms in the [2026-10-20 re-run](verification/2026-10-20-streaming-rerun.md)) to v0.20 |
| v0.9 | Block states | M2: BLD-04, BLD-06 | Top and double slabs, waterlogging, door hinges, floor/ceiling buttons and levers, wall grindstones, corner-stair light; reserved state ranges for later content | Done (owner, 2026-10-22): ready to tag `v0.9` once merged; [delivery record](#35-v09-delivery-record--2026-10-22). Block states (#40), Java player movement, wall posts, powered doors, honey, scaffolding and powder snow; the owner's play is the player evidence for v0.8 and v0.9 (the checklist and controller session were dropped). Rows stay Implemented, unverified: the windowed performance check was deferred to v0.20 (owner, 2026-10-22) |
| v0.10 | Updates | M7 early: UX-04b, part of UX-05a | In-game update check, signed download, pre-update world backup and install on Windows and Apple Silicon Macs ([design](updates.md)); worlds whose level file can't be read are no longer started fresh. See [v0.10 scope](#v010-scope-updates) | Release prepared (2026-10-22): Cargo 0.10.0, release notes and the `UPDATE_SIGNING_KEY` secret; tag `v0.10` once merged (owner exception: before the exit check, which needs a later release). Rows Implemented, unverified until the owner updates a real v0.10 install (the exit check); the stepping-stone release every player installs by hand |
| v0.11 | Menus and settings | M7 early: UX-03a/b, UX-05a | Java's title screen, world list and Edit World screen; Java-style options screens, key rebinding, GUI scale and per-category volume; consistent menu navigation and error screens. See [v0.11 scope](#v011-scope-menus-and-settings) | Release prepared (2026-10-24): Cargo 0.11.0 and release notes; every work item implemented, unverified; options-file failures now show a notice; the menu and title benchmark scenes exist, but their [first run](verification/2026-10-24-v0.11-menu-frame-time.md) was capped at the display's 120 Hz by the machine's state and doesn't count. Done (owner, 2026-10-25): tag `v0.11` from `ec74556` without its exit checks (owner exception). Rows stay Implemented, unverified; the menu and title frame-time run moved to v0.20, and problems found in the owner's ordinary play come back as fixes |
| v0.12 | Building materials | Close M2 | Missing wood, stone, copper and colour family members, the unregistered reference fuel items, signs, banners, item frames, cauldrons and campfires; coral wall fans; falling and dripping dripstone, tilting dripleaf. See [v0.12 scope](#v012-scope-building-materials) | Done (owner, 2026-10-29): merged in [#55](https://github.com/BrendanH18/VoxelCraft/pull/55) at `ec7e18b`; tag `v0.12` from it. The owner's own play is the player evidence (no problems found). Release prepared (2026-10-29): Cargo 0.12.0 and release notes. Items 0–6 implemented, unverified (capacity raise; wood, stone, copper, colour and utilities tracks; data pass, explosions, double chests; `registry_audit`: 669 → 247 missing blocks, the rest belonging to later releases). The remaining gaps of v0.12's own blocks were closed on 2026-10-29 ([record](verification/2026-10-29-v0.12-finish.md)): sign editing for CLI, LAN and controller players, lodestone compasses, slime blocks for mobs and items, arrows hitting vehicles, cushions and decorations, cauldrons putting out fires, and a hidden-window memory leak. Frame time: one valid `materials` run (539 FPS average, 137.8 FPS 1% low) and a profile showing the per-frame mob-model rebuild, not texture paging, as the cost; the three-run gate moved to v0.20 as for v0.8, v0.9 and v0.11. Rows that weren't ready moved out under the 2026-10-25 policy (see the [scope](#v012-scope-building-materials)). Rows stay Implemented, unverified until the frame-time check in v0.20 |
| v0.13 | Terrain | M3: GEN-01..03 | Density terrain with overhangs from the climate noise (original noise, FE-01); caves and aquifers fitted to it, springs, glow lichen and an ore pass; deep-dark terrain with sculk; giant spruce and jungle trees and the remaining biome features; bubble columns, fire spread, offhand and shields carried from v0.12. See [v0.13 scope](#v013-scope-terrain) | Done (owner, 2026-10-31): merged in [#57](https://github.com/BrendanH18/VoxelCraft/pull/57) at `a8c3172`; the owner's own play is the player evidence. Release prepared (2026-10-31): Cargo 0.13.0 and release notes; tag `v0.13` from the merge that carries them. Items 0–6 implemented, unverified and integrated ([integration record](verification/2026-10-30-v0.13-integration.md)): generation 2 for new worlds only, density terrain, underground and deep dark, giant trees and biome features, bubble columns, fire odds, off-hand, shields and totems. Known gaps: floating rocks above some peaks, hand-tuned densities, inert sculk until v0.16, fire odds unchecked against the 26.3 artifact. Rows stay Implemented, unverified until the frame-time check in v0.20 |
