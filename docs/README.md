# VoxelCraft documentation

[Project README](../README.md) · [Contributing](../CONTRIBUTING.md)

## Goals, status and planned changes

- [Roadmap](roadmap.md): authoritative scope, dependencies and completion criteria.
- [Restructuring plan](restructuring.md): planned verification, content, gameplay,
  asset and module changes, including how to retain pending v0.7 work.

Neither a planned task nor a historical release report establishes verified parity.

## Playing

- [Gameplay](gameplay.md) and [survival items](survival-items.md).
- [Colours](colors.md), [bastions](bastions.md) and [Nether biomes](nether-biomes.md).
- [Overworld terrain](overworld-terrain.md), [structures](overworld-structures.md)
  and [aquatic mobs](aquatic-mobs.md).
- [Mob parity](mob-parity.md), [Nether mobs](nether-mobs.md) and [redstone](redstone.md).
- [Player rendering](player-rendering.md), [music](music.md) and [particles](particles.md).

Feature guides describe their documented implementation and limitations. Use the
roadmap and recorded checks to assess completeness against the current target.

## Engineering and delivery

- [Agent instructions](../AGENTS.md) and [engineering guide](agent-guide.md).
- [Architecture](architecture.md), [development tools](development.md) and
  [hosted agents/commands](agents.md).
- [Release guide](releases.md).
- [Dated performance investigation](performance-2026-10-07.md).
- [Initial v0.7 assessment and first correctness fixes](verification/2026-10-12-v0.7-assessment.md):
  tested commits, reference failures, regression/save checks and remaining acceptance work.
- [Content and original asset integrations](verification/2026-10-12-content-and-assets.md):
  reference-backed building/plant fixes and embedded resource/restart evidence.
- [Turtle equipment and dual-effect potions](verification/2026-10-12-turtle-equipment.md):
  helmet, brewing, splash geometry and save compatibility; remaining player acceptance.
- [Complete fuel inventory](verification/2026-10-12-fuel-inventory.md):
  implemented fuel integrations and the explicit missing-registration inventory.
- [Shared player sessions and consumption](verification/2026-10-13-player-session.md):
  actor ownership, reference timing, interruption/death and persistence checks;
  remaining shared-action extraction and player acceptance.
- [Shared mining transactions](verification/2026-10-13-shared-mining.md):
  reference reach, progress, fatigue and ice support; input interruption,
  partner removal and process-restart evidence; remaining acceptance.
- [Shared block placement](verification/2026-10-13-shared-placement.md):
  actor-owned validation, paired blocks, low-cube targeting and process-restart
  checks; remaining shape/state and ordinary-player acceptance.
- [Shared melee](verification/2026-10-13-shared-combat.md): actor-owned attacks,
  Java attack-strength charging, reference reach and tool attributes; remaining
  weapon families, attack indicator and ordinary-player acceptance.
- [Bucket review fixes](verification/2026-10-13-review-bucket-fixes.md): shared
  use-priority ordering, Adventure denial and Java hit-face pouring.
- [Mining review fixes](verification/2026-10-13-review-mining-fixes.md): shared
  destroy delay, progress keys, door/plant drop rules, Creative egg/sword and Haste.
- [Gameplay skylight and cave spawning](verification/2026-10-13-lighting-spawning.md):
  propagated sky light, cave/torch/reference scenarios, clock/weather inputs and
  Overworld/End process-restart checks; remaining shape and spawn differences.
- [Amethyst geodes](verification/2026-10-13-amethyst-geodes.md):
  Java 26.3 geode feature parameters, wall bud states 2400–2415, layout/bud/
  chunk-order tests, benchmark and remaining differences.
- [M5 scene baseline](verification/2026-10-13-m5-baseline.md):
  first recording of all five initial scenes and its raw frame samples,
  corrected as invalid (paused, occluded-window and population-loss runs);
  it must be re-measured.
- [Benchmark harness review fixes](verification/2026-10-13-review-bench-fixes.md):
  pause/tick/presentation/population validity checks, input isolation, no saves,
  terrain-following cameras, telemetry and runner gate fixes; proposed roadmap text.

## History

[Archived roadmaps and task briefs](archive/README.md) preserve earlier decisions,
allocations and review reports. They are superseded historical records, not
current instructions or a second roadmap.
