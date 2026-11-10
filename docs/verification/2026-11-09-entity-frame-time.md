# Entity drawing and tick frame time (2026-11-09)

[Documentation index](../README.md) · [Roadmap §6](../roadmap.md) · [Requirement inventory](../requirements.md) · [Previous profile](2026-10-29-v0.12-finish.md#frame-time) · Results: [before](../../benchmarks/results/2026-11-09-61e1f75-m5-offscreen/summary.json) ([frames](../../benchmarks/results/2026-11-09-61e1f75-m5-offscreen/frame-times.json.gz)), [after](../../benchmarks/results/2026-11-09-e90b5d0-m5-offscreen/summary.json) ([frames](../../benchmarks/results/2026-11-09-e90b5d0-m5-offscreen/frame-times.json.gz))

The 240 FPS gate (roadmap §6, PERF-02) was last measured on 2026-10-29 and
failed: the slowest 1% of frames averaged 138–229 FPS. This record measures
the current game, finds what the slow frames do, and removes the two largest
costs. **The gate is still not shown to pass.** Valid windowed runs were not
possible today, so every number below comes from offscreen comparison runs,
which are not gate evidence. The windowed three-run suite is the remaining
step.

- Requirements: PERF-02 (240 FPS in every scene), PERF-03 (streaming budget).
- Base commit `c96240b`. Measured commits: before `61e1f75` (the base plus
  the benchmark harness changes below, no game change), after `e90b5d0`.
- Implemented, unverified.

## Result

One offscreen run per scene per build, 30 sampled seconds after a 10-second
warm-up, 1920 x 1080, 256-block view, enhanced graphics. Gate thresholds for
reference: 1% low at least 240 FPS, p99 at most 4.17 ms.

| Scene | Build | Average FPS | 1% low FPS | p50 ms | p99 ms | Worst ms |
|---|---|---|---|---|---|---|
| survival | before | 625.5 | 226.3 | 1.52 | 3.83 | 9.9 |
| | after | 651.1 | 299.6 | 1.51 | 2.50 | 13.0 |
| building | before | 604.3 | 219.2 | 1.50 | 3.95 | 8.5 |
| | after | 626.5 | 271.0 | 1.46 | 2.81 | 9.0 |
| entities | before | 583.7 | 224.0 | 1.55 | 3.83 | 8.2 |
| | after | 620.5 | 299.5 | 1.48 | 2.69 | 9.7 |
| automation | before | 602.5 | 215.4 | 1.48 | 4.01 | 8.6 |
| | after | 623.1 | 263.3 | 1.46 | 2.89 | 9.5 |
| materials | before | 554.4 | 200.5 | 1.56 | 4.44 | 8.2 |
| | after | 607.1 | 299.0 | 1.53 | 2.69 | 7.8 |
| streaming | before | 494.9 | 64.1 | 1.45 | 13.52 | 23.4 |
| | after | 526.4 | 73.3 | 1.42 | 11.50 | 21.5 |
| menus | before | 820.2 | 361.7 | 1.11 | 2.09 | 13.5 |
| | after | 842.0 | 449.0 | 1.11 | 2.03 | 7.7 |
| title | before | 4,436 | 3,892 | 0.22 | 0.25 | 0.8 |
| | after | 4,487 | 4,042 | 0.22 | 0.24 | 0.6 |

Validity: none of these runs counts toward the gate. Each is marked
ineligible in its report (drawn offscreen, 30-second sample, one run). Within
that, every simulation scene completed 600 of 600 ticks with no paused time,
no frame took over 50 ms, and the entity scene held 160–161 mobs. The other
world scenes carried 250–310 mobs from natural herds.

Read as a comparison, all five ordinary simulation scenes' 1% lows rose
from below 240 FPS to above it and all five p99 values are under 4.17 ms
afterwards (four already were; `materials` was not), the menus gained, and
streaming still fails
by a wide margin for a different reason ([what remains](#what-remains)).

## Method and machine state

The reference Apple M5 MacBook Pro (4 performance and 6 efficiency cores,
16 GB, macOS 27.2). The machine was not in the state the gate requires:

- It ran on battery throughout (43% at the start, 18% at the end; low power
  mode off). The [development guide](../development.md#live-performance-scene-suite)
  asks for AC power.
- It was in use when the work began: a browser was decoding video, and
  keyboard or pointer input had arrived within the last second. The other
  agents build in the cloud, but their own processes were running.
- `caffeinate -d -u` kept the display awake during the runs.

Windowed runs of the unchanged base (`c96240b`) did not produce valid
samples:

| Run | Result |
|---|---|
| building, 5 s smoke | 33 frames skipped: window hidden or occluded |
| building, full 60 s | all 4,896 intervals skipped (81.6 FPS is the skip path's 8 ms wait) |
| building, 5 s smoke, twice more | 2 skipped, p50 2.99 ms; then 0 skipped, p50 7.99 ms |
| entities, 5 s smoke | 0 skipped, p50 3.02 ms, 238 FPS average |

Three of the five runs skipped frames. Of the two that presented every
frame, one had a 7.99 ms median: the 120 Hz display holding frames, which
the development guide records as a battery effect. (The smoke runs' other
figures mean little: they sample five seconds after a one-second warm-up.)

The comparison therefore used the renderer's offscreen path, as the
2026-10-29 record did, now behind a flag instead of a local edit
(`--benchmark-offscreen`, or `tools/benchmark_scenes.py --offscreen`). It
draws the ordinary frame into an offscreen image at the same size and
presents nothing. The first offscreen runs grew to 3.6 and 4.1 GB: with nothing
presented, nothing held the main thread to the GPU's pace, and submitted
frames queued up. An offscreen benchmark frame now waits for the frame two
back to finish on the GPU, as a window's three images would make it. With
that, the same two scenes (`entities` and `building`) peaked at 413 and
473 MiB, and the frame interval is the larger of the CPU's and the GPU's
time.

Each build ran the eight scenes once, before first and then after, with
`python3 tools/benchmark_scenes.py --executable … --offscreen --seconds 30`.
One pass used 5% of the battery, which is why there is one run per scene and
not three. A single run per side does not measure run-to-run variation. For
scale, the 2026-10-16 baseline's three windowed `building` runs spanned
261–273 FPS in 1% low.

Reports now include `frame_phases_ms`: mean time per phase of the frame over
all frames, over the slowest 1%, and over the frames that ran a tick. The
phases are streaming and polling, ticks, uploads, entity geometry, the rest
of the scene, acquiring the image, drawing, and presenting (offscreen: the
wait for the GPU).

## What the slow frames were doing

The slowest 1% of frames are tick frames. At about 600 FPS, one frame in
thirty runs a 20 Hz tick, and a tick cost more than the rest of the frame
put together. Before the change (milliseconds, mean):

| Scene | Tick frames: tick | Tick frames: entities | Tick frames: total | Slowest 1%: total |
|---|---|---|---|---|
| survival | 2.95 | 0.68 | 3.87 | 4.42 |
| building | 3.03 | 0.69 | 3.99 | 4.56 |
| entities | 2.79 | 0.76 | 3.81 | 4.47 |
| automation | 3.04 | 0.75 | 4.05 | 4.64 |
| materials | 2.90 | 1.17 | 4.42 | 4.99 |
| streaming | 10.08 | 0.48 | 11.12 | 15.61 |

So the 1% low is set by the tick plus one frame's ordinary work, and the
entity geometry the 2026-10-29 profile found is the second cost, not the
first. On ordinary frames the main thread was not the limit at all: it
spent 0.55–0.59 ms of each 1.6–1.7 ms frame waiting for the GPU.

A 15-second `sample` of the before build in `materials` (7,437 main-thread
samples) splits the two costs:

| Where | Samples | Share |
|---|---|---|
| `Entities::mesh`: rebuilding every entity's vertices | 1,835 | 24.7% |
| … of which mob models (`model::build`) | 1,451 | 19.5% |
| … of which the item frame wall | 370 | 5.0% |
| `Renderer::set_entities`: copying them to the GPU | 1,600 | 21.5% |
| `signs::text`: sign text, rebuilt each frame | 601 | 8.1% |
| `Renderer::draw_view` | 1,404 | 18.9% |
| `World::mesh_input` (re-meshing changed chunks) | 660 | 8.9% |
| `Game::tick` | 814 | 10.9% |
| … of which random block ticks | 535 | 66% of the tick |
| … of which `redstone_blocks::component` | 311 | 38% of the tick |
| … of which `copper::ages` | 79 | 10% of the tick |
| … of which mobs (`update_mobs`) | 169 | 21% of the tick |

Two things stand out. The entity copy is as expensive as the build: about
2 MB of vertices a frame went into a staging buffer that wgpu allocates
anew for every write. And two thirds of a tick was random block ticks.
A tick makes three of them per 16³ section in the simulation distance, up
to about 30,000, and nearly all land on blocks with no random-tick rule.
Each one still ran the whole rule chain, including a redstone component
lookup that makes three other lookups before it can say "not redstone
ore".

## What changed

**Plain boxes are drawn as GPU instances.** Mob models are boxes. Each box
used to become 36 vertices (1,152 bytes) transformed on the CPU. It is now
one 96-byte `BoxInstance`: an affine map from box space to camera-relative
space, the two corners, the colour, the light, and the six face shades
computed on the CPU exactly as before. The vertex shader (`vs_box` in
`entity.wgsl`) expands it into the same 36 vertices in the same order, and
the fragment shader is unchanged. Per box the CPU evaluates the model's
transform four times instead of 36 and writes a twelfth of the bytes.

Geometry whose faces are painted individually keeps the vertex path
unchanged: player skins, armor, held blocks and items, name tags, lead
ropes, sign text and painting tiles. So do translucent boxes (the slime
shell), which must draw after everything opaque, in order. Nothing is
cached between frames, nothing is culled that was drawn before, and
animation runs at full rate; the set of mobs drawn is decided by the same
distance and behind-the-camera test. Split-screen views and LAN clients
build and upload a mesh per view as before, and the first-person hand has
its own pass, which this does not touch.

**Random block ticks look their rule up in a table.** The rule chain is now
a pure classification of the block state, computed once per state and read
from a table. The rules, their order and the random numbers drawn are the
same.

The entity phase and the tick after the change (milliseconds, mean):

| Scene | Entities, all frames | Tick, tick frames | Tick frames: total | Slowest 1%: total |
|---|---|---|---|---|
| survival | 0.65 → 0.09 | 2.95 → 1.94 | 3.87 → 2.36 | 4.42 → 3.34 |
| building | 0.67 → 0.10 | 3.03 → 2.18 | 3.99 → 2.69 | 4.56 → 3.69 |
| entities | 0.73 → 0.09 | 2.79 → 1.69 | 3.81 → 2.23 | 4.47 → 3.34 |
| automation | 0.72 → 0.10 | 3.04 → 2.32 | 4.05 → 2.85 | 4.64 → 3.80 |
| materials | 1.16 → 0.46 | 2.90 → 1.80 | 4.42 → 2.68 | 4.99 → 3.35 |
| streaming | 0.46 → 0.05 | 10.08 → 8.73 | 11.12 → 9.29 | 15.61 → 13.64 |
| menus | 0.57 → 0.05 | 3.08 → 1.79 | 3.86 → 2.14 | 2.77 → 2.23 |

Average FPS rose only 3–10% because ordinary frames are bound by the GPU,
at about 1.5 ms a frame offscreen: the time the main thread no longer
spends on entities it now spends waiting (0.58 → 1.04 ms in `survival`).

### Alternatives not taken

- **Caching each mob's model in local space.** A walking mob's limbs, a
  turning head and every time-driven idle animation change the model each
  frame, so the cache would miss for exactly the mobs that cost the most,
  and it needs invalidation for pose, equipment, age, colour and death.
  Instancing gets the saving without keeping anything between frames, so
  there is nothing to go stale.
- **Building on worker threads.** It would shorten the build but not the
  copy, which was half the cost, and it competes with chunk meshing for the
  same cores.
- **Frustum culling before building.** The existing test already drops mobs
  behind the camera, so a frustum would remove well under half of the rest,
  and it risks mobs popping at the screen edge. At 0.09 ms for 250–300 mobs
  it is no longer worth that.
- **Writing straight into the upload buffer.** It saves one copy of the
  2 MB; instancing removes eleven twelfths of the data instead.

## The picture

Scripted captures of 106 mobs (one of each of the 74 kinds and 32 more herd
animals and monsters) standing on a platform, from three camera positions,
with the game paused and the animation clock held
(`--open-menu pause --animation-time 7.3`, new for this: idle animations,
clouds and water otherwise follow the wall clock even when paused, and two
captures of one build differed by 2,000–3,700 pixels).

| View | Before vs before again | Before vs after |
|---|---|---|
| high | 0 of 2,073,600 pixels differ | 23 differ, all single pixels |
| low | 0 | 46 differ: 17 one- or two-pixel-wide groups, and two patches of 15 and 10 pixels changing by at most 8 of 255 |
| side | 0 | 78 differ: 33 one- or two-pixel-wide groups, and one patch of 35 pixels |

The single pixels are box edges: the GPU now does the last step of the
transform, and its rounding moves an edge across a pixel centre here and
there. The patches are places where two boxes of one model have faces in
exactly the same plane, so which one shows was already decided by rounding.
The 35-pixel patch is the pufferfish, 11 by 5 pixels at that distance: its
belly box shares its four side faces with its body box
(`model/aquatic.rs`, `PUFFER`). Before, the body's yellow won the tie; now
the lighter belly band does. That is the one difference a person could see
in these captures, and it is a defect in the model that this change exposes
rather than fixes: the tie could go either way on another GPU. The two
fainter patches are the same effect between faces of one colour, which
changes only the per-texel noise.

Unit tests cover the rest:

- For every mob kind in six states (walking, hurt, burning, a baby, dying,
  and 180 blocks away), the instances expanded as the shader expands them
  equal the triangles the CPU path builds: identical texture coordinates,
  colour, light and material for every vertex, and positions within
  rounding.
- A single box is expanded in the same vertex order as before, and a
  translucent box is never instanced.
- A rebuilt mesh follows a mob's pose, colour, age and equipment, a mob
  behind the camera is left out and comes back unchanged, and a removed mob
  leaves nothing behind.
- The random-tick table equals the rule chain for every block state, and
  fire and lava (which were tried first and could fall through) match no
  later rule.

## Checks

`scripts/remote-check.sh --full` on the finished code (`e90b5d0` plus a
one-line test fix, committed with this record) passed on a Namespace Linux
instance: `cargo fmt`, both clippy runs with no warnings, 2,336 tests with
default features (21 ignored) and 2,131 without (20 ignored), none failing.
The WGSL validation test covers the new vertex shader, and the benchmark
runner's six gate tests pass. Local builds were used only to run the game.

Not checked: split-screen and LAN drawing were not run (their code path is
the same per-view build and upload, exercised only by the single-view runs
here). No capture was taken of armored mobs, players, name tags or leads
beside instanced boxes. The instanced path was run only on Metal on the M5.

## What remains

1. **Valid windowed runs.** Three runs per scene on AC power with the
   machine idle and the window visible, at the final commit. Until then
   PERF-02 stays Partial. On today's offscreen numbers the five ordinary
   simulation scenes clear both thresholds with 23–60 FPS to spare in 1%
   low, which is a reason to expect a pass, not a pass.
2. **Streaming (PERF-03) fails for its own reason.** p99 was 11.5 ms
   against a budget of 8.33 ms, and tick frames average 9.3 ms. A
   14-second `sample` of the after build on the streaming route puts 1,400
   of the tick's 1,641 samples (85%) in the mob update, and 1,234 of the
   tick's samples (75%) in `Generator::column`, 774 of them called from
   `spawning::spawn_list`. Spawning asks the terrain generator about a
   column, and on a miss in the generator's column cache that recomputes a
   whole chunk's density columns on the main thread. No
   frame exceeded 50 ms in either 30-second run, which is shorter than the
   gate's 180 seconds.
3. **The rest of the tick.** It is still 1.7–2.3 ms in a tick frame and up
   to 2.9 ms in the slowest. After the table, what the before profile
   leaves is random ticks' own loop and block reads, mob updates (about a
   fifth of the old tick), fluids and redstone contact boxes. No profile of
   the after build was taken in a static scene.
4. **Sign text and decorations.** `materials` still spends 0.46 ms a frame
   on entity geometry, mostly sign text rebuilt every frame (8% of the main
   thread before) and the item frame wall.
5. **The GPU bounds the average** at about 650 FPS offscreen at 1080p. That
   is not a gate problem, but it means further CPU savings on ordinary
   frames won't raise average FPS.
6. **Coincident faces in models.** The pufferfish belly, and any other
   model with two boxes in one plane, should be separated so the result
   doesn't depend on rounding.
