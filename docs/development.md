# Development tools

[← Back to VoxelCraft](../README.md) · [Documentation index](README.md) · [Architecture](architecture.md) · [Contributing](../CONTRIBUTING.md) · [Agent guide](agent-guide.md)

Run commands from the repository root. See the README for platform prerequisites.

## Checks

Use the same checks as [CI](../.github/workflows/ci.yml):

```sh
cargo fmt --check
cargo clippy --release --all-targets -- -D warnings
cargo test --release
```

The Linux CI job also checks the release binary's embedded assets with
`python3 tools/validate_assets.py --executable target/release/voxelcraft` and
runs the benchmark-gate tool tests. It also runs
`python3 tools/check_doc_links.py` (and its tests,
`python3 -m unittest tools/test_check_doc_links.py`), which checks that every
relative link and `#anchor` in the tracked Markdown files resolves, using
GitHub's heading-anchor rules, and prints each broken one as
`file:line: target`. Documentation-only changes skip the Cargo chain; run
the link check instead. CI runs Cargo with `--locked`, so commit
`Cargo.lock` changes with the code that needs them. The **Security** workflow
runs `cargo audit --deny warnings` weekly and on pull requests that change `Cargo.toml`,
`Cargo.lock` or `.github/workflows/security.yml`. Dependabot proposes weekly
crate and GitHub Actions updates.

Linux Wayland title bars use winit's supported `wayland-csd-adwaita-crossfont`
backend with system FreeType and Fontconfig. X11, Wayland, window titles and
decoration controls remain enabled. This replaces the `ab_glyph` backend's
[unmaintained ttf-parser dependency](https://rustsec.org/advisories/RUSTSEC-2026-0192.html);
the [Linux build prerequisites](../CONTRIBUTING.md#set-up) include the native
font development libraries. Auditing denies warnings as well as vulnerabilities,
so unmaintained or unsound dependencies fail CI rather than silently passing.
The lockfile's Tokio dependency comes through `wasm-bindgen-futures` only for
its experimental Emscripten configuration; keep it patched even though desktop
builds do not use that path.

## Remote checks

`scripts/remote-check.sh` runs the checks on a [Namespace](https://namespace.so)
cloud instance instead of this machine, so several agents can check their
work at once without competing for the Mac's memory or cargo's build lock.
It needs the `nsc` command-line tool, installed and logged in
(`nsc login`).

```sh
scripts/remote-check.sh            # cargo fmt here, then clippy and tests remotely
scripts/remote-check.sh --full     # also the --no-default-features clippy and tests
scripts/remote-check.sh -- cargo test --release some_test   # any command
scripts/remote-check.sh -- 'cargo build --release && ls "$CARGO_TARGET_DIR"/release'  # one argument: a shell line
```

Each run creates a bare Linux instance (8 vCPU, 16 GB by default,
`VC_REMOTE_SHAPE` to change it), uploads the working tree with uncommitted
edits and untracked, non-ignored files, runs the command and destroys the
instance. The Rust toolchain, cargo registry and target directory live on
the 20 GB `voxelcraft-check` cache volume (`VC_REMOTE_CACHE_TAG`), so
dependencies build once and later runs start warm. On the current Namespace
plan (checked 2026-11-05), instances with a cache volume are limited to
8 vCPU and 16 GB with a volume of at most 20 GB; larger shapes or volumes
fail with "not supported". Uploaded files get the
upload time as their modification time, so the `voxelcraft` crate and its
build script always rebuild for the uploaded tree. `VC_REMOTE_KEEP=1`
leaves the instance running for inspection (`nsc ssh <id>`, then
`nsc destroy <id> --force`).

The instance runs Linux, like the Linux CI job: it checks the code and
tests but not macOS- or Windows-only code, the GPU, or performance. Run
the game, screenshots, scripted scenes and benchmarks locally; frame-time
gates are measured on the reference M5 MacBook Pro. CI runs the same checks
on every pull request: Linux on the owner's `namespace-profile-gitrunner`
Namespace profile, macOS on an inline `nscloud-*` label with a cache volume,
and Windows on GitHub's `windows-latest`, since the Namespace plan doesn't
allow Windows instances. Timing on 8 vCPU with dependencies cached (2026-11-05): clippy about
1 minute, compiling the release test binaries about 8 minutes, the tests
under a minute, so a default run takes about 11 minutes and `--full`
about 21. That is slower than the M5, but runs don't share a machine or
wait on each other.

## Headless simulation foundation

The `voxelcraft` library contains world simulation, collision, players,
survival, weather, entities, inventories, crafting and CPU mesh generation.
The default `client` feature adds the desktop executable and its window,
GPU, audio and UI dependencies. Disable it to use the engine without those
libraries or devices:

```sh
cargo clippy --release --no-default-features --all-targets -- -D warnings
cargo test --release --no-default-features
cargo run --release --no-default-features --example headless
cargo run --release --no-default-features --example headless -- --nether
cargo run --release --no-default-features --example headless -- --end
```

The example streams terrain and advances 200 fixed gameplay ticks without
creating a window, GPU, audio device or render meshes. It runs faster than
real time and does not write saves. `World::new_headless` disables mesh jobs;
`simulation::tick_world` advances fluids, falling blocks, furnaces, fire,
random block ticks and leaf decay. `simulation::tick_player` uses the same
movement and survival code as singleplayer.

The desktop loop uses `FixedClock` at 20 Hz (50 ms), matching Java's normal
[gameplay tick rate](https://www.minecraft.net/en-us/article/minecraft-java-edition-1-20-3).
The player moves one Java tick per step (`src/player/travel.rs`). Rendering polls
streaming jobs independently and interpolates positions between ticks;
mouse look and the hand animation update each frame. Jump and mining taps
are retained until the next game tick. Offline pause clears
fractional clock time. Catch-up is bounded to five ticks per frame; a longer
stall discards excess elapsed time instead of taking one large physics step.
A day is Java's 24000 ticks, 20 minutes (the saved `time` is a day fraction, so older
ten-minute-day worlds load at the same time of day).

This is a foundation for LAN and split-screen. The client still owns the
singleplayer session, action validation, damage/death handling, dimension
travel and saves. The example is not a dedicated or multiplayer server.

Gameplay sky and block light are maintained by `World` in both feature configurations.
`set_block`, `explode`, terrain streaming and `simulation::tick_world` resolve
pending light changes before returning. When calling individual batched
world systems directly, call `update_block_light` before querying their
light. Dark chunks allocate no light arrays; lit chunks store two levels per
byte. Worker meshes carry geometry only and cannot overwrite gameplay light.
Covered crops require level 9 in their cell; saplings sample the cell above.
Growth reads propagated gameplay skylight, and
slabs/stairs retain their current opaque-cell lighting approximation.
`World::sky_light` returns an optional propagated level; incomplete neighboring
columns are unknown. `Ctx::spawn_light` takes explicit gameplay clock/weather
brightness from `simulation::lighting::SpawnLight`, separately from visual
daylight. See the [lighting/spawning evidence](verification/2026-10-13-lighting-spawning.md).

## Benchmarks

```sh
# Terrain generation and meshing; no window or GPU required.
cargo run --release -- --bench --rd 8

# CPU + GPU frame times for a 360° sweep at 1600 × 900; requires a GPU and windowing session.
cargo run --release -- --bench-render --rd 8 --pose 0,100,0,0,-10
```

Render distance uses **32-block chunks**: `--rd 8` spans 256 blocks,
`--rd 16` spans 512 blocks. See [recorded results and rendering design](architecture.md#performance-design).

## Screenshots and scripted scenes

`--screenshot` waits for the world to load, writes a PNG, then exits.
`--pose` sets `x,y,z,yaw,pitch` (angles in degrees) and starts the player flying.
Use `--wait` to let the scene simulate before capture. To compare two
builds pixel for pixel, capture a paused scene with the animation clock held:
`--open-menu pause --animation-time 7.3`. Idle animations, clouds and water
follow the wall clock even while the game is paused, so without the held
clock two captures of one build differ. Scripted runs
(`--screenshot`, `--bench-render`) ignore saved options. Saves and logs normally
use the [per-user data folder](releases.md#player-data-and-old-saves); add
`--data-dir target/screenshots` to isolate a scripted run. In `--place` and
`--spawn`, `~` for the y coordinate means the terrain surface
(`--spawn allay,x,y,z,item` also gives the allay an item to hold); `--place` also
takes a raw block id instead of a name, for oriented states such as stairs
facing east. Placing the lower half of a double plant (tall flowers, tall
seagrass, pitcher plants and mature pitcher crops) also places its upper half,
as item placement does. `--sign x,y,z,line|line|line|line` writes the front text of a
sign placed there (y must be a number).
`--brush x,y,z,face,strokes` brushes the suspicious block at x,y,z from
that face (1–9 strokes, adding ocean-ruin loot if it has none) and holds
its dust stage for the capture.
`--decor x,y,z,face,item[,held]` hangs an item frame or painting
on, or stands an armor stand against, a face (`up`, `down`, `north`, `south`,
`east`, `west`) of the block at x,y,z, as the item would; a frame holds `held`
and a stand wears it.
`--dye colour[,colour...]` mixes those dyes (`DyedItemColor.applyDyes`) into
the first hotbar stack and every worn piece that is dyeable, after `--give`
and `--wear`: leather armor, leather horse armor and wolf armor.
`--open-menu title --screenshot <file>` captures the title screen instead;
`select`, `create`, `multiplayer` and `direct` capture Select World, Create New
World, Play Multiplayer and Direct Connection, and `edit`, `recreate` and
`backup` capture Edit World, Re-Create and the newer-version backup prompt for
`--world`'s save (else the most recently played world).

These examples use a separate `screenshots` save. `--new` ignores its existing
save, which is replaced when the game saves; use a different `--world` name to
keep a previous scene.

```sh
# Landscape.
cargo run --release -- --world screenshots --new --seed 42 --creative \
    --screenshot shot.png --pose 0,190,0,45,-35

# Flowing water.
cargo run --release -- --world screenshots --new --seed 42 --creative \
    --place 0,~,0,water --pose 0,120,-10,90,-30 --wait 1.5 --screenshot water.png

# Mobs at night.
cargo run --release -- --world screenshots --new --seed 42 --creative \
    --spawn zombie,6,~,2 --spawn pig,4,~,-2 --time 0.75 \
    --pose 0.5,92,0.5,0,-8 --wait 1.5 --screenshot mobs.png
```

For UI captures, add `--f3` for the debug overlay, `--open-inventory` for
the inventory or `--open-menu pause|options|video|sound|controls|mouse|keybinds|accessibility|lan` for the menus, `--weather rain` for rain (snow in cold biomes), and `--give iron_pickaxe --give coal,16` to fill it (`--drop` throws items
in front of the player instead). In
survival, `--health 5 --air 6 --food 8` sets the vitals; `--health 0` opens the death
screen. `--xp 30` sets the experience level and `--orbs 40` spawns orbs worth 40
points four blocks ahead. `cargo test --release icon_sheet -- --ignored` writes every item icon
to `target/item_icons.png`.

To point a capture at a structure, find it without starting the game:
`cargo run --release --no-default-features --example locate_structures -- 42`
prints the nearest village, outpost, ancient city, trial chamber, trail
ruins, ruined portal, buried treasure, ocean ruin, temple, shipwreck,
monument, stronghold, mineshaft and desert well to the origin for that seed
in the latest world generation, then the buried treasure chest's depth, the
nearest trail ruin's piece bounds (for cut-away poses), the nearest Nether
fossil and the eight nearest bastions with their kind (run it through
`scripts/remote-check.sh -- 'cargo run …'` to keep the build off the Mac).
To dig into buried content with `--place x,y,z,air`, list the cells top
down: removing a cell under sand or gravel first drops it into the hole. The camera looks along `(cos yaw, sin yaw)` in x and z,
so a pose of `x-40,y+35,z+40,-45,-30` looks down at a structure at `x,y,z`.
Underground structures need a camera in open air inside them and
`--effect night_vision,600` (or `night_vision,infinite`); `--effect` also
takes `ambient`, `hideparticles` and `hideicon` after the amplifier, as in
`--effect conduit_power,infinite,0,ambient`. `--no-hud` also hides the
inventory, so leave it off for UI captures. The
[2026-11-09 stabilisation pass](verification/2026-11-09-stabilisation-pass.md)
lists the poses it used.

## Hands-on runs

No release or check requires these (owner, 2026-11-14): the owner's play is
the hands-on evidence. If an agent is asked to play the game through
computer use anyway, it needs two things the scripted scenes don't. The game has to be an
app: a bare `target/release/voxelcraft` isn't listed, so wrap the build with
`scripts/test-app.sh BINARY`, which makes an ad-hoc signed
`target/hands-on/VoxelCraft Test.app` with its own bundle identifier, and
launch that with `open -n … --args --data-dir "$PWD/target/hands-on/data" --no-update-check`.
And the screen has to be unlocked: a locked session has no windows, so
attaching fails with `cgWindowNotFound` and windowed benchmark runs are
rejected as occluded. Check with
`ioreg -n Root -d1 -a | grep -A1 CGSSessionScreenIsLocked` before starting,
and leave hands-on play and gate runs for when the owner is at the machine
rather than working around the lock.

## Sound export

```sh
cargo run --release -- --export-sounds
```

Writes every synthesized sound to `target/sounds/*.wav` and prints statistics.
This command exits without opening the game window. See [procedural sound](architecture.md#procedural-sound)
for synthesis and mixer details.

## Command-line reference

```sh
cargo run --release -- --help
```

`--help` and `--version` exit successfully when used alone. Flags and defaults
are defined in [src/main.rs](../src/main.rs).

## Live performance scene suite

The initial scene definitions are in `benchmarks/scenes.json`. Run
`python3 tools/benchmark_scenes.py` for three measured runs per scene, with ten
seconds of warm-up and sixty measured seconds (streaming uses 180 seconds).
The tool builds a release executable with a runtime source fingerprint, records
hardware and executable identity, and writes per-frame samples and summaries
under ignored `target/benchmarks/`. Set `CARGO_TARGET_DIR` normally if you use a
shared build cache. Run benchmarks on AC power without concurrent builds or GPU
workloads. A `building` run averaging exactly 120 FPS means something outside the
game held frames to the 120 Hz display (seen on battery on 2026-10-24), so its
figures measure the display, not the game.

Use `--quick --repeats 1` only to smoke-test the runner. Such results are always
ineligible for acceptance. `--native-resolution WIDTHxHEIGHT` writes a separate
native-resolution run; use a separate `--output` folder. The normal reference
configuration requests 1920 x 1080 physical pixels and rd=8. Each report records
the actual surface dimensions. VSync is disabled and the normal renderer,
world streaming and 20 Hz simulation remain active. Controlled camera flight
and initial health are explicit fixture overrides; these scenes do not replace
ordinary-player tests or the full roadmap biome/encounter matrix.
Survival and streaming cameras fly 24 blocks above the generator's surface along
their routes and face their direction of travel. The entity scene holds dawn with
no daylight cycle or natural spawning, so its 160 mobs do not burn. Until sampling
starts it also removes every mob outside that 160-mob fixture, because chunk
generation adds creature herds that `doMobSpawning` doesn't stop. The
`materials` scene clears the ground ahead of the camera and lays one state of
every v0.12 block (states 16384–29695) over a 25 x 25 grid three blocks
apart. It adds stone beside a block that needs support and writes text on
both sides of every sign. In front of the grid is a wall of item frames
holding items. Building it takes several seconds, which count toward cold
load but not toward warm-up or samples. Runs whose view or simulation distance
differs from `benchmarks/scenes.json` (256 and 160 blocks) are rejected.

The two menu scenes open each screen in their `screens` list for two seconds
in turn. `menus` does this in a world at spawn, with the camera turning: the
pause menu, Options and each Options sub-screen, then the inventory. `title`
starts on the title screen with no world open and cycles through Select World,
Create New World, Multiplayer, Direct Connection, Options and its sub-screens.
The pause screens stop the simulation as Java's do, so these runs skip the
tick and pause checks. `title` also skips the view-distance check. Run just
these two with `--scenes menus,title`.

A benchmark run ignores keyboard, pointer, controller and focus input. It never
autosaves or saves, including when its window is closed. Its window asks for
focus and stays above other windows, since one started from a terminal or an
agent otherwise opens behind the active app; it can't show over a locked
screen or a display that has gone to sleep. Keep it visible and unobstructed. Frames skipped because the surface is hidden or
occluded are counted, and any skipped frame makes the run invalid.

Reports include average FPS, the reciprocal of the mean of the slowest 1% of
frames, p50/p95/p99/worst frame time, stalls over 50 ms, cold load and process
RSS. World scenes also report `frame_phases_ms`: mean milliseconds per phase
of the frame (streaming and polling, the due ticks, uploads, entity geometry,
the rest of the scene, acquiring the image, drawing, presenting) over all
frames, over the slowest 1% and over the frames that ran a tick, with `other`
for time outside the phases. At several hundred FPS the slowest 1% are the
tick frames, so read the gate's tail there before profiling the average. They also include completed versus expected simulation ticks, paused and
discarded simulation time, unpresented frames and entity populations at sample
start and end. Present mode is reported as requested and as resolved from the
surface's supported modes, along with the relevant options. GPU memory is
reported in two forms. The chunk quad arena's used and reserved bytes are always
recorded. Device-allocator totals are `null` where the backend provides no
allocator report, including Metal on macOS; `null` never means zero.
Where the adapter supports timestamp queries, benchmark runs time each
frame's render passes on the GPU (`gpu_timestamps` says whether they did).
The timestamps are read back a few frames late, so timing never stalls a
frame; a frame whose readback slot is still busy is left untimed. Each scene
report keeps `frame_acquire_ms` and `frame_gpu_ms` beside `frame_times_ms`,
and `gpu_timestamp_ms` gives GPU time (mean, p50, p99, slowest 1% mean,
worst) over all frames, the slowest 1% of frames, frames that waited more
than 4 ms to acquire an image and the rest. The pass timestamps leave out
uploads and presentation.

Each report has `acceptance_eligible_configuration`, `run_valid` and
`ineligible_reasons`. A run is valid only with zero paused time, completed ticks
within max(2, 0.5%) of 20 per sampled second (neither applies to the menu
scenes), zero unpresented frames and, for
the entity scene, start and end populations within 5% of 160. The runner's
`reference_gate_passed` also requires enhanced graphics, an uncapped resolved
present mode, a matching source fingerprint, M5 hardware, all scenes and three
repeats. The fingerprint covers the files embedded by `build.rs`, including
`packaging/icons/*`. `reference_gate_problems` lists each failure. Run
`python3 -m unittest tools/test_benchmark_scenes.py` after changing the gate.
Neither this suite nor the older offscreen sweep alone proves 1.0 performance
acceptance.

When the window can't stay visible or the machine is on battery, compare
builds with `--offscreen` (and `--seconds N` for a shorter sample, with
`--executable` for each build). The scene is drawn into an offscreen image of
the same size and nothing is presented; each frame waits for the frame two
back to finish on the GPU, as a window's three images would make it, so the
interval is the larger of the CPU's and the GPU's time. These runs are for
comparison only: every report is marked ineligible and the gate never passes
on them ([example](verification/2026-11-09-entity-frame-time.md)).

The low-level flags are `--benchmark-scene`, `--benchmark-output`,
`--benchmark-warmup`, `--benchmark-duration`, `--benchmark-offscreen`,
`--benchmark-frame-latency`, `--benchmark-window-mode`,
`--benchmark-no-gpu-timestamps` and `--size WIDTHxHEIGHT`.
The legacy `--bench-render` remains available for historical comparisons.

### Presentation experiments

The runner can change how frames reach the screen. Each option is recorded
in the summary's `presentation_options` and in every run's `options`. The
defaults are the reference configuration and add no flags.

- `--frame-latency N` (1–3, default 2) sets the surface's maximum frame
  latency. On Metal, wgpu 30 clamps it to 1–2 and gives the layer one more
  drawable than that, so 1 means two drawables and both 2 and 3 mean three,
  the most a `CAMetalLayer` allows. Reports record
  `metal_maximum_drawable_count`.
- `--window-mode borderless` uses macOS's native fullscreen Space;
  `--window-mode exclusive` switches the display mode and asks for a
  1920 × 1080 mode, falling back to the largest. Reports record the mode
  the window ended up in and the display, with its modes for exclusive runs.
- `--no-gpu-timestamps` turns GPU timing off, to check it costs nothing.

The gate still needs 1920 × 1080 physical pixels. A borderless run on the
built-in display renders at the display's own size, so the game marks it
"resolution is not 1920x1080" and the run is invalid. Compare its acquire
waits and GPU time, not its FPS. An exclusive run is 1920 × 1080 only if the
display offers that mode. Even then, any frame latency or window mode other
than the default makes the protocol a "presentation experiment" that can't
pass the gate. Adopting one for the gate is the owner's decision in
[roadmap §6.1](roadmap.md#61-reference-configuration-and-frame-time-gate),
followed by a normal run with the new default.

The experiment owed for PERF-02/03 runs on the idle M5 on AC power with the
display held awake. The first command builds the executable; the rest reuse
a copy of it, so another checkout's build can't replace it. Each takes about
15 minutes.

```sh
out=target/benchmarks/$(date +%F)-presentation
scenes=building,automation,materials
caffeinate -d python3 tools/benchmark_scenes.py --scenes $scenes --output $out/reference
cp ${CARGO_TARGET_DIR:-target}/release/voxelcraft $out/voxelcraft
exe=$out/voxelcraft
caffeinate -d python3 tools/benchmark_scenes.py --executable $exe --scenes $scenes --output $out/no-gpu-timestamps --no-gpu-timestamps
caffeinate -d python3 tools/benchmark_scenes.py --executable $exe --scenes $scenes --output $out/latency-1 --frame-latency 1
caffeinate -d python3 tools/benchmark_scenes.py --executable $exe --scenes $scenes --output $out/borderless --window-mode borderless
caffeinate -d python3 tools/benchmark_scenes.py --executable $exe --scenes $scenes --output $out/exclusive --window-mode exclusive
```

`--frame-latency 3` isn't in the list because on Metal it is the same as the
default. Read each run's `gpu_timestamp_ms` in `summary.json`. A frame's
GPU time is measured after its own acquire, and the wait can depend on
earlier frames still in flight, so these are clues, not proof:

- If the frames that waited over 4 ms to acquire, and the frames just
  before them, show GPU times like the rest, the GPU is probably not late.
  The wait is then more likely the window server releasing a drawable, and
  the window-mode runs show whether skipping the compositor removes it.
- If the slowest 1%, or the frames before a slow acquire, show long GPU
  times, suspect the GPU work. Profile the GPU before changing presentation.
- If `no-gpu-timestamps` differs from `reference` by more than run-to-run
  noise, the timing itself costs something; say so in the record.
- If `latency-1` makes acquire waits longer, the drawable count matters, but
  three is already Metal's maximum.
