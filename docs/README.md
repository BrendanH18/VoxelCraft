# VoxelCraft documentation

[Project README](../README.md) · [Contributing](../CONTRIBUTING.md)

## Goals, status and planned changes

- [Roadmap](roadmap.md): authoritative scope, dependencies and completion criteria.
- [Requirement inventory](requirements.md): every core requirement as its own row,
  with state, owning release, evidence and acceptance cases.
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
  Java attack-strength charging, reference reach and tool attributes, the
  crosshair attack indicator, and copper tools, armor and nuggets; spears and
  ordinary-player acceptance remain.
- [Bucket review fixes](verification/2026-10-13-review-bucket-fixes.md): shared
  use-priority ordering, Adventure denial and Java hit-face pouring.
- [Mining review fixes](verification/2026-10-13-review-mining-fixes.md): shared
  destroy delay, progress keys, door/plant drop rules, Creative egg/sword and Haste.
- [Gameplay skylight and cave spawning](verification/2026-10-13-lighting-spawning.md):
  propagated sky light, cave/torch/reference scenarios, clock/weather inputs and
  Overworld/End process-restart checks; remaining shape and spawn differences.
- [v0.7.0 play-test checklist](verification/2026-10-14-v0.7.0-playtest.md):
  the manual pass to run before tagging v0.7.0.
- [v0.7.0 defect inventory](verification/2026-10-13-v0.7-defect-inventory.md):
  the release checklist of every known defect and Java difference in existing
  features, with sizes and sources.
- [Shared right-click use chain](verification/2026-10-13-shared-use.md):
  one session use chain for desktop and CLI, outcome reporting, CLI and rider
  behavior changes and remaining Adventure-mode difference.
- [Lighting and spawning review fixes](verification/2026-10-13-review-lighting-fixes.md):
  spawn-point exclusion, thunder/rain thresholds, detectors, conductor start
  cells, incremental LAN replica relight with measured cost; End ambient gap.
- [World, lighting and structure fixes](verification/2026-10-13-world-fixes.md):
  growth light, igloo depth, entity/particle skylight and v0.7.0 follow-ups,
  each with its outstanding acceptance checks.
- [Amethyst geodes](verification/2026-10-13-amethyst-geodes.md):
  Java 26.3 geode feature parameters, wall bud states 2400–2415, layout/bud/
  chunk-order tests, benchmark and remaining differences.
- [Content review fixes](verification/2026-10-13-review-content-fixes.md):
  dark/pale oak 2x2 growth, complete reference fishing loot, sponge draining,
  armor reach and separate-process lock evidence; remaining differences.
- [M5 scene baseline](verification/2026-10-13-m5-baseline.md):
  first recording of all five initial scenes and its raw frame samples,
  corrected as invalid (paused, occluded-window and population-loss runs);
  it must be re-measured.
- [Benchmark harness review fixes](verification/2026-10-13-review-bench-fixes.md):
  pause/tick/presentation/population validity checks, input isolation, no saves,
  terrain-following cameras, telemetry and runner gate fixes; proposed roadmap text.
- [Mob ground navigation](verification/2026-10-14-mob-navigation.md) and
  [swimming, flying and climbing](verification/2026-10-14-mob-navigation-swim-fly.md):
  Java 26.3 evaluators, path search and recomputation, with benchmarks.
- [Strider lava paths](verification/2026-10-15-strider-lava-paths.md):
  `StriderPathNavigation`, `StriderGoToLavaGoal`, the half-block lava standing
  height and a generated-Nether scene.
- [Natural spawning](verification/2026-10-14-natural-spawning.md) and
  [spawn categories](verification/2026-10-15-spawn-categories.md): Java's
  `NaturalSpawner` ring, caps, despawning and biome spawn lists.
- [Block shapes, light and interaction](verification/2026-10-14-block-shapes-light-interaction.md):
  face-mask light occlusion, gameplay-light meshes and outline targeting.
- [Loaded-region activity and save integrity](verification/2026-10-14-simulation-activity-save-integrity.md):
  simulation distance tickets, sleeping work and saved delays.
- [M5 scene baseline, second run](verification/2026-10-15-m5-baseline.md):
  12 of 15 valid runs, uncapped presentation, streaming stalls, and the two
  harness defects it exposed.
- [M5 scene baseline](verification/2026-10-16-m5-baseline.md): the first
  valid reference baseline (15/15 runs); the gate fails in streaming, which
  stalls every 32 blocks.
- [Death, pause and mid-motion saves](verification/2026-10-15-death-pause-save-scenarios.md):
  the shared death and respawn step, the pause rule and cross-process piston
  and repeater saves.

## History

[Archived roadmaps and task briefs](archive/README.md) preserve earlier decisions,
allocations and review reports. They are superseded historical records, not
current instructions or a second roadmap.
