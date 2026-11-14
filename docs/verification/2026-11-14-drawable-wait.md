# Where the slow frames go: drawable waits, not game work — 2026-11-14

[Roadmap gate](../roadmap.md#stabilisation-gate-before-v017) · [2026-11-11 gate run](2026-11-11-gate-run.md) · [Documentation index](../README.md)

The [2026-11-11 gate run](2026-11-11-gate-run.md#scene-run) failed the
240 FPS gate because its slowest frames spent 7–8 ms waiting to acquire a
drawable. It recorded no GPU time, so whether the GPU or the window
compositor held the drawables was open, and the steady-state scenes were
below the [2026-10-16 baseline](2026-10-16-m5-baseline.md) with no profile.
This record answers both with the GPU timing added in
[#82](2026-11-11-gate-run.md#follow-up-instrumentation-and-aiming-commands).

None of these runs is gate evidence. Each is one 30-second sample, and the
owner was using the Mac (a browser was playing video), so the machine was
not idle. They compare configurations run back to back on the same
machine. The summaries are kept in
[benchmarks/results/2026-11-14-931d6a6-m5-diagnostics](../../benchmarks/results/2026-11-14-931d6a6-m5-diagnostics/).

## Build

`main` at `931d6a6` (source identical to the v0.14 candidate; only
documentation changed after it), `cargo build --release` on the owner's M5
(10-core GPU, 16 GB, macOS 27.2.1), on AC power with the display held awake.
The no-paging build below changed only three shaders and was thrown away
after the runs.

## Windowed: the GPU is idle while the frame waits

`python3 tools/benchmark_scenes.py --scenes entities,automation --repeats 1
--seconds 30`, plus `--frame-latency 1` and `--window-mode exclusive`.
Cells are 1% low FPS / p99 ms (average FPS).

| Configuration | entities | automation |
|---|---|---|
| Default (windowed, latency 2) | 52.0 / 17.62 (238) | 52.1 / 17.67 (236) |
| Frame latency 1 | 71.9 / 11.54 (220) | 101.0 / 9.37 (316) |
| Exclusive fullscreen | 110.0 / 8.84 (120) | 111.3 / 8.80 (120) |

- In the default runs the slowest 1% of frames took about 19.2 ms, of which
  18.4 ms was waiting to acquire a drawable, two 120 Hz refreshes. The game's
  own work in those frames (ticks, entities, streaming, draw recording) was
  under 1 ms. About 2,200 frames of 7,100 in each scene waited more than
  4 ms; they waited 8.4–8.6 ms on average while their GPU work averaged
  1.9 ms (p99 2.7–3.4 ms, 62 frames timed). The GPU was done long before
  the drawable came back.
- Frame latency 1 (two drawables instead of three) doesn't help: the GPU
  waits more and the average falls in one scene.
- Exclusive fullscreen pins every frame to the 120 Hz refresh (average
  exactly 120 FPS) even though Immediate presentation was requested, and its
  display mode wasn't 1920 × 1080, so it is not a way to pass the gate.
- These windowed figures are worse than on 2026-11-11 (two refreshes of
  waiting rather than one), which fits a busier compositor: the video was
  being composited at the same time.

## Offscreen: the game's own frame cost passes

`--offscreen` draws into an offscreen image of the same size and presents
nothing, so a frame takes the longer of the CPU's and the GPU's time and
the compositor plays no part. Same four scenes for each build, in the order
current, no paging, current.

| Scene | Current, first | No paging | Current, second |
|---|---|---|---|
| building | 130.5 / 3.75 (585) | 343.3 / 2.60 (637) | 342.9 / 2.64 (626) |
| entities | 351.6 / 2.69 (592) | 339.7 / 2.63 (607) | 336.0 / 2.68 (566) |
| automation | 225.3 / 3.31 (604) | 346.3 / 2.59 (638) | 307.1 / 3.03 (621) |
| survival | 358.3 / 2.21 (642) | 366.5 / 2.16 (660) | 367.5 / 2.11 (648) |

- The second current-build pass is at or above the gate in every scene:
  1% low 307–368 FPS and p99 2.1–3.0 ms, against 240 FPS and 4.17 ms. The
  first pass's building and automation runs had long frames that didn't
  repeat; the machine was in use, so treat one-off outliers as noise until
  an idle run says otherwise.
- **Texture paging is not the regression.** The no-paging build samples
  every block from the first 2,048-layer page with `textureSample` (wrong
  textures, the cost of an unpaged adapter) instead of the paged
  `textureSampleGrad` switch added with the v0.12 capacity raise
  (`05831cda`). It is within noise of the current build: building 343.3
  against 342.9, survival 366.5 against 367.5. GPU time per timed frame
  was 1.3–1.7 ms with paging off and 1.4–2.6 ms with it on, but few frames
  were timed (18–211 a run), so that difference isn't established.

## What this means for the gate

- The game's own work fits the gate with room to spare. The windowed
  failures are the macOS window compositor returning drawables once per
  120 Hz refresh, which a game presenting through wgpu can't avoid: wgpu's
  acquire blocks, Metal allows at most three drawables, and exclusive
  fullscreen is held to the refresh rate.
- The steady-state drop from the 2026-10-16 baseline is in that wait, not
  in the game. The game's own work is about 0.6 ms of a 1.9 ms average
  frame in the 11-11 runs; the rest is acquire.
- In ordinary play the game is capped at the display rate already: Max
  Framerate defaults to 120, as in Java. The wait matters only to uncapped
  benchmark runs.

The gate as written in [§6.1](../roadmap.md#61-reference-configuration-and-frame-time-gate)
measures presented frames in a window, so it can't pass on this machine
while the compositor behaves this way. Changing what it measures (for
example the game's own frame time, or treating drawable waits like the
display-pacing stretches ruled out on 2026-10-17) is the owner's decision.
§6.1 already lets the owner tag a version that misses the gate once it has
been measured.

Still owed before the tag: a full windowed scene run (three runs of every
scene) on the idle M5 at the commit to be tagged.
