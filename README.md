# VoxelCraft

A Minecraft-style voxel game written from scratch in Rust with
[wgpu](https://wgpu.rs), built for performance first.

```sh
cargo run --release                 # play (continues ./saves/world if it exists)
cargo run --release -- --new --seed 42 --rd 12
cargo run --release -- --help
```

## Controls

| Input | Action |
|---|---|
| Mouse | Look (click the window to capture the mouse) |
| W A S D | Move |
| Space | Jump / swim up / fly up — double-tap to toggle flying (creative) |
| Left Shift | Fly down |
| Left Ctrl or R | Sprint |
| F | Toggle flying (creative) |
| Left / right click | Break / place block (hold to repeat) |
| Middle click | Pick block |
| E | Inventory (click to move stacks; creative shows the block palette) |
| G | Toggle survival / creative |
| 1–9, scroll wheel | Select hotbar slot |
| `[` / `]` | Decrease / increase render distance |
| T | Skip ahead 2 in-game hours |
| M | Mute / unmute sound (`--mute` starts muted, `--volume 0..1` sets the master volume) |
| V | Toggle vsync |
| F1 | Toggle HUD |
| F3 | Debug screen |
| F11 | Fullscreen |
| Esc | Release mouse (press again to save and quit) |

Press F3 for the debug screen (FPS, position, biome, chunk and draw stats).
The world autosaves every two minutes and on exit; only player-modified
chunks are stored, everything else regenerates from the seed.

## Features

- Infinite procedurally generated terrain: oceans, beaches, plains,
  forests, deserts, snowy tundra, taiga and mountains, with spaghetti caves,
  deep caverns, ores, oak and spruce trees and cacti
- Flood-fill sky and block light with smooth lighting and ambient occlusion
- Day/night cycle with a procedural sky: square sun and moon, sunset glow,
  rotating stars and drifting blocky clouds; distance and underwater fog
- Flowing water: falls, spreads up to 7 blocks toward the nearest drop,
  dries up without a source, and forms infinite sources; lowered surfaces
- Walking, swimming and flying with AABB collision
- Survival and creative modes: timed block breaking with crack overlay,
  drops, a 36-slot inventory with stacks, and a creative block palette
- Break, place and pick blocks, with a selection outline and hotbar
- Procedurally generated, mipmapped block textures — the game ships no assets
- Procedural sound, synthesized in code at startup (~25 ms): material-specific
  break/place/footstep sounds (stone, wood, dirt, grass, gravel, sand, snow,
  leaves, glass, water), jump and landing thuds, splashes and swimming,
  inventory clicks, wind and cave ambience with dripping water, positional
  panning and distance falloff, and a muffled mix while underwater

## Performance design

| Technique | Why |
|---|---|
| 32³ chunks; uniform chunks stored as a single block | Most sky and deep-rock chunks cost 1 byte instead of 32 KiB |
| Greedy meshing | Merges coplanar faces with identical texture/AO/light into one quad |
| 8-byte vertices, UVs derived in shader | Roughly 4x smaller than Minecraft's vertex format |
| One shared quad index buffer | No per-chunk index data |
| Lighting computed inside mesh jobs over a 15-block margin | Exact at chunk borders with zero shared mutable light state, so every job runs in parallel |
| Worker thread pool with nearest-first scheduling and chunk versioning | Generation and meshing never block the render thread; stale results are dropped |
| Copy-on-write `Arc` chunk data | Mesh jobs snapshot neighbours without locks or copies |
| Frustum culling, front-to-back opaque and back-to-front translucent sorting | Less overdraw; correct water blending |
| Camera-relative rendering, reverse-Z infinite projection | Stable precision far from the origin and at long view distances |
| Synchronous remesh of edited chunks only | Block edits appear the same frame; lighting ripples update on workers |
| Event-driven water simulation, batched per tick | Untouched oceans cost nothing; a flood tick takes ~0.5 ms and remeshes on workers |

### Numbers (Apple M5, release build)

```text
$ voxelcraft --bench --rd 8
generate (1 thread): 0.195 ms/chunk
light+mesh (1 thread): 1.15 ms per dense chunk
stream rd=8 on 9 workers: 2344 chunks loaded, 1576 meshed in 0.17 s

$ voxelcraft --bench-render --rd 8     # 1600x900, GPU-synchronised each frame
avg 1.22 ms (818 fps), p99 1.71 ms — 355 draw calls, 0.75M quads

$ voxelcraft --bench-render --rd 16    # 512-block view distance
avg 2.55 ms (392 fps), p99 5.26 ms — 1016 draw calls, 1.64M quads
```

Render distance is measured in 32-block chunks, so `--rd 8` is 256 blocks
(Minecraft's 16) and `--rd 16` is 512 blocks (Minecraft's 32).

## Code layout

```text
src/
  main.rs            argument parsing, event loop
  app.rs             window, input, game loop, HUD state, day/night
  player.rs          movement physics and collision
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
    chunk.rs         chunk storage
    block.rs         block registry
    terrain.rs       world generation
    noise.rs         Perlin noise and hashing
    storage.rs       save files
  render/
    mod.rs           wgpu pipelines, culling, draw submission, screenshots
    textures.rs      procedural block textures
    shaders/         WGSL
```

## Development

```sh
cargo test --release
cargo run --release -- --bench
cargo run --release -- --bench-render --pose 0,100,0,0,-10
cargo run --release -- --screenshot shot.png --pose 0,190,0,45,-35
cargo run --release -- --place 0,~,0,water --pose 0,120,-10,90,-30   # scripted scenes
cargo run --release -- --export-sounds   # write every sound to target/sounds/*.wav with stats
```

## Sound

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
