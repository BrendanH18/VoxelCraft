# Door hinges, floor and ceiling switches, grindstones and lanterns — 2026-10-20

[Roadmap](../roadmap.md#release-plan) · [Requirement inventory](../requirements.md) · [Slabs and stairs](2026-10-20-slabs-stairs.md) · [ID reservations](../architecture.md#release-id-reservations-v09v017)

Scope: BLD-04c (door hinges) and BLD-04e (floor and ceiling buttons and
levers, wall and ceiling grindstones, lanterns), on `t3/continue-build-brief`
after `5e9f025`. Reference: the Java Edition 26.3 server
(`server-26.3-classes.jar`), decompiled with Vineflower 1.11.1 under ignored
`target/`. Classes read: `DoorBlock` (`getHinge`, `getShape`,
`canSurvive`), `FaceAttachedHorizontalDirectionalBlock`
(`getStateForPlacement`, `canSurvive`, `getConnectedDirection`),
`ButtonBlock`, `LeverBlock`, `GrindstoneBlock`, `LanternBlock`
(`getStateForPlacement`, `canSurvive`, `updateShape`), `TorchBlock`,
`StandingAndWallBlockItem`, `Block.canSupportCenter`,
`BlockBehaviour.isFaceSturdy`, `Direction.orderedByNearest` and the
`Blocks` entries for the lanterns. Implementation started by a delegated
agent and finished by the integrator.

## Delivered behavior

- [`world/mount_states.rs`](../../src/world/mount_states.rs) owns the new
  states. Existing ids keep their meaning:
  - the old door ids are the left-hinge states;
  - switch mounts 0 and 5 are floor and ceiling facing south, and mounts 1–4
    are the wall states;
  - 933–936 are the floor grindstones.
- **Door hinges** (`DoorBlock.getHinge`): a door hinges away from a lower
  door half beside it, then away from the side with more full-collision
  blocks (counting the cells beside both halves), and otherwise by which
  half of the cell was clicked, measured in the placed cell. Right-hinge
  states exist for oak, iron and the 11 other wood doors. They mine, drop,
  open, close and take redstone like their left twins and keep their hinge
  when toggled. An open door swings to Java's side for its hinge, so two
  doors placed side by side open apart. Upper halves store no hinge and read
  the lower half's, as before.
- **Floor and ceiling buttons and levers**
  (`FaceAttachedHorizontalDirectionalBlock`) face the player's horizontal
  direction: mounts 6–8 are the floor and 9–11 the ceiling, facing north,
  east and west. Placement takes the first of the six directions, in
  `Direction.orderedByNearest` order, whose block has a sturdy face toward
  it. A switch pops off when that face goes. Stone, oak, crimson, warped,
  pale oak and bamboo buttons all have the new mounts.
- **Grindstones** take wall and ceiling states with Java's shapes. Because
  `GrindstoneBlock.canSurvive` is always true, the nearest look direction
  wins whatever is there.
- **Lanterns and soul lanterns** are new blocks. Each has standing, hanging
  and waterlogged states, emits light 15 or 10, has hardness 3.5, is mined
  best with a pickaxe and needs no tool to drop. Both have Java's shapes (one
  pixel higher when hanging) and the crafting recipe (a torch or soul torch
  inside eight iron nuggets). A lantern hangs or stands by the nearest
  vertical look direction whose block supports its centre, and is wet when
  placed in a water source. A hanging lantern breaks when the block above
  can no longer hold it (`updateShape`), leaving its water.
- Support follows Java for torches, soul torches, doors and attached
  switches. A standing torch needs `canSupportCenter` (fence posts and glass
  hold one). Doors and wall-attached blocks need a sturdy face (glass has
  one; a bottom slab's top doesn't). Beds use the same sturdy-top rule.

| Ids | States |
| --- | --- |
| 7680–7691 | Lever mounts 6–11, off then on |
| 7696–7767 | Button mounts 6–11 for stone, oak, crimson, warped, pale oak and bamboo (12 each) |
| 7800–7903 | Right-hinge lower doors: oak, iron, then the 11 wood doors in `forms` order (8 each) |
| 7904–7911 | Grindstones: wall, then ceiling, four facings each |
| 7920–7927 | Lantern, then soul lantern: standing dry, standing wet, hanging dry, hanging wet |

Other ids in 7680–8191 stay unregistered.

## Checks

- `cargo fmt`, `cargo clippy --release --all-targets -- -D warnings` and
  `cargo test --release` pass on the tested commit
  `abbda8fa5c4e3abeb418746d3e08ea4f64c07498` (1,422 tests), as do the headless
  (`--no-default-features`) clippy and tests (1,330).
- Unit tests in [`mount_states/tests.rs`](../../src/world/mount_states/tests.rs):
  - every new id is registered, and the old ones keep their meaning;
  - right-hinge doors share their twin's properties;
  - open right-hinge doors swing to Java's side, and double doors swing apart;
  - `getHinge` holds for neighbouring doors, solid-block balance and click
    halves;
  - mounts round-trip and support the right side;
  - `orderedByNearest` order holds;
  - sturdy and centre support follow the block shape;
  - lantern states, light, drops, shapes and water are right, and a hanging
    lantern breaks when its ceiling goes.
- [`tests/mount_placement.rs`](../../tests/mount_placement.rs) places through
  `PlayerSession::place_held`, the path shared by desktop, controller, LAN
  and the CLI agent. It checks that a second door beside the first hinges
  right and keeps the hinge when opened. It also checks floor buttons and
  ceiling levers facing the look direction, a lever mounting on a wall,
  lanterns hanging and standing, and item use.
- Windowed captures were reviewed for shape and facing:
  - floor lever and button facing east;
  - floor, wall and ceiling grindstones;
  - standing and hanging lanterns;
  - a ceiling lever;
  - a left-hinge and a right-hinge open door side by side.

## Remaining

- Lantern textures are procedural stand-ins, not Java's art. Lanterns are
  not yet placed by structures or sold by villagers.
- Not checked: a door placed beside a powered block opening at once, as
  `getStateForPlacement` sets `powered` and `open`. Done 2026-10-21 in
  [powered door placement](2026-10-21-wall-posts-powered-doors.md).
- Not yet played by hand with a mouse or a physical controller.
