# Slab types and stairs halves and corners — 2026-10-20

[Roadmap](../roadmap.md#release-plan) · [Requirement inventory](../requirements.md) · [Capacity raise](2026-10-20-state-capacity.md) · [Earlier block shapes](2026-10-14-block-shapes-light-interaction.md) · [ID reservations](../architecture.md#release-id-reservations-v09v017)

Scope: BLD-04a (top, bottom, double and waterlogged slab states) and BLD-04b
(upside-down stairs and corner shapes), on `t3/continue-build-brief` after
`5e9f025` and the capacity raise. Reference: the Java Edition 26.3 server
(`server-26.3-classes.jar`), decompiled with Vineflower 1.11.1 under ignored
`target/`. Classes read: `SlabBlock` (`getStateForPlacement`,
`canBeReplaced`, `getShape`, `useShapeForLightOcclusion`, the slab loot
table's `type=double` count), `StairBlock` (`getStateForPlacement`,
`getStairsShape`, `canTakeShape`, `updateShape`, the `SHAPE_OUTER` /
`SHAPE_INNER` shapes and `useShapeForLightOcclusion`), `StairsShape`,
`SlabType`, `BlockPlaceContext` (`replacingClickedOnBlock`,
`getClickedPos`), `PistonMovingBlockEntity` and
`Block.updateFromNeighbourShapes`. Implementation started by a delegated
agent and finished by the integrator.

## Delivered behavior

- [`world/slab_stair_states.rs`](../../src/world/slab_stair_states.rs) gives
  each of the 39 stairs and slab families every Java state. Existing ids
  keep their meaning: the four facings of bottom, straight, dry stairs and
  the bottom, dry slab. The new states take ids 4096–7215 (table below);
  7216–7679 stay unregistered.
- A per-state table decodes family, facing, half, corner shape and water,
  so shape, light and placement code never decodes forms per call. Every new
  state shares its family's name, textures, mining, sounds, fire odds and
  item: it drops and is picked as the existing slab or stairs. A double slab
  drops two (silk touch too).
- **Slabs** (`SlabBlock.getStateForPlacement`): bottom from a top face, top
  from an underside, and from a side by whether the click is above the
  cell's middle. A matching single slab takes the second half and becomes the
  double slab. That happens when it is clicked on its open side
  (`canBeReplaced` with `replacingClickedOnBlock`), or when it fills the
  target cell of a click on a neighbouring block. Other slab kinds place
  beside it instead. The double slab is a full opaque cube and never
  waterlogged. Placement checks the placed state's collision boxes against
  players (`isUnobstructed`), so a player standing on a bottom slab blocks
  only its upper half.
- **Stairs** (`StairBlock`): facing from the player's horizontal direction,
  half from the clicked face and height, and corner shape from the
  neighbours (`getStairsShape`, `canTakeShape`) at placement. Any edit to a
  cell beside stairs (breaking, explosions, pistons, commands) reshapes them
  (`updateShape`). Stairs a piston pushes take their shape where they land
  (`updateFromNeighbourShapes`). Only a change in a neighbour's stairs
  facing or half triggers the lookups.
- Shapes for rendering, collision and the crosshair outline are Java's slab
  and quadrant step boxes for every facing, half and corner. Light occlusion
  uses the same shapes (`useShapeForLightOcclusion`) for every stairs state
  and single slab; double slabs block light like a full block.
- Top slabs and upside-down stairs have a sturdy top: fire burns on them
  and monsters can spawn on them. Bottom slabs still can't hold spawns.

| Ids | States |
| --- | --- |
| 4096–7059 | Stairs: 76 per family in `FAMILIES` order. `local = ((top × 5 + shape) × 2 + wet) × 4 + facing` runs over 0..80; locals 0–3 are the existing ids, and local `n` ≥ 4 is `4096 + family × 76 + n − 4` |
| 7060–7215 | Slabs: four per family (bottom wet, top dry, top wet, double) |
| 7216–7679 | Reserved for v0.9, unregistered |

## Performance

Two hot-path costs found while measuring were fixed in the same change:

- `Block::uses_shape_for_light_occlusion` decoded forms on every call, and
  heightmaps ask it for every transparent cell, air included. It is now a
  static per-state table.
- `Block::is_waterlogged` read a separate table on every mesh face. That
  table was all false before this change, so the compiler had folded the
  read away. The flag now lives in `BlockInfo`, which meshing already loads.

The light shape table interns shapes before computing face masks, since
about 3,100 new states share a few dozen shapes.

Headless `--bench --rd 8`, alternating the pre-change tree (capacity raise
only) and this change four times each on the M5 MacBook Pro:

| Measure | Before | After |
| --- | --- | --- |
| Gameplay light initialisation | 50.7–51.7 ms | 33.2–35.3 ms |
| Snapshot + mesh, 1 thread | 0.660–0.674 ms/chunk | 0.570–0.589 ms/chunk |
| Generation, 1 thread | 0.454–0.474 ms/chunk | 0.453–0.475 ms/chunk |
| Stream rd 8 on 9 workers | 0.52–0.54 s | 0.53–0.55 s |

## Checks

- `cargo fmt`, `cargo clippy --release --all-targets -- -D warnings` and
  `cargo test --release` pass (1,394 tests), as do the headless
  (`--no-default-features`) clippy and tests (1,302).
- Unit tests: every state decodes and round-trips; existing ids keep their
  meaning; `getStairsShape` gives the expected shape for every facing and
  half, with neighbours behind and in front turned either way, parallel
  neighbours, the other half and `canTakeShape` blockers. Every stairs
  state's shape matches Java's quadrants voxel by voxel, and collision and
  outline match it. Light faces are tested per state. Every new state shares
  its family's properties, and the water pairs cover every state except
  double slabs.
- [`tests/slab_stair_placement.rs`](../../tests/slab_stair_placement.rs)
  places through `PlayerSession::place_held`, the path shared by desktop,
  controller, LAN and the CLI agent. It covers slab halves by face and click
  height; merging from the open side and into a neighbour's target cell;
  other kinds not merging; and a double slab refused inside a player, with
  the desktop and CLI paths agreeing. It also covers stairs facing, halves
  and corners at placement; reshaping after breaking and after an explosion;
  drops; save and reload of every new state; and the creative palette
  listing each item once. A piston test covers pushed stairs.
- Windowed captures of every stairs shape, both halves, and bottom, top and
  double slabs on a test platform (`--place` with raw ids) were reviewed for
  shape and texture placement.

## Remaining

- Waterlogged states report their pairs through
  `slab_stair_states::water_pair`; placing into water, buckets and flow
  through their shapes come from the
  [waterlogging work](2026-10-20-waterlogging.md).
- Structure generation is unchanged: layouts still place the existing
  stairs states, while Java's templates can hold corners and upside-down
  stairs.
- Not yet played by hand: placement feel with a mouse and a physical
  controller, and the windowed scene suite with the new states.
