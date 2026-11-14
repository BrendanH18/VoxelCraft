# VoxelCraft roadmap

[← Back to VoxelCraft](../README.md) · [Documentation index](README.md) · [Contributing](../CONTRIBUTING.md) · [Architecture](architecture.md) · [Development](development.md)

## Current plan — updated 2026-11-13

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

**Versions (owner, 2026-11-13): the next release is v0.14.** The owner
asked for everything built since v0.13 to ship as one release, v0.14,
rather than tagging v0.14, v0.15 and v0.16 one by one. v0.14 therefore
carries the planned v0.14 (structures), v0.15 (wildlife) and v0.16 (deep
dark and trials) scopes and the v0.17 groundwork merged by the build-5000
campaign's first wave. Later versions move down two: villages and raids is
now v0.15, the End and the Wither v0.16, redstone and automation v0.17, and
polish and performance v0.18, before 1.0. Records, commits, pull requests
and code comments written before 2026-11-13 use the old numbers: their
v0.15 and v0.16 are parts of v0.14, and their v0.17 to v0.20 are v0.15 to
v0.18. The owner also dropped the campaign's 5,000-item target the same
day: its first wave is merged and no further waves are planned.

**Priority (owner, 2026-11-12): building resumes.** The owner asked for
building to continue, with a target of 5,000 new features or improvements
(dropped on 2026-11-13) split across agents and merged without waiting for review. That lifts the
content freeze of the [stabilisation gate](#stabilisation-gate-before-v017):
v0.15 starts now, while the gate's exit stays owed (the owner's play of a
generation 3 world, a frame-time run that passes or an owner decision on
the drawable wait, and the `v0.14` tag). The integrator read the size of
the request as also lifting [§7](#7-feature-completion-and-release-policy)'s
limit of one waiting version for this push, so work continues into later
versions in release-plan order once v0.15's scope is implemented. That
reading is the owner's to change. Nothing else is relaxed: each piece is
checked against the Java 26.3 reference, an agent runs every merged batch
before it is offered to the owner, rows stay Implemented, unverified until
the owner has played, and nothing is tagged. The
[campaign record](verification/2026-11-12-build-5000/README.md) says how
the work is cut up, checked and counted.

**Priority (audit, 2026-11-09): stabilise before more content.** The
[2026-11-09 audit](verification/2026-11-09-project-audit.md) found v0.14,
v0.14's three parts merged to `main` without anyone having run them, the
frame-time gate failing and unmeasured since 2026-10-29, Java 26.3 content
that no release owned, and a stale requirement inventory. The owner asked
for every finding to be fixed. Until the
[stabilisation gate](#stabilisation-gate-before-v017) passed, no new
content was to start: v0.15 waited for the owner's play of v0.14
and for one tag that carries them (the owner lifted that freeze on
2026-11-12, above; the play and the tag are still owed). From now on every release records a frame-time
run ([§6.1](#61-reference-configuration-and-frame-time-gate)), an agent runs
each version hands-on before it is offered to the owner
([§7](#7-feature-completion-and-release-policy)), and every Java 26.3
registry name the game lacks has a release that owns it
([§2](#2-status-and-tracking-rules)).

**Priority (owner, 2026-10-22): updates and menus before more content.**
Releases will be frequent, so players must be able to upgrade easily. v0.10
is the stepping stone: it ships the in-game updater ([design](updates.md))
as soon as it works, and every player installs it by hand once. From then on
the game updates itself where it was installed from a release package on
Windows x64 or an Apple Silicon Mac, in a folder the player can change.
Other copies (Intel Mac and Linux source builds, a Mac app run from the disk
image or a read-only folder, a Windows copy not from the installer) show the
release page instead and upgrade by hand ([updates](updates.md)). v0.11
polishes the title screen, world management and options, and content
resumes with v0.12. This pulls those parts of M7
forward; the later releases moved up two numbers (building materials is now
v0.12, polish and performance v0.18).

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
| Archaeology | Decorated pots and sherd patterns, and archaeology in structures other than ocean ruins (narrowed by the owner on 2026-11-05) | Trail ruins, desert wells and ocean ruins as explorable structures with their loot (v0.14); suspicious sand and gravel, brushing them, and the sherds the ocean-ruin tables name, as plain items (v0.14), so warm ocean ruins give sniffer eggs the Java way |
| Commands | The full command and selector language | The cheat console's testing commands (game mode, rules, time, weather, teleport, give, kill, difficulty, spawn point) |
| Custom skins | Skin selection and loading | The default player models |
| Resource and data packs | Loading packs | The built-in Java 26.3 recipes, loot tables and tags they would replace |
| Mod and plugin APIs | All of it | — |

**Fidelity exception FE-01 — original terrain noise (owner, 2026-10-30).**
The Overworld keeps VoxelCraft's own noise and terrain algorithms; Java's
generator is not ported, so a seed does not reproduce Java's world. GEN-01
to GEN-03 are accepted on the gameplay behaviours they list (biome and
landform variety, overhangs, cave and aquifer types, ore bands, features
and their continuity across chunk borders), checked by VoxelCraft fixtures
and play rather than Java seed/coordinate fixtures. Player impact: Java
seeds and coordinates from guides don't carry over. FE-02 below is the
other approved exception. Two things lean on it without the owner having
ruled on them: the Nether's terrain also uses original noise (GEN-04a), and
the 2026-11-09 terrain blending converts VoxelCraft's block-distance density
to Java's units where a structure's beard applies
([record](verification/2026-11-09-structure-terrain-blending.md)).

**Fidelity exception FE-02 — original structure templates (proposed by
the audit on 2026-11-09, approved by the owner on 2026-11-10).** v0.14 build
structures from original templates because Java's template files are
Mojang's work and the project ships only original or licensed assets
([v0.14 scope](#v014-scope-structures)). Until 2026-11-10 no exception
was recorded, so the GEN-05 rows read as if a block-for-block Java layout
were coming. FE-02 records what is already true and says what the templates are held
to instead:

- **Excepted:** block-for-block Java layouts. A Java seed does not
  reproduce a Java village, outpost, bastion, ancient city or trial
  chamber. Player impact: structure guides that rely on exact layouts
  don't carry over.
- **Still required (the catalogue contract):** Java's placement grids,
  biome rules, start pools, pool names and weights, depth and distance
  limits, terrain adaptation, residents, spawn overrides, loot tables and
  maps; and a template catalogue that covers Java's piece *kinds* at
  comparable sizes. A village needs its profession buildings with fixed
  job sites, meeting point, streets, farms, pens, lamps, decorations and
  an abandoned variant; a trial chamber its corridor, chamber, spawner,
  vault and reward-room kinds; an ancient city its centre, walls,
  structures and hidden rooms.

Agents work to the catalogue contract, and the GEN-05 layout rows read
"original templates (FE-02)". The catalogues that fall short of it are
listed under [content assigned by the audit](#content-assigned-by-the-2026-11-09-audit).
New structures (woodland mansions in v0.15, End cities in v0.16) are drawn
as original templates to the same contract.

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

Three rules were added by the 2026-11-09 audit, which found content and rows
that had fallen through:

- **Every missing registry name has an owner.**
  `tests/fixtures/java/26.3/registry_gaps.json` lists each Java 26.3 block,
  item and potion the game doesn't register, with the release that owns it
  (`v0.15` to `v0.18`, `deferred`, or `none` for technical and creative-only
  names). `tests/java_registry.rs` fails when a missing name isn't listed,
  when a listed name has been registered, or when a release isn't one of
  those values, so the list can't drift. Moving a name to another release is
  an edit to that file, made with the scope change here.
- **Name-keyed tables don't skip silently.** Villager and wandering-trader
  trades and loot tables refer to items by name. The same test fails when a
  name neither resolves nor is listed as a gap, so a typo no longer looks
  like missing content. Brewing is compared both ways with Java in
  `tests/java_brewing.rs`.
- **A row can't keep a release that has shipped.** When a version is
  implemented, each of its rows in the [inventory](requirements.md) is
  either moved to Implemented, unverified or given the later release that
  owns what remains, in the same change. The inventory is refreshed against
  `main` before each tag.

Keep the active roadmap focused on present scope, dependencies, status and
next work. History lives in the [delivery records](delivery-records.md). Keep implementation details and benchmark reports in the development
documentation; release notes describe only delivered behavior.

## 3. Delivery records

What each version delivered, the 0.6 starting inventory and the dated
assessment follow-ups are in the [delivery records](delivery-records.md)
(moved there on 2026-11-09). One line per shipped or merged version:

- **v0.7.0**, tagged 2026-10-14: [delivery record](delivery-records.md#33-v070-delivery-record--2026-10-14),
  with the [0.6 starting inventory](delivery-records.md#3-starting-inventory--evidence-not-a-new-completion-claim)
  and the [assessment follow-ups](delivery-records.md#assessment-follow-ups-2026-10-13-to-2026-10-16).
- **v0.8**, tagged `v0.8.0` 2026-10-20: [delivery record](delivery-records.md#34-v080-delivery-record--2026-10-20)
  and [scope](delivery-records.md#v08-scope-finish-m0-and-m1).
- **v0.9**, tagged 2026-10-22: [delivery record](delivery-records.md#35-v09-delivery-record--2026-10-22).
- **v0.10**, tagged 2026-10-22: [scope and record](delivery-records.md#v010-scope-updates).
- **v0.11**, tagged 2026-10-25: [scope and record](delivery-records.md#v011-scope-menus-and-settings).
- **v0.12**, tagged 2026-10-29: [scope and record](delivery-records.md#v012-scope-building-materials).
- **v0.13**, tagged 2026-10-31: [scope and record](delivery-records.md#v013-scope-terrain).
- **v0.14**, merged, not tagged: [scope and status](#v014-scope-structures).
- **v0.14, wildlife part**, merged, not tagged: [scope and status](#v015-scope-wildlife) and the
  [final integration record](verification/2026-11-06-v0.15-final-integration.md).
- **v0.14, deep dark and trials part**, merged, not tagged: [scope and status](#v016-scope-deep-dark-and-trials) and the
  [integration record](verification/2026-11-07-v0.16-integration.md).

## 4. Core requirement catalogue

The dated assessment follow-ups that opened this section are in the
[delivery records](delivery-records.md#assessment-follow-ups-2026-10-13-to-2026-10-16).

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
  Java placement rules, terrain alignment, traps, block entities,
  residents, spawners and appropriate loot. Layouts are original templates
  held to the [FE-02](#14-what-can-follow-10) catalogue contract (Java's
  piece kinds at comparable sizes), not block-for-block Java layouts.
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
  supported desktop platforms, including an in-game update check and
  install from a published release ([design](updates.md)). M5 macOS is the
  performance reference; Windows and Linux retain correctness/build
  regression coverage.
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
Tags are two-part (owner, 2026-10-22): `v0.9` or `1.0`, not `v0.9.0`
([releases](releases.md)).
Exception (owner, 2026-10-20): v0.8 was declared done with its implementation
complete but its player acceptance and streaming budget not yet passed; those
checks moved to later versions (see its status and the
[delivery record](delivery-records.md#34-v080-delivery-record--2026-10-20)).
Exception (owner, 2026-10-25): v0.11 is tagged without its exit checks,
the owner's play and the idle-machine menu frame-time run. The frame-time
run moved to v0.18 and the rows stay Implemented, unverified.
Exception (owner, 2026-10-22): v0.10 is tagged once merged, before its exit
check, because the owner wants players on the updater as soon as possible
and the check (updating a real v0.10 install) needs a later release to update
to. The rows stay Implemented, unverified until it passes.
Exception (owner, 2026-10-29): v0.12 is tagged after the owner's play,
without its frame-time gate: the three-run `materials` set moved to v0.18
with the gate, and the rows stay Implemented, unverified until it passes.
Exception (owner, 2026-10-31): v0.13 is tagged after the owner's play,
without a frame-time gate: the 240 FPS check stays with v0.18, and the rows
stay Implemented, unverified until it passes.

Work on the earliest unreleased version. Start a later one early only where
its dependencies allow and it doesn't stall the current one. Version 1.0 is
every mandatory requirement in this roadmap, Verified; nothing in it is
optional for 1.0 unless the owner records a deferral or fidelity exception.

| Version | Theme | Milestones | Main content | Status |
|---|---|---|---|---|
| v0.8 | Foundations | Finish M0, M1 | Valid M5 baseline and the per-requirement inventory; mob pathfinding, Java spawn categories, shape-aware light, simulation distance; pause/death/save scenarios on every input path | Done (owner, 2026-10-20), tagged `v0.8.0` at `58b547b`. Rows stay Implemented, unverified: player acceptance moved to v0.9 and the streaming budget (PERF-03) to v0.18; [delivery record](delivery-records.md#34-v080-delivery-record--2026-10-20) |
| v0.9 | Block states | M2: BLD-04, BLD-06 | Top and double slabs, waterlogging, door hinges, floor/ceiling buttons and levers, wall grindstones, corner-stair light; reserved state ranges for later content | Done (owner, 2026-10-22), tagged `v0.9`. Rows stay Implemented, unverified: the windowed performance check moved to v0.18; [delivery record](delivery-records.md#35-v09-delivery-record--2026-10-22) |
| v0.10 | Updates | M7 early: UX-04b, part of UX-05a | In-game update check, signed download, pre-update world backup and install on Windows and Apple Silicon Macs ([design](updates.md)); worlds whose level file can't be read are no longer started fresh. See [v0.10 scope](#v010-scope-updates) | Tagged `v0.10` on 2026-10-22 before its exit check (owner exception). Rows stay Implemented, unverified until the owner updates a real v0.10 install; [record](delivery-records.md#v010-scope-updates) |
| v0.11 | Menus and settings | M7 early: UX-03a/b, UX-05a | Java's title screen, world list and Edit World screen; Java-style options screens, key rebinding, GUI scale and per-category volume; consistent menu navigation and error screens. See [v0.11 scope](#v011-scope-menus-and-settings) | Done (owner, 2026-10-25), tagged `v0.11` from `ec74556` without its exit checks. Rows stay Implemented, unverified: the menu and title frame-time run moved to v0.18; [record](delivery-records.md#v011-scope-menus-and-settings) |
| v0.12 | Building materials | Close M2 | Missing wood, stone, copper and colour family members, the unregistered reference fuel items, signs, banners, item frames, cauldrons and campfires; coral wall fans; falling and dripping dripstone, tilting dripleaf. See [v0.12 scope](#v012-scope-building-materials) | Done (owner, 2026-10-29), tagged `v0.12` ([#55](https://github.com/BrendanH18/VoxelCraft/pull/55)). Rows stay Implemented, unverified: the `materials` frame-time gate moved to v0.18; [record](delivery-records.md#v012-scope-building-materials) |
| v0.13 | Terrain | M3: GEN-01..03 | Density terrain with overhangs from the climate noise (original noise, FE-01); caves and aquifers fitted to it, springs, glow lichen and an ore pass; deep-dark terrain with sculk; giant spruce and jungle trees and the remaining biome features; bubble columns, fire spread, offhand and shields carried from v0.12. See [v0.13 scope](#v013-scope-terrain) | Done (owner, 2026-10-31), tagged `v0.13` ([#57](https://github.com/BrendanH18/VoxelCraft/pull/57)). Rows stay Implemented, unverified until the frame-time check in v0.18; [record](delivery-records.md#v013-scope-terrain) |
| v0.14 | Structures | M3: GEN-05..07 | A Java structure-template (jigsaw) system; villages, outposts, bastions, fossils, shipwrecks, ocean ruins and monuments built from it; buried treasure and explorer maps; Java loot random sequences. See [v0.14 scope](#v014-scope-structures) | In progress (2026-11-03): scope approved with original templates (no fidelity exception yet); all of items 0–7 implemented, unverified: world generation 3, the jigsaw engine and villages ([record](verification/2026-11-03-v0.14-jigsaw-villages.md)), outposts and bastions ([record](verification/2026-11-03-v0.14-outposts-bastions.md)), ruined portals and buried treasure ([record](verification/2026-11-03-v0.14-portals-treasure.md)), fossils, desert wells and trail ruins ([record](verification/2026-11-03-v0.14-fossils-wells-trail-ruins.md)), maps, explorer maps and Java loot sequences ([record](verification/2026-11-03-v0.14-maps.md)); terrain blending for villages and outposts added on 2026-11-09 ([record](verification/2026-11-09-structure-terrain-blending.md)). Merged, not tagged: waits for the [stabilisation gate](#stabilisation-gate-before-v017) and the owner's play |
| v0.14 (planned as v0.15) | Wildlife | M4: MOB-03/04, FOO, ENV-03 | Bats, frogs, pandas, polar bears, armadillos, mooshrooms, strays, bogged, phantoms and zombie horses (with lightning); drowned conversion and the cat spawner; melon, pumpkin and beetroot crops; animal and mount acceptance. See [v0.14 scope](#v015-scope-wildlife) | Implemented, unverified (2026-11-04): started early while v0.14 waits for the owner's play; four tracks (lightning and undead mounts, crops and bees, passive mobs, hostiles) merged and council-reviewed ([integration](verification/2026-11-04-v0.15-integration.md)); copper golems (2026-11-05), then creaking and resin, allays, sniffers and ocean ruins with suspicious blocks (2026-11-06, [integration](verification/2026-11-06-v0.15-final-integration.md)) complete the scope; awaits the owner's play |
| v0.14 (planned as v0.16) | Deep dark and trials | M3 + M4 + PRO-07 | Ancient cities with sculk and the warden; trial chambers with breezes, trial spawners, vaults and the mace; tridents, spears and the other modern weapon families; a jukebox with music discs. See [v0.14 scope](#v016-scope-deep-dark-and-trials) | Implemented, unverified (2026-11-07): scope approved by the owner on 2026-11-06; four tracks (sculk and the warden, ancient cities and music, trial chambers, weapons) merged and integrated ([integration](verification/2026-11-07-v0.16-integration.md)); awaits the owner's play |
| — | Stabilisation | Gate before the v0.14 tag | No new content: an agent hands-on pass over v0.14, the entity frame-time fix and a full scene run, terrain blending for generation 3 structures, the audit's data fixes, the owner's play, and one tag for v0.14. See the [stabilisation gate](#stabilisation-gate-before-v017) | In progress (2026-11-09). The owner lifted its content freeze on 2026-11-12; the owner's play, the frame-time gate and the tag are still owed |
| v0.15 | Villages and raids | Close M4 | Working lecterns, looms, cartography tables and stonecutters; illagers, woodland mansions, raids; remaining villager behaviour; the M4 exit checks. Added by the 2026-11-09 audit ([list](#content-assigned-by-the-2026-11-09-audit)): the remaining Java 26.3 mobs (happy ghasts, nautiluses, camel husks, parched), mobs holding and picking up equipment, chainmail, books, bundles, spyglasses, armor trims, the remaining status effects and potions, and the village template catalogue | Planned. Part of its list was built early by [build-5000 wave 1](verification/2026-11-12-build-5000/wave-1.md) and ships in v0.14 (see the [assigned content](#content-assigned-by-the-2026-11-09-audit)) |
| v0.16 | The End and the Wither | Close M3; M5 | End cities and ships (the last GEN-05 structures, then the M3 exit checks), shulkers and shulker boxes, elytra and fireworks, dragon respawn; Wither summoning, Nether stars and beacons; the no-cheat progression playthrough. Added by the 2026-11-09 audit ([list](#content-assigned-by-the-2026-11-09-audit)): poplar, cinnabar and sulfur with their terrain and the sulfur cube, abandoned camps, mob heads, infested blocks, nether gold ore, conduits, and the reassessment of dungeons, mineshafts, strongholds, shipwrecks and monuments | Planned |
| v0.17 | Redstone and automation | M6 | Java update order, pistons with entity push and slime/honey, the crafter, remaining sensors and workstations; reference farms, clocks and sorters across save/reload. Added by the 2026-11-09 audit: the furnace minecart | Planned |
| v0.18 | Polish and performance | M7: UX, §6 | Typography/contrast, material and mob art, inventory/recipe navigation, HUD/workstations, title/world identity, atmosphere and accessibility ([scope](#v020-scope-presentation-and-performance)); subtitles, asset and sound review in play (including the v0.12 blocks' own sounds), and menu fixes found since v0.11; the 240 FPS gate in every scene, including v0.8's streaming budget (PERF-03), and the performance check for the v0.8 and v0.9 rows, v0.11's menu and title frame-time run and v0.12's `materials` runs, which moved here unverified ([v0.12 profile](verification/2026-10-29-v0.12-finish.md#frame-time)). The per-frame entity model rebuild that profile found was removed on 2026-11-09 ([record](verification/2026-11-09-entity-frame-time.md)); the streaming route's main-thread terrain column rebuilds from mob biome queries were addressed in the [streaming follow-up](verification/2026-11-09-streaming-biomes-and-gen07.md); valid windowed gate runs, the remaining tick work and per-frame sign text are still owed | Planned; presentation scope expanded by the owner on 2026-11-05. Each release now records its own frame-time run ([§6.1](#61-reference-configuration-and-frame-time-gate)), so what stays here is the final gate, not every release's first measurement |
| v1.0 | Acceptance | M7 exit, §8 | Every mandatory row Verified; fresh-world and long-session playthroughs; save-compatibility policy; installer and update checks; current stable Java scope reconciled | Planned |

Notes:

- v0.10 and v0.11 were inserted on 2026-10-22 (owner priority: updates and
  menus before content). Releases planned as v0.10–v0.18 before that date are
  now v0.12–v0.20, and the documents were renumbered to match; older commits
  and pull requests use the old numbers.
- v0.14 and v0.15 interleave M3 and M4 because structures need their residents:
  ancient cities need the warden, trial chambers breezes and bogged, mansions
  and raids the illagers. M4's exit checks run at v0.15; M3's run at v0.16,
  once End cities and ships (GEN-05) exist alongside shulkers and elytra.
- M2 comes before structures because Java templates use the full set of block
  states (waterlogged stairs, top slabs, hinged doors).
- v0.13's terrain-noise decision was made on 2026-10-30: original noise,
  recorded as fidelity exception FE-01 in [§1.4](#14-what-can-follow-10).
- Recheck the latest stable Java release when planning each version (§1.2) and
  slot new core content into an unreleased version as an explicit scope change.
- When a version ships, set its status here, add a delivery record to the
  [delivery records](delivery-records.md) like
  [v0.7.0's](delivery-records.md#33-v070-delivery-record--2026-10-14) and list
  its unfinished members in the release notes. Moving an item to another version is an edit to this
  table, recorded with the reason.
- The 2026-11-09 audit slotted every piece of Java 26.3 content that had no
  release into v0.15 to v0.18 (see
  [content assigned by the audit](#content-assigned-by-the-2026-11-09-audit)).
  These are the audit's assignments, made on the owner's instruction to fix
  its findings; the owner may move any of them. v0.15 and v0.16 are now the
  two largest releases in the plan and are the natural ones to split.
- Performance no longer waits for v0.18. Each release records a frame-time
  run and may not make a scene worse without a recorded reason
  ([§6.1](#61-reference-configuration-and-frame-time-gate)); v0.18 keeps the
  final gate and the presentation work.
- After 1.0: multiplayer (dedicated servers, per-player LAN and split-screen
  dimensions) and the side features in [§1.4](#14-what-can-follow-10) get their
  own v1.x roadmap.

<a id="stabilisation-gate-before-v017"></a>

#### Stabilisation gate: before the v0.14 tag

Added by the [2026-11-09 audit](verification/2026-11-09-project-audit.md).
v0.14's three parts (planned as v0.14, v0.15 and v0.16) were merged
without being run, so the next release
is a gate, not a version: it adds no blocks, items, mobs or structures, and
v0.15 was not to start until it passed; on 2026-11-12 the owner asked for
building to continue, which lifts that freeze and leaves the exit below
owed. Work items:

1. **Data fixes.** *Implemented, unverified (2026-11-09,
   [record](verification/2026-11-09-audit-data-fixes.md)).* Brewing is
   compared both ways with a fixture extracted from the 26.3 server jar
   (279 recipes: 259 match, 20 wait for the four trial potions); fermented
   spider eye, the golden carrot, rabbit's foot and corruption mixes and
   Invisibility were added. Eight villager offers and one wandering-trader
   offer that named items wrongly are restored. Every missing registry name
   has a release in `registry_gaps.json` (122 blocks, 309 items, 5 potions),
   checked by `tests/java_registry.rs`.
2. **Entity frame time.** *Implemented; the gate run is still owed
   (2026-11-09, [record](verification/2026-11-09-entity-frame-time.md)).*
   Plain mob boxes are drawn as GPU instances and random block ticks look
   their rule up in a table. Offscreen comparison runs, one a side, moved
   the five simulation scenes' 1% low from 201–226 to 263–300 FPS and p99
   from 3.8–4.4 to 2.5–2.9 ms. They are not gate evidence: the machine was
   on battery and in use, so no windowed run was valid. Streaming
   failed its budget in that run (p99 11.5 ms against 8.33 ms). The
   [streaming follow-up](verification/2026-11-09-streaming-biomes-and-gen07.md)
   retains worker-computed original terrain heights with loaded columns so
   gameplay biome queries no longer rebuild evicted density fields. Three
   short offscreen comparisons per build reduced p99 to 3.635–4.157 ms and
   the slowest 1%'s mean tick phase to 2.722–3.046 ms. These remain ineligible:
   other desktop workloads were active, some repeats missed tick tolerance
   and both builds had unexplained outliers. The gate remains unverified.
   *Full windowed run (2026-11-11,
   [record](verification/2026-11-11-gate-run.md#scene-run)):* all 24
   runs were valid and the gate fails. Most world-scene runs have a 1% low
   near 110–115 FPS with p99 at one 120 Hz refresh (8.3–8.6 ms); their
   slowest frames wait 7–8 ms to acquire a drawable while the game's own
   work stays near 1 ms. Why the drawable is late is the open question.
3. **Terrain blending for generation 3 villages and outposts.**
   *Implemented, unverified (2026-11-09,
   [record](verification/2026-11-09-structure-terrain-blending.md)).* A
   first attempt with Java's `beard_thin` kernel alone failed: pieces stood
   up to 19 blocks from the surrounding ground and the kernel reaches 12.
   The delivered version first chooses each rigid piece's floor so the
   ground around it is within reach, rejecting pieces (and starts) that
   can't fit, then applies the Java-shaped kernel before surface rules and
   features. Pillars and hillside cuts are gone in the tested fixtures (at
   most 1 foundation block, 3 blocks of ring error and a 3-block step,
   against 19 before). The cost is availability on steep ground: over 2,025
   placement cells on each of three seeds, village starts fell from 996 to
   791 and outposts from 146 to 100, and the mean village lost about three
   pieces. Generations 1 and 2 are unchanged. Ancient cities, trail ruins
   and trial chambers keep their older approximations of `beard_box`,
   `bury` and `encapsulate`.
4. **An agent run of v0.14.** *Scripted captures done; limited
   hands-on play now reached (2026-11-09,
   [record](verification/2026-11-09-stabilisation-pass.md)).* Villages,
   an outpost, a ruined portal, trail ruins, a warm ocean ruin, an ancient
   city, a trial chamber and 17 of the v0.14 mob kinds were drawn
   from the merged build and looked at. Bastions, Nether fossils, desert
   wells, buried treasure, cold ocean ruins and the other content that
   record lists were not captured. Two
   hands-on sessions through computer use played for zero minutes: a bare
   executable isn't visible to it (fixed by `scripts/test-app.sh`), and
   the owner's screen was locked, which leaves no window to attach to.
   A later GPT-6.1 Terra session in the [streaming follow-up](verification/2026-11-09-streaming-biomes-and-gen07.md)
   created a survival world, moved along the shoreline and into the ocean,
   used inventory and pause menus, saved and reopened. It also inspected
   city and trial-chamber captures. *Further captures and play
   (2026-11-10, [record](verification/2026-11-10-stabilisation-captures.md)):*
   bastions of all four kinds, Nether fossils, a desert well, buried
   treasure dug out with its loot, a cold ocean ruin, trail ruins cut away,
   the v0.14 blocks, brushing and the v0.14 items were drawn and
   looked at. Three defects were found and fixed: Nether fossils now
   apply Java's `beard_thin`, generation 3 swamp shores get grass and
   Java's swamp water pools instead of bare dirt, and the F3 columns no
   longer overlap. A Terra session reached a village, the new mobs, a
   trident, a trial chamber and an ancient city before the screen locked.
   It found Java's `@s` target rejected by `/effect` and `/enchant`
   (fixed). *Look after the fixes (2026-11-11,
   [record](verification/2026-11-10-stabilisation-captures.md#look-after-the-fixes-2026-11-11)):*
   seed 9's Nether fossil now rests on a bearded shelf and generation 3
   swamp shores are grass with pools. A further Terra session lost the
   window to the screen lock before playing anything. *Hands-on play
   (2026-11-11, [record](verification/2026-11-11-gate-run.md#hands-on-play)):*
   with the owner at the machine, Terra played the jukebox (pass) and a
   save and reload (position, inventory, weather, time, effects and the
   jukebox's disc survived). It couldn't turn the camera reliably, so
   trading, brewing, the mace, the trident, spawner waves, vaults,
   shriekers and the warden, lightning, a bastion and brushing by hand
   were reached at most and remain unverified. Its one defect, the
   player's own effect swirls spawning at twice Java's rate in blended
   colours, is fixed. For the next pass, Java's aiming commands were added
   (2026-11-11): `/tp @s x y z yaw pitch`, `/tp … facing …` and `/rotate`.
5. **Documentation.** *Done (2026-11-09).* History moved to the
   [delivery records](delivery-records.md), the
   [inventory](requirements.md) refreshed against `main` and its 26
   stranded rows given releases, and `tools/check_doc_links.py` added to
   CI.
6. **Still owed before the tag.** The owner's play of a new generation 3
   world. The encounters listed under item 4 in the agent hands-on pass.
   Cargo is at 0.14.0 (0.16.0 until the owner's renumbering on 2026-11-13),
   and `packaging/RELEASE-NOTES.md` covers all of v0.14 with its known gaps.
   Three valid windowed runs of every scene on AC power with the machine
   idle (PERF-02/03), including investigation of any remaining streaming
   outliers. The 2026-11-11 run has three valid runs of every scene and
   fails the 240 FPS gate ([record](verification/2026-11-11-gate-run.md#scene-run));
   the drawable wait it found needs fixing or an owner decision before
   the tag. Benchmark runs now record GPU time per frame and can set the
   frame latency and window mode; the
   [experiment matrix](development.md#presentation-experiments) is owed on
   the idle M5 (2026-11-11). Leaving out the display-pacing stretches doesn't rescue it, and
   its steady-state scenes are below the 2026-10-16 baseline, which needs a
   profile and a fix or a written reason. ([FE-02](#14-what-can-follow-10) was approved by the owner on
   2026-11-10.)
   Ancient cities and trial chambers now run in the GEN-07 order corpus
   ([record](verification/2026-11-09-streaming-biomes-and-gen07.md)).
   Then the tag.

**Exit:** the owner has played a generation 3 world built from the merged
tree and reported the result; problems found are fixed or listed in the
release notes; the scene suite has a valid run recorded for the tagged
commit; `v0.14` is tagged from a commit whose Cargo version and release
notes cover all three parts of v0.14 together. One tag is the audit's
recommendation, because generation 3 has only ever existed with all three
versions' structures in it; the owner may still tag them separately from
the older merge commits.

#### Content assigned by the 2026-11-09 audit

Java 26.3 content that no release owned, and rows left behind when their
release was implemented. `tests/fixtures/java/26.3/registry_gaps.json`
carries the same assignments name by name and is checked by
`tests/java_registry.rs`; this table is the summary. Each line is an
explicit scope change under [§1.2](#12-reference-version-policy), made by
the audit on the owner's instruction, and the owner may move it.

| Content | Release | Rows | Why there |
|---|---|---|---|
| Happy ghasts, dried ghasts and harnesses; nautiluses, zombie nautiluses and nautilus armor; camel husks; parched; copper and netherite horse armor | v0.15 | MOB-03e, MOB-04b, FOO-04c | M4 (the living world) closes at v0.15, so every remaining mob has to exist by then |
| Mobs holding weapons and tools, wearing and picking up equipment; jockeys; zombie reinforcements; endermites | v0.15 | MOB-02d, MOB-04a, MOB-04b | Illagers and raid captains need held weapons and banners anyway |
| Chainmail armor, experience bottles, writable and written books, the globe and other banner patterns | v0.15 | MOB-05a, PLY-05b, AUT-04c | Each is a villager trade that is listed as a gap today; books go with the lectern, patterns with the loom |
| Bundles, spyglasses, spectral arrows, armor trims and their templates, dyed leather armor, blue and brown eggs, frosted ice from Frost Walker | v0.15 | BLD-05b, PLY-05c, PRO-03, PRO-04 | General items with no dependency on later content; trims and dyes go with the smithing table and cauldron work in this release |
| Glowing, Levitation, Health Boost, Hero of the Village, Raid Omen, Wind Charged, Weaving, Oozing and Infested, with the four potions that need them; Luck stays without a potion recipe, as in Java | v0.15 | PLY-03b, PRO-02 | Raids need the omen and hero effects; the trial potions were left out of v0.14 |
| The village template catalogue (profession buildings with fixed job sites, decorations, animals in pens, abandoned villages), and the trial chamber and ancient city catalogues, to the [FE-02 contract](#14-what-can-follow-10) | v0.15 | GEN-05b, GEN-05j | Villages are v0.15's subject, and mansions use the same engine |
| Poplar wood, leaves and saplings; shrubs; cinnabar and sulfur families, sulfur springs and caves, sulfur spikes, the sulfur cube and its bucket; golden dandelions and shelf mushrooms | v0.16 | BLD-01a, BLD-01b, GEN-03, MOB-04b | Promised for v0.13 and dropped. They change Overworld generation, so they wait for the next world generation, which v0.16 opens when M3 closes |
| Abandoned camps with their chests and the explorer maps only they give | v0.16 | GEN-05f, GEN-06 | The last Overworld structure of 26.3; closes with M3 |
| Dungeons, mineshafts and strongholds piece by piece (with infested blocks); shipwrecks and monuments (with conduits and Conduit Power); fortresses | v0.16 | GEN-05c, GEN-05e, GEN-05g, MOB-01c | Targeted at v0.14 and left unchanged; M3's exit checks need them |
| Mob heads and skulls, wither roses, nether gold ore, and the End blocks already planned (purpur, chorus, end rods, shulker boxes) | v0.16 | BLD-01b, BLD-05a, PRO-06 | The Wither needs wither skeleton skulls; the rest is End or Nether terrain |
| Explorer maps for structures that exist (desert pyramid, warm ocean ruins, buried mineshaft, buried ancient city) and the woodland mansion map | v0.15 | GEN-06 | Cartographer trades, with the cartography table |
| Furnace minecart | v0.17 | FOO-04b | With rails and carts |
| Spawn eggs | v0.18 | UX-03c | Creative inventory work |
| Pottery sherds beyond the seven the ocean ruins give | Deferred | DEF-02 | Archaeology, already deferred |
| Barrier, light, command, jigsaw, structure and test blocks, debug stick, knowledge book, petrified oak slab, air variants | None | — | Technical or creative-only, with no survival role |

Build-5000 wave 1 ([record](verification/2026-11-12-build-5000/wave-1.md))
delivered part of this list early, and it ships in v0.14, Implemented,
unverified: mob equipment slots and drops, zombie reinforcements, endermites,
chainmail armor, experience bottles, copper and netherite horse armor,
nautilus armor (the item), harnesses, bundles, dyed leather armor, and the
eleven missing status effects. What remains of those rows stays with v0.15.

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

Done (owner, 2026-10-20), tagged `v0.8.0`: the M5 baseline, the inventory, Java
navigation and spawning, shape-aware light, simulation distance and save
scenarios, Implemented, unverified. Player acceptance moved to v0.9 and the
streaming budget (PERF-03) to v0.18. [Full scope](delivery-records.md#v08-scope-finish-m0-and-m1).

### M2 — Complete materials and interactions

Close BLD, the confirmed 0.6 integration defects and relevant ENV/FOO block
rules. Complete family members, states, acquisition, recipes, tool/loot/fuel/
compost/repair integration and ordinary building utilities.

Exit: sample every required family through survival acquisition, crafting,
placement, interaction, mining and save/reload. Sponges drain water;
supported fuels burn; compostables compost; buds/trees/cocoa drop and grow
correctly. A builder can use all required orientation/shape/waterlogging
states with consistent collision and lighting.

#### v0.12 scope: building materials

Done (owner, 2026-10-29), tagged `v0.12`: the 32,768-state raise, the wood,
stone, copper, colour and utilities tracks and the 26.3 data pass, Implemented,
unverified. Unready rows moved to v0.13 and later releases; the blocks' sounds
and `materials` frame-time runs to v0.18. [Full scope](delivery-records.md#v012-scope-building-materials).

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

#### v0.13 scope: terrain

Done (owner, 2026-10-31), tagged `v0.13`: generation 2 density terrain (FE-01),
underground and deep dark, giant trees and features, and the items carried
from v0.12, Implemented, unverified; the frame-time check is with v0.18.
[Full scope](delivery-records.md#v013-scope-terrain).

#### v0.14 scope: structures

Approved by the owner on 2026-11-03. Templates are original VoxelCraft
designs: the jigsaw algorithm, pools, placement, loot and residents follow
Java, but no Minecraft template file or block-for-block layout is copied.
No fidelity exception was recorded for this on 2026-11-03. The owner
approved [FE-02](#14-what-can-follow-10) on 2026-11-10, so the GEN-05
layout rows now read "original templates (FE-02)".

0. **Decision and IDs.** The template decision above; v0.14's state,
   item and texture ranges split per track in the
   [architecture reservations](architecture.md#v014-track-reservations).
1. **World generation 3.** New structure layouts change untouched
   terrain, so they apply to new worlds only: `worldgen=3`, with
   generation 2's terrain. Generation 1 and 2 output stays pinned by
   golden chunk hashes.
2. **Template and jigsaw engine** (GEN-05a). A text template format
   (blocks, style roles, connectors, beds), weighted pools with
   fallbacks, and Java's `JigsawPlacement` expansion: breadth-first
   connectors, depth and distance limits, free-space checks, rigid and
   terrain-following pieces ([design](villages.md#generation-3-jigsaw-villages)).
3. **Villages on templates** (GEN-05b). The five village styles built
   from original town centres, streets, houses, farms, pens and lamps.
4. **Outposts and bastions on templates** (GEN-05e, GEN-05g).
5. **New structures** (GEN-05f). Fossils, desert wells, ruined portals,
   trail ruins (without archaeology, DEF-02) and buried treasure.
6. **Explorer maps** (GEN-06) from loot and cartographers; Java loot
   random sequences for chest loot.
7. **Checks.** Chunks identical whatever order, thread or cache state
   generates them (GEN-07); save/reload of generation 3 worlds; looted
   chests stay looted; chunk generation time before and after.

Items 0–3 and the GEN-07 check land first, in one change; items 4–6
follow in later changes.

Status (2026-11-03): items 0–7 are implemented, unverified. Items 0–3
and the GEN-07 check ([record](verification/2026-11-03-v0.14-jigsaw-villages.md))
landed first; items 4–6 followed in four parallel tracks, each with its
record:

- 4: [outposts and bastions](verification/2026-11-03-v0.14-outposts-bastions.md).
  Generation 3 outposts and bastions are jigsaw-built with Java's start
  pools, pool names and weights (bastions use a compact pool graph) and
  `bastion_generic_degradation`; residents come from template markers
  with Java's mob-pool weights.
- 5a: [ruined portals and buried treasure](verification/2026-11-03-v0.14-portals-treasure.md).
  Java's placement grid, seven settings (Nether too), vertical placements
  and processors; buried treasure on beaches; `/locate` for both.
- 5b: [fossils, wells and trail ruins](verification/2026-11-03-v0.14-fossils-wells-trail-ruins.md).
  Overworld and Nether fossils, desert wells and buried trail ruins.
- 6: [maps and loot](verification/2026-11-03-v0.14-maps.md). Empty and
  filled maps with Java's update and colours, explorer maps from chests
  and 26.3 cartographer trades (buried treasure maps included), and
  Java's `LootTable.fill` with per-world random sequences for structure
  chests.

Loot isn't terrain, so it isn't held to the generation number: chests
generated from now on in worlds of any generation fill with Java's
`LootTable.fill`, including the cartographer table for village
cartographer chests and explorer maps. Chests already saved keep their
contents, and the blocks of generations 1 and 2 stay pinned by their
golden hashes.

Gaps found: Java 26.3's abandoned camp (already a recorded gap) isn't
in this scope, so the explorer maps only it gives stay absent, as do the
mansion, trial chamber and ancient city maps until those structures
exist. No allays (the outpost allay cage stays empty), dried ghasts
(Nether fossils), archaeology (DEF-02), recovery compasses, map scaling
or locking UI, item-frame map images or LAN map image sync. The
expansion hack isn't modelled, and decoration randomness uses VoxelCraft
streams, not Java's. The beardifier wasn't modelled either, so pieces stood
on pillars and in hillside cuts; since 2026-11-09 villages and outposts
choose floors the ground can meet and blend the terrain around them
([record](verification/2026-11-09-structure-terrain-blending.md)).
Generation 3 chunk generation measured within 1% of generation 2 near
outposts, bastions, deserts, swamps and trail ruins, and 8% slower next
to a ruined portal (0.626 against 0.579 ms per chunk).

<a id="v015-scope-wildlife"></a>

#### v0.14 scope, wildlife part (planned as v0.15)

Approved by the owner on 2026-11-04, started before v0.14's play because
it doesn't touch structure generation. Mobs, blocks and items follow Java
26.3's behaviour, drops, spawn rules and timers; models, textures and
sounds are original. Four tracks run in parallel, each with its own IDs
in the [architecture reservations](architecture.md#v015-track-reservations):

0. **Decision and IDs.** This scope and the per-track ranges.
1. **Lightning and undead mounts** (ENV-03, FOO-04c). Lightning in
   thunderstorms with Java's per-chunk odds and lightning-rod attraction;
   fire (the existing `lightning_fire`), charged creepers, pigs to
   zombified piglins, villagers to witches, copper de-oxidation, the
   rod's redstone pulse; skeleton-horse traps, skeleton and zombie
   horses; snow piling up in snowfall with `max_snow_accumulation_height`;
   zombies turning into drowned underwater and husks into zombies.
   **Implemented, unverified (2026-11-04)** in track 1: strikes, flashes,
   original thunder, conversions, charged creepers, unwaxed copper cleaning,
   powered rods, skeleton traps, saved rideable undead horses and natural
   zombie-horse riders, snow gamerule and underwater conversion timers.
   [Evidence and parity gaps](verification/2026-11-04-v0.15-lightning.md):
   integrator mooshroom/waxed-root hooks, jockey spears, trap enchantments,
   exact underwater horse travel and visual/ordinary-play evidence remain.

2. **Crops and bees** (FOO-01, FOO-02, BLD-01c). Melon and pumpkin stems
   and attached stems, beetroots; bees, bee nests and hives, pollination
   and crop boosts, honey bottles and honeycomb, honeycomb blocks;
   honeycomb waxing of the 56 waxed copper blocks and of signs, and
   candles in survival. **Crop slice implemented, unverified (2026-11-04):**
   the 28 crop states, soil/spacing growth formula, beetroot's Java 2/3
   tick gate and divided bone-meal gain, fruit attachment/release, seed
   planting and drops, melon yields, seed/soup recipes, beetroot in the
   existing village palette and farmer harvest/replant recognition.
   **Bee slice implemented, unverified:** nests/hives with saved occupants,
   natural and flower-adjacent sapling nests, nectar/pollination/crop boosts,
   breeding, anger/stinging, smoke-aware harvesting, honeycomb and Silk Touch
   transport. **Waxing implemented, unverified:** 56 copper blocks, wax/unwax
   transactions and recipes, oxidation protection and sign edit protection.
   **Candles obtainable, unverified:** string above honeycomb crafts the
   existing plain candle; dyeing and candle-cake uses supply the remaining
   survival variants ([evidence](verification/2026-11-04-v0.15-crops-bees.md)).
3. **Passive mobs** (MOB-03). Bats; frogs, tadpoles, frogspawn and the
   three froglights; pandas with their genes; polar bears; armadillos,
   scutes and wolf armor; mooshrooms and suspicious stew; ocelots; the cat
   spawner in villages and swamp huts.
   *Implemented, unverified (track 3, 2026-11-04):* mooshrooms (red/brown,
   cow goals and wheat breeding, variant mutation, milk/stew, stored flower
   effects, shearing, lightning conversion hook) and ambient bats (26.3
   spawning, ceiling resting, flight). Suspicious stew is craftable and
   edible with flower-specific effects, including Saturation. Original
   models, save/reload and shared player actions are covered by focused
   tests; see the [record](verification/2026-11-04-v0.15-passive-mobs.md)
   for source discrepancies and remaining parity gaps. Frogs/tadpoles,
   pregnancy and frogspawn hatching, biome-tag variants, tongue predation,
   axis-oriented froglights and tadpole buckets are also implemented,
   unverified. CatSpawner now implements the five-occupied-bed/five-cat
   village rule, periodic hut attempts and the once-only black hut resident,
   also unverified. Polar bears now have cub groups/defence, neutral anger,
   standing attacks, swimming and fish loot (implemented, unverified).
   Pandas now have inherited genes and mutations, bamboo breeding, snacks,
   sneezes, rolls, thunder fear and retaliation (implemented, unverified).
   Armadillos now roll around threats, shed scutes and can be brushed;
   owner-equipped wolf armor absorbs damage, repairs, dyes and washes
   (implemented, unverified). Jungle ocelots now flee, trust fish-feeding
   players, breed, hunt chickens/baby land turtles and persist trust
   (implemented, unverified).
4. **Hostiles** (MOB-04, PRO-02). Strays, bogged and phantoms (insomnia,
   `spawn_phantoms`, membranes), and tipped arrows. *Implemented, unverified
   (2026-11-04): natural variants, conversion/shearing, original models, loot,
   saved rest and phantom state, phantom custom spawning/circle/swoop flight,
   Slow Falling brewing, breath collection, lingering clouds and tipped-arrow
   crafting; shared bows/crossbows retain potion components. Skeleton and
   phantom AI scheduling, armor/chainmail parity, graphical evidence and
   performance acceptance remain open. See
   [track evidence](verification/2026-11-04-v0.15-hostiles.md).*
5. **Integration and checks.** Cross-track pieces (lightning turning
   mooshrooms), spawn tables together, save/reload of every new mob and
   block, and per-track verification records. *Implemented, unverified
   (2026-11-04): all four tracks merged, a three-provider council review
   (three rounds) fixed nine bugs, and checks and the headless benchmark pass; ordinary
   play remains. See the [integration record](verification/2026-11-04-v0.15-integration.md).*

Moved to later v0.14 changes (owner, 2026-11-04): the creaking and
resin, allays and sniffers. Copper golems and statues were approved next
on 2026-11-05, as recorded below.

**Copper golems and statues — approved 2026-11-05.** The next v0.14
slice completes their survival loop against the pinned Java 26.3 reference:
player construction, item transport from copper chests, oxidation,
waxing and axe scraping, conversion to statues and reactivation, original
models and sounds, and persistent mob, carried-item and statue state.
Shared player actions cover the existing input adapters. Automated
reference, save/reload and performance checks accompany an agent hands-on
run; the owner's ordinary play remains separate evidence. This work may
proceed while v0.14 awaits that play. Creaking/resin, allays and sniffers
remain subsequent slices. *Implemented, unverified (2026-11-05):* construction
replaces the source copper block with a matching copper chest; golems transport
component-bearing stacks, weather independently of day-time commands, wax/scrape,
wear a shearable poppy, and become named pose-bearing statues. Statues preserve
name and pose through placement, mining, saves and held-item rendering; axe
reactivation creates a fresh named golem after all oxidation/wax is removed.
Original models, textures and synthesized golem action sounds are included.
Four council rounds approved the result; mounted item use at statues is covered
by the shared-action regression. Documented navigation, pattern, generic
mounted-block and LAN edge cases remain follow-ups.
See the [integration evidence and remaining gaps](verification/2026-11-05-v0.15-copper-golems.md)
and [identifier allocations](architecture.md#v015-copper-golem-reservations).
This does not close the remaining wildlife scope or establish owner play.

**Creaking, allays, sniffers and ocean ruins — approved 2026-11-05.** The
owner approved the last three wildlife slices as four parallel tracks
against the pinned Java 26.3 reference, with original models, textures,
sounds and templates, IDs in the
[architecture reservations](architecture.md#v015-final-slice-reservations):

- **A. Creaking and resin.** Creaking hearts (axis, uprooted/dormant/awake,
  natural) in generated pale oak trees, the creaking (frozen while watched,
  bound to and protected by its heart, damaged only through it), resin
  clumps, the resin block, resin bricks with stairs, slab, wall and
  chiseled bricks, and eyeblossoms that open at night and close by day.
  Track A (2026-11-05/06): hearts and protectors, the resin family,
  eyeblossoms (with pots, dyes and stew) and generation 3 pale garden hearts
  and eyeblossom patches are **Implemented, unverified**. See
  [the implementation record](verification/2026-11-05-v0.15-creaking.md) for
  checks and the remaining brain, presentation and lifecycle differences.
- **B. Allays.** Item gifting, liking and collection, note-block
  preference, jukebox dancing and amethyst duplication, and allays in the
  outpost allay cage. *Implemented, unverified (2026-11-05):* flying allays,
  shared gifting/return and Java-equality collection/delivery, saved
  player/note memories, note vibrations and two persistent generation-3
  cage residents. Original model, procedural skin and synthesized sounds;
  summon and LAN index 70. Dancing state and duplication callback are
  present, but survival dancing/duplication remains a gap because this
  checkout has no jukebox/music-disc playback. Mansion allays remain a gap.
  See the [sources, checks and differences](verification/2026-11-05-v0.15-allays.md).
- **C. Sniffers.** Sniffer eggs and hatching (faster on moss), sniffing and
  digging up torchflower seeds and pitcher pods, breeding, and the
  torchflower and pitcher crops, plants and recipes.
  Implemented, unverified (2026-11-05): crop/flower states, growth, bone meal,
  pots, dyes, compost and stew; saved sniffers, digging gifts and explored memory,
  seed breeding/temptation, egg laying and scheduled moss-boosted hatching,
  with original sniff, search, dig, egg crack/hatch/plop cues and digging and
  egg particles. Search/navigation and animation remain partial; survival egg acquisition
  requires Track D. [Track C evidence](verification/2026-11-05-v0.15-sniffers.md).
- **D. Ocean ruins and suspicious blocks.** Warm and cold ocean ruins (the
  remaining v0.14 GEN-05 structure) with Java's placement, variants and
  chest loot, drowned residents, and their suspicious sand and gravel.
  This narrows DEF-02 (owner, 2026-11-05): suspicious sand and gravel and
  brushing them are now core, so sniffer eggs come from warm ocean ruins
  the Java way. The pottery sherds those loot tables name exist only as
  items; decorated pots and their patterns stay deferred.
  Track D status (2026-11-05): **Implemented, unverified** suspicious blocks,
  shared brushing, seven plain sherds and generation-3 original-template
  warm/cold clustered ruins with chest loot and drowned. Since integration
  (2026-11-06) the warm table's 1-in-15 slot yields `Item::SNIFFER_EGG`, so
  sniffers are obtainable in survival. Brushing shows Java's dust particles and
  emerging reward (2026-11-06); dedicated brush sounds, LAN replication of
  the emerging reward and full reference random-stream/layout fidelity
  remain gaps. See [evidence](verification/2026-11-05-v0.15-ocean-ruins.md).


New generation (creaking hearts, eyeblossoms, ocean ruins, outpost
allays) applies to generation 3 only, which hasn't been tagged; the
golden hashes of generations 1 and 2 stay unchanged.

*Integrated, implemented, unverified (2026-11-06):* the four tracks merged
in order A, C, D, B with the cross-track pieces wired (sniffer eggs in warm
ruins; mob, icon, sound and texture orderings reconciled). Codex reached its
usage limit mid-round, so Claude Opus 5.5 finished every track, and builds ran
on Namespace instances. The owner's play remains. See the
[integration record](verification/2026-11-06-v0.15-final-integration.md).

<a id="v016-scope-deep-dark-and-trials"></a>

#### v0.14 scope, deep dark and trials part (planned as v0.16)

Approved by the owner on 2026-11-06, started while v0.14's structures and wildlife wait
for the owner's play. Blocks, items, mobs and structures follow Java
26.3's behaviour, loot tables, spawn rules and timers; models, textures,
sounds, music and structure templates are original (no fidelity exception
yet). New structures generate in generation 3, which hasn't been tagged;
the golden hashes of generations 1 and 2 stay unchanged. Four tracks run
in parallel, each with its own IDs in the
[architecture reservations](architecture.md#v016-track-reservations):

0. **Decision and IDs.** This scope and the per-track ranges.
1. **A. Sculk and the warden** (MOB-04, GEN-02). Game events and
   vibrations with Java's frequencies, ranges and wool occlusion; sculk
   sensors and calibrated sculk sensors (redstone output and phases);
   catalysts spreading sculk and veins from mob deaths with their XP;
   shrieker warning levels, Darkness and warden summoning; the warden
   (emerging, sniffing, per-entity anger, melee, sonic boom, digging back
   down, its loot); the `spawn_wardens` rule.
   Implemented, unverified (2026-11-06): game events and vibrations with
   Java's frequencies, radii, wool occlusion and sneaking (`world::vibration`,
   `emit_game_event`), active sensors and calibrated sensors (power by
   distance, comparator frequency, amethyst resonance, the crafted
   calibrated sensor), shriekers with per-player warning levels, Darkness
   and summoning, catalysts taking death experience and spreading sculk,
   and the warden (brain, anger, melee, sonic boom, emerging, digging,
   Darkness pulses, loot, saves). Gaps: Java's random sequences, some
   event sources (mounting, buckets, shearing, teleports), lightmap
   darkness; see the [record](verification/2026-11-06-v0.16-sculk-warden.md).
2. **B. Ancient cities, loot and music** (GEN-05, PRO-07). Ancient cities
   from the jigsaw engine in deep-dark biomes, with summoning shriekers,
   reinforced deepslate, the portal-frame centrepiece and Java's
   `ancient_city` and `ancient_city_ice_box` chest tables; echo shards and
   the recovery compass; Swift Sneak; disc fragments and disc 5; a jukebox
   playing music discs (original tunes, redstone output, allay dancing and
   duplication); ancient city maps where their loot names them.
   *Implemented, unverified (2026-11-06):* generation 3 ancient cities from
   the jigsaw engine with Java's placement (24/8, salt 20083232, deep-dark
   stub check, depth 7, distance 116, empty spawn overrides) and original
   templates under Java's pool names; reinforced deepslate (55 hardness, no
   drop, immovable), summoning shriekers and sculk patches; both chest
   tables; echo shards, disc fragments, disc 5, the recovery compass (saved
   per player) and Swift Sneak; the jukebox with all 22 Java 26.3 discs
   (Java lengths and comparator outputs, original procedural tunes,
   redstone, hoppers, allay dancing, save/reload) and creeper discs.
   Gaps: skulls, armor trims and experience bottles in the loot, the
   shorter record hearing range (24 blocks, Java 64), music ducking and the template geometry are
   recorded in the [track B record](verification/2026-11-06-v0.16-ancient-cities.md).
3. **C. Trial chambers** (GEN-05, MOB-04, PRO-07). Trial chambers from the
   jigsaw engine with Java's placement; trial spawners (normal and ominous,
   per-player scaling, detection and cooldowns); vaults and ominous vaults
   with trial keys and their reward tables; Bad Omen to Trial Omen and
   ominous bottles; the breeze, breeze rods and wind charges; bogged and
   other chamber spawner residents; trial chamber maps.
   *Implemented, unverified (2026-11-06):* generation 3 trial chambers
   with Java's placement and pool topology on an original, reduced template
   catalogue; trial spawners (all six states, per-player scaling and
   detection, 30-minute cooldown, Java's 14 configurations and reward
   tables, ominous conversion and ominous item spawners); vaults and ominous
   vaults with per-player rewards; trial keys, ominous bottles, Bad Omen to
   Trial Omen; the breeze with its jump/shoot/slide brain, deflection and rod
   drops; wind charges and the shared `entity::wind::wind_burst`; the
   cartographer's trial explorer map. Gaps (decorated pots, the potions the
   ominous drop table names, swords on ominous zombies, presentation) are in
   the [Track C record](verification/2026-11-06-v0.16-trial-chambers.md).
4. **D. Weapons** (PRO-07, ENV-04). The heavy core and the mace (smash
   attack, fall-damage reset, Density, Breach, Wind Burst); tridents
   (throwing, Loyalty, Riptide, Channeling, Impaling, drowned drops,
   breaking pointed dripstone); spears of every Java tier (charge and
   lunge attacks, the Lunge enchantment); the spear entries already
   named in bastion and temple loot.
   Implemented, unverified (2026-11-06): the heavy core block, the mace
   (smash, fall-damage impulse, area knockback, Density, Breach, Wind
   Burst's launch), tridents (charge, saved thrown entity, Loyalty,
   Riptide, Channeling, Impaling, pickup, dripstone breaking, drowned
   that spawn with, throw and drop them), all seven spears (recipes,
   smithing, jab, charged attack, Lunge), enchanting/anvil rules and the
   loot entries, on every input path
   ([record](verification/2026-11-06-v0.16-weapons.md)). Open: mob spear
   use, Java's held-item poses, Wind Burst's block interaction (Track C
   hook) and the mace recipe's breeze rod (Track C).
5. **Integration and checks.** Cross-track pieces (Wind Burst on wind
   charges, summoning shriekers in ancient cities, discs in vault loot),
   spawn tables together, save/reload of every new mob, block and item,
   a council review, the Namespace checks and per-track verification
   records.
   *Integrated, implemented, unverified (2026-11-07):* the four tracks
   merged, their append-only orderings made contiguous (warden 72, breeze
   73; Swift Sneak 40, Wind Burst 41), the duplicated impulse fall rule
   reconciled, and the jukebox, trident, Wind Burst, breeze rod and trial
   loot links wired. The owner's play remains. See the
   [integration record](verification/2026-11-07-v0.16-integration.md).

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
The updater comes first, in v0.10, then menus and settings in v0.11 (owner,
2026-10-22); the rest of M7 follows the content releases.

The owner added the graphics and UI review to this plan on 2026-11-05.
Its eight work areas are part of the [v0.18 scope](#v020-scope-presentation-and-performance)
below, with their existing requirement owners. This plans presentation work;
it does not mark it implemented, reopen shipped releases or change the
earliest-unreleased-version rule.

#### v0.10 scope: updates

Tagged `v0.10` on 2026-10-22 before its exit check (owner exception): the
signed in-game updater for Windows and Apple Silicon Macs and safe level-file
failures, Implemented, unverified until a real v0.10 install updates.
[Full scope](delivery-records.md#v010-scope-updates).

#### v0.11 scope: menus and settings

Done (owner, 2026-10-25), tagged `v0.11` without its exit checks: Java's title,
world and options screens, key rebinding, GUI scale and sound categories,
Implemented, unverified; the menu and title frame-time run moved to v0.18.
[Full scope and known gaps](delivery-records.md#v011-scope-menus-and-settings).

<a id="v020-scope-presentation-and-performance"></a>

#### v0.18 scope: presentation and performance

**Planned; added by the owner on 2026-11-05.** Complete the graphics and UI
work below alongside the existing sound review, subtitles, recipe discovery,
menu fixes and carried performance work. Preserve familiar Java layouts,
block and mob silhouettes, pixel art and distinct biome/dimension palettes.
Use original or appropriately licensed assets. FE-01's original Overworld
terrain remains in scope as approved; this review does not authorize a
generator replacement, different gameplay rules or unrelated graphics modes.

The [2026-11-05 visual review](verification/2026-11-05-graphics-ui-review.md)
records the observations, source evidence and limits behind this scope.
Scripted captures establish appearance, not ordinary-play or performance
acceptance. No requirement is promoted by this plan addition.

Start with a short tracked guide at `docs/art-and-ui.md` and reproducible
comparison scenes; the guide is a planned deliverable, not yet written.
The guide should define pixel density, palette/contrast ranges, natural versus
manufactured material patterns, glyph metrics, spacing, borders, slots,
focus/selection/disabled states and progress icons. Reuse the existing
[asset pipeline](restructuring.md#str-04--original-asset-pipeline),
[menu toolkit](../src/app/gui/mod.rs), item rendering and animation systems.
This is focused improvement of working components, not an engine or UI rewrite.

Use Java 26.3 reference fixtures for observable layouts, visual behavior and
settings; original artwork does not waive that contract. As each visual lands,
add its functioning Java option instead of leaving the corresponding v0.11
omission in place. Cover Entity Shadows, Clouds, Mipmap Levels, menu-background
blur, panorama speed, text-background opacity and sky-flash suppression under
UX-03f, checking exact labels, ranges and defaults against the selected client.
Assess the loading-screen Monochrome Logo option where original branding has
an applicable loading screen; it is not a title-wordmark color switch. Do not
add inert controls. Record any remaining in-scope omission explicitly.

Work items, with priority within v0.18:

1. **Typography, contrast and label fit — highest priority (UX-01,
   UX-03f).** Adopt a cleaner original or appropriately licensed pixel font
   with measured glyph widths and consistent baselines. Make ordinary labels and values
   readable at rest through sensible metrics, wording and layout; retain
   overflow handling for genuinely long content. Distinguish panel, text,
   active, focused and disabled colors. Audit the intended palette and the
   flat-color path in [the UI shader](../src/render/shaders/overlay.wgsl)
   against the sRGB render target before retuning colors; the review identified
   a question to investigate, not a proved color-conversion defect. Coordinate
   this work with item 8 so larger scales and accented text are supported by
   the same font/layout system.

2. **Core material art — highest priority (UX-01, BLD-05c).** Redraw natural
   stone and its ore backgrounds first, then dirt, grass, leaves, sand and
   common woods. Use
   deliberate pixel clusters, recognizable material patterns and controlled
   contrast; reduce overpowering greens while preserving biome differences.
   Keep 16 × 16 block textures and useful procedural generation. Review
   tiling over large walls, cave ceilings and hillsides as well as individual
   tiles, inventory icons and held/dropped models. Existing mipmaps and
   shape-aware icons are foundations to retain, not missing systems to rebuild.
   Record provenance and runtime/memory costs through the asset manifest.

3. **Inventory and recipe discovery — high priority (UX-03c).** Match Java
   26.3's creative tabs, item ordering, grid dimensions and scrolling behavior,
   recorded in layout fixtures at supported GUI scales; keep search readily
   available. Build the already-planned Java recipe discovery flow: unlocking, searchable
   categories/results, craftable filtering, ingredient availability and recipe
   selection into the crafting grid with Java's transaction rules. Use the
   shared recipe/action data; do not duplicate recipes in the UI or create
   items through a presentation shortcut. Clarify equipment grouping and
   silhouettes, add a default-player preview and make the recipe control
   discoverable in survival. Custom skin selection remains deferred. Functional
   workstation/container work stays with AUT-04 and its owning releases;
   v0.18 improves and verifies those screens after their behavior is present.
   Add backward-compatible per-player storage for known/highlighted recipes
   and book/filter state, keyed by stable recipe identifiers rather than list
   indices. New players follow Java's initial state. Legacy players lacking
   recipe data start from that state and re-evaluate Java's unlock criteria
   using their saved inventory and any supported persisted progression evidence.
   Do not invent unrecorded historical unlocks or grant every recipe. Preserve
   explicitly saved world rules, use Java's default book/filter settings where
   absent, and test normal discovery after migration, including worlds with
   limited crafting enabled; old players must retain a playable acquisition path.
   Implement `limited_crafting` (`doLimitedCrafting`) with the same shared
   transaction rules, as already assigned to UX-03c in [game rules](game-rules.md).
   Save/restart and legacy-player migration are explicit parts of this row.

4. **Mob artwork, grounding and motion — high priority (MOB-06, UX-01,
   BLD-05c).** Preserve cuboid construction and improve original faces,
   markings, fur/wool patterns and
   material contrast. Add inexpensive ground shadows for mobs and dropped
   items using Java's per-entity radius, height fading and visibility rules,
   controlled by Entity Shadows (UX-03f). Respect supporting terrain without
   changing hitboxes or simulation light. Use the existing model/animation
   systems and review motion separately for readable poses, interpolation and Java action
   timing. The screenshots did not establish an animation defect. Asset format
   support must grow with any new resource type: the existing static box-model
   format is not an animated-entity skin pipeline. Profile entity submission
   and resolve the recorded per-frame model rebuild bottleneck before adding
   per-frame detail/shadow cost, or record measurements showing it no longer
   applies. (Resolved on 2026-11-09: plain mob boxes are GPU instances,
   [record](verification/2026-11-09-entity-frame-time.md). Textured, flat and
   translucent parts still build on the CPU each frame, so measure any new
   per-frame detail against that record.) Preserve simulation correctness while reducing that render cost.

5. **HUD, workstations and secondary screens — high priority (UX-01,
   UX-03c/g).** Apply the shared style to menus, inventory/recipe panels,
   HUD and death/pause/error screens: consistent borders, spacing, text
   hierarchy, focus, selection and disabled states. Improve slot separation
   and pixel icons for flames, progress arrows, equipment hints and status
   effects. Match Java's background darkening and menu-blur behavior, with the
   working blur control under UX-03f. Default the FPS counter off only when
   no saved preference exists; keep existing saved true/false values and the
   debug tools. Use Java's death actions: Respawn/Title Screen normally and
   Spectate World/Title Screen in Hardcore, including the reference button
   delay and quit confirmation. Preserve immediate-respawn, pause and session
   ownership rules (PLY-01c), including existing split-screen/LAN behavior.
   Apply shared component fixes to updater and error dialogs without altering their
   confirmations, recovery guidance or signed-update behavior.

6. **Title identity and world recognition — medium priority (UX-01,
   UX-03e).** Create an original pixel-art wordmark and a composed, pre-rendered
   cubemap panorama of VoxelCraft terrain and structures with Java's panorama
   behavior. Preserve the familiar menu arrangement, version and update status. Add real world
   thumbnails and a useful empty-list message; match the reference icon size,
   first-join capture/refresh behavior and Edit World's Reset Icon action.
   Record those details in fixtures rather than requiring vanilla save-format
   compatibility. Thumbnail creation/loading needs bounded cost and a safe
   fallback for old saves or missing images;
   it must not block loading a valid world or change its gameplay data.

7. **Atmosphere and visibility — medium to high priority (UX-01/02).** Match
   Java 26.3's fog ranges, shape, colors and transitions by dimension, biome,
   weather, fluid and status effect using reference fixtures; do not assume a
   historical render-distance fraction or adopt snapshot fog changes. Compare
   ground-level and elevated views, reduce rain's near-camera
   visual weight, and refine water's surface pattern and clarity using its
   existing waves, Fresnel response and sky reflection. Match Java's flat and
   volumetric block-shaped cloud modes and underside shading, with the
   corresponding Clouds control (UX-03f). Refine ambient darkness,
   warm torchlight and contact shading in interiors. Preserve biome/weather/
   dimension distinctions, underwater readability and authoritative gameplay
   light levels; spawning, detection and other mechanics must not change with
   visual settings. Retain the Nether's strong palette as a useful reference
   and evaluate the End separately. Measure changes within existing graphics
   modes before adopting them; no photorealistic rendering overhaul is implied.

8. **Accessibility throughout the visual work — required before 1.0
   (UX-03d/f, UX-02).** Retain GUI scaling and motion controls. Complete the
   planned directional subtitles from sound events, with readable text
   backgrounds. Improve keyboard/controller focus and selection cues without
   relying on color alone. Replace the font renderer's `?` substitution for
   ordinary accented characters in world names, signs, name tags and other
   user-entered text, including text shown with maps or held items where
   supported. Audit UI, sign and entity-label renderers and text entry together;
   make measurement, cursor movement, selection, clipping and saved text agree.
   Existing sign text must load unchanged and must not be silently truncated
   when font metrics change; retain Java's line-width rules for new edits.
   This is text support, not a new commitment to a full translated-language
   catalogue or resource packs. Large GUI scales must keep routine labels
   and actions usable. Apply these requirements from the first typography/
   component changes, not as a separate final reskin.

**Sequence and dependencies.** Establish the guide and comparison scenes,
then typography/shared UI contrast and core materials. Inventory/recipe and
workstation presentation follows its shared layout and gameplay dependencies.
New per-frame mob/shadow work follows resolution of the measured entity cost.
Title identity and atmospheric refinement follow the material/style decisions.
Accessibility is part of each step. Existing sound/music/particle review,
the v0.12 blocks' own sounds and carried visual gaps (including container
opening/lid animation) remain in this release; these graphics findings do
not replace them. Keep all carried performance work from v0.8/v0.9, v0.11
menus/title, v0.12 materials and later unverified content, including PERF-03.

**Evidence for implementation.** First extend the capture harness with an
explicit GUI-scale override (scripted runs currently ignore saved options)
and deterministic recipe-book and Hardcore-death setup. Reuse existing menu,
inventory and `--open-block` workstation capture; add only missing states.
Use the existing [scripted-scene tools](development.md#screenshots-and-scripted-scenes) for
repeatable before/after material, mob/item shadow, inventory/workstation,
title/world-list and dimension-appropriate day/dusk/night/weather scenes
covering all three dimensions.
Record commit, seed/generation, pose, placements, entities, time/weather,
resolution and GUI/graphics settings; include 1920 × 1080 and 1280 × 720
layouts with automatic and large supported scales. Keep artifacts under
ignored `target/` and record methods/results in tracked verification notes.
Inspect ground contact on uneven terrain and check visibility underwater and
in torch-lit interiors. Rendered scenes establish appearance only. Interactive
changes need the existing automated controller/CLI adapter coverage and
relevant save/restart checks; the owner's ordinary play supplies player
evidence, without a new formal acceptance checklist or physical-controller
session. Agent observations remain separate from that evidence.

Measure renderer, asset and UI changes in the relevant existing M5 scenes,
including entities, menus/title and materials. Add reproducible weather,
underwater and Nether benchmark fixtures under §6.2 before atmospheric tuning;
those scenes are not in the current runner. Update the title scene for the
panorama and compare its cost against the old sky-only baseline. Record
startup, memory and cache costs where applicable. The unchanged
[§6 gate](#6-performance-acceptance) requires 1% low at least 240 FPS and p99
at most 4.17 ms at 1920 × 1080 physical pixels and 256-block view range, plus
the separate streaming budget. FPS displayed in a screenshot, an offscreen
capture or a passing build does not satisfy it. Update the existing
[requirement rows](requirements.md#ux--presentation-usability-and-distribution)
as work lands; none is promoted by this plan addition.

**v0.18 exit:** UX-01/02, UX-03c–g, MOB-06 and BLD-05c meet their recorded
scope, with the owner's ordinary-play evidence and applicable reference,
adapter and persistence checks. Close the carried v0.11 menu/title checks for
UX-03a/b and all carried performance runs, including PERF-03, with the final
visuals enabled. Rows reach Verified; any unfinished work requires an explicit
owner-approved move under the release policy, not a silent deferral past 1.0.

**M7 exit:** all mandatory core rows are Verified; every remaining exception/
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
- Every release records a run (audit, 2026-11-09). Before a version is
  offered to the owner, the scene suite runs on the reference machine at
  the commit to be tagged, and its per-scene results go in
  `benchmarks/results/` and the version's verification record. A scene
  whose 1% low or p99 is worse than the previous recorded run needs a
  profile and either a fix or a written reason in that record. The owner
  may still tag a version that misses the 240 FPS gate, as before, but not
  one that was never measured. This replaces moving each release's
  frame-time check to v0.18: between 2026-10-16 and 2026-10-29 the
  `building` scene's 1% low fell from 261–273 to 229 with nobody looking.

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
   test alone is insufficient. The owner's own ordinary play is this
   evidence (owner, 2026-10-22): there is no formal checklist pass or
   physical-controller session; controller and CLI paths rely on the
   automated adapter tests, and problems found in play go back to the agents.
5. **Persistence:** save/restart and unload/reload relevant state, including
   interrupted work, carried components, scheduled actions and cleared loot.
6. **Performance:** measure the feature in representative loaded gameplay;
   no unbounded caches/queues or unexplained regressions.
7. **Delivery evidence:** record the commit, scope, test results, manual
   steps/captures and remaining differences; update the requirement state.

Three release steps were added by the 2026-11-09 audit, after v0.16 (now part of v0.14) was
merged with its record saying "Not run: the game":

- **An agent runs it first.** Before a version is offered to the owner, an
  agent launches the merged build and looks at every new structure, mob,
  block and screen in it (scripted scenes, and hands-on through computer
  use on the model [AGENTS.md](../AGENTS.md) names). What it saw and what
  it couldn't check go in the version's record. This is agent evidence; it
  doesn't replace the owner's play, it makes sure that play isn't the first
  time the game is started.
- **A frame-time run is recorded**, as [§6.1](#61-reference-configuration-and-frame-time-gate)
  now requires.
- **At most one version waits.** While a version is implemented and waiting
  for the owner's play, agents may work ahead on the next one where its
  dependencies allow, as the release plan says. They don't start a third:
  three unplayed versions is how 60,000 lines reached `main` unseen. This
  limit is the audit's; the owner can lift it for a given release. The
  owner's 2026-11-12 request for 5,000 features is read as lifting it for
  that push ([current plan](#current-plan--updated-2026-11-13)); the other
  two steps still apply to everything it merges.

Use the repository's required check chain for source changes:
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
Keep roadmap updates reviewable alongside implementation changes and release
notes.

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
