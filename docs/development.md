# Development tools

[← Back to VoxelCraft](../README.md) · [Documentation index](README.md) · [Architecture](architecture.md) · [Contributing](../CONTRIBUTING.md) · [Agent guide](agent-guide.md)

Run commands from the repository root. See the README for platform prerequisites.

## Checks

[CI](../.github/workflows/ci.yml) runs these on every pull request and push
to `main`, and it is the full check (owner, 2026-11-22): agents run targeted
checks in Boat through `scripts/boat-check.sh -- CMD` while working and leave
the full suite to CI ([before handing work back](../AGENTS.md#before-handing-work-back),
[Boat checks](#boat-checks)):

```sh
cargo fmt --check
cargo clippy --profile ci --all-targets -- -D warnings
cargo test --profile ci
```

The `ci` profile (in `Cargo.toml`, owner, 2026-11-21) is the release profile
without cross-crate link-time optimisation and with 16 codegen units: the same
optimisations and assertions, but the ~70 test binaries build and link
about three times as fast (259 s instead of 858 s after a source change on
a `large` Boat sandbox). Its output goes to `target/ci/`. Only shipped builds use `--release`
([releases](releases.md)).

The Linux CI job also checks the built binary's embedded assets with
`python3 tools/validate_assets.py --executable target/ci/voxelcraft` and
runs the benchmark-gate tool tests. The **ruff** job lints and
format-checks the Python tools with `uvx ruff@0.17.5 check` and
`uvx ruff@0.17.5 format --check` (settings in `ruff.toml`; run
`uvx ruff@0.17.5 format` to apply the formatting). The Linux job also runs
`python3 tools/check_doc_links.py` (and its tests,
`python3 -m unittest tools/test_check_doc_links.py`), which checks that every
relative link and `#anchor` in the tracked Markdown files resolves, using
GitHub's heading-anchor rules, and prints each broken one as
`file:line: target`. Documentation-only changes skip the Cargo chain; run
the link check instead. CI runs Cargo with `--locked`, so commit
`Cargo.lock` changes with the code that needs them. The **Security** workflow
runs `cargo audit --deny warnings` weekly and on pull requests that change `Cargo.toml`,
`Cargo.lock` or `.github/workflows/security.yml`. The **CodeQL** workflow
analyses the workflows, Python, Rust and the one Java source on pushes to
`main`, pull requests and weekly. Dependabot proposes weekly
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

## Boat checks

`scripts/boat-check.sh` runs checks on a [Boat](https://boat.dev)
sandbox instead of this Mac. It needs the `boat` CLI, installed and signed in
(`curl -fsSL https://boat.dev/install | sh`), `jq` and git.

```sh
scripts/boat-check.sh          # cargo fmt here, then clippy and tests in Boat
scripts/boat-check.sh --full   # also the --no-default-features checks
scripts/boat-check.sh -- cargo test --profile ci some_test   # any command
scripts/boat-check.sh --status # this worktree's sandbox
scripts/boat-check.sh --done   # delete it when the work ends
scripts/boat-check.sh --save   # rebuild the warm snapshot from origin's default branch
```

Agents use `-- CMD` (owner, 2026-11-22): the targeted tests and clippy for
what they changed while working, and the failing job or test when CI fails.
The default and `--full` modes run nothing CI doesn't, so they're for
reproducing a CI failure a narrower command can't, not a step before every
hand-off ([the targeted checks](../AGENTS.md#before-handing-work-back)).

Each worktree has one long-lived sandbox (owner, 2026-11-21), named
`voxelcraft-check-<worktree>-<time>-<pid>` (unique to the run that made it,
so an interrupted create cleans up only its own sandbox), because the Boat
account's daily starts are shared by every project and one `boat run` per
check used a start each. The
first check creates it from the saved snapshot `voxelcraft-warm`
(`VC_BOAT_SNAPSHOT`), or bare when there is none, and records its id in the
worktree's git directory (`.git/.../boat-check/`). Every later check reuses
it:

1. `cargo fmt` runs here.
2. The working tree (tracked and untracked, non-ignored files) is pushed to
   a fresh directory in the sandbox.
3. It's synced into the checkout at `/home/user/voxelcraft` with
   `rsync --checksum --delete`: deleted files go, and only files whose
   contents changed are rewritten, with the current time. `boat push` keeps
   this computer's timestamps, which cargo could otherwise take for
   unchanged files.
4. The checks run there with `boat exec --timeout 0`. Cargo's target
   directory (`/home/user/target`) stays between checks, so a check rebuilds
   only what changed.

The sandbox stops as soon as each check ends, whatever its size (owner,
2026-11-22; it replaces the 2026-11-21 idle timeouts of 15 minutes, 5 on
`xlarge`). Idle time bills like work, while a stopped sandbox bills nothing,
and Boat keeps the machine, unbilled, for 10 minutes after the sandbox reads
`stopped`: a resume in that time, or while it is still `stopping`, usually
takes back the same machine, which [uses no start](https://docs.boat.dev/limits.md)
and restores nothing ([stop](https://docs.boat.dev/snapshots/stop.md#resume-within-10-minutes)).
So the next check resumes it, without `--type` unless its size must change
(a new size always needs a new machine and a start), and a check more than
10 minutes later uses one start and comes back from the snapshot, target
directory included. `VC_BOAT_IDLE=<seconds>` instead leaves the sandbox up
that long through auto-stop.

Boat's auto-stop is a deadline, not an idle timer. While a check runs, the
script keeps it 30 minutes ahead and renews it every 10 minutes from a loop
that first checks the script is still alive (owner, 2026-11-22; it was two
hours ahead, renewed every 30 minutes by a loop that outlived a killed
script). A check killed outright (`kill -9`) leaves its sandbox to stop by
itself within 30 minutes; an interrupted one (Ctrl-C, `kill`, a closed
terminal) stops it at once, even while Boat is still creating it.
`--stop` stops it by hand; `--done` deletes it and is how an agent ends its
work. Agents sharing a worktree each set
`VC_BOAT_SLOT`, so no two share a sandbox; a lock in the git directory stops
two checks in one slot from racing. Starts use `--fail-fast` and retry, so a
busy moment costs nothing. When a minute of those tries finds no machine, one
start waits for Boat to free one, for up to `VC_BOAT_WAIT` seconds (default
900; 0 gives up at once). An interrupted or timed-out wait cleans up like any
start, and a sandbox Boat finishes after the script stopped looking is adopted
or deleted by the slot's next run (2026-11-23: busy spells outlasted the minute while machines freed
within a few). When Boat refuses a start (no starts left today), the script
stops and says so.

Sandboxes are `large` (8 vCPU, 16 GB; `VC_BOAT_TYPE` to change it) for every
mode (owner, 2026-11-21). The test build runs out of memory on 8 GB, and
`xlarge` (16 vCPU, 32 GB) was slower, not faster: from the same warm state
it took 450 s to `large`'s 197 s for the test build after a source change
and 1,543 s to 1,006 s for the full chain, at about 2.8 times the price.
They start with `--no-env`, so none
of the account's stored secrets go in. The script installs the Linux audio,
device and font libraries and `rsync` when they're missing and installs the
Rust toolchain `rust-toolchain.toml` pins, the one CI and release builds use.

`--save` builds origin's default branch, not the worktree, in a fresh
sandbox named `voxelcraft-warm-build-<time>-<pid>`, runs the full checks
there and saves it, checkout and target directory included, as
`voxelcraft-warm`. The build sandbox is deleted when the script ends,
however it ends short of `kill -9`, or only stopped while a save from it may
still be running; a killed `--save` leaves it to stop within 30 minutes. New check
sandboxes start from a copy of it with every dependency built. Re-run it
after dependency or toolchain changes.

Earlier rules, kept for reading old records: builds ran on the Mac with
every worktree sharing one cargo target directory (2026-10-17; replaced on
2026-11-22, since a shared target could hand one worktree another's build
and its workaround, `touch build.rs` and `cargo clean --release -p
voxelcraft`, forced rebuilds before benchmarks), with at most four agents
in parallel (2026-10-17, set for the owner's 16 GB machine; lifted on
2026-11-21). Builds moved to Namespace on 2026-11-05, agents iterated on the
Mac again from 2026-11-20, and Boat took over the same day; each check then
used its own `boat run` and a daily start until the long-lived sandbox of
2026-11-21.

Sandboxes run Linux, like the Linux CI job: they check the code and tests
but not macOS- or Windows-only code, the GPU or performance. Run the game,
screenshots, scripted scenes and benchmarks locally; frame-time gates are
measured on the reference M5 MacBook Pro. CI runs the same checks on every
pull request: Linux on the owner's `namespace-profile-gitrunner` Namespace
profile, macOS on an inline `nscloud-*` label with a cache volume, and
Windows on GitHub's `windows-latest`, since the Namespace plan doesn't allow
Windows instances.

## Headless simulation foundation

The `voxelcraft` library contains world simulation, collision, players,
survival, weather, entities, inventories, crafting and CPU mesh generation.
The default `client` feature adds the desktop executable and its window,
GPU, audio and UI dependencies. Disable it to use the engine without those
libraries or devices:

```sh
cargo clippy --profile ci --no-default-features --all-targets -- -D warnings
cargo test --profile ci --no-default-features
cargo run --profile ci --no-default-features --example headless
cargo run --profile ci --no-default-features --example headless -- --nether
cargo run --profile ci --no-default-features --example headless -- --end
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
(`--screenshot`, `--bench-render`) ignore saved options, so `--gui-scale auto|N`
sets the GUI Scale for the run (capped at what fits the window, as in the
options) and `--size WxH` the window's size in physical pixels.
`--gamerule advance_time=false` holds `--time` for the run, and
`--gamerule advance_weather=false` holds `--weather` (`clear`, `rain` or
`thunder`); any game rule the game enforces works. Saves and logs normally
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
the inventory, `--open-recipe-book` for the recipe book (in the inventory, or
beside a crafting table opened with `--open-block`), `--creative-tab <id>` for
the creative inventory on one of Java's tabs (`building_blocks`, `search`,
`combat`, ...; today every tab shows the one creative palette) or `--open-menu pause|options|video|sound|controls|mouse|keybinds|accessibility|lan` for the menus, `--weather rain` for rain (snow in cold biomes), and `--give iron_pickaxe --give coal,16` to fill it (`--drop` throws items
in front of the player instead). In
survival, `--health 5 --air 6 --food 8` sets the vitals; `--health 0` opens the death
screen, and with `--hardcore` (which makes a world the run creates a Hardcore
world) it is the Hardcore death. `--open-block` opens any workstation or
container block, the crafting table included. `--xp 30` sets the experience level and `--orbs 40` spawns orbs worth 40
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
`scripts/boat-check.sh -- 'cargo run …'` to keep the build off the Mac).
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

### Screenshots on Boat

`scripts/boat-capture.sh` renders captures in this worktree's
[Boat sandbox](#boat-checks) and copies the PNGs back, so an agent can see a
visual change without running the game on the Mac:

```sh
scripts/boat-capture.sh --scene materials          # a group from the comparison scenes
scripts/boat-capture.sh --scene inventory --scene day
scripts/boat-capture.sh                            # every comparison scene
scripts/boat-capture.sh --name water -- --world w --new --seed 42 --creative \
    --place 0,~,0,water --pose 0,120,-10,90,-30 --rd 4 --size 1280x720
```

It runs through `scripts/boat-check.sh -- CMD`, so it uses the same sandbox,
slot (`VC_BOAT_SLOT`), lock and warm `ci` build, and the sandbox stops when
it ends. The first run on a sandbox installs Xvfb and Mesa's software Vulkan
driver (lavapipe). Each run builds the game with the `ci` profile, starts one
virtual X screen of 3840 × 2160 and runs
[`tools/capture_scenes.py`](../tools/capture_scenes.py), one game per capture
with its own data folder. The PNGs, `manifest.json` (the commit, the
renderer and every capture's settings and arguments) and the log of any
capture that failed land in `target/captures/<commit>[-dirty]/` (`--out`
picks another folder). The game sees `llvmpipe (LLVM …) (Vulkan)` as its GPU.

**Software captures show appearance only, never performance.** They run at
a few frames per second, and nothing about their timing says anything about
the Mac. The FPS counter that shows in UI captures is not a measurement.
The frame-time gate stays on the reference Mac
([§6.1](roadmap.md#61-reference-configuration-and-frame-time-gate)).

Differences from Metal seen so far (2026-11-23,
[record](verification/2026-11-23-v0.18-foundation.md)):

- Colour: the surface is an sRGB format on both, and textures are sRGB, so
  colours match to within rounding. UI flat colours are encoded the same
  way on both (see the [colour pipeline](art-and-ui.md#colour-pipeline)).
- Anti-aliasing: the game requests no MSAA, so edges are aliased on both.
- Precision: llvmpipe computes in 32-bit floats like the Mac's GPU, but its
  texture filtering and derivative maths are not bit-identical: compare
  captures from one renderer with each other, never a Boat capture with a
  Mac capture pixel for pixel.
- Speed: a world at render distance 8 took about 5½ minutes to settle and
  capture on a `large` sandbox, so the comparison scenes use render
  distance 6 for world scenes, 4 for in-world UI scenes and 2 for the title
  screens.

### Comparison scenes

[`tools/capture_scenes.json`](../tools/capture_scenes.json) lists named,
reproducible before/after scenes for v0.18's visual work, in five groups:

- `materials`: a stone and ore wall over deepslate, a torch-lit stone room's
  ceiling, a stepped grass and dirt hillside, a small forest, sand dunes and
  a palette wall of 32 common blocks. All are built with `--place` high
  above the terrain, so they don't change when world generation does.
- `mobs`: nine mobs and dropped items on uneven grass at sunrise (held
  below the sunburn threshold, so the undead survive the wait) and at
  midnight.
- `ui`: the survival HUD, inventory, recipe book, a crafting table with the
  recipe book, a furnace, the creative inventory on two tabs, the pause menu,
  Video Settings, the death screen and the Hardcore death, each at
  1920 × 1080 and 1280 × 720 with Auto and fixed scale-2 GUI scales. Auto
  is already the largest scale that fits (4 and 3), so it is the "large"
  case.
- `title`: the title screen, Select World with four fixed saves and empty,
  and Create New World, at the same four sizes.
- `atmosphere`: the seeded landscape at noon, dusk and midnight, in rain and
  a thunderstorm, inside a water tank, and in the Nether and the End. Track
  8's fog, clouds, rain and light are compared on these, and on two scenes
  it appended: `nether-arrival` (the Nether where its portal arrives, in
  open air) and `cave-ceiling-night` (the torch-lit room at midnight)
  ([record](verification/2026-11-23-v0.18-atmosphere.md)).

Each scene records its seed, game mode, pose, placements, spawned mobs,
time, weather, dimension, resolution, GUI scale, render distance and held
animation time; `defaults` fills in what a scene leaves out, and every
scene holds the time and weather with game rules. `python3
tools/capture_scenes.py list` prints them, and `args NAME` prints the
command line of each variant. `scripts/boat-capture.sh` puts one folder of
PNGs per commit under `target/captures/`. Mobs move during the 1.5 seconds
the mob scenes wait, so those two scenes are the only ones that differ
between runs of one build.

To add a scene, append it to the JSON with a new name, rather than changing
an existing scene, so earlier captures keep comparing like with like. The
`capture_scenes` integration test checks that every block, item, mob,
effect and setting it names exists.

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

Three atmosphere scenes (v0.18 Track 8,
[record](verification/2026-11-23-v0.18-atmosphere.md)) measure what the fog,
clouds and weather cost. `weather` flies the survival orbit in a full
thunderstorm held at noon (`advance_time` and `advance_weather` off), with
rain sheets, the rain fog, the storm's sky and clouds, and lightning bolts
and sky flashes. `underwater` clears a 35 x 35 column at spawn and builds a
glass-walled tank of still water 33 x 33 and 12 deep on sand with a fixed
scatter of seagrass, with the camera 5.5 blocks above its floor, turning:
water fog, the surface from below and translucent water. `nether` starts in
the Nether where its portal arrives for the seed and turns the camera in
place there. Their reports record the dimension and the rain and thunder
levels. Run them with `--scenes weather,underwater,nether`.

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
`reference_gate_passed` also requires every run's `frame_thresholds_passed`
(the 120 FPS gate offscreen, no frame over 50 ms windowed), enhanced graphics, an uncapped resolved
present mode, a matching source fingerprint, M5 hardware, all scenes and three
repeats. The fingerprint covers the files embedded by `build.rs`, including
`packaging/icons/*`. `reference_gate_problems` lists each failure. Run
`python3 -m unittest tools/test_benchmark_scenes.py` after changing the gate.
Neither this suite nor the older offscreen sweep alone proves 1.0 performance
acceptance.

The frame-time gate (owner, 2026-11-14; [roadmap §6.1](roadmap.md#61-reference-configuration-and-frame-time-gate))
needs two full suites at the commit to be tagged. `--offscreen` runs the
gate itself: each scene is drawn into an offscreen image of the same size and
nothing is presented; each frame waits for the frame two back to finish on
the GPU, as a window's three images would make it, so the interval is the
larger of the CPU's and the GPU's time. A run passes with a 1% low of at
least 120 FPS and p99 at most 8.33 ms. The plain windowed suite then checks
for stalls: a run passes when no frame is over 50 ms. Offscreen runs also
compare builds when the window can't stay visible (add `--seconds N` for a
shorter sample, which never passes, and `--executable` for each build;
[example](verification/2026-11-14-drawable-wait.md)).

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
