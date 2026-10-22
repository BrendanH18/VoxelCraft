# Waterlogging and wall, fence and pane connections — 2026-10-20

[Roadmap](../roadmap.md#release-plan) · [Requirement inventory](../requirements.md) · [Slabs and stairs](2026-10-20-slabs-stairs.md) · [Doors and mounts](2026-10-20-doors-mounts.md) · [ID reservations](../architecture.md#release-id-reservations-v09v017)

Scope: BLD-04f (waterlogging across the waterloggable blocks VoxelCraft
has) and the BLD-04d connection rules it touches, on `t3/continue-build-brief`
after `5e9f025`. Reference: the Java Edition 26.3 server
(`server-26.3-classes.jar`), decompiled with Vineflower 1.11.1 under ignored
`target/`. Classes read:

- **Waterlogging:** `SimpleWaterloggedBlock` (`placeLiquid`,
  `pickupBlock`, `canPlaceLiquid`), `LiquidBlockContainer`, `BucketPickup`
  and `FlowingFluid` (`canPassThroughWall`, `getNewLiquid`, `spreadTo`);
- **Items and blocks that use it:** `BucketItem` (`use`, `emptyContents`),
  `BottleItem`, `DispensibleContainerItem`, `SpongeBlock`, `CoralPlantBlock`
  (`tryScheduleDieTick`, `scanForWater`, `tick`) and `SeaPickleBlock`;
- **Light, fire and explosions:** `ExplosionDamageCalculator`, `FireBlock`
  odds and `propagatesSkylightDown`;
- **Pistons:** `PistonBaseBlock.moveBlocks` and `PistonMovingBlockEntity`;
- **Connections:** `FenceBlock.connectsTo`, `WallBlock` (`connectsTo`,
  `updateShape`, `shouldRaisePost`, `makeShapes` and the test shapes),
  `IronBarsBlock.attachsTo` and `Block.isExceptionForConnection`.

Implementation started by a delegated agent and finished by the
integrator, who wrote the world tests below.

## Delivered behavior

- [`world/waterlog.rs`](../../src/world/waterlog.rs) owns the pairs for
  every waterloggable family VoxelCraft already had:
  - fences, iron bars and panes, walls, chains, trapdoors and rails;
  - ladders, chests and leaves;
  - amethyst buds and clusters, pointed dripstone, hanging roots and
    dripleaves, mangrove propagules and roots, and dead coral.

  Slabs, stairs and lanterns report their pairs from their own modules.
  Each new partner shares its original's name, textures, shape, mining and
  drops. Sea pickles and live coral were registered holding water; their new
  partners are their dry states.
- **Placement and edits:** any waterloggable state written into a water
  source is waterlogged, whether placed by a player, pushed or set by
  commands, as Java's `getStateForPlacement` and `updateShape` do. Breaking a
  block that holds water leaves the source. A piston lands a waterlogged block
  dry and leaves air behind (`PistonBaseBlock.moveBlocks`).
- **Buckets, bottles and dispensers:**
  - a water bucket fills a waterloggable block in place
    (`LiquidBlockContainer.placeLiquid`), and lava doesn't;
  - an empty bucket takes the water back (`pickupBlock`), and a block that
    can't survive dry then breaks;
  - a glass bottle fills from the water a block holds and leaves it;
  - dispensers do the same with buckets, and water evaporates in the Nether.
- **Flow:** a waterlogged block is a source that stays put. Water moves into
  or out of a waterloggable cell only through faces its collision shape
  leaves open (`canPassThroughWall`). Flowing water never waterlogs a block.
  Two sources beside one, over a solid or source floor, do, as Java's source
  conversion makes a new source there (off with `water_source_conversion`).
  Lava hardens only a water block, not the water a block holds.
- **Rendering and light:** the mesher draws a waterlogged block's water and
  then its model. Waterlogged states dampen light by 1, because a fluid stops
  `propagatesSkylightDown`. They never burn (`FireBlock` odds are 0).
  Explosions spare them, because water's blast resistance (100) protects the
  cell.
- **Sponges, coral and pickles:** a sponge takes a waterlogged block's
  water and continues through it. Kelp and seagrass go with their water.
  Dry live coral dies on a scheduled tick 60–99 ticks later unless water
  reaches it (`tryScheduleDieTick`). Dry sea pickles are dead: they don't
  glow or grow.
- **Connections (BLD-04d):**
  - Fences, walls, panes and bars join sturdy faces (full blocks, the
    backs of stairs), except leaves, pumpkins and melons
    (`isExceptionForConnection`).
  - Walls also join bars, panes and gates across their run, and fences no
    longer do.
  - Walls use Java's 26.3 shapes and read the block above. A side is tall
    where that block covers it.
  - `shouldRaisePost` decides the post: it drops in a straight run unless a
    wall post, a torch or a covering block over the middle holds it up, and
    it never stands with tall sides at both ends.
- One sturdy-face check (`mount_states::face_sturdy`) now serves supports,
  connections and `isCollisionShapeFullBlock` for monster spawning, replacing
  three copies with different edge cases.

## Performance

The waterlogged flag lives in `BlockInfo`, so meshing pays no extra load
for it. Headless `--bench --rd 8` of the merged v0.9 tree against the
pre-v0.9 baseline, four alternating runs each on the M5 MacBook Pro:

| Measure | Before | After |
| --- | --- | --- |
| Snapshot + mesh, 1 thread | 0.650–0.662 ms/chunk | 0.554–0.560 ms/chunk |
| Gameplay light initialisation | 49.6–51.5 ms | 33.2–33.6 ms |
| Generation, 1 thread | 0.452–0.459 ms/chunk | 0.451–0.452 ms/chunk |
| Stream rd 8 on 9 workers | 0.49–0.50 s | 0.47–0.48 s |

## Checks

- `cargo fmt`, `cargo clippy --release --all-targets -- -D warnings` and
  `cargo test --release` pass on the tested commit
  `abbda8fa5c4e3abeb418746d3e08ea4f64c07498` (1,422 tests), as do the headless
  (`--no-default-features`) clippy and tests (1,330).
- [`waterlog/world_tests.rs`](../../src/world/waterlog/world_tests.rs) covers:
  - blocks placed into water holding it and leaving it when broken;
  - buckets filling and draining, with lava doing neither;
  - flow through open faces only, per shape;
  - waterlogged stairs spilling only through their front;
  - two sources waterlogging a block where one doesn't, and the game rule;
  - explosions sparing wet blocks;
  - dry live coral dying unless water reaches it;
  - partner properties: fire odds and light differ, nothing else;
  - fence and wall connections to sturdy faces, leaves excluded, and the
    wall post and tall sides for open, fence-capped and roofed walls.
- PR #40 review fixes, each with a world test:
  - a wet rail keeps its water through shape changes;
  - flowing water and lava leave dry waterloggable plants alone, and the
    drop search doesn't route through them (`canHoldSpecificFluid`), so
    water beside them still spreads the other ways;
  - a pending coral death survives save and reload.

  A bucket emptied into an already waterlogged block changes nothing and
  still empties, as in Java (`canPlaceLiquid` ignores the waterlogged
  state); a test pins that down.
- The module's own tests cover the pair layout, symmetry and registration.
  Existing tests were updated where the new rules are Java's:
  - water poured into a rail waterlogs it instead of breaking it;
  - a sponge leaves dry live coral rather than dead coral.
- A windowed capture of waterlogged stairs, slabs, a lantern, a fence, iron
  bars and a wall in a stone trench was reviewed: water fills the trench
  from them, and the fence and bars join the trench walls.

## Remaining

- Waterloggable blocks VoxelCraft doesn't have yet (signs, campfires,
  candles, scaffolding, lightning rods, copper grates, sculk blocks and
  others) take their pairs when they arrive.
- Wall posts don't propagate down a stack of walls: Java stores each
  wall's `UP` and a lower wall copies an upper wall's post
  (`shouldRaisePost`), while VoxelCraft derives wall shapes from the cell
  above only. Matching it needs stored wall-post states (new ids), an owner
  scope decision. Resolved 2026-10-21 by
  [stored wall posts](2026-10-21-wall-posts-powered-doors.md).
- Not played by hand: bucket and bottle use with a mouse and a physical
  controller, and swimming through waterlogged blocks.
