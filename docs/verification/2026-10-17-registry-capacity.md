# Block-state registry capacity — 2026-10-17

[Roadmap](../roadmap.md) · [Requirement inventory](../requirements.md) · [Restructuring plan](../restructuring.md)

Scope: M0's "identify registry/state capacity limits before adding further
content", measured on `t3/roadmap-next-stage-v0-8` at `77a6885`. It is the
assessment M0 needs; the encoding change and the range reservations stay with
BLD-06 in v0.9.

## Measured

A block state is a `Block(u16)` below `STATE_CAPACITY` (4096). Unregistered
ids are named `unknown` in `block::INFO`. Counting registered ids with a
throwaway test:

| Measure | Value |
|---|---|
| Registered states (including air) | 1,833 |
| Free ids | 2,263 |
| Highest registered id | 2524 |
| Free runs of 64 or more | 835–899, 969–1099, 1801–1899, 2077–2149, 2416–2499, **2525–4095 (1,571)** |

Current states of the families v0.9 extends:

| Family | Kinds | States |
|---|---|---|
| Slabs | 39 | 39 (bottom only) |
| Stairs | 39 | 156 (four facings; no upside-down or waterlogged states) |
| Doors | 13 | 131 (no hinge) |
| Buttons | 6 | 72 (wall only) |
| Levers | 1 | 12 (wall only) |
| Trapdoors | 6 | 96 |
| Fence gates | 12 | 96 |
| Fences, panes, walls, bars, chains | 13, 17, 20, 1, 1 | 54 |
| Rails | 4 | 46 |

## v0.9 and v0.12 demand, if each Java state gets its own id

Rough counts from Java 26.3's properties (`SlabBlock` type and waterlogged,
`StairBlock` half and waterlogged, `DoorBlock` hinge,
`FaceAttachedHorizontalDirectionalBlock` face):

| Addition | Extra ids |
|---|---|
| Top and double slabs, waterlogged bottom and top | about 156 |
| Upside-down stairs, then waterlogged for both halves | about 468 |
| Door hinges | about 131 |
| Floor and ceiling buttons and levers | about 84 |
| Waterlogged fences, panes, walls, bars, chains, trapdoors, rails, leaves, lanterns | about 210 |
| Other v0.9 states (wall grindstones, ladders, signs and so on) | about 100 |
| **v0.9 total** | **about 1,150 of 2,263 free** |

v0.12 then adds whole families: three wood sets (pale oak, bamboo and
another), each with stairs, slabs, fences, gates, doors, trapdoors, buttons
and signs in every waterlogged/hinged/mounted form (about 120 ids each before
sign rotations); stone families (mud brick, prismarine variants); 16
banner colours with 16 standing rotations and 4 wall facings (about 320);
standing signs with 16 rotations per wood; coral, sea pickles, cauldrons,
campfires and dripstone states. That is well over the roughly 1,100 ids left
after v0.9.

## Finding

- **The 4096-state space holds v0.9 but not v0.12.** Waterlogging is the
  largest single consumer: as separate ids it roughly doubles every
  waterloggable family.
- Raising `STATE_CAPACITY` is bounded by item encoding: block items use
  `BLOCK_ITEM_BASE` (0x8000) + id within a `u16`, so the ceiling is 32,768
  states without changing `Item`. The per-state static tables (`INFO`,
  `EMISSION`) and the chunk palette's stack `lookup` array
  (`src/world/chunk.rs`) grow linearly with it; 16,384 would make that lookup
  32 KB per palette build.
- Options for BLD-06 in v0.9, before any new state is added: (a) raise
  `STATE_CAPACITY` (8,192 or 16,384) and keep one id per state; (b) store
  waterlogging (and possibly other boolean properties) outside the id, as a
  per-chunk layer, so the id space grows only with real shapes; (c) both.
  Each option needs a save-version bump with old saves loading unchanged.
- Whichever option is chosen, v0.9 starts by reserving id ranges for
  v0.9–v0.19 in this record's successor, so concurrent content work never
  collides.

No code changed for this assessment.
