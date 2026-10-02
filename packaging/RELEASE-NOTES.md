VoxelCraft 0.2.0 is a big update over the first early-access build: new
dimensions, a much larger world, fire and explosives, shaped building blocks,
armor and beds, and the first multiplayer features.

Choose your download:

- **Windows 10/11 x64:** `VoxelCraft-0.2.0-windows-x64-Setup.exe`
- **Mac with Apple Silicon (M-series), macOS 13+:** `VoxelCraft-0.2.0-macos-apple-silicon.dmg`
- **Mac with Intel, macOS 13+:** `VoxelCraft-0.2.0-macos-intel.dmg`

## What's new since 0.1.0

- **A bigger world:** fourteen biomes, including jungles, savannas, swamps,
  deserts, terraced badlands, snowy taiga and mountains, plus rivers, oceans,
  rain and snow.
- **The Nether:** build an obsidian frame and light it with flint and steel.
  Expect lava seas, netherrack caverns, soul sand, glowstone and packs of
  zombified piglins. One block there is eight in the Overworld.
- **The End:** visit its floating islands and obsidian pillars with the
  `/dimension end` console command. There's no dragon or portal progression yet.
- **Fire and TNT:** fire spreads and burns out, rain puts it out, lava sets
  things alight, netherrack burns forever, and TNT and creepers blow holes in
  the landscape.
- **Armor and beds:** craft leather, iron, gold and diamond armor. Sleep
  through the night and set your respawn point.
- **Shaped blocks:** stairs, slabs, fences, fence gates, ladders, and doors
  that open. Sneak with Left Shift to build safely at the edge of a drop.
- **A first-person hand** that swings, holds your item and lowers while you
  switch items.
- **Many worlds:** a title screen with your saved worlds, plus seeds and modes
  for new ones.
- **Looks and feel:** Enhanced graphics with directional sunlight and
  reflective animated water, or the original Classic look. Combat sounds, an
  FPS counter (Options → FPS Counter) and a search field in every inventory.
- **Command console:** press **/**, **T** or **`` ` ``** (backtick) for `/give`, `/tp`,
  `/time`, `/weather`, `/gamemode`, `/setblock`, `/dimension` and `/help`.
- **Steadier simulation:** gameplay now runs at a fixed 20 ticks per second,
  like Minecraft, with smooth movement at any frame rate.
- **First multiplayer features (command line):** a world can host up to eight
  extra players controlled from a terminal, such as scripts or AI agents. Mobs
  hunt them just as they hunt you, and `/splitscreen <name>` shows any of them
  in split-screen beside your own view. The downloads include the
  `voxelcraft-agent` client, and Windows adds a **VoxelCraft (host agent
  players)** Start Menu entry; see
  [hosted players](https://github.com/BrendanH18/VoxelCraft/blob/main/docs/agents.md).
  Gamepad split-screen and LAN joining are in progress.

Worlds from 0.1.0 load in 0.2.0. New terrain generates with the new biomes
beside the areas you've already explored. Back up your worlds before
upgrading anyway, and don't open 0.2.0 worlds in 0.1.0.

## Installing

These builds have **no verified publisher signature**; Mac downloads are also
**not notarized**. Windows may show a SmartScreen warning. If you trust the
download, choose **More info → Run anyway** when available. Windows Smart App
Control and managed-device policies can block unsigned apps outright.

On Mac, drag the app into Applications and try launching it once. If blocked,
open **System Settings → Privacy & Security → Open Anyway**, then confirm Open.
See [Apple's instructions](https://support.apple.com/en-us/102445).

To update, quit the game, then run the new Windows installer over the old one,
or replace the app in Applications on Mac. Worlds, options and logs live in
`%LOCALAPPDATA%\VoxelCraft` on Windows and `~/Library/Application Support/VoxelCraft`
on Mac, and updating or uninstalling keeps them.

See the bundled **INSTALL.txt / READ ME FIRST.txt** for controls, migration
from source-build saves and troubleshooting. Checksums are in `SHA256SUMS.txt`.

This game is still in development. Report problems with your OS, GPU, steps
to reproduce and `voxelcraft.log` through
[GitHub Issues](https://github.com/BrendanH18/VoxelCraft/issues).
