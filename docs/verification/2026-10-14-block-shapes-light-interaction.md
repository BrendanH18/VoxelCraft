# Block shapes for light and interaction — 2026-10-14

[Roadmap](../roadmap.md) · [Lighting and spawning](2026-10-13-lighting-spawning.md)

Scope: ENV-05 (shape-aware light) and PLY-02 (targeting against outline
shapes), on `t3/v0-8-release` after `4fea268`. Reference: Java Edition 26.3
server (SHA-1 `33680f5f2ac32864d6d7cf5e56a705fdb3e05f4c`) `LightEngine`
(`shapeOccludes`, `getOcclusionShape`), `BlockBehaviour.useShapeForLightOcclusion`,
`getLightBlock`, `Shapes.faceShapeOccludes` and the per-block `getShape`
outlines, decompiled locally under ignored `target/` for reading only. Tests
and captures ran on the M5 MacBook Pro.

## Delivered behavior

- [`world/light_shape.rs`](../../src/world/light_shape.rs) builds a once-per-
  state table of 16×16 face masks for dry partial blocks (slabs, stairs, snow
  layers, farmland, dirt paths and the matching Nether forms). Light crosses a
  face unless the union of the two blocks' facing masks covers it, as in
  `Shapes.faceShapeOccludes`. These blocks now have zero light dampening;
  eight snow layers stay fully opaque. Each propagation edge costs two byte
  lookups and four word comparisons.
- Heightmaps use the same rule: a bottom slab's own cell sees the sky, and the
  cell below it does not.
- Meshes are lit from the authoritative gameplay light arrays. Mesh workers
  copy a snapshot with a one-cell margin and no longer flood their own,
  divergent light. Light passes bump affected mesh versions, so stale
  snapshots are discarded, and synchronous edit meshes wait for light to settle.
- Targeting uses outline shapes (`getShape`) on every input path (desktop,
  controller and CLI agent share `World::raycast`): fence arms, gates (lowered
  in walls), ladders, cross plants (grass, ferns, flowers, saplings, mushrooms),
  torches and low cube forms. A ray through a sapling's empty corner reaches
  the block behind it.

## Evidence

- Unit tests: slabs, snow and soil cover only their boundary faces; stair
  faces rotate and two opposite stairs union to a full face; side light passes
  a bottom slab but not its bottom; rotating stairs relights; a bottom shape's
  own sky cell is a source but the cell below is not; meshes consume supplied
  levels without reflooding; light passes invalidate mesh versions and
  synchronous edits wait for light.
- `tests/lighting_persistence.rs`: two complete chunks of sky/block arrays,
  including slabs, stairs, soil and snow, are identical after save and restart.
- `tests/shared_placement.rs`: desktop and CLI targeting agree through a
  sapling's empty corners.
- Full `cargo test --release` passed.
- `--bench --rd 8` (M5): single-thread mesh time fell from 0.947 to 0.647 ms per
  chunk (gameplay light initialisation for the same 120 chunks: 53.6 ms, work
  the game already did); streaming meshed 2840 chunks in 0.55 s, up from 2364.
- Screenshots of a forest (seed 12345) and a torch-lit stone room at
  midnight are visually identical to `t3/v0-8-release` at `d036b49`.

## Remaining differences and open work

- Stair light shapes use the straight shape; inner/outer corner stairs are
  computed from neighbours in VoxelCraft and don't yet change light masks.
- Flowers have no Java XZ render/outline offset (`OffsetType.XZ`); the outline
  is centred to match the rendered model.
- Waterlogged partial states don't exist (M2).
- Top and double slab states don't exist yet, so `SlabBlock.getStateForPlacement`
  (clicked half, stacking into a double slab) can't be matched; they need new
  state IDs and belong with M2 block states.
- Ordinary-player acceptance and an M5 frame-time scene remain. ENV-05 and
  PLY-02 stay **Partial**.
