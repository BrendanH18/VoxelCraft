# Streaming re-run — 2026-10-20

[Roadmap](../roadmap.md#v08-scope-finish-m0-and-m1) · [Requirement inventory](../requirements.md) · [Stall fix](2026-10-17-streaming-stalls.md) · [M5 baseline](2026-10-16-m5-baseline.md) · [Summary](../../benchmarks/results/2026-10-20-5e9f025-m5-streaming/summary.json) · [Compressed raw frame samples](../../benchmarks/results/2026-10-20-5e9f025-m5-streaming/frame-times.json.gz)

Scope: the PERF-03 re-run of the streaming scene after the
[stall fix](2026-10-17-streaming-stalls.md) moved dungeon layout and geode
ticks to the workers. Run with
`caffeinate -dis python3 tools/benchmark_scenes.py --scenes streaming` on the
M5 MacBook Pro (10 cores, 16 GB), clean tree at `5e9f025`, runtime source
SHA-256 `9b488571…`, 1920 × 1080 physical, enhanced graphics, 256-block view
distance, 160-block simulation distance, 10 s warm-up and 180 s samples.

## Results

| Run | Valid | Frames | p50 | p99 | Worst | Frames > 50 ms | Peak RSS |
|---|---|---|---|---|---|---|---|
| 1 | **No**: 1,034 unpresented frames (window occluded) | 45,104 | 2.37 ms | 12.18 ms | 154.0 ms | 1 | 660 MiB |
| 2 | Yes | 51,457 | 3.48 ms | 5.54 ms | 22.5 ms | **0** | 302 MiB |
| 3 | Yes | 44,932 | 4.13 ms | 8.17 ms | 65.0 ms | **2** (56.5 ms at 105.3 s, 65.0 ms at 163.8 s) | 267 MiB |

Every run completed 3,600–3,601 ticks with no paused or discarded
simulation time, and every executable matched the measured source.

Against the [baseline](2026-10-16-m5-baseline.md)'s 451–453 frames over 50 ms
per run (worst 123–150 ms, in 53–54 bursts), the periodic stalls are gone:
the valid runs have zero and two isolated frames over 50 ms, not bursts.
The 910 ms frame from the invalid 2026-10-17 run did not recur.

## Verdict against the §6.1 streaming budget

The budget is no frame over 50 ms and p99 at most 8.33 ms.

- Run 2 passes (worst 22.5 ms, p99 5.54 ms).
- Run 3 **fails** on two isolated frames (56.5 and 65.0 ms); its p99 of
  8.17 ms passes.
- Run 1 is not evidence: the window was occluded during the run, most likely
  by the agent session's app taking focus.

**PERF-03 is not met.** Two things remain: find the cause of the rare
isolated 55–65 ms frames, then run three valid runs within budget.

## Remaining

- Profile the isolated hitches. They are too rare (two in 180 s) for
  `sample`'s 30 s window, so first add frame-phase timing to the frames over
  50 ms (upload, meshing hand-off, chunk insertion, entity load).
- Repeat the three-run scene with the window frontmost throughout.
