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
(`--screenshot`, `--bench-render`) ignore `saves/options.txt`. In `--place` and
`--spawn`, `~` for the y coordinate means the terrain surface.

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
the inventory or `--open-menu pause|options` for the menus, and `--give iron_pickaxe --give coal,16` to fill it. In
survival, `--health 5 --air 6` sets the vitals; `--health 0` opens the death
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

The parser currently prints help and exits with status 2. Its flags and defaults
are defined in [src/main.rs](../src/main.rs).
