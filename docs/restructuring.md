# Project restructuring plan

[Documentation index](README.md) · [Roadmap](roadmap.md) · [Agent instructions](../AGENTS.md)

This implementation plan records delivered slices and remaining work. The
repository/agent-notes migration established versioned guidance; the initial
assessment delivered the first content/action/fixture changes. Each task's
status below distinguishes those slices from the larger work still planned.
The roadmap remains authoritative for product scope and completion. Link evidence
here and update its affected requirements as each change lands.

## Pending v0.7 work: retain and assess

As inspected on 2026-10-12, branch
[`t3/continue-project-build`](https://github.com/BrendanH18/VoxelCraft/tree/t3/continue-project-build)
was retained and merged into the assessment branch at `22ad25c`. Its original
branch remains at package version 0.7.0, commit
[`f5bf0d1`](https://github.com/BrendanH18/VoxelCraft/commit/f5bf0d169487452383aa3b6ab588b29fe08814fb).
It contains substantial animals, breeding, companions, leads, mounts, boats,
LAN implementation and tests. Its presence and historical test reports do not
establish completion under the new roadmap. Preserve that branch and any
uncommitted work; a new plan is not a reason to delete it.

| Pending work | Relevant roadmap requirements | Next disposition |
| --- | --- | --- |
| Animals, breeding, companions, leads and naming | MOB-03, MOB-06, FOO-02 | Partial; retained in `22ad25c`, which also corrected the Java 26.3 lead recipe (`retained_transport_recipes_match_java_26_3_and_consume_the_grid`). Finish interaction, persistence and performance acceptance |
| Boats, mounts, storage and transport | FOO-04, AUT-05 | Partial; retained in `22ad25c` with the Java 26.3 saddle recipe (same test). Finish interaction/persistence/performance checks |
| Shared fixes, including save locking and interaction integration | PLY-04, UX-04, UX-05 | Useful turtle gating and save-lock code identified; full player/platform acceptance remains |
| LAN networking and multiplayer presentation | Post-1.0 scope | Preserve existing code; defer further expansion and maintain shared behavior |

Before integrating the branch:

1. Record its current tip and inventory changes against `main`, including
   uncommitted work separately. Historical briefs help identify intended scope.
2. Classify each behavior under the new roadmap. Review existing
   `tests/release_0_7.rs`, `tests/lan_loopback.rs` and the LAN harness on that branch
   before writing duplicate checks.
3. Verify relevant single-player behavior, controller/CLI regressions, old/new
   save round trips and representative performance. Report remaining differences.
4. Integrate useful work through a reviewed PR, keeping LAN expansion out of
   single-player acceptance. Choose merge versus focused cherry-picks based on
   dependencies; do not assume tightly coupled commits can be separated safely.
5. Reconcile version and release claims only after a coherent scope passes its
   checks. Do not bump versions, release, merge or delete branches as part of
   this documentation migration.

Pause new feature expansion while this assessment and M0/M1 foundations are
established. Continue bounded bug fixes that preserve existing work.

The [initial assessment and fix report](verification/2026-10-12-v0.7-assessment.md)
records the tested source, passing existing suites, independent reference
failures and first shared content/action changes. It is not a completed v0.7
acceptance pass; the original branch and uncommitted patch remain preserved.

## STR-01 — Reference fixtures and performance scenes

**Status:** Reference item/building/plant fixtures, reproduction tools and process-restart checks implemented; all five initial live scenes are implemented. The first three-run M5 1080p recording ([evidence](verification/2026-10-13-m5-baseline.md)) is invalid: it contains a paused streaming run, occluded-window samples and entity population loss. The harness now rejects such runs ([review fixes](verification/2026-10-13-review-bench-fixes.md)), and the baseline must be re-measured. Native baselines and the broader reference inventory remain in progress.
**Roadmap:** M0; PLY-01, GEN-07, UX-04/05; performance acceptance.

The [lighting/spawning fixtures and restart report](verification/2026-10-13-lighting-spawning.md)
adds independently reproduced cave/torch/border scenes and day/weather cases.

Extend existing headless integration tests rather than replacing them. Add
`tests/fixtures/java/<reference-version>/` for expected behavior and selected
generation outputs. Every fixture records source/provenance, reference version,
setup and expected outcomes; do not bless current Rust output as Java evidence.
Add per-feature scenarios for acquisition/use, boundary conditions, interrupted
work and save/reload. Keep lightweight rule tests beside their modules.

Define reproducible benchmark scenes/routes for ordinary survival, dense
building, entities, active automation and streaming. Record commit, seed,
hardware, physical resolution, view distance, warm-up/sample procedure,
frame-time distribution, memory and streaming hitches in machine-readable
results. Measure native resolution separately. Do not turn the existing
1600x900 synchronized render benchmark into a claim of 1080p acceptance.

**Acceptance:** the first scenario independently detects a known parity defect;
fixtures can be reproduced; save/reload changes observable state correctly;
the scene runner reports the roadmap's 1% low and p99 metrics. Capture a baseline
even if it misses the target. Require M5 hardware evidence for its performance gate.

## STR-02 — Shared content definitions and tags

**Status:** Shared planks, fuel definitions, compost classes and fishing weights implemented, with audited building/plant integrations. The full reference fuel inventory and every currently registered fuel integration
are implemented ([evidence](verification/2026-10-12-fuel-inventory.md)); 171
reference fuel items remain missing registrations. Recipe/registry/presentation
consolidation remains in progress. **Roadmap:** M2; BLD-01/02/03/06, FOO-01/02, AUT-03/04.
**Dependency:** STR-01 coverage for the first migrated content family.

Introduce `src/content/` with authoritative content definitions and typed tags.
Migrate one family at a time from duplicated crafting ingredient, fuel and
composting lists. Centralize recipe/loot integration metadata and presentation
references while retaining specialized behavior code for growth, placement and
other stateful rules. Start with static Rust definitions and compact lookup
tables; a runtime data loader is not a prerequisite.

Preserve numeric block/item/state meanings, texture mappings and legacy save
compatibility. Treat version-specific exceptions explicitly; a generic material
tag must not silently determine every gameplay behavior. Validate expected
integrations and record intentional ineligible/unsupported cases.

**Acceptance:** the migrated family has complete acquisition, recipe, drop,
fuel/compost and presentation checks where applicable; a missing expected
integration produces a useful failure. Old saves keep their IDs. Parsing or
allocation is not introduced into per-cell/per-frame lookups; benchmarks show
no unexplained regression.

## STR-03 — Shared gameplay actions and session ownership

**Status:** Partial: shared bucket/bottle execution, held-armor validation and
actor-owned food/drink consumption are implemented. `PlayerSession` now owns
identity, movement, inventory, vitals, mode, workstation inputs and personal
respawn data in the client and CLI. [Session evidence](verification/2026-10-13-player-session.md)
records automated checks and outstanding acceptance. Actor-owned mining is also
implemented ([mining evidence](verification/2026-10-13-shared-mining.md)), including
reference reach, progress and ice rules. Shared block placement is implemented
([placement evidence](verification/2026-10-13-shared-placement.md)), including
paired-cell validation, inventory spending and low-cube targeting. Melee is
shared with Java attack-strength charging ([combat evidence](verification/2026-10-13-shared-combat.md)).
Buckets and bottles share Java's entity-then-block-then-item priority,
Adventure permission and hit-face pouring on desktop and CLI
([bucket review evidence](verification/2026-10-13-review-bucket-fixes.md)).
Open screens and their targeting use Java 26.3 ranges from the session on every
input path: block containers stay valid within block reach + 4 of the cell,
trading and vehicle storage within entity reach + 4 of the entity's box
(pinned in `combat.json`), and the desktop now closes block screens when the
player walks out of range. Bucket targeting uses block reach; merchant and
vehicle targeting use entity reach. The old shared 6.0 reach constant is gone.
The right-click chain is shared ([use evidence](verification/2026-10-13-shared-use.md)):
`PlayerSession::use_held` runs entity, item and block uses for the desktop and
CLI (and so the headless LAN server), reporting screens, beds, vehicle storage
and trading for adapters to open. Bows and crossbows are tick-based session
state. `Game::puppet` state swapping is removed
([evidence](verification/2026-10-13-shared-use.md#no-state-swapping)): the host,
controller seats and LAN peers run hands through `hands::Actor` and container
clicks through `screen::Screen` on their own session, `Body`, crafting grid and
hand animation; beds, respawn points, pick-block, drops and LAN screen
open/close act on the named player. LAN PvP names its attacker and target.
Remaining STR-03 work: container transactions are still app-level (`Screen`)
rather than session-owned, so the CLI keeps its separate chest/anvil/
grindstone/smithing/trade commands. Device and menu presentation stay adapter
work by design; the crosshair attack indicator reads the session charge.
**Roadmap:** M1; PLY-01/04, AUT-04/05, UX-04/05.
**Dependency:** STR-01 regression coverage and review of pending v0.7 changes.

Extract device-independent action validation and execution from the desktop
application and CLI-agent paths. Input adapters submit actions with explicit
player identity; shared code validates reach, mode, timing, held item and
inventory before mutating state. Separate player/session state from camera,
menus, hand animation and device resources. Return events for presentation.

Begin with bucket use, including ordinary pouring and creature capture, because
the 0.6 audit found different behavior across input paths. Then migrate mining,
placement, combat, item use and workstation/container transactions in small
steps. Replace controller `Game::puppet` state swapping as interactions move.
Preserve each action's current frame/tick timing until a reference-backed
behavior correction is made explicitly. This is not a multiplayer expansion.

**Acceptance:** the same scenario has consistent gameplay outcomes through the
desktop, controller and CLI adapters; invalid actions cannot partially consume
inventory or edit the world. Headless scenarios exercise the shared execution
path. Death, interruption, containers and save/reload preserve the correct
player's state; visual/input checks confirm ordinary play.

## STR-04 — Original asset pipeline

**Status:** Embedded original PNG/model/WAV pipeline, manifest and validation implemented; ordinary-player/visual/audio/performance acceptance in progress. **Roadmap:** M7; UX-01/02, MOB-06.

Add `assets/` for original or appropriately licensed textures, models, audio and
music, with a manifest recording stable names, provenance/licenses and mappings
to runtime resources. Keep procedural generation where useful and allow
individual handmade assets to replace outputs without changing gameplay IDs.
Add validation/packaging tools for dimensions, formats, references, animations
and complete resource inclusion. Design paths for packaged executables, not
just execution from the repository root.

**Acceptance:** one original asset loads correctly in development and packaged
builds; missing resources produce actionable errors; existing procedural
resources still work; visual/audio player checks and startup/frame/memory
measurements cover the change. Asset provenance is reviewable.

## STR-05 — Focused modules, then evaluate a workspace

**Status:** Focused command parsing/execution, entity lifecycle/spawning and application startup/save/tick/action modules implemented. Desktop/headless checks pass. Workspace evaluation retains the existing library/client feature boundary; shared action/session extraction continues under STR-03. **Roadmap:** supports M1–M7; PLY-04, UX-04/05.
**Dependency:** relevant behavior coverage; session extraction informs boundaries.

The [lighting/spawning integration](verification/2026-10-13-lighting-spawning.md)
separates gameplay brightness inputs from visual daylight and extends the focused
entity cycle. Existing shape/cap/clock differences and player/performance
acceptance remain open.

Separate command parsing from execution in `src/agent.rs`, application
coordination from gameplay rules in `src/app/mod.rs`, and entity lifecycle
from individual AI systems in `src/entity/mod.rs`. Extract one responsibility
per focused change; use existing subsystem modules where appropriate.

Keep the reusable library and `client` feature working throughout. Consider
separate engine/client Cargo workspace crates only after those boundaries are
clear and the split has a concrete benefit. A workspace split is a later
decision, not an automatic requirement or a performance claim.

**Acceptance:** desktop and `--no-default-features` builds/tests pass; extracted
simulation code has no device dependency; public interfaces and save behavior
remain compatible; changed hot paths have relevant benchmark evidence. Existing
v0.7 work can be integrated without an unnecessary parallel rewrite.

## STR-06 — Documentation and verification records

**Status:** Repository migration and first dated verification record implemented;
ongoing per-feature evidence workflow continues.
**Roadmap:** M0; every requirement's delivery evidence.

Use `docs/README.md` to distinguish player guides, engineering documents,
plans/status and archives. Keep `docs/roadmap.md` as the only active roadmap and
root `AGENTS.md` as the shared instructions; `CLAUDE.md` imports them.
Archive superseded briefs with explicit historical context.

As features are assessed, add dated reports under `docs/verification/` recording
requirement IDs, tested commit, reference version, automated/manual/save and
performance evidence, and remaining differences. Link reports from affected
roadmap records; do not create a second independent status ledger.

**Acceptance:** a fresh checkout includes all active guidance without an external
notes directory or hook; links resolve; planned and verified states are distinct.
Each newly Verified feature links to reproducible evidence for its defined scope.

## Implementation order

Finish the repository migration, assess pending v0.7 work, then start STR-01.
Use a small vertical slice: reference bucket/fuel scenarios, first content
definitions, shared bucket execution, save/reload and performance verification.
Proceed with STR-02/03 as their coverage permits. STR-04 and STR-05 follow actual
quality and maintainability needs; no blanket engine rewrite is authorized by
this plan. STR-06 evidence updates accompany each completed feature.

## Cargo workspace decision

Retain the current reusable library plus optional `client` feature. The
headless build already excludes window/GPU/audio/device dependencies, and
existing integration tests consume the same library. The focused modules make
ownership and responsibilities reviewable without introducing separate package
versions, manifests and build/test coordination now. A crate split does not
fix action ownership or duplicate gameplay rules; STR-03 addresses those directly.
Revisit a workspace when a second independent consumer or dependency/compile
measurement demonstrates a concrete benefit. This is the required evaluation,
not a performance claim or a deferred requirement to create more crates.
