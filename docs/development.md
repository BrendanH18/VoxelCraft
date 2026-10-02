# Development tools

[← Back to VoxelCraft](../README.md) · [Architecture](architecture.md) · [Contributing](../CONTRIBUTING.md)

Run commands from the repository root. See the README for platform prerequisites.

## Checks

Use the same checks as [CI](../.github/workflows/ci.yml):

```sh
cargo fmt --check
cargo clippy --release --all-targets -- -D warnings
cargo test --release
```

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
Player collision retains substeps of at most 1/120 second. Rendering polls
streaming jobs independently and interpolates positions between ticks;
mouse look and the hand animation update each frame. Jump and mining taps
are retained until the next game tick. Offline pause clears
fractional clock time. Catch-up is bounded to five ticks per frame; a longer
stall discards excess elapsed time instead of taking one large physics step.
The existing ten-minute VoxelCraft day is preserved.

This is a foundation for LAN and split-screen. The client still owns the
singleplayer session, action validation, damage/death handling, dimension
travel and saves. The example is not a dedicated or multiplayer server.

Gameplay block light is maintained by `World` in both feature configurations.
`set_block`, `explode`, terrain streaming and `simulation::tick_world` resolve
pending light changes before returning. When calling individual batched
world systems directly, call `update_block_light` before querying their
light. Dark chunks allocate no light arrays; lit chunks store two levels per
byte. Worker meshes carry geometry only and cannot overwrite gameplay light.
Covered crops require level 9 in their cell; saplings sample the cell above.
Skylight for growth still uses the existing open-sky approximation, and
slabs/stairs retain their current opaque-cell lighting approximation.

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
Use `--wait` to let the scene simulate before capture. Scripted runs
(`--screenshot`, `--bench-render`) ignore saved options. Saves and logs normally
use the [per-user data folder](releases.md#player-data-and-old-saves); add
`--data-dir target/screenshots` to isolate a scripted run. In `--place` and
`--spawn`, `~` for the y coordinate means the terrain surface; `--place` also
takes a raw block id instead of a name, for oriented states such as stairs
facing east.
`--open-menu title --screenshot <file>` captures the title screen's world list
instead.

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
the inventory or `--open-menu pause|options` for the menus, `--weather rain` for rain (snow in cold biomes), and `--give iron_pickaxe --give coal,16` to fill it (`--drop` throws items
in front of the player instead). In
survival, `--health 5 --air 6 --food 8` sets the vitals; `--health 0` opens the death
screen. `cargo test --release icon_sheet -- --ignored` writes every item icon
to `target/item_icons.png`.

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
