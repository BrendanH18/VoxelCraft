# Engine architecture

[← Back to VoxelCraft](../README.md) · [Development tools](development.md)

VoxelCraft builds its world, meshes, textures and sounds in Rust. This guide
covers the rendering and simulation choices behind the game.

## Performance design

| Technique | Why |
|---|---|
| 32³ chunks; uniform chunks stored as a single block | Most sky and deep-rock chunks cost 1 byte instead of 32 KiB |
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
generate (1 thread): 0.218 ms/chunk
light+mesh (1 thread): 0.824 ms per dense chunk
stream rd=8 on 9 workers: 2344 chunks loaded, 1576 meshed in 0.21 s

$ voxelcraft --bench-render --rd 8     # 1600x900, GPU-synchronised each frame
avg 1.10 ms (~900 fps) — 661 draw calls, 0.42M quads drawn

$ voxelcraft --bench-render --rd 16    # 512-block view distance
avg 2.05 ms (~490 fps) — 1708 draw calls, 0.89M quads drawn, 52 MB of quad data
```

Render distance is measured in 32-block chunks, so `--rd 8` is 256 blocks
(Minecraft's 16) and `--rd 16` is 512 blocks (Minecraft's 32).

## Simulation boundary

`src/lib.rs` exposes the reusable engine without window, GPU or audio code.
The desktop executable imports that library. With `--no-default-features`,
the `client` dependencies and executable are excluded. A headless `World`
streams terrain and runs gameplay without mesh jobs; meshing scratch memory
is allocated only when an actual mesh is built.

Singleplayer advances movement, survival, weather, actions, world systems,
entities and time in fixed 50 ms steps. Rendering and worker polling run
independently, with position interpolation for the camera, mobs, arrows,
smoke, dropped items, falling blocks and primed TNT. Collision substeps remain
at most 1/120 second. Offline menus pause the game clock.

Gameplay block light belongs to `World`, with incremental removal and
increase queues. Edits, streaming and fixed world steps resolve it without
waiting for meshes. Render jobs still compute their own light snapshots;
their results cannot replace gameplay light. Crops and saplings use this
light even in headless worlds.

The optional desktop agent host accepts bounded, versioned JSON-lines TCP
requests. Network workers enqueue commands; the game thread validates and
executes them. Device-free agent sessions own independent movement, vitals and
inventory. Streaming, fire and growth use the union of active player ranges;
agent-only chunks outside the host view stay unmeshed. Named agent profiles
persist in the root level file. See [the CLI protocol and limitations](agents.md).

Full client/session action extraction, mob target identities, independent
simultaneous dimensions, authoritative skylight, shape-aware light occlusion,
desktop networking and split-screen remain future work.
See [the headless smoke run and checks](development.md#headless-simulation-foundation).

## Code layout

```text
src/
  lib.rs             reusable engine exports (no device dependencies)
  main.rs            desktop argument parsing, event loop
  simulation/
    mod.rs           fixed 20 Hz clock, world/player steps, interpolation
    survival.rs      health, hunger, exhaustion, regeneration (unit tested)
    weather.rs       weather state and precipitation queries
  app/
    mod.rs           window, input, fixed game ticks, rendering, damage entry point
    actions.rs       hotbar selection, mining and eating progress (unit tested)
    recipe_book.rs   recipe navigation, responsive layout and hit testing
    hud.rs           HUD: hotbar, hearts/food/bubbles, containers, death and F3
    menu.rs          pause menu and options screen
    settings.rs      options file (per-user data folder: saves/options.txt)
    weather.rs       rain/snow render geometry
    mobs.rs          mob glue: melee, loot, explosions, entity events, --spawn
    items.rs         dropped items: spawning, pickup, throwing, death drops
    containers.rs    chest screens and shift-click quick moves
    farming.rs       hoe tilling, bone meal, trampling farmland
    doors.rs         doors, ladders and gates: placing, opening, breaking
    hand.rs          first-person hand animation: swings, item switches, bob
  inventory.rs       inventory slots, stacking, saved container overflow
  crafting.rs        crafting grids and recipes
  mining.rs          mining speed, harvest rules, tool wear, melee damage
  item.rs            item registry: blocks, materials, food and tools
  player.rs          player movement
  physics.rs         shared AABB-vs-block collision, ray-vs-box test
  entity/
    mod.rs           mob list, spawning/despawning rules, events, explosions
    mob.rs           mob kinds, AI, movement and combat state
    projectile.rs    skeleton arrows
    item.rs          dropped items: physics, merging, despawning, saving
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
    nether.rs        Nether caverns
    fortress.rs      Nether fortress layouts (Java's pieces), painted per chunk
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
