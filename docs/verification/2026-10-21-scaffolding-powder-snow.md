# Scaffolding and powder snow — 2026-10-21

[Roadmap](../roadmap.md#release-plan) · [Requirement inventory](../requirements.md) · [ID allocations](../architecture.md#release-id-reservations-v09v017) · [Waterlogging](2026-10-20-waterlogging.md)

Scope: PLY-02c / BLD, on detached base `d17fb37` plus the integrator's
uncommitted Java player-movement port, in the assigned `new-blocks` worktree.
The port was retained. The integrator owns roadmap/requirement edits; this
record supplies evidence and the proposed row for integration. No commits,
branches, resets, stashes or other worktree changes were made.

Reference: Minecraft Java Edition **26.3**, local
`server-26.3-classes.jar` and `client/client-26.3.jar`, with Mojang class
names, decompiled using Vineflower. Classes read: `ScaffoldingBlock`,
`ScaffoldingBlockItem`, `PowderSnowBlock`, `SolidBucketItem`, `Entity`,
`LivingEntity`, `Player`, `EntityCollisionContext`, `CollisionContext`,
`InsideBlockEffectType`, `Block`, `Blocks`, `BlockBehaviour` /
`BlockStateBase`, `BucketItem`, `FallingBlock`, and `FireBlock`.
Data read: `recipe/scaffolding.json`, `loot_table/blocks/powder_snow.json`,
`tags/block/mineable/shovel.json`, `tags/block/snow.json`,
`tags/item/freeze_immune_wearables.json`,
`tags/entity_type/powder_snow_walkable_mobs.json`, and
`tags/entity_type/freeze_immune_entity_types.json`. Fuel uses the checked-in
26.3 `fuel_integrations.json` fixture (scaffolding: bamboo provider, 50 ticks).
No Mojang assets were copied.

## Delivered behavior

- **Scaffolding states:** distance 0–7, bottom and waterlogged. Distance is
  inherited from scaffolding below, zero over a sturdy top face, or the
  minimum horizontal neighbour distance plus one. Bottom means distance
  greater than zero with no scaffolding below.
- **Placement:** all adapters reach shared `PlayerSession::place_held`.
  Holding scaffolding uses a full-cell target shape. Normal side clicks
  extend upward; top clicks extend along the player's horizontal look.
  Sneaking follows the clicked face (reversed for an inside hit, whose face
  is opposite the dominant look axis, ties to Y then Z as in
  `Direction.getApproximateNearest`). Horizontal searches stop after seven existing scaffolds; vertical searches stop at
  the build limit. Direct unsupported placement fails, but the item can
  extend a supported run to a distance-seven falling block.
- **Stability:** placement and neighbour changes schedule a one-tick update
  in the existing persisted block-tick queue. A formerly stable block whose
  new distance is seven breaks with its drop. One already at distance seven
  becomes a falling block. Updates propagate through columns and outward
  runs. Landing uses the existing falling-block subsystem and checks that
  scaffolding can survive. Pending updates and falling blocks survive
  save/reload and chunk suspension through the persisted queue; loading a
  chunk does not re-tick its scaffolding (Java doesn't). A tick whose lower
  or horizontal neighbour is unloaded waits a tick rather than collapsing a
  build at the edge of the loaded area.
- **Pistons:** `PushReaction.POPPED`: a piston destroys scaffolding with its
  drop, and the scaffolding it supported then collapses.
- **Collision and support:** Java's top plate and corner posts form the
  stable outline; bottom states also have the 2/16 bottom plate. A player's
  original feet height, sneak/descending flag, fall distance and leather
  boots determine collision, rather than the swept box's lower bound.
  Above and not descending gives the stable collision shape; otherwise an
  unstable bottom rim can still collide, with an empty shape elsewhere.
  Placement collision is empty. Scaffolding is climbable; sneaking descends
  instead of applying the ladder hold. Its top supports mounted blocks.
- **Scaffolding integrations:** six bamboo plus string in the Java 3×3
  pattern crafts six. It breaks instantly, drops/picks as scaffolding, burns
  for 50 ticks, and has fire odds 60/60 when dry and 0/0 when waterlogged.
  Shared waterlog pairs preserve distance and bottom; buckets fill/drain it
  and breaking wet scaffolding leaves its water source.
- **Powder snow:** a stack-size-one solid bucket places through shared
  bucket execution and returns an empty bucket in Survival. An empty bucket
  picks up the whole block. Dispensers place it only into air and share
  bucket pickup. Powder snow persists as a block state; pick-block gives
  the bucket.
- **Powder collision and movement:** fall distance strictly greater than
  2.5 gives a 0.9-high collision shape, including while descending. Leather
  boots give a full block only when above and not descending. Otherwise
  the player sinks. Feet inside set the exact float-widened stuck vector
  `(0.9f, 1.5, 0.9f)` and reset fall distance; body contact freezes even
  when the feet are outside. Powder landings do no fall damage. The
  `wasInPowderSnow && canEntityWalkOnPowderSnow` climbing branch lifts a
  jumping/horizontally blocked leather-booted player at 0.2 blocks/tick.
- **Freezing:** +1 frozen tick per contact tick, capped at 140; −2 outside
  or with immunity. Fully frozen players take one point on entity ticks
  divisible by 40 (first damage is tick 160 for entry on tick 1). Any worn
  leather armour piece prevents freezing; only the feet slot's leather
  boots allow walking. Spectators are immune and creative players take no
  damage. Freeze damage bypasses armour points; Protection, Resistance
  and the `freeze_damage` game rule apply. Frost is Java's `ADD_VALUE`
  modifier `-0.05F * percentFrozen` on the 0.1 base speed, before the
  sprint/Speed/Slowness multipliers, and only while the block 0.2 below
  the feet is not air (`tryAddFrost`).
  Powder extinguishes players; burning players destroy contacted powder.
  Frozen ticks are additive saved player/profile fields and sent in LAN
  state. Old saves default to zero.
- **Presentation, sound and light:** original procedural bamboo lattice,
  powder grains, blue frozen health hearts (absorption hearts stay gold, as
  in `Gui.renderHearts`) and a frost screen edge whose opacity follows
  `percentFrozen`. Scaffolding uses the shared wood sound material and
  powder snow the snow material, including bucket fill/empty. Shared views have
  powder-snow fog when the camera is inside. The block has no ordinary
  occlusion or suffocation, attenuates light by one, and culls adjacent
  powder faces.
- **Mining and loot:** hardness 0.25. Unlike ordinary snow, powder snow is
  absent from Java 26.3's shovel mining tag: a shovel gets no speed bonus.
  Its loot table is empty, including with Silk Touch. Acquisition is by
  bucket, with generation and cauldrons outside this task.

## IDs

| Kind | Allocation |
| --- | --- |
| Block states | 7936–7967: `7936 + distance + 8*bottom + 16*waterlogged`; default 7943 |
| Block state | 7968: powder snow; 7969–7999 unused |
| Items | 1070: scaffolding; 1071: powder snow bucket |
| Textures | Base highest layer L=1762; 1763 top, 1764 side, 1765 powder, 1766 frozen full heart, 1767 frozen half heart |
| Reserved unused textures | 1768–1770; another task owns L+9..L+12 |

`Item::from_block` uses reserved item 1070 for every scaffold state and 1071
for powder snow, preserving other extended-block item IDs. Existing saved
identifiers and enum orderings are retained.

## Review fixes in this pass

The earlier draft compiled and its tests passed. Review against the Java
sources changed:

- pistons now pop scaffolding (it was pushed as a normal block);
- the frost slowdown was a multiplier on the final speed; it is now Java's
  `ADD_VALUE` on the base, which differs while sprinting or under effects;
- scaffolding and powder snow had the default stone sounds;
- a chunk load no longer schedules a tick for every scaffold in it;
- the inside-hit face tie-break follows `Direction.getApproximateNearest`
  (ties go to Y, then Z).

## Performance

`cargo test --release --lib scaffolding_powder_snow_tick_cost -- --ignored
--nocapture` on an Apple M5 (release): 1,152 settled scaffolds cost 0.30 ms
over 100,000 block-tick steps, as settled blocks have no pending ticks and
nothing scans them; 10,000 player ticks climbing take 10.8 ms and 10,000
inside powder snow 31.7 ms (about 3 µs each). The headless example reports
0.43 ms/tick over 200 overworld ticks. No 1080p/256-block frame-time scene
was measured.

## Checks

Run on 2026-10-22 in the `new-blocks` worktree (uncommitted, on `d17fb37`
plus the movement port):

- `cargo fmt && cargo clippy --release --all-targets -- -D warnings &&
  cargo test --release` passes: 1,473 tests, 0 failed, 12 ignored.
- `cargo clippy --release --no-default-features --all-targets -- -D warnings`,
  `cargo test --release --no-default-features` (1,381 passed, 0 failed,
  11 ignored) and `cargo run --release --no-default-features --example
  headless` pass.
- [`world/scaffolding/tests.rs`](../../src/world/scaffolding/tests.rs),
  [`player/travel/powder_snow_tests.rs`](../../src/player/travel/powder_snow_tests.rs)
  and the freezing tests in `simulation/survival.rs` cover:
  - Java distance inheritance, horizontal limit, bottom property,
    placement direction/inside-hit/build limits, break cascade versus
    falling, a wet scaffold leaving its water when it falls, and falling
    blocks surviving save and landing;
  - conditional collision from above, inside and while sneaking, the
    bottom rim, powder's strict 2.5 threshold and exact widened 0.9f height;
  - whole player ticks standing on a column, sneaking down it, climbing out
    and walking into it from the side;
  - exact stuck displacement, the steady sink of `-0.08 * 0.98f * 1.5` per
    tick, reset fall distance, boots standing, sneaking into snow, boots
    climbing, and a long fall landing at 0.9 then sinking without damage;
  - every leather armour slot, boots-slot checks, the 140-tick cap, the
    40-tick damage phase (ticks 160 and 200), thawing by 2, frost speed with
    sprinting, extinguishing, fall cushioning, and freezing through the
    shared `tick_player` with and without boots;
  - recipe, fuel, fire odds, light, mining speeds, empty Silk Touch loot,
    pick-block, waterlogging, shared-session placement, bucket exchange,
    dispensers, piston popping, burning players melting powder, on-disk
    chunk/item/frozen-tick persistence and pending-tick unload/reload.
- Two windowed captures (`--place` raw state IDs, then one second of ticks)
  were reviewed: a three-high column, a supported run and powder snow on a
  stone platform; and a run overhanging the platform edge, where only the
  unsupported cells gained the bottom plate after their scheduled ticks.

## Remaining and verification limits

- **Out of scope:** snowy-slopes powder-snow generation (v0.11 terrain) and
  cauldron interactions (v0.10).
- Player tests are derived from the local Java source and data; no live
  Java/Rust trace capture has been compared for this task.
- The minimal collision context is explicitly the player path. Existing mob
  physics retains its previous empty-context API. Java's walkable mob tag
  (rabbit, endermite, silverfish, fox), mob freezing / freeze-immune entity
  tags, and mob powder-snow AI need entity-system integration. Player armour
  immunity and spectator/creative rules are implemented.
- Falling scaffolding shares the existing engine's falling-block motion,
  rather than a new Java `FallingBlockEntity` trajectory/collision port.
- Java's dedicated sound events (scaffolding and powder-snow
  step/break/place, bucket fill/empty, freezing hurt) map to the nearest
  shared materials. Snowflake particles from moving in powder snow and the
  powder-snow fall sounds (`fallOn` at 4 and 7 blocks) are not implemented.
- The frost overlay, frozen hearts and bamboo/powder textures are original
  procedural art, not Java's artwork.
- Item entities, mobs and other non-player collisions use the empty
  context: they always meet the stable scaffolding shape and sink through
  powder snow. Java gives each entity its own context, so items inside a
  scaffold fall through it and walkable mobs stand on powder snow.
- Inside-block effects use the player's final box, not Java's swept
  movement path (`checkInsideBlocks`), as for cobwebs in the movement port.
- The full-cube target while holding scaffolding applies to placement and
  mining; the drawn selection outline still uses the ordinary outline.
- Full ordinary desktop/controller/LAN/CLI-agent play and the M5
  1080p/256-block acceptance scenes remain necessary before marking the
  feature Verified. Input paths share the `PlayerSession` placement and
  bucket code; no per-adapter play was run.

## Integration — 2026-10-22

Merged into `t3/e2f55514` with the movement port, wall posts, powered doors
and honey. The integrator resolved the overlapping registrations (texture
layers 1763–1767 beside honey's 1771–1773, `Block::COUNT` 1775; both waterlog
pair tables; the creative list), kept honey's movement effects next to
powder snow's fire handling, moved the powder snow bucket's icon to the
slot after the honey bottle (both had taken the same one), and updated tests
for the merged APIs (`apply_effects_from_blocks` takes the motion,
`tick_player` takes the world mutably). The full chain, the
`--no-default-features` checks and the headless example pass on the merged
tree.

Codex review fixes (2026-10-22): a riding player refreshes powder snow
contact and its leather-armour freeze immunity every tick
(`Player::refresh_riding_contact`; `powder_snow_equipment` runs before the
mounted split); catching fire clears `ticks_frozen` (`igniteForTicks` calls
`clearFreeze`); and redstone wire, diodes and pressure plates now stand on
any sturdy top face as Java's `canSurvive` rules require (wire also on
hoppers, plates also on centre support), so scaffolding, top slabs and
upside-down stairs hold redstone. VoxelCraft has no `SupportType.RIGID`
query: diodes and plates use the full sturdy face, which is stricter for a
few shapes.
