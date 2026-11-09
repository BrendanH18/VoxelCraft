# Structure terrain blending investigation — 2026-11-09

**Stopped at the owner's explicit stop condition; no terrain changes delivered.**
A Java-shaped finite beard did not satisfy the requested ground/pillar/cut
bounds with generation 3's current placements. Shipping it would leave the
reported visible problem and add a new native-density approximation. The only
delivered code is a reusable device-free benchmark/location example; generation,
placement, foundations, clearing, IDs, saves and networking remain unchanged.

Baseline source: `c96240b8c9c2f4feb4317a6533791fe9ef5b0620`.

## Reference inspected

The owner's local Minecraft Java Edition 26.3 server class artifact, inspected
with `javap -p -c`: `Beardifier.forStructuresInChunk`, `sampleValueUnchecked`,
`getBeardContribution`, `getBuryContribution`, kernel initialization, and
`JigsawPlacement$Placer.tryPlacingChildren`. No artifact, class dump or
machine-local reference path is included here.

Java's `beard_thin` uses a rigid piece's bounding box and ground-level delta,
with horizontal distance to the box and signed vertical distance from its ground
plane. The contribution is zero outside the 24³ kernel: offsets -12 through 11.
Inside it, the Gaussian is `exp(-(dx²+(dy+0.5)²+dz²)/16)` multiplied by the signed
inverse-distance factor. Piece weight is 0.8, junction weight 0.4. Terrain-
projected pieces contribute their junctions rather than rigid boxes.

`bury` is `max(0, 1-length(dx,dy/2,dz)/6)`; `encapsulate` uses 0.8 times that
function with all three distances to the piece box halved. `beard_box` uses
vertical distance to the box for the Gaussian and signed ground distance for
its sign factor. These modes do not provide an unlimited terrain leveller.

## Experiment and failure

The uncommitted prototype kept structure placement on unadapted seed-only
terrain and created a separate bounded adapted-column cache. It summed the
Java-shaped contributions at block resolution after the native 4×8×4 noise
interpolation, before surface rules and decoration. Region lookups included the
assembler extent, kernel and column ring. Foundations remained a fallback and
village/outpost clear-above painting was disabled.

The native density measures distance in blocks rather than Java noise-router
units. The experiment used `clamp(native_density/32,-0.25,0.25)` where a beard
contribution was nonzero, and left native solidity unchanged elsewhere. That
conversion is an experiment, **not an approved FE-01 extension or parity claim**.
Its kernel used exact square root instead of Java's fast inverse square root.

The first prototype was probed remotely with:

```sh
cargo test --release --no-default-features world::terrain::beard -- --nocapture
```

Remote release test results for that prototype:

| Check | Result |
| --- | --- |
| Kernel sign, centre value, support and 0.8-weight ring inequalities | Passed |
| Sloped village/outpost geometry across seeds 42, 12345, 2024 | Failed on the first village; subsequent fixtures not reached |
| Seed 42 nearest village | 22 pieces; 974 examined columns |
| First template origin | (-384,70,-784) |
| Maximum original ground/floor difference | 19 blocks |
| Maximum blended first-ring ground/floor difference | 19 blocks |
| Maximum foundation gap in examined footprints | 0 blocks |
| Maximum adjacent terrain step in examined footprint/ring | 9 blocks |
| Requested regression bounds in the experiment | Foundation gap ≤2, ring difference ≤3, adjacent step ≤3 |
| Focused test total | 1 passed, 1 failed; 1815 filtered out |

These were pre-decoration terrain-height diagnostics, not a completed check of
actual foundation block columns, ordinary play, persistence or visual quality.
The bounds were not increased after failure. A single piece's weighted kernel
at first-ring offset `(1,-2,0)` exceeds +0.25 and at `(1,1,0)` is below -0.25,
which supplies a local smoothing expectation independent of the existing
output. It does **not** constrain the highest terrain surface: a hill above the
kernel can remain above the locally cleared space. The 19-block ring difference
exposes exactly that distinction. The first experiment's footprint check
covered ground-plane rigid pieces, excluding upper rigid pieces with inherited
ground planes; it is
sufficient to falsify the proposal, not evidence of complete feature coverage.

Generation 3 projects a start at its centre and rigid children from connectors;
it does not ensure their surrounding terrain falls inside the beard's vertical
reach. The native FE-01 density and steep slopes also differ from Java's.
Multiplying the kernel or changing density units cannot affect a sample where
the kernel is zero. Applying `beard_box` to villages, widening the kernel,
forcing a target height across a ring, or retaining a tall clear-above carve
would each introduce a different, unapproved adaptation policy.

The prototype was removed from the delivered tree. Its patch and raw logs remain
under ignored `target/structure-blending/` in the implementation worktree for
inspection; they are not repository requirements, assets or shipped code.
Ancient cities retain their current coupled carve-and-fill/sculk painter;
trail ruins and trial chambers also retain their current behavior.

## Smallest alternative requiring a scope decision

First constrain rigid-piece placement using unadapted footprint **and ring**
terrain, from the seed alone. Select a floor whose surrounding heights fit the
kernel's reach; reject that candidate piece or start when no such floor exists.
The reach test is necessary but insufficient: validate the predicted combined
density surface over each footprint and ring, including other pieces, so no
upper terrain shelf survives above a locally cleared band. Then apply the Java
kernel before the existing surface rules and feature placement. This changes
generation-3 layouts/availability on steep terrain, but
can keep the existing generation formats, region cache model and shipped
V1/V2 outputs. It requires evaluating reach across connected pieces and water,
then proving terrain, junction, content/loot and GEN-07 behavior together.

If retaining every existing generation-3 layout is required, the alternative is
an explicit native-density terrain envelope that blends each floor into its
surroundings. Its reach must grow for larger height differences or it will make
steep outer walls. That is a broader FE-01 exception, not Java's finite beard,
and should be approved before implementation. Neither alternative was silently
substituted in this task.

## Headless baseline and final checks

`examples/structure_terrain_bench.rs` uses fresh generators on each of five
runs, 300 chunks per run (25 horizontal columns × all 12 vertical chunks), seed
12345. Locate runs outside the timer on a separate generator. The village
locates at (320,81,-192), chunk (10,-6); open terrain centres on chunk (0,0).

Baseline on one Namespace Linux 8-vCPU/16-GB instance:

| Scene | Five runs, ms/chunk | Mean ms/chunk |
| --- | --- | --- |
| Origin (`open` scene label) | 1.003, 0.988, 0.987, 0.958, 0.980 | 0.983 |
| Village area | 0.925, 0.949, 0.936, 0.898, 0.895 | 0.921 |

The origin fixture was not exhaustively proven free of underground structures;
this is a baseline, not the requested structure-free overhead acceptance check.

No valid before/after performance comparison is claimed: the prototype failed
correctness and was withdrawn before final timing. Do not interpret the
baseline as M5 streaming or frame-time evidence.

Final `scripts/remote-check.sh --full` passed on the delivered tree:

| Configuration | Passed | Failed | Ignored |
| --- | ---: | ---: | ---: |
| Default features (unit, binary, integration and doc tests) | 2331 | 0 | 21 |
| `--no-default-features` (unit, integration and doc tests) | 2126 | 0 | 20 |

Formatting and both release clippy runs (`--all-targets -- -D warnings`) passed.
V1 `generated_chunk_hashes_stay_identical` and V2
`generation_2_chunk_hashes_stay_identical` passed unmodified in both
configurations. The original GEN-07 corpus and existing structure loot/resident
and persistence regressions passed; no new adaptation/order acceptance is
claimed. The final-source location example completed remotely and reproduced
all three anchors below. New relative documentation links, Markdown fences and
`git diff --check` were checked. These passing results apply to the unchanged
generator plus the benchmark tool, not the withdrawn prototype.

## Views and remaining evidence

`cargo run --release --no-default-features --example structure_terrain_bench -- --locations`
computes the following anchors without generating neighbours or running the game:

| View | Seed | Locate (x,y,z) | Headless layout bounds, min → max | Proposed `--pose` |
| --- | --- | --- | --- | --- |
| Desert village | 7 | (-192,0,-272) | (-230,62,-303) → (-168,85,-242) | `-232,100,-232,-45,-28` |
| Taiga village on sloped terrain | 42 | (-896,0,912) | (-922,67,876) → (-870,77,919) | `-936,95,952,-45,-25` |
| Outpost on sloped terrain | 42 | (-232,97,1096) | (-251,81,1087) → (-223,114,1115) | `-266,125,1130,-45,-34` |

Common capture arguments (append the row's `--seed` and `--pose`):

```text
--new --creative --mute --no-hud --no-update-check --rd 4 --size 1920x1080 --wait 5 --time 0.25 --weather clear
```

Use distinct `--world` names and ignored capture directories, for example:

```text
--seed 7 --pose -232,100,-232,-45,-28 --world terrain-blend-desert --data-dir target/structure-blending/views/desert --screenshot target/structure-blending/views/desert.png
--seed 42 --pose -936,95,952,-45,-25 --world terrain-blend-taiga --data-dir target/structure-blending/views/taiga --screenshot target/structure-blending/views/taiga.png
--seed 42 --pose -266,125,1130,-45,-34 --world terrain-blend-outpost --data-dir target/structure-blending/views/outpost --screenshot target/structure-blending/views/outpost.png
```

These are proposed diagnostic captures of the remaining problem, **not views of
a delivered terrain change**. Camera direction was checked against the existing
`Player::forward` convention; framing was not visually verified. No game process,
local build, computer-use session or screenshot was started. No visual,
owner-play, new blend persistence or new blend performance acceptance is claimed.
Existing generation hash fixtures were not edited or regenerated.

## Proposed roadmap / requirement row changes

Keep the current village/outpost terrain-adaptation gap open under GEN-05b/e.
Link this record as a failed finite-kernel-only approach, and split the next work
into a placement-envelope scope decision followed by density adaptation. Keep
GEN-07, FE-01, ancient-city `beard_box`, trail-ruin `bury` and trial-chamber
`encapsulate` acceptance unchanged. Do not mark terrain blending implemented or
Verified from this investigation.
