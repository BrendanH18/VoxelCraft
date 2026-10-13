# Benchmark harness review fixes — 2026-10-13

[Roadmap](../roadmap.md) · [STR-01](../restructuring.md#str-01--reference-fixtures-and-performance-scenes) · [Corrected M5 recording](2026-10-13-m5-baseline.md) · [Scene specification](../../benchmarks/scenes.json) · [Development guide](../development.md#live-performance-scene-suite)

These fixes address confirmed review findings in the live scene harness. **No
new measurement was taken.** The full suite was not run because other builds
were running on the machine. The earlier M5 recording is invalid and must be
re-measured.

## Changes

| Finding | Fix |
| --- | --- |
| A paused simulation was undetected; streaming run 3 recorded 0 ticks | `FixedClock` now counts paused wall time, including dropped fractional time. Reports record expected and completed ticks, `paused_simulation_ms` and `simulation_ticks_within_tolerance`. A run is valid only with zero paused time and ticks within max(2, 0.5%) of 20 per sampled second. |
| Stray input or focus loss could pause a run | Benchmark runs ignore keyboard, mouse, wheel, cursor, modifier and focus events, raw mouse motion and controllers. They do not grab the cursor. The CLI rejects `--benchmark-scene` with menus, the console, LAN, agents or split screen. |
| Samples included hidden or occluded windows (found during this review) | The renderer counts frames skipped because no surface image was available. `unpresented_frames` must be 0. |
| Autosave fired during samples | Autosave is disabled for every scripted run. `save()` does nothing in a benchmark, and closing a benchmark window no longer saves. Screenshot runs still save only when their window is explicitly closed, as before. |
| The entity scene lost its population (148–150 of about 208) | The scene holds dawn (`day_time` 0.0; daylight 0.35, below the 0.45 sunburn threshold) with `doDaylightCycle` and `doMobSpawning` off, so the specified 160 mobs neither burn nor are diluted. Reports record `entities_start`, `entities_end` and `expected_population`. Drift beyond 5% makes the run invalid. |
| GPU memory reported 0 when unavailable | Device-allocator totals (`gpu_memory_allocated_bytes`, `gpu_memory_reserved_bytes`) are `null` without an allocator report. Chunk-arena used and reserved bytes are read directly from the arena, not from per-frame statistics. |
| Route cameras stayed at spawn height | Survival and streaming cameras fly at the generator's surface height plus 24 blocks. Height is bilinearly interpolated from 8-block `Generator::column` samples, taking the maximum from 16 blocks behind to 32 ahead and never below sea level. Cameras face the direction of travel, with a -0.2 rad pitch. |
| The gate ignored graphics; options were incomplete; present mode was hard-coded; the fingerprint missed build inputs | The configuration check requires enhanced graphics and an uncapped resolved present mode. Reports record the requested and resolved modes; wgpu exposes only the requested `Auto*` value, so the resolved mode follows wgpu's documented fallback over the surface's supported modes. Reports also record the relevant options. The runner checks graphics, pause, unpresented frames, ticks and source match per run. It writes `gate_problems`, `reference_gate_problems` and `all_runs_valid`. The source fingerprint now includes `packaging/icons/*`. |

## Tests

- `simulation::tests::paused_and_discarded_time_are_accounted_separately`.
- `performance::tests` covers flag parsing (all scene names, invalid names
  and duration/warm-up limits). It also covers eligibility: paused and
  tick-short runs, population drift, hidden windows, configuration options
  and the scene specification's agreement with the runtime constants.
- `render::tests::auto_present_modes_resolve_like_wgpu` and
  `app::benchmark::tests::routes_face_their_direction_of_travel`.
- `python3 -m unittest tools/test_benchmark_scenes.py` covers the runner gate,
  including the recorded paused streaming run, legacy reports, each per-run
  requirement, protocol requirements and fingerprint inputs.

## Smoke check (not a measurement)

`python3 tools/benchmark_scenes.py --quick --repeats 1 --scenes entities --output <temporary folder>`
ran end to end on the M5 while other compilers were active. It held 160 of
160 mobs and recorded 0 paused milliseconds and 0 unpresented frames.
The present mode was `AutoNoVsync`, resolved to `Immediate`. Device-allocator
memory was `null` on Metal, and the chunk arena was nonzero. The run was
correctly marked invalid for its protocol length and for a 1.34 s load hitch:
78 of the 100 expected ticks, with 1,074 ms discarded. Its median frame was
8.333 ms, which suggests that presented frames may be paced at 120 Hz even with
`Immediate` on this display. Check this during the real run, because a
120 Hz present cap would make the 240 FPS gate unreachable.

## Remaining measurement

Re-measure on an otherwise idle M5. No concurrent builds or GPU work should run,
and the game window must stay visible and unobstructed:

```sh
CARGO_TARGET_DIR=/path/to/cache python3 tools/benchmark_scenes.py --repeats 3 --output target/benchmarks/m5-baseline-v2
```

Accept the result only if `summary.json` reports `all_runs_valid: true`.
Record `reference_gate_problems` with the results.

## Proposed roadmap updates

`docs/roadmap.md` was not edited. Proposed replacement for the paragraph
beginning "The [full initial M5 baseline]":

> The [first M5 scene recording](verification/2026-10-13-m5-baseline.md) is
> invalid: streaming run 3 was paused (0 ticks), 11 of 12 non-streaming runs
> sampled an occluded window rather than rendering, and the entity scene lost
> its undead to daylight. The [harness fixes](verification/2026-10-13-review-bench-fixes.md)
> reject paused, tick-short, unpresented and population-drifting runs, isolate
> benchmark input and disable saves. The five-scene 1080p/256-block baseline
> must be re-measured; no valid baseline or frame-time result exists yet.
> Native resolution, profiling, hitch budgets and the broader encounter matrix
> remain open; the performance target is not achieved.

M0 should remain open until a valid re-measured baseline exists. Its exit asks
for "a reproducible baseline".
