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

## Round 2 (2026-11-09)

The integrator approved alternative 1 under the owner's standing instruction to
fix this: constrain generation-3 placement, then add the finite Java-shaped
kernel. Generation 3 is unreleased; its layouts and availability may change.
This section supersedes round 1's no-delivery status for this new work only;
round 1's investigation above is retained. Parent: `3468db642354eea9709bbaa2158ade97be626930`.

### Placement and density design

Village and outpost assembly use seed-only, unadapted columns throughout.
The existing surface sampler retains its waterline clamp for submerged ground.
For each rigid footprint and a two-block ring, intersect the ground-plane
intervals `[surface-6, surface+6]`. Derive the physical template floor using
its ground-level delta. This is deliberately narrower than the kernel's
vertical support: reaching a hill is not sufficient to remove an upper shelf.
Try admissible heights nearest the ordinary centre/connector preference first,
with lower heights breaking ties. Rigid-to-rigid joins retain their alignment.
A rigid child of a terrain-following parent reads the heightmap at the parent's
connector, including vertical outpost feature-plate joins.

Before accepting a candidate, evaluate the combined rigid-box and junction
contributions against the native density. Check its footprint/ring and those
of affected existing rigid pieces: support gap at most 2, first-ring error at
most 3, adjacent terrain step at most 3. The physical template floor is the
reference, including the upper outpost pieces; the inherited Java ground plane
is used only for the density terms. Also constrain rigid physical floors whose
boxes are within two blocks on both horizontal axes to differ by at most one:
template floors and air replace density, so a density-only check cannot protect
a neighbour's ring from that replacement. No acceptance tolerance was raised.
Failed floor trials proceed through the engine's existing rotations, weighted
pool entries and fallback. A whole start is rejected only if its selected start
piece has no acceptable floor; small remaining villages are retained and counted.

Original outpost feature plates contain connectors but no blocks. The bounded
assembler allows their own vertical child to overlap that empty parent's
reserved box after heightmap projection; every other free-space reservation
still applies. Otherwise the placeholder box would suppress cages and residents
on flat ground. This exception is limited to the adapted assembler.

Adaptation discovers village/outpost layouts through bounded placement-region
caches, including the assembler extent, kernel and column ring. A separate
bounded cache holds immutable adapted columns. It copies the cached base noise
field rather than computing it again. No neighbour chunks are generated, and
layout construction never reads the adapted cache. Kernel values are tabulated
once. The contribution is summed per block after native noise interpolation,
before surface rules, carvers and features; positive support density is protected
from later carvers. Both directed junctions are retained on the child without
adding random draws. Existing non-adapted assembly keeps its random sequence.

Village/outpost clear-above painting is disabled. Their existing foundation
painter fills remaining air/water as a fallback. Streets, trees, large ground
features and surface plants read adapted heights. Tree/large-feature origins
avoid building footprints plus canopy reach and check seed-only cave/ravine
support; small plants at a vertical chunk boundary also check their support.
This prevents cross-chunk placement from assuming an uncarved surface below.
A whole-tree clearance check also rejects crowns obstructed by terrain in
beard-influenced columns; it reads the complete tree shape from seed-only
terrain masks, with a small per-tree column cache. Intentional embedded roots
at/below the tree's ground remain allowed. Clearance ignores other trees and
never depends on already-generated chunks.

### Departures from Java and scope

The floor envelope, floor search, neighbouring-floor constraint, combined-field
placement validation and empty-parent reservation exception are generation-3
placement policies for the original templates/native terrain, not Java layouts.
The original templates use a layer-zero ground delta of zero; directed junctions
and rigid-child deltas are carried explicitly. Template catalogues, random
sequence, priorities and the expansion hack retain their documented differences.

The kernel has Java's 24-cube support (offsets -12 through 11), 0.8 rigid-box
weight and 0.4 junction weights. The Gaussian uses double precision then rounds
to float; inverse square root is exact rather than Java's fast approximation.
Where the summed beard is nonzero, native block-distance density becomes
`clamp(native_density/32, -0.25, 0.25)` before addition. This is a native-unit
conversion, not Java's noise router. Neither its kernel nor a separate density
envelope is widened. Outside finite support, native solidity is retained.
This is an explicit FE-01 difference, not a full Java terrain-parity claim.
Protecting positive-beard support from native caves/ravines is also a native
generation policy; it is not a port of Java's carver pipeline.

Ancient-city `beard_box` remains a gap (the existing coupled carve/fill/sculk
painter is unchanged). Trail-ruin `bury` and trial-chamber `encapsulate` remain
gaps and retain their existing painters. No IDs, save schemas, update keys,
container precedence or LAN protocol fields were changed. Generations 1 and 2
continue through the original paths; their golden fixtures were not edited.

### Availability

| Seed | Village starts before → after | Mean pieces/village before → after | Outpost starts before → after | Removed/shrunk villages | After below two houses |
| --- | ---: | ---: | ---: | ---: | ---: |
| 42 | 340 → 265 | 28.106 → 25.242 | 53 → 39 | 82 | 10 |
| 12345 | 347 → 277 | 27.571 → 24.892 | 43 → 25 | 74 | 9 |
| 2024 | 309 → 249 | 27.405 → 24.948 | 50 → 36 | 65 | 12 |
| Total | 996 → 791 | 27.702 → 25.027 | 146 → 100 | 221 | 31 |

205 of 996 village starts disappear (20.6%). Removed starts plus all remaining
villages below two houses total 236/996 (23.7%); the per-seed values are 25.0%,
22.8% and 23.3%. These are below the one-third stop threshold. The mean is
computed over extant villages, including small ones.

The census is a fixed square of placement cells, x/z -22 through 22, on each
seed: 2,025 cells per seed, 6,075 per structure family. The paired before path
uses the unchanged assembler and start eligibility; the after path uses the
region-cached generated layout. A house is a template with homes, so a medium
house with two beds still counts as one house. Counts below two houses are
reported separately, including already-small villages. The stricter total of
removed starts plus *all* remaining villages below two houses also stays below
one third. No bound was loosened to recover availability.

Reproduce remotely:

```sh
scripts/remote-check.sh -- 'cargo test --release --no-default-features --lib terrain_envelope -- --ignored --nocapture'
```

### Generated-block and order evidence

Final-block regression limits: foundation pillar ≤2 except over water,
first-ring ground/floor error ≤3, and adjacent terrain step ≤3.

| Seed | Structure | Pieces | Examined footprint/ring columns | Tallest foundation | Maximum ring error | Maximum step |
| --- | --- | ---: | ---: | ---: | ---: | ---: |
| 42 | Village | 13 | 448 | 0 | 1 | 1 |
| 42 | Outpost | 8 | 580 | 1 | 3 | 2 |
| 12345 | Village | 29 | 1162 | 0 | 2 | 2 |
| 12345 | Outpost | 7 | 459 | 1 | 3 | 2 |
| 2024 | Village | 36 | 989 | 0 | 2 | 3 |
| 2024 | Outpost | 7 | 459 | 1 | 2 | 2 |

The acceptance test reads final generated blocks, excludes explicit upper
building cells when finding terrain, and recognizes coloured terracotta as
terrain. It compares pre-structure generated terrain blocks (including carvers)
with final foundation material so natural terracotta cannot be mistaken for a
pillar. Water exemptions apply only to foundation counts. These checks include
all rigid pieces, not just start pieces or predicted heights.

The reusable `--probe-village` example fails on the parent generator: seed 42's
nearest village has 22 pieces, 364 first-ring samples and maximum error 19.
The delivered generator is checked on all three seeds. Early diagnostic probes
omitted coloured terracotta and could mistake a geological band for a missing
surface; those results are superseded by the corrected block tests. Surface
material assertions apply to surviving soil, excluding cave openings and
covering boulders/logs; small-plant support is checked separately.

The historical street/house overlap regression originally located two specific
review starts that the new envelope can reject. Its exact original layouts and
feature positions are retained as unbounded painter fixtures, with the same
chest/bed/job-site assertions; it additionally checks actual generated nearest
villages on seeds 12345 and 2024. No shipped golden expectations were changed.

The new GEN-07 corpus includes the sloped village/outpost beard extents, including
chunks reached from outside the piece boxes, in reversed, parallel, independently
evicted and fully cold-cache runs. Existing order tests remain in place.

The focused diagnostic used release optimization with LTO disabled and 16
codegen units to shorten iteration (`CARGO_PROFILE_RELEASE_LTO=false
CARGO_PROFILE_RELEASE_CODEGEN_UNITS=16`): headless clippy passed, all four
beard/origin/material tests passed, and both ignored census tests passed.
Surface checks covered 1,045 changed desert and 629 changed taiga columns outside
templates. Every retained sampled outpost registered residents (39, 25, 36).
These diagnostic flags are not used for the hand-off chain or timings. A final
focused run also passed the additional whole-tree obstruction test (five beard
module tests total) and the historical/generated content regression. The census
is unchanged because canopy validity does not participate in layout assembly.

The final implementation passed `scripts/remote-check.sh --full` with the
normal release profile, including formatting and both clippy runs with
`-D warnings`:

| Configuration | Passed | Failed | Ignored |
| --- | ---: | ---: | ---: |
| Default features | 2,337 | 0 | 23 |
| `--no-default-features` | 2,132 | 0 | 22 |

Both runs passed the unchanged generation-1/2 block/feature golden fixtures,
the six-fixture final-block test, surface/plant and whole-tree checks, the new
sloped-structure GEN-07 test and the existing structure, persistence, loot,
resident, bed/job-site, spawn-override, LAN and input-adapter regressions.
The two ignored census tests were also run explicitly and passed on the final
implementation (2 passed, 0 failed). Cloud `cargo fmt --check`, Markdown fence
balance and relative documentation links passed separately. These automated
checks do not replace ordinary owner play or screenshot evidence.

### Headless cost

| Scene | Parent mean ms/chunk | After mean ms/chunk | Change |
| --- | ---: | ---: | ---: |
| Origin, original-scene phase | 1.0719 | 1.0867 | +1.4% |
| Original village anchor `(320,-192)` | 1.0172 | 0.9899 | -2.7% |
| Surviving village `(-208,400)` | 0.9215 | 0.9817 | +6.5% |

All three comparisons are on the same instance. The repeated origin in the
surviving-village phase measured 1.0851 → 1.0563 ms/chunk (-2.6%). The origin
changes sign between phases, so this variation is not evidence that an unchanged,
beard-free scene became faster; both away-scene comparisons are below the 5%
budget and the village cost is below 15%.
The original anchor has no retained village after the change, so the additional
surviving-village scene is the meaningful adaptation-cost check.

One optional measurement launch failed during the helper's existing package
setup with a transient repository error; it ran no benchmark. The unchanged
retry completed. No software was installed on the Mac and no additional
software was requested outside the remote-check helper.

The final comparison pins the village scene to block x/z `(-208,400)` on seed
12345 in both binaries (`--village=-208,400`). The old nearest village at
`(320,-192)` is rejected under the new placement rules, so independently locating
the nearest village in each binary would compare different scenes. The origin
is unchanged. The origin fixture is proven free of village/outpost beard
influence, not of every underground structure family.

Each timed run creates a fresh generator and generates 300 chunks: 25 horizontal
columns times all 12 vertical rows. Both binaries are built before four
alternating before/after pairs, five runs per scene per pair (20 runs per scene
per binary and village anchor; the origin is repeated in both phases). The
table uses those interleaved runs; preliminary relocated-scene
and cold post-build comparisons are not performance evidence. All measurements
are on one Namespace 8-vCPU/16-GB Linux instance (`87m54sls18vh0`). They are not M5
frame-time data.

Each variant's two scene commands (run four times, alternating the saved parent
and after binaries on one instance for the reported comparison):

```sh
cargo run --release --no-default-features --example structure_terrain_bench -- --village=-208,400
cargo run --release --no-default-features --example structure_terrain_bench -- --village=320,-192
```

Run commands through `scripts/remote-check.sh -- 'CMD'`; compiling both variants
in one remote command keeps the comparison on one instance. Preserve each binary
before rebuilding the other variant, and touch `build.rs` after switching source.

### Updated screenshot anchors

| View | Seed | Locate (x,y,z) | Layout bounds, min → max | Proposed `--pose` |
| --- | --- | --- | --- | --- |
| Desert village | 7 | (-192,0,-272) | (-225,62,-312) → (-169,86,-232) | `-246,108,-220,-45,-28` |
| Sloped taiga village | 42 | (-896,0,912) | (-922,67,876) → (-870,77,919) | `-936,95,952,-45,-25` |
| Sloped outpost | 42 | (1544,79,-2920) | (1535,67,-2939) → (1563,92,-2911) | `1510,116,-2886,-45,-34` |

Common capture arguments, followed by exactly one row below:

```text
--new --creative --mute --no-hud --no-update-check --rd 4 --size 1920x1080 --wait 5 --time 0.25 --weather clear
```

```text
--seed 7 --pose -246,108,-220,-45,-28 --world terrain-blend2-desert --data-dir target/structure-blending/round2/views/desert --screenshot target/structure-blending/round2/desert.png
--seed 42 --pose -936,95,952,-45,-25 --world terrain-blend2-taiga --data-dir target/structure-blending/round2/views/taiga --screenshot target/structure-blending/round2/taiga.png
--seed 42 --pose 1510,116,-2886,-45,-34 --world terrain-blend2-outpost --data-dir target/structure-blending/round2/views/outpost --screenshot target/structure-blending/round2/outpost.png
```

These are exact proposed capture arguments for the integrator, not screenshots
already taken. Framing has only been checked geometrically. No local build,
game process, screenshot or computer-use session was run by this agent.

### Remaining evidence and proposed row updates

Owner play, the integrator's before/after captures, M5 gameplay/frame times and
a new ordinary-play save/restart cycle remain unverified. Existing persistence,
loot, resident, bed/job-site, spawn-override and adapter regressions are reported
with the full checks above. The sampled geometry and census do not establish
all-seed or all overlapping-structure geometry parity. Independent neighbouring
starts can contribute to the same density field; placement validation checks
one assembled layout, while generation discovers all influencing layouts.

Proposed integrator edits (not applied to roadmap/requirements here): record the
approved unreleased generation-3 layout change and scoped village/outpost
adaptation under GEN-05b/e; add this evidence to GEN-07; record the finite-kernel
native density conversion explicitly under FE-01. Keep ancient-city `beard_box`,
trail-ruin `bury`, trial-chamber `encapsulate`, full Java layout/noise parity and
owner/M5 acceptance open. Do not mark the entire generation requirements Verified.
