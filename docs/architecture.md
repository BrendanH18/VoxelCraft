# Engine architecture

[← Back to VoxelCraft](../README.md) · [Development tools](development.md)

VoxelCraft builds its world, meshes, textures and sounds in Rust. This guide
covers the rendering and simulation choices behind the game.

## Performance design

| Technique | Why |
|---|---|
| 32³ chunks; uniform, byte-ID and palette storage | Uniform chunks store one u16; ordinary dense chunks stay 32 KiB, and chunks with up to 256 extended states use 32 KiB + a 512-byte palette |
| Bounded immutable surface-column cache | Reuses height and biome noise across vertical chunks; at most 512 columns (about 4 MiB plus cache headers), with noise calculated outside the worker lock |
| State-indexed light emission | A single byte lookup avoids orientation decoding in each lighting cell |
| Greedy meshing | Merges coplanar faces with identical texture/AO/light into one quad |
| Detail quads for shaped blocks | Stairs, fences and doors use the same 12-byte quad record, flagged to cover part of a cell in 1/16 steps, so they share the chunk passes and face culling |
| Face-direction culling | Quads are grouped by facing per chunk; groups facing away from the camera are skipped (~45% fewer quads drawn) |
| 12-byte quad records with vertex pulling | The vertex shader expands each quad from a storage buffer; ~9x smaller than Minecraft's 4 vertices x 28 bytes |
| Pooled quad arena | All chunk meshes share a few large GPU buffers (best-fit free list), so drawing needs no per-chunk buffer binds |
| One shared quad index buffer | No per-chunk index data |
| Render lighting computed inside mesh jobs over a 15-block margin | Exact at chunk borders with zero shared mutable light state, so every job runs in parallel |
| Incremental gameplay block light, packed into nibbles only in lit chunks | Updates without render jobs; source removal and streaming relight from surviving sources |
| Worker thread pool with nearest-first scheduling and chunk versioning | Generation and meshing never block the render thread; stale results are dropped |
| Copy-on-write `Arc` chunk data | Mesh jobs snapshot neighbours without locks or copies |
| Frustum culling, front-to-back opaque and back-to-front translucent sorting | Less overdraw; correct water blending |
| Camera-relative rendering, reverse-Z infinite projection | Stable precision far from the origin and at long view distances |
| Synchronous remesh of edited chunks only | Block edits appear the same frame; lighting ripples update on workers |
| Event-driven water simulation, batched per tick | Untouched oceans cost nothing; a flood tick takes ~0.5 ms and remeshes on workers |

### Recorded benchmarks (Apple M5, release build)

These are measurements recorded during development, not guarantees for other
hardware or scenes. Render timings use a 1600 × 900 viewport and synchronize
the GPU after every frame; they include CPU and GPU work.

```text
$ voxelcraft --bench --rd 8
generate (1 thread): 0.205 ms/chunk
light+mesh (1 thread): 0.655 ms per dense chunk, 149146 quads (27117 bytes/chunk GPU)
stream rd=8 on 9 workers: 2344 chunks loaded, 1576 meshed in 0.17 s

$ voxelcraft --bench-render --rd 8     # 1600x900, GPU-synchronised each frame
avg 1.17 ms (~855 fps) — 813 draw calls, 0.29M quads drawn

$ voxelcraft --bench-render --rd 16    # 512-block view distance
avg 2.40 ms (~417 fps) — 2652 draw calls, 0.76M quads drawn, 57 MB of quad data
```

The headless figures above were remeasured on 2026-10-07 (seed 12345, seven-run medians).
See [the regression investigation](performance-2026-10-07.md) for commit attribution,
phase timings and output verification. The render figures were recorded separately.

The render sweep was re-run on 2026-10-26 at `05831cd` (M5 on battery, three runs
each). It now draws more: 0.54M quads at `--rd 8` and 1.60M at `--rd 16`. Averages
were 2.36, 3.17 and 2.73 ms at `--rd 8` (median ~366 fps) and 7.54, 7.53 and
6.87 ms at `--rd 16` (median ~133 fps). The README quotes these medians.

Render distance is measured in 32-block chunks, so `--rd 8` is 256 blocks
(Minecraft's 16) and `--rd 16` is 512 blocks (Minecraft's 32).

## Block and texture IDs

`Block(pub u16)` indexes 32,768 static property entries directly (raised from
4096 to 16,384 for v0.9, owner decision 2026-10-20, and to 32,768 for v0.12,
owner decision 2026-10-25; one ID per Java state, waterlogged states
included). 32,768 is the most the block-item encoding allows
(`BLOCK_ITEM_BASE` 0x8000 + state). State IDs are
append-only and must be below `world::block::STATE_CAPACITY`; unregistered
entries remain invisible, including the unused IDs in legacy saves. Constants
and `Block(n)` construction retain their existing shape. Generation builds a
u16 scratch chunk, then chooses uniform storage, direct bytes for IDs below
256, byte palette indices for at most 256 distinct states, or direct u16 IDs.
Mesh jobs still take immutable `Arc<ChunkData>` snapshots; edits copy on write.
Palettes are expanded by row into meshing scratch storage, outside the lighting
and greedy-meshing loops. Repeated edits reuse palette entries; a full palette
is rebuilt to discard removed states before promoting to direct storage.

Chunk files hold RLE records of `(u16 run length, u16 state ID)`, both
little-endian, under a `VXC2` header, `VXC3` once any saved state is 4096
or above, or `VXC4` once any is 16,384 or above. Builds from before each
capacity raise refuse the newer header, and worlds that use no states newer
than a build knows stay readable by it. The reader also accepts
`VXC1` records with byte state IDs and preserves their numeric values. A
chunk file in a format this build doesn't know (a newer build's) fails to
load, and play continues on regenerated terrain; saving then renames it to
`chunks.bin.unknown-<unix seconds>` instead of replacing it. Runs that never
save, such as benchmarks and screenshots, leave it alone. Existing inventory,
container, entity and level IDs keep their meanings: block items below 256
retain their IDs; higher block states use `item::BLOCK_ITEM_BASE + state_id`
(currently `0x8000 + state_id`), except the explicitly reserved honey block
item 1072 (state 8128). Always convert with `Item::from_block` or
`Item::from`, rather than casting a block ID to an item ID.

Block texture constants and `BlockInfo::tex` are u16. Quads still occupy
12 bytes, with 12 layer bits (4096 layers, raised from 2048 for v0.12): the
low byte remains in word 1; normal/cross quads use word 1 bits 21..24 for the
high bits. Shaped quads use word 0 bits 5, 11 and 17, since their cell
coordinates need only five bits, and bit 30, the top bit of the lower v
bound, which is at most 15 on a face with area (faces without area are not
emitted).
AO, lighting, bounds and normal-face corner coordinates retain full precision.
The GPU requests enough array layers for the registered textures when the
adapter supports them. Otherwise it binds up to eight pages of the largest
power-of-two size the adapter supports (`tex::page_layers`): two pages of
2048 on Apple GPUs once `tex::COUNT` passes 2048, and 256-layer pages on
adapters with only wgpu's default limit, which therefore hold at most 2048
layers. With all of v0.12's textures (`tex::COUNT` about 2,690) an adapter
needs arrays of at least 512 layers; one limited to 256 stops at startup
with that message. Eight pages is the current implementation limit;
supporting such adapters again needs more page bindings (a follow-up).
Empty pages share
the first texture; explicit gradients preserve mip selection across page
branches. Shaders specialize away paging on adapters using a single array.
Item icons retain their own texture array, addressed from `tex::ITEM_BASE`
(4096; the shaders use the same literal). Append block layers and update `tex::COUNT`; paging is automatic
up to `tex::CAPACITY`. New block states also need their registry properties,
base/orientation/material rules, recipes and creative-palette entries as usual.

## Simulation boundary

`src/lib.rs` exposes the reusable engine without window, GPU or audio code.
The desktop executable imports that library. With `--no-default-features`,
the `client` dependencies and executable are excluded. A headless `World`
streams terrain and runs gameplay without mesh jobs; meshing scratch memory
is allocated only when an actual mesh is built.

Singleplayer advances movement, survival, weather, actions, world systems,
entities and time in fixed 50 ms steps. Rendering and worker polling run
independently, with position interpolation for the camera, mobs, arrows,
smoke, dropped items, falling blocks and primed TNT. The player moves one
Java tick per step (`Player::tick`, a port of `LivingEntity.travel` and
`Entity.move` with Java's swept collision); mob collision substeps remain at
most 1/120 second. Offline menus pause the game clock.

Gameplay sky and block light belong to `World`, with incremental removal and
increase queues. Edits, streaming and fixed world steps resolve it without
waiting for meshes. Render jobs still compute their own light snapshots;
their results cannot replace gameplay light. Crops, saplings and other growth
read this gameplay sky and block light, including in headless worlds.
Complete column heightmaps seed gameplay skylight, including in the End.
Uncertain light at incomplete loaded borders is explicitly unknown.

The optional desktop agent host accepts bounded, versioned JSON-lines TCP
requests. Network workers enqueue commands; the game thread validates and
executes them. Desktop and device-free agents use the same `PlayerSession` for
identity, movement, inventory, vitals, mode, workstation inputs and personal
spawn data. The session owns validated fixed-tick food/drink and mining transactions;
adapters handle sounds, hand animation and damage/death presentation. Streaming,
fire and growth use the union of active player ranges;
agent-only chunks outside the host view stay unmeshed. Named agent profiles
persist in the root level file. See [the CLI protocol and limitations](agents.md).

Controller and LAN players run the shared actions on their own session
(`hands::Actor` for hands, `screen::Screen` for container clicks); no player's
state is swapped into the host's. Independent simultaneous dimensions and
shape-aware light occlusion remain future work. A first desktop LAN slice uses
bounded binary-framed TCP and authoritative chunk snapshots; see [LAN](lan.md).
Split-screen is implemented, and each view blends Nether biome fog around its
own eye.
See [the headless smoke run and checks](development.md#headless-simulation-foundation).

## Code layout

```text
src/
  lib.rs             reusable engine exports (no device dependencies)
  main.rs            desktop argument parsing, event loop
  session.rs         actor-owned core state, personal spawn selection
  session/
    consumption.rs   fixed-tick food/drink validation and transactions
    mining.rs        actor-owned progress, validated destruction and outcomes
  content/
    mod.rs           shared plank ingredient tag (including explicit Nether fuel exceptions)
    fuel.rs          first furnace fuel definitions, in Java ticks
    mining.rs        build-embedded ice support tag and bounded state lookup
  survival_items/
    bucket.rs        shared fluid-bucket/bottle execution and client presentation effects
  simulation/
    mod.rs           fixed 20 Hz clock, world/player steps, interpolation
    survival.rs      health, hunger, exhaustion, regeneration (unit tested)
    weather.rs       weather state and precipitation queries
    lighting.rs      reference day/weather brightness supplied to entity rules
  app/
    mod.rs           window, input, fixed game ticks, rendering, damage entry point
    actions.rs       hotbar/mining state and consumption presentation mirror
    recipe_book.rs   recipe navigation, responsive layout and hit testing
    hud.rs           HUD: hotbar, hearts/food/bubbles, containers, death and F3
    gui/             menu toolkit: screen stack, focus and navigation for mouse,
                     keyboard and controller, sliders, text fields and clipboard,
                     scrolling option lists, confirm and error screens
    menu.rs          pause menu and Open to LAN screen on the toolkit
    options/         Java's Options screen and sub-screens, shared by the title
                     screen and the pause menu
    title/           title screen, Select World, Edit World, Create New World,
                     the newer-version backup prompt, Multiplayer and the
                     update screen, all on the toolkit
    settings.rs      options file (per-user data folder: saves/options.txt)
    weather.rs       rain/snow render geometry
    mobs.rs          mob glue: melee, loot, explosions, entity events, --spawn
    items.rs         dropped items: spawning, pickup, throwing, death drops
    containers.rs    chest screens and shift-click quick moves
    enchanting.rs    enchanting table screen: item and lapis slots, offers
    anvil.rs         anvil input/result rules, level payment and wear
    smithing.rs      smithing table input/result screen and consumption
    farming.rs       hoe tilling, bone meal, trampling farmland
    doors.rs         doors, ladders and gates: placing, opening, breaking
    hand.rs          first-person hand animation: swings, item switches, bob
  inventory.rs       inventory slots, stacking, saved container overflow
  crafting.rs        crafting grids and recipes
  mining.rs          mining speed, harvest rules, tool wear, melee damage
  item.rs            item registry: blocks, materials, food and tools
  enchant.rs         enchantments: Java's definitions, effects, table offers,
                     anvil rules, java.util.Random port
  smithing.rs        diamond-to-Netherite metadata-preserving transforms
  player/            Java player tick: travel, collision, fluids and currents
  physics.rs         shared AABB-vs-block collision, ray-vs-box test
  entity/
    mod.rs           mob list, spawning/despawning rules, events, explosions
    mob.rs           mob kinds, AI, movement and combat state
    projectile.rs    skeleton arrows
    nether.rs        piglins, brutes, hoglins, zoglins, striders: sensing, bartering,
                     anger, zombification, bastion residents, spawn table, saves
    mob/nether_ai.rs their per-tick movement, attacks and strider lava physics
    mob/selector.rs  Java's flagged GoalSelector, for dolphins and turtles
    mob/dolphin.rs   the dolphin's 26.3 goals, item play and treasure search
    mob/turtle.rs    the turtle's travel, go-home, go-to-water and egg goals
    model/nether.rs  their box models
    item.rs          dropped items: physics, merging, despawning, saving
    chunk_storage.rs entities of unloaded chunks, out of the level until the chunk loads
    model.rs         animated box models -> camera-relative triangles
  mesh.rs            lighting + greedy meshing (runs on workers)
  workers.rs         thread pool
  bench.rs           headless generation/meshing benchmark
  audio/
    mod.rs           game-side handle, player-state sounds, device setup (cpal)
    sounds.rs        materials, sound ids and synthesis recipes
    mixer.rs         real-time mixer (voices, panning, underwater filter, limiter)
    dsp.rs           RNG, filters, envelopes, fades
    export.rs        --export-sounds WAV dump and stats
  world/
    mod.rs           chunk streaming, edits, heightmaps, raycasting
    lighting.rs      incremental gameplay block light, independent of meshes
    fluid.rs         water and lava flow simulation
    falling.rs       falling sand/gravel, edit settling, explosion craters
    furnace.rs       furnace contents, smelting and fuel
    chest.rs         chest contents
    growth.rs        random block ticks: crops, saplings, grass, farmland, leaf decay
    fire.rs          scheduled fire ticks, spread, burnout and random-tick lava ignition
    chunk.rs         chunk storage
    block.rs         block registry
    shape.rs         box shapes of stairs, fences, gates, ladders and doors
    terrain.rs       world generation
    nether.rs        Nether caverns, biome surface rules, shared grid-column cache
    nether_biome.rs  Java's multi-noise Nether biomes, fog sampler, spawn lists
    nether_biome_blocks.rs  crimson/warped/soul blocks (ids 1600..=1793)
    nether_features.rs  huge fungi, vines, deltas, basalt columns (Java ports)
    nether_decoration.rs  per-chunk feature placement across chunk borders
    nether_flora.rs  nylium, vine growth and Nether bone meal
    fortress.rs      Nether fortress layouts (Java's pieces), painted per chunk
    bastion.rs       reserved bastion-remnant loot data
    stronghold.rs    stronghold rings and layouts (Java's pieces), painted per chunk
    structure.rs     shared structure-piece frames, chunk painting and chest loot
    brewing.rs       brewing stand contents and Java's brewing mixes
    spawner.rs       spawner table, structure spawners and chests on chunk load
    noise.rs         Perlin noise and hashing
    storage.rs       save files
  render/
    mod.rs           wgpu pipelines, culling, draw submission, screenshots
    arena.rs         pooled GPU storage for chunk quads
    entity.rs        entity pass (one dynamic vertex buffer per frame)
    block_model.rs   free-standing textured blocks (falling sand, dropped items)
    hand.rs          first-person hand and held item (Minecraft's transforms)
    item_sprites.rs  procedural item icons (their own texture array, layers 256+)
    ui.rs            HUD geometry: rects, bitmap text, block icons
    textures.rs      procedural block textures
    shaders/         WGSL
```

## Procedural sound

Every sound is synthesized from noise, damped sinusoids, envelopes and
filters on a background thread at startup, with 2–4 seeded variants each.
Crunchy materials use filtered random-impulse "crackle" with a resonant
body band and a low thump; wood is a few inharmonic damped modes (a hollow
knock); glass breaks with a bright burst and falling tinkles; water is
swept low-passed noise plus rising-pitch bubbles. The mixer runs in the
audio callback: the game sends small commands over a bounded lock-free
channel, and up to 32 voices are resampled, panned and attenuated by
distance relative to the listener, then low-passed underwater and passed
through a peak limiter. With no audio device, the game logs a warning and
runs silently.

## Dependencies

wgpu and winit (graphics and windowing), cpal (audio output), glam (math),
bytemuck, crossbeam-channel, rustc-hash, font8x8, png, pollster, log and
env_logger.

## Release ID reservations (v0.9–v0.19)

Reserved on 2026-10-20 when `STATE_CAPACITY` rose to 16,384 (BLD-06), so
concurrent content work never collides. Allocate inside your release's range,
record each allocation in the table of the change that uses it, and leave
unused IDs unregistered. A release that outgrows its range takes the next
free block from the unassigned reserve, recorded here.
The ranges are unchanged by the 2026-10-22 renumbering: v0.10 (updates) and
v0.11 (menus and settings) add no states, so each later range kept its
content and took its release's new number.

| Block states | Release | Use |
| --- | --- | --- |
| 2525–4095 | v0.12 | New building-material families' base states |
| 4096–7679 | v0.9 | Slab types and stair halves, shapes and waterlogged states (BLD-04a/b) |
| 7680–8191 | v0.9 | Door hinges, floor/ceiling buttons and levers, wall grindstones and other mounts (BLD-04c/e) |
| 8192–8703 | v0.9 | Waterlogged states of the other waterloggable families (BLD-04f) |
| 8704–9727 | v0.12 | Sign and banner rotations, waterlogged and hinged forms of new families |
| 9728–10239 | v0.13 | Terrain blocks and states |
| 10240–10751 | v0.14 | Structure blocks and states |
| 10752–11263 | v0.15 | Wildlife and crop states |
| 11264–11775 | v0.16 | Deep dark and trial chamber states |
| 11776–12287 | v0.17 | Village workstation and raid states |
| 12288–12799 | v0.18 | End and Wither states |
| 12800–14335 | v0.19 | Redstone and automation states |
| 14336–16383 | — | Unassigned reserve |
| 16384–32767 | v0.12 and later | Added by the 32,768 raise (2026-10-25); see the v0.12 tracks below |

### v0.12 track reservations

Reserved on 2026-10-25 when `STATE_CAPACITY` rose to 32,768, so the
[v0.12 tracks](roadmap.md#v012-scope-building-materials) can run in
parallel. Each track allocates only inside its own rows. v0.12's older
ranges (2525–4095 and 8704–9727) are kept for the integrator and the data
pass; a track that outgrows its range asks for a block of them, or of the
reserve, and records it here.

| Block states | Item IDs | Texture layers | Track |
| --- | --- | --- | --- |
| 16384–20479 | 1080–1099 | 1776–1999 | 1. Wood (BLD-01a, BLD-04) |
| 20480–22527 | 1100–1109 | 2000–2127 | 2. Stone and minerals (BLD-01b) |
| 22528–24575 | 1110–1129 | 2128–2383 | 3. Copper (BLD-01c); v0.15's waxed forms are not in this range |
| 24576–28671 | 1130–1159 | 2384–2575 | 4. Colour (BLD-01d) |
| 28672–29695 | 1160–1189 | 2576–2767 | 5. Utilities and special blocks (BLD-05b, BLD-02b, ENV-04) |
| — | 1190–1199 | — | 6. Data pass (reference fuel items) |
| 29696–32767 | 1200–1299 | 2768–4095 | Unassigned reserve for v0.13 and later |

Texture layers, unlike state and item IDs, are never saved, so the
integrator may pack each track's layers together when merging; define a
track's layers as offsets from one base constant so that is a one-line
change. Block items still use `Item::from_block`; the item rows are for
items that place no block state (item frames, paintings, armor stands and
the like).

Allocations so far:

| Block states | Meaning | Record |
| --- | --- | --- |
| 4096–7059 | Stairs states beyond bottom, straight and dry: 76 per family in `slab_stair_states::FAMILIES` order | [slabs and stairs](verification/2026-10-20-slabs-stairs.md) |
| 7060–7215 | Slab states beyond bottom and dry: bottom wet, top dry, top wet, double, per family | [slabs and stairs](verification/2026-10-20-slabs-stairs.md) |
| 7680–7691 | Lever mounts 6–11 (floor and ceiling facing north, east, west), off then on | [doors and mounts](verification/2026-10-20-doors-mounts.md) |
| 7696–7767 | Button mounts 6–11 for stone, oak, crimson, warped, pale oak and bamboo | [doors and mounts](verification/2026-10-20-doors-mounts.md) |
| 7800–7903 | Right-hinge lower doors: oak, iron, then the 11 wood doors | [doors and mounts](verification/2026-10-20-doors-mounts.md) |
| 7904–7911 | Wall and ceiling grindstones | [doors and mounts](verification/2026-10-20-doors-mounts.md) |
| 7920–7927 | Lantern and soul lantern: standing or hanging, dry or wet | [doors and mounts](verification/2026-10-20-doors-mounts.md) |
| 8000–8039 | Stored wall UP=false, dry then wet, per `wall_states::FAMILIES`; existing wall IDs (including their wet partners) mean UP=true (owner-approved 2026-10-21); 8040–8127 remain reserved/unregistered | [wall posts and powered doors](verification/2026-10-21-wall-posts-powered-doors.md) |
| 8128 (8129–8143 reserved, unused) | Honey block; explicit block item 1072 and honey bottle item 1073; base texture maximum L=1762, honey side/top/bottom layers 1771–1773 (L+9..L+11), 1774 (L+12) reserved unused | [honey](verification/2026-10-21-honey-block.md) |
| 7936–7967 | Scaffolding: `7936 + distance + 8*bottom + 16*waterlogged`, distance 0–7; default 7943 | [scaffolding and powder snow](verification/2026-10-21-scaffolding-powder-snow.md) |
| 7968 | Powder snow; 7969–7999 remain unused in this task's reservation | [scaffolding and powder snow](verification/2026-10-21-scaffolding-powder-snow.md) |
| 8192–8476 | Waterlogged partners of 285 existing states (fences, bars, panes, walls, chains, trapdoors, rails, ladders, chests, leaves, amethyst, dripstone, dripleaves, roots, propagules, dead coral) and dry partners of sea pickles and live coral, in `waterlog::RUNS` order; append runs only at the end | [waterlogging](verification/2026-10-20-waterlogging.md) |
| 16384–16403 | Wood track: X then Z axis of oak, spruce, birch, jungle, acacia, dark oak, mangrove, cherry and pale oak logs and the bamboo block (`wood::LOGS`); the existing log ids mean Y | [v0.12 wood](verification/2026-10-25-v0.12-wood.md) |
| 16416–16496 | Wood track: wood, stripped log and stripped wood per overworld wood, Y/X/Z: `16416 + 9*wood + 3*kind + axis` | [v0.12 wood](verification/2026-10-25-v0.12-wood.md) |
| 16497–16499 | Wood track: block of stripped bamboo, Y/X/Z | [v0.12 wood](verification/2026-10-25-v0.12-wood.md) |
| 16512–17023 | Wood track: trapdoors, buttons and plates of spruce, birch, jungle, acacia, dark oak, mangrove, cherry and polished blackstone, 64 per family (`wood::switches`; polished blackstone leaves its trapdoor slots unregistered) | [v0.12 wood](verification/2026-10-25-v0.12-wood.md) |
| 17024–18559 | Wood track: standing, wall, hanging and wall hanging signs, 128 per wood (112 used; `wood::signs`) | [v0.12 wood](verification/2026-10-25-v0.12-wood.md) |
| 18560–19327 | Wood track: shelves, 64 per wood (`wood::shelves`) | [v0.12 wood](verification/2026-10-25-v0.12-wood.md) |
| 19328–19583 | Wood track: chiseled bookshelf, `19328 + 64*facing + slots`; 19584–20479 remain unused in the track's reservation | [v0.12 wood](verification/2026-10-25-v0.12-wood.md) |
| 20480–20511 | v0.12 stone and minerals full blocks (29 in `stone_blocks` order), then the quartz pillar's Y, X and Z axes; texture layers 2000–2035 (`stone_blocks::t`, offsets from `t::TEX`) | [stone and minerals](verification/2026-10-25-v0.12-stone.md) |
| 20512–21471 | Stairs for 12 v0.12 families, 80 states each (`slab_stair_states` local order, local 0 is the item); `slab_stair_states` families 39–50 | [stone and minerals](verification/2026-10-25-v0.12-stone.md) |
| 21472–21541 | Slabs for those 12 families plus cut and cut red sandstone, 5 states each: bottom, bottom wet, top, top wet, double; families 39–52 | [stone and minerals](verification/2026-10-25-v0.12-stone.md) |
| 21552–21595 | 11 v0.12 walls, 4 states each: UP, UP wet, no UP, no UP wet (`wall_states` families 20–30); 21542–21551 and 21596–22527 stay unused; items 1100–1109 unused | [stone and minerals](verification/2026-10-25-v0.12-stone.md) |
| 22528–23372 (23373–24575 reserved, unused) | v0.12 copper (BLD-01c), `world::copper`: per family, `stride` ids per oxidation stage (unaffected, exposed, weathered, oxidized) from its origin: block 22528 (stride 1; 22528 unused, the unaffected block stays 231), cut 22532, chiseled 22536, grate 22540 (2), bulb 22548 (4), stairs 22564 (80; `slab_stair_states` families 53–56), slab 22884 (5), door 22904 (24), trapdoor 23000 (32), bars 23128 (2), chain 23136 (6), lantern 23160 (4), chest 23176 (24; only the 8 single states registered), lightning rod 23272 (24; only the 12 unpowered states registered), torch 23368–23372 (standing, then wall facing south, north, east, west). No item IDs (all are block items). Texture layers 2128–2200: 18 per stage from 2128, then the torch | [copper](verification/2026-10-25-v0.12-copper.md) |
| 24576–27135 | v0.12 wool then concrete stairs, 16 colours each in `DyeColor` order, 80 states per family (`slab_stair_states` local order, local 0 is the item); `slab_stair_states` families 57–88 (the family field widened from 6 to 7 bits in memory only) | [colour](verification/2026-10-25-v0.12-colour.md) |
| 27136–27295 | Slabs for those 32 families, 5 states each: bottom, bottom wet, top, top wet, double | [colour](verification/2026-10-25-v0.12-colour.md) |
| 27296–27567 | Candles, plain then the 16 dyes, 16 states each: `(candles - 1) * 4 + lit * 2 + wet` | [colour](verification/2026-10-25-v0.12-colour.md) |
| 27568–27601 | Candle cakes, plain then the 16 dyes: unlit, lit; 27602–27615 stay unused | [colour](verification/2026-10-25-v0.12-colour.md) |
| 27616–27935 | Standing banners (16 rotations per colour), then wall banners (4 facings per colour, `Facing::ALL` order) | [colour](verification/2026-10-25-v0.12-colour.md) |
| 27936–27943 | Straw bed: foot then head, 4 facings each; 27944–28671 stay unused. Items 1130–1145 are the 16 cushions (1146–1159 unused); texture layers 2384–2456 (`colour_blocks::tex`, offsets from `tex::TEX`; 2457–2575 unused) | [colour](verification/2026-10-25-v0.12-colour.md) |
| 28672–28679 | Cauldrons: empty, water levels 1–3, lava, powder snow levels 1–3 (`utilities` module) | [utilities](verification/2026-10-25-v0.12-utilities.md) |
| 28680–28713 | Flower pot, then 33 potted plants in `utilities::POTTED` order; 28714–28720 free for later potted plants | [utilities](verification/2026-10-25-v0.12-utilities.md) |
| 28721–28726 | Lodestone; respawn anchor charges 0–4; 28727 unused | [utilities](verification/2026-10-25-v0.12-utilities.md) |
| 28728–28791 | Campfires: `28728 + soul*32 + wet*16 + signal*8 + unlit*4 + facing` (north, east, south, west) | [utilities](verification/2026-10-25-v0.12-utilities.md) |
| 28792–28871 | Coral wall fans: `28792 + dead*40 + colour*8 + dry*4 + facing` | [utilities](verification/2026-10-25-v0.12-utilities.md) |
| 28872–28879 | Ender chests: `28872 + wet*4 + facing` | [utilities](verification/2026-10-25-v0.12-utilities.md) |
| 28880–28903 | Trapped chests: `28880 + wet*12 + type*4 + facing`; only `type` single is registered (left and right wait for double chests) | [utilities](verification/2026-10-25-v0.12-utilities.md) |
| 28904–28919 | Decorated pots: `28904 + wet*8 + cracked*4 + facing` | [utilities](verification/2026-10-25-v0.12-utilities.md) |
| 28920–28951 | Big dripleaf `facing` and `tilt`: `28920 + wet*16 + tilt*4 + facing`; north with no tilt stays 1950 and its wet partner, so 28920 and 28936 are unregistered; 28952–29695 unused | [utilities](verification/2026-10-25-v0.12-utilities.md) |
| (items) 1160–1163 | Item frame, glow item frame, painting, armor stand (`Item::ITEM_FRAME`…); 1164–1189 unused. Texture layers 2576–2685 (`utilities::t`, offsets from `utilities::TEX`); 2686–2767 unused | [utilities](verification/2026-10-25-v0.12-utilities.md) |

Wood track textures use layers **1776–1984** as offsets from
`wood::tex::BASE` (stripped log sides and ends, quarter-turned log sides,
seven trapdoors, sign and hanging sign item sprites, shelf bodies and
fronts, and the chiseled bookshelf's top, side and 64 fronts); 1985–1999
stay unused. The track uses no item ids (1080–1099 stay unused): signs, hanging signs, shelves and
the chiseled bookshelf are block items (`Item::from_block`).

PLY-02c items: **1070 scaffolding**, **1071 powder snow bucket**. Scaffolding
uses its reserved item ID rather than `BLOCK_ITEM_BASE + state`. Powder snow
has no block item; pick-block returns its bucket. The original highest texture
layer at base `d17fb37` is **L = 1762** (soul lantern). This task allocates
**1763–1767**: scaffold top, scaffold side, powder snow, frozen full heart,
frozen half heart. **1768–1770** (L+6..L+8) stay unused; the other task owns
L+9..L+12. Textures are original procedural pixels.

## Assessment branch ID reservations

The assessment inspected `main` and retained v0.7 `f5bf0d1` before allocating
these ranges. Preserve their meanings; do not reuse the unused patterned slots.

| Block states | Meaning |
| --- | --- |
| 2160–2164 | Hanging mangrove propagule ages 0–4 |
| 2200–2222 | Pale oak stairs/slab/fence/gate/door states |
| 2223–2245 | Bamboo stairs/slab/fence/gate/door states |
| 2246–2268 | Bamboo mosaic layout; only stairs/slab 2246–2250 are registered |
| 2269–2298 | Pale oak trapdoor/button/pressure plate states |
| 2299–2328 | Bamboo trapdoor/button/pressure plate states |
| 2360–2365 | Mud brick stairs/slab/wall |
| 2366–2371 | Prismarine stairs/slab/wall |
| 2372–2377 | Prismarine brick stairs/slab; 2377 is reserved, unregistered |
| 2378–2383 | Dark prismarine stairs/slab; 2383 is reserved, unregistered |
| 2400–2415 | Wall amethyst buds/clusters (GEN-02): facing south, north, east, west × small, medium, large, cluster |

| Item ids | Meaning |
| --- | --- |
| 1000–1039 | Retained v0.7 animal items |
| 1040 | Turtle helmet |
| 1041–1046 | Audit potion and splash-potion slots |
| 1047 | Enchanted golden apple (Java 26.3 `ENCHANTED_GOLDEN_APPLE`; 1041 was already an audit potion slot) |
| 1048–1049 | Unallocated; keep free |
| 1050–1069 | Java 26.3 copper equipment: tools 1050–1054 (pickaxe, shovel, axe, hoe, sword), copper nugget 1055, armor 1056–1059 (helmet, chestplate, leggings, boots); 1060–1069 reserved (spears) |

Texture layers 1753–1758 append pale oak/bamboo doors and trapdoors; existing
layer meanings remain intact. These definitions inherit the existing shaped
block representation; adding the family is not proof of complete Java state,
lighting or waterlogging parity. See the [content/resource evidence](verification/2026-10-12-content-and-assets.md).

## Focused application and simulation modules

`src/agent/command.rs` owns syntax, coordinates and completion;
`src/agent.rs` owns execution and timed inputs. Entity tick coordination is in
`src/entity/lifecycle.rs` and population/cage activation in
`src/entity/spawning.rs`; individual AI/animal/mount systems retain their
existing modules. `src/entity/chunk_storage.rs` moves entities whose storage
chunk unloads out of the level and back when it loads (Java's per-chunk
entity sections); stored entities are saved in the dimension's existing
entity properties ([evidence](verification/2026-10-17-entity-storage.md)). Application startup, disk-save assembly, fixed gameplay
steps and client action adapters are in `src/app/startup.rs`, `save.rs`,
`tick.rs` and `player_actions.rs`. Window events and frame rendering remain
in the coordinator. Controller/CLI/desktop action ownership is a separate,
ongoing extraction; moving a file alone does not complete STR-03.
`src/session/placement.rs` owns validated block placement and actor outcomes;
`src/content/placement.rs` maps existing door-family states without adding IDs.
Client/CLI adapters retain other item interactions and presentation. See the
[placement evidence and remaining boundaries](verification/2026-10-13-shared-placement.md).

Turtle equipment uses standalone item **1040** for the turtle helmet.
The retained v0.7 animal reservation 1000–1039 remains untouched. Potion indices
36–38 append normal/long/strong Turtle Master. Their normal items reserve
1041–1043 and splash items 1044–1046, because the old ranges are full. Existing potion/item meanings
stay unchanged. The [turtle equipment report](verification/2026-10-12-turtle-equipment.md)
records the accompanying implementation and acceptance evidence.
