# Stored wall posts and powered door placement — 2026-10-21

[Roadmap](../roadmap.md#release-plan) · [Requirement inventory](../requirements.md) · [Waterlogging](2026-10-20-waterlogging.md) · [Doors and mounts](2026-10-20-doors-mounts.md) · [ID reservations](../architecture.md#release-id-reservations-v09v017)

Scope: the remaining wall-post propagation gap in BLD-04d and placement
beside power in BLD-04c. Implementation worktree `wall-posts`, detached at
`d17fb37`, with uncommitted changes. The integrator owns roadmap and
requirement-row updates; neither file is changed in this worktree.
The owner approved stored wall-post IDs 8000–8127 on 2026-10-21.

Reference: Minecraft Java Edition **26.3**, local Mojang-named
`~/Code/voxelcraft-java-reference/26.3/server-26.3-classes.jar`.
`WallBlock` and `DoorBlock` were independently decompiled with the supplied
Vineflower into `26.3/wall-posts-scratch/out/net/minecraft/world/level/block/`.
Reference methods read:

- `WallBlock.getStateForPlacement`, `updateShape`, `topUpdate`, `sideUpdate`,
  `isConnected`, `connectsTo`, `updateSides`, `makeWallState`,
  `shouldRaisePost`, `makeShapes`, `getFluidState`, `propagatesSkylightDown`.
- `DoorBlock.getStateForPlacement` and `setPlacedBy`.
- The jar's `data/minecraft/tags/block/wall_post_override.json`: standing
  torches (including copper), tripwire, signs, banners, pressure plates and
  cactus flower. Copper torches and cactus flowers are not registered here.

## Delivered behavior

Each wall stores `UP`. An upper wall with `UP=true` raises the wall below,
including a straight wall with opposite tall sides. Otherwise Java's
corner/empty, opposite-tall-side, post-override tag and covered-centre
rules decide the post, in that order. Horizontal NONE/LOW/TALL sides
continue to derive from neighbouring blocks and the bottom collision
face of the block above. Geometry reads the stored post for collision,
outline and mesh generation. Wall light behavior is retained: dry opacity
0, wet opacity 1, without full-cell occlusion.

World edits calculate a newly placed wall's post and update affected
neighbours. A changed post walks down loaded wall cells until no further
state changes, including across chunk boundaries, without recursive
wall-update calls. The normal edit effects still invalidate geometry,
lighting and navigation and schedule water ticks. Waterlogging and
draining retain UP; changing UP retains water. Both post states have the
same material, mining, drop and pick-block item. Centre-support queries
include the centre of no-post wall arms, so torches and lanterns can stand
on straight walls and raise their posts.

Door placement samples the neighbour signal at the final lower **or upper**
cell. Any signal opens both halves immediately, before a simulation tick,
for oak, iron and all eleven other existing door families. The existing
saved redstone edge set records POWERED at placement; manually closing a
powered wooden door does not create a false rising edge on its first tick.
Hinges are retained. The `PlayerSession::place_held` path is shared by
desktop, controller, CLI agent and LAN; the legacy `World::place_iron_door`
helper receives the same initial power rule.

## Saved identifiers

Existing wall IDs and their existing waterlogged partners now mean
**UP=true**, matching Java's default wall state. No existing ID is
renumbered, reassigned to another material or removed. An older save has no
post bit to recover: old walls load with posts raised; a subsequent
neighbour update computes the current Java result. This can initially show
posts on an old straight run that previously derived no post. New saves
preserve the selected UP value exactly. The existing VXC3 state format
already supports these IDs; no save-format change is needed.

| IDs | Meaning |
| --- | --- |
| 8000–8039 | UP=false, dry then wet for each of the twenty `wall_states::FAMILIES` |
| 8040–8127 | Reserved for this task, unregistered |

Family order is the original IDs
`258, 264, 270, 276, 282, 288, 294, 300, 306, 312, 318, 324, 330, 331,
332, 822, 828, 834, 2365, 2371`, in that order. The twenty original
materials and their existing wet IDs are unchanged.

## Checks

Regression coverage added:

- `wall_states/tests.rs`: exact allocation/old-ID meaning, all twenty
  materials' properties/items/water/light, all sixteen low-arm connection
  masks, standing vs wall torches, plates, standing vs hanging lanterns,
  full roofs, fences and both upper-wall UP values; opposite tall arms
  with an upper raised post; dry/wet forty-wall stacks crossing chunk
  boundaries; cap removal and horizontal changes; file save/reload of
  both post values followed by further propagation.
- `tests/mount_placement.rs`: all twenty walls through real session
  raycast placement in dry/wet straight runs; all thirteen doors beside
  lower-half power, upper-half power and no power; immediate open state,
  later power removal/restoration and manual closure; right hinge and
  saved POWERED edge restoration; the legacy iron-door helper.
- `mesh.rs`: a torch-capped stack produces a lower post's actual mesh
  face for dry and wet walls; an uncapped stack does not; wet walls also
  emit water geometry.
- The earlier waterlogging shape test now explicitly obtains placement's
  stored UP before asserting geometry; its reference expectations are
  retained.

Check results (2026-10-21, `wall-posts` worktree): `cargo fmt && cargo
clippy --release --all-targets -- -D warnings && cargo test --release` passed
(1,077 library tests, 7 ignored, plus every integration test binary). The
ignored `wall_stack_update_timing` test measured 0.33 ms per cap edit that
walks a 40-wall stack (release build, 2,000 edits).

## Remaining

No new signs, banners, copper torches or cactus flowers are introduced;
their post-override cases remain unavailable until those blocks exist. Horizontal wall sides are derived,
not independent saved properties: arbitrary Java command-created states
whose sides disagree with their neighbours are not represented. This
retains the existing compact representation for ordinary play.

Physical mouse/controller play and the full 1080p/240 FPS acceptance gate
remain unverified. These slices do not promote either entire requirement
to Verified.
