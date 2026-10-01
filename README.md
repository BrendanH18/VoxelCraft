<div align="center">

<h1>VoxelCraft</h1>

<p><strong>A voxel world, built from scratch in Rust.</strong><br>
Explore, build, and survive in a procedurally generated sandbox powered by wgpu.</p>

<p>
  <a href="https://github.com/BrendanH18/minecraft_rust/actions/workflows/ci.yml"><img src="https://github.com/BrendanH18/minecraft_rust/actions/workflows/ci.yml/badge.svg" alt="CI status"></a>
  <a href="Cargo.toml"><img src="https://img.shields.io/badge/Rust-2024_edition-dea584?style=flat" alt="Rust 2024 edition"></a>
  <a href="https://wgpu.rs"><img src="https://img.shields.io/badge/graphics-wgpu-478cbf?style=flat" alt="Graphics powered by wgpu"></a>
  <a href="#license"><img src="https://img.shields.io/badge/license-MIT%20%2F%20Apache--2.0-62a86b?style=flat" alt="License: MIT or Apache 2.0"></a>
</p>

<p>
  <a href="#play"><strong>Play</strong></a> ·
  <a href="#screenshots">Screenshots</a> ·
  <a href="docs/gameplay.md">Gameplay guide</a> ·
  <a href="docs/architecture.md">Inside the engine</a> ·
  <a href="CONTRIBUTING.md">Contribute</a>
</p>

<img src="docs/images/landscape.jpg" alt="VoxelCraft's forested hills, sandy coastline, and ocean, generated entirely in Rust" width="960">

<p><em>Procedural terrain. Procedural textures. Procedural sound. No bundled game assets.</em></p>

</div>

## Explore, build, survive

- **A world to explore.** Infinite terrain with forests, deserts, snowy biomes,
  mountains, oceans, caves, ores, and trees.
- **Two ways to play.** Build freely in creative, or play survival with timed
  mining, block drops, a stackable inventory, health, hunger, and respawning.
- **Craft, cook and store.** Make tools in five tiers, craft torches and
  building blocks, smelt ores and cook food in furnaces, and keep your haul in
  chests. Tools wear out; food restores hunger and supports natural healing.
- **Farm and grow.** Till soil with a hoe, sow seeds found in tall grass and
  harvest wheat for bread. Saplings grow into trees, leaves fall from felled
  trees, and grass creeps back over bare dirt.
- **A world that moves.** Flowing water and lava, falling sand, a day/night
  cycle, drifting clouds, herds of animals, and zombies, skeletons, creepers
  and spiders that come out at night.
- **Light and sound from code.** Smooth sky and block lighting, ambient
  occlusion, material-specific footsteps, positional audio, and underwater effects.
- **Built for speed.** Parallel chunk generation, greedy meshing, compact GPU
  quad records, and a shared mesh arena keep the world streaming around you.

VoxelCraft is an early sandbox project. See the [gameplay guide](docs/gameplay.md) for the survival
rules and available recipes.

## Play

Build from source with a **recent stable Rust toolchain** (edition 2024;
developed with Rust 1.98) and a GPU supported by wgpu's Metal, Vulkan, or
DirectX 12 backends. CI checks builds on macOS, Linux, and Windows.

On Debian/Ubuntu, install the audio and device headers before building:

```sh
sudo apt-get install libasound2-dev libudev-dev pkg-config
```

```sh
git clone https://github.com/BrendanH18/minecraft_rust.git
cd minecraft_rust
cargo run --release
```

This starts survival and resumes `./saves/world` when a save exists. To try
creative with a fixed seed in a separate world:

```sh
cargo run --release -- --creative --world creative --seed 42
```

| Option | What it does |
| --- | --- |
| `--creative` / `--survival` | Choose a game mode |
| `--world <name>` | Choose a save under `./saves/<name>` |
| `--seed <n>` | Set the seed for a new world |
| `--rd <chunks>` | Set view distance in 32-block chunks; default `8` = 256 blocks |
| `--no-vsync` | Uncap the frame rate |
| `--mute` / `--volume <0..1>` | Set audio at startup |
| `--help` | Show all options, including benchmarks and screenshots |

Worlds autosave every two minutes and on exit. Only modified chunks are stored;
the rest regenerates from the seed. Inventory, health, air, hunger, and furnace
contents are saved too. `--new` starts fresh in the selected save
and replaces it when saving, so choose a new `--world` name to keep an old world.

### First steps

Click the window to capture the mouse. Move with **W A S D**, look with the
mouse, and press **Space** to jump. **Left click** breaks blocks or attacks mobs;
**right click** places blocks or opens a crafting table, furnace or chest
(hold **Shift** to build against one instead). Press **E**
for inventory and its 2×2 crafting grid in survival, and **G** to switch modes.

For a new survival world:

1. Punch a few logs from a tree. Open the inventory with **E**, put a log in
   the crafting grid, and click the result to turn it into four planks.
2. Arrange four planks in a 2×2 square to make a crafting table. Two planks
   stacked vertically make four sticks.
3. Place the table and right-click it for a 3×3 grid. Make a wooden pickaxe
   with three planks across the top row and two sticks down the middle below.
4. Mine stone with the pickaxe to collect cobblestone. Eight cobblestone in
   a ring around an empty center make a furnace. Make a stone pickaxe before
   mining iron ore; gold and diamond ore need an iron pickaxe or better.
5. Put ore or raw meat in the furnace's top slot and fuel below: coal,
   charcoal, logs, planks, or sticks. Hold **right click** with food selected
   for 1.6 seconds to eat when hungry.

Stone mined by hand drops nothing. Sprinting needs more than six food points
(three drumsticks). Mined blocks and mob loot drop as items: walk over them
to pick them up. Press **Q** to drop the selected item (**Ctrl+Q** for the
whole stack), or click outside the inventory window to throw what you're
holding. Dying drops everything you carry where you fell, and dropped items
vanish after five minutes, so go back for them.

Click **Recipes** in the inventory or crafting screen to browse ingredient
layouts with the arrow buttons or scroll wheel. Hover an ingredient to see its
name and alternatives. Copy the preview into your own grid to craft; recipes
that need a larger grid point you to a crafting table. An **Eating** progress
bar appears below the crosshair while you hold right-click with food.

| Input | Action |
| --- | --- |
| Left Ctrl or R | Sprint (survival needs more than three drumsticks) |
| 1–9 or scroll wheel | Select hotbar slot |
| Middle click | Pick block |
| Q / Ctrl+Q | Drop one item / the whole stack |
| Shift + click | Move a stack between a container and the inventory, or craft as many as fit |
| F or double-tap Space | Toggle flight in creative |
| Space / Left Shift | Fly up / down |
| `[` / `]` | Decrease / increase view distance |
| F3 / F11 | Debug overlay / fullscreen |
| Esc | Pause menu: back to game, options (render distance, FOV, sensitivity, volume, vsync), save and quit |

See the [gameplay guide](docs/gameplay.md) for all controls, survival rules, and mob behavior.

## Screenshots

<table>
  <tr>
    <td width="50%"><img src="docs/images/sunset.jpg" alt="Square sun setting above a grassy coastline"><br><strong>Day turns to night</strong><br>A procedural sky with sun, moon, stars, and clouds.</td>
    <td width="50%"><img src="docs/images/night-mobs.jpg" alt="Zombies and pigs in a voxel landscape at night"><br><strong>Company after dark</strong><br>Animated mobs with wandering and combat behavior.</td>
  </tr>
  <tr>
    <td width="50%"><img src="docs/images/water.jpg" alt="Water flowing down stepped voxel terrain"><br><strong>Water finds its way</strong><br>Flowing sources, waterfalls, and underwater fog.</td>
    <td width="50%"><img src="docs/images/inventory.jpg" alt="Creative inventory showing the available block palette"><br><strong>Build something</strong><br>A creative block palette and nine-slot hotbar.</td>
  </tr>
  <tr>
    <td width="50%"><img src="docs/images/crafting.png" alt="Wooden pickaxe recipe in a crafting table, with its ingredient guide alongside"><br><strong>From wood to tools</strong><br>Browse recipes and copy their ingredients into the crafting grid.</td>
    <td width="50%"><img src="docs/images/furnace.png" alt="Lit furnace smelting iron ore, with fuel and cooking progress visible"><br><strong>Smelt and cook</strong><br>Turn ore into ingots and raw food into cooked meals.</td>
  </tr>
  <tr>
    <td width="50%"><img src="docs/images/hunger.png" alt="Survival hearts and hunger bar, with an eating progress indicator below the crosshair"><br><strong>A bite to eat</strong><br>Hold right-click with food and watch the bite progress.</td>
    <td width="50%"><img src="docs/images/dropped-items.jpg" alt="Dropped items lying on grass: a steak, glass, a torch, a poppy, an apple, a diamond pickaxe and cobblestone"><br><strong>Pick it up</strong><br>Mined blocks and loot drop as spinning items you walk over to collect.</td>
  </tr>
  <tr>
    <td width="50%"><img src="docs/images/farming.jpg" alt="Rows of wheat at every growth stage on wet and dry farmland, with oak and spruce saplings behind"><br><strong>Sow and reap</strong><br>Wheat ripens from green shoots to golden ears on tilled soil.</td>
    <td width="50%"><img src="docs/images/chest.jpg" alt="Chest screen with diamonds, logs, iron ingots and steak above the player's inventory"><br><strong>Stash it</strong><br>Chests hold 27 stacks; shift-click to move whole stacks.</td>
  </tr>
</table>

## Inside the engine

The renderer uses greedy meshing, 12-byte quad records expanded in the vertex
shader, and pooled GPU buffers. Generation and lighting run on a worker pool;
frustum and face-direction culling reduce the work sent to the GPU.

Recorded development benchmarks on an **Apple M5**, in a **release build** at
**1600 × 900**, with the GPU synchronized after each frame:

| View distance | Average frame time | Approx. frame rate |
| --- | --- | --- |
| 256 blocks (`--rd 8`) | 1.10 ms | 900 fps |
| 512 blocks (`--rd 16`) | 2.05 ms | 490 fps |

These measurements include CPU and GPU work and vary with hardware and scene.
Read the [architecture notes](docs/architecture.md) for the full results,
rendering design, code layout, and sound synthesis. The
[development guide](docs/development.md) shows how to run benchmarks, script
scenes, capture screenshots, and export sounds.

## Contributing

Bug reports, ideas, and pull requests are welcome. Start with
[CONTRIBUTING.md](CONTRIBUTING.md) for setup, checks, and useful details to
include in a report.

## License

Licensed under [MIT](LICENSE-MIT) or [Apache 2.0](LICENSE-APACHE), at your option.

Unless you explicitly state otherwise, any contribution intentionally
submitted for inclusion in the work by you, as defined in the Apache-2.0
license, shall be dual licensed as above, without any additional terms or
conditions.

VoxelCraft is an independent project, not affiliated with or endorsed by
Mojang Studios or Microsoft. Minecraft is a trademark of Mojang Studios.
It contains no Minecraft code or assets.
