# v0.14 tag gate run: both suites pass at `45870304` — 2026-11-16

[Roadmap gate](../roadmap.md#stabilisation-gate-before-v017) · [§6.1](../roadmap.md#61-reference-configuration-and-frame-time-gate) · [2026-11-14 gate run](2026-11-14-gate-run.md) · [Documentation index](../README.md)

Both scene suites at the v0.14 tag candidate, after the
[streaming stall fix](2026-11-14-gate-run.md#fix). Every scene passes the
[120 FPS gate](../roadmap.md#61-reference-configuration-and-frame-time-gate)
offscreen, and no frame in either suite reaches 50 ms.

## Runs

`main` at `45870304` (clean tree), built by the runner with
`cargo build --release`, on the owner's M5 (10-core GPU, 16 GB, macOS
27.2.1) on AC power, with the display held awake by `caffeinate -d` and a
user-activity assertion (`caffeinate -u`). The owner was away and no other
work ran on the machine. The windowed suite ran first, from 10:55 to 11:33
local time, then the offscreen suite to 12:08; the screen stayed unlocked
throughout:

```sh
python3 tools/benchmark_scenes.py --output target/benchmarks/v0.14-tag-windowed
python3 tools/benchmark_scenes.py --offscreen --output target/benchmarks/v0.14-tag-offscreen
```

The runner judged all 48 runs valid (1920 × 1080, immediate presentation,
no unpresented frames, simulation ticks within tolerance, an executable
built from the measured source) and both suites' `reference_gate_passed`
true. No run discarded simulation time. Summaries and per-frame samples
are kept in
[2026-11-16-4587030-m5-offscreen](../../benchmarks/results/2026-11-16-4587030-m5-offscreen/)
and [2026-11-16-4587030-m5-windowed](../../benchmarks/results/2026-11-16-4587030-m5-windowed/).

Cells are 1% low FPS / p99 ms (average FPS; worst frame ms).

### Offscreen suite (the gate)

| Scene | Run 1 | Run 2 | Run 3 |
|---|---|---|---|
| survival | 334.8 / 2.86 (544; 11.4) | 332.6 / 2.86 (544; 11.8) | 290.5 / 3.30 (532; 14.0) |
| building | 279.1 / 3.46 (514; 7.6) | 282.4 / 3.45 (515; 5.4) | 314.4 / 2.99 (520; 7.7) |
| entities | 294.5 / 3.17 (497; 15.6) | 313.1 / 2.98 (511; 10.0) | 308.8 / 3.11 (506; 5.2) |
| automation | 324.2 / 2.98 (524; 5.5) | 282.4 / 3.45 (518; 5.3) | 281.6 / 3.43 (513; 6.0) |
| materials | 267.6 / 3.29 (508; 8.5) | 259.3 / 3.39 (500; 7.8) | 259.9 / 3.33 (504; 8.0) |
| streaming | 227.5 / 3.20 (554; 13.1) | 237.3 / 3.02 (566; 14.7) | 237.9 / 2.99 (567; 14.7) |
| menus | 400.8 / 2.42 (687; 5.0) | 401.5 / 2.42 (687; 5.3) | 399.6 / 2.42 (687; 6.1) |
| title | 3654.0 / 0.25 (4475; 30.0) | 3721.4 / 0.26 (4295; 0.7) | 3733.0 / 0.26 (4306; 0.6) |

Every run meets the 120 FPS 1% low and 8.33 ms p99, with the lowest 1% low
227.5 FPS and the highest p99 3.46 ms. Streaming now passes: its worst frame
is 14.7 ms, against 293–298 ms at `1ee87eb1`.

### Windowed suite (stalls only)

| Scene | Run 1 | Run 2 | Run 3 |
|---|---|---|---|
| survival | 103.6 / 5.10 (522; 21.6) | 107.2 / 8.49 (479; 16.1) | 101.0 / 8.55 (401; 21.9) |
| building | 103.2 / 8.22 (495; 21.5) | 132.9 / 3.92 (513; 20.9) | 128.7 / 3.97 (512; 21.3) |
| entities | 131.5 / 3.75 (497; 22.6) | 126.5 / 3.97 (504; 23.1) | 126.0 / 3.92 (514; 21.2) |
| automation | 124.9 / 4.04 (510; 23.4) | 101.6 / 8.35 (490; 21.9) | 100.1 / 8.42 (462; 23.3) |
| materials | 100.7 / 8.27 (469; 22.9) | 127.9 / 4.30 (488; 22.7) | 127.1 / 4.28 (485; 23.0) |
| streaming | 122.7 / 4.66 (541; 24.0) | 104.5 / 8.21 (522; 23.1) | 99.9 / 8.49 (380; 22.7) |
| menus | 98.6 / 8.52 (405; 22.1) | 102.7 / 8.11 (561; 23.1) | 140.2 / 3.20 (571; 22.3) |
| title | 99.2 / 8.35 (528; 22.7) | 99.4 / 8.25 (547; 20.8) | 146.4 / 2.81 (570; 23.1) |

No frame exceeds 24.0 ms, so every scene passes the 50 ms stall check,
streaming included. The 1% lows sit at 98.6–146.4 FPS because of the
drawable waits described in the [drawable-wait record](2026-11-14-drawable-wait.md),
as before: the slowest 1% of frames spend most of their time in `acquire`.

## Comparison with the previous run

Against the [2026-11-14 offscreen suite](2026-11-14-gate-run.md#offscreen-suite-the-gate)
at `1ee87eb1`, streaming improved (1% low 201.8–215.0 → 227.5–237.9 FPS,
worst frame 298 → 15 ms) and menus, title and materials are within run to
run noise. Six runs are slower: survival run 3, building runs 1 and 2,
entities run 1 and automation runs 2 and 3, with 1% lows of 279–295 FPS
against 322–341 and p99 of 3.17–3.46 ms against 2.85–2.98. §6.1 asks for a
profile and a reason for any such drop.

The frame phases of the slowest 1% place all of it in `present`, which
offscreen is the wait for the GPU to finish the frame:

| Run | 1% low | Frame | Present | Ticks | Streaming | Draw |
|---|---|---|---|---|---|---|
| building, 11-14 (three runs) | 324–327 | 3.06–3.08 | 2.14–2.20 | 0.39–0.46 | 0.05–0.07 | 0.26–0.28 |
| building runs 1–2, 11-16 | 279–282 | 3.54–3.58 | 2.74–2.81 | 0.22–0.31 | 0.07–0.09 | 0.27–0.31 |
| automation, 11-14 (three runs) | 322–323 | 3.10–3.11 | 2.09–2.15 | 0.36–0.56 | 0.05–0.16 | 0.27 |
| automation runs 2–3, 11-16 | 282 | 3.54–3.55 | 2.78–2.84 | 0.25–0.32 | 0.04–0.07 | 0.26 |
| survival run 3, 11-16 | 290 | 3.44 | 2.70 | 0.20 | 0.18 | 0.22 |

(milliseconds, means over the slowest 1% of frames)

The CPU phases are the same or smaller, and the other runs of the same
scenes match 11-14. The one source change between the two commits is #87
(`src/world/{loot,maps,mod,spawner}.rs`): generation workers now roll
seeded structure chests and draw their explorer maps, and the main thread
only numbers the maps and installs the chests. That work happens when a
chunk is generated and installed, and adds nothing to the renderer or its
GPU submissions. The slower runs wait about 0.6 ms longer for the GPU, and
survival and automation each also have a run at the 11-14 speed. The likeliest
reason is the order: on 11-14 the offscreen suite ran first on a cool
machine, while today it followed 37 minutes of windowed rendering. That
hasn't been tested by running both commits in the same order, so a code
cause isn't ruled out. Every run still clears the gate's 1% low by more
than twice and its p99 with 4.9 ms to spare.

## Status

- Both suites pass at `45870304`, the v0.14 tag candidate, and the
  streaming stalls are gone in both.
- PERF-02 and PERF-03 meet their acceptance conditions on this run, and
  the owner marked both Verified (2026-11-16). v0.18 measures the gate
  again.
- The owner's play of a new generation 3 world remains before the tag.
