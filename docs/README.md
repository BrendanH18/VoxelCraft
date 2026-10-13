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

## History

[Archived roadmaps and task briefs](archive/README.md) preserve earlier decisions,
allocations and review reports. They are superseded historical records, not
current instructions or a second roadmap.
