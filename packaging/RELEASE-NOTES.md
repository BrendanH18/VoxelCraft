VoxelCraft 0.3.0 makes a fresh survival world beatable. You can now travel
from your first tree to the Ender Dragon and the credits, with experience,
enchanting, brewing and Netherite along the way. It also adds a lot more to
build with and fight, a visible player and armor, music, particles, game rules
and commands.

VoxelCraft is an unofficial fan project, inspired by Minecraft Java Edition. It
is not affiliated with or endorsed by Mojang Studios or Microsoft, and it
contains no Minecraft code or assets.

Choose your download:

- **Windows 10/11 x64:** `VoxelCraft-0.3.0-windows-x64-Setup.exe`
- **Mac with Apple Silicon (M-series), macOS 13+:** `VoxelCraft-0.3.0-macos-apple-silicon.dmg`
- **Mac with Intel, macOS 13+:** `VoxelCraft-0.3.0-macos-intel.dmg`

## What's new since 0.2.0

### Survival path

- **Experience:** orbs drop from mobs, ore and smelting, and fill an XP bar
  with levels.
- **Endermen and ender pearls:** Endermen teleport, stare back and carry
  blocks. Pearls throw you across the landscape.
- **Blazes and fortresses:** Nether fortresses have blaze spawners, loot chests
  and nether wart. Blazes drop rods for fuel and brewing.
- **Brewing:** nether wart and a brewing stand make potions, with extended and
  strengthened versions. Gunpowder makes them splash potions.
- **Strongholds and the End portal:** craft eyes of ender, follow them to a
  stronghold and fill the portal frame to reach the End.
- **The Ender Dragon:** fight it around the obsidian pillars, shoot the end
  crystals that heal it, then step through the exit portal. Gateways lead to the
  outer islands.
- **Enchanting and anvils:** enchant at a table (bookshelves raise the levels),
  then repair, combine and rename with anvils.
- **Netherite and smithing:** mine ancient debris, smelt scrap, and upgrade
  diamond gear at a smithing table.
- **Bastion remnants:** four kinds of Nether ruin with loot, including
  Netherite upgrade templates.
- **Credits:** the first time you leave the End through the exit portal, a
  skippable scrolling credits screen plays.

### World and content

- **Mossy cobblestone and dungeons:** monster rooms with spawners and chests.
- **Abandoned mineshafts:** rails, supports, cobwebs, loot, and cave spider
  spawners in their spider corridors.
- **Ores:** copper, redstone and emerald, with raw metal drops.
- **Stones and deepslate:** granite, diorite, andesite and deepslate, plus
  blackstone, basalt, magma and chains in the Nether. Underground heights now
  follow Java's layout, with deepslate near the bottom.
- **Wood sets:** every wood type with stairs, slabs and walls.
- **Dyes and 16 colours:** dyes, wool, carpets, beds, stained glass and panes,
  terracotta, glazed terracotta, concrete powder and concrete. Dye your sheep
  and shear them.
- **Everyday items:** shears, compass and clock (with animated icons), milk
  buckets, bowls, snowballs and eggs, cake, pumpkin pie and mushrooms.
- **Crops and fishing:** carrots, potatoes, and a fishing rod with bobber that
  catches cod, salmon, junk and treasure.
- **New mobs:** cave spiders, slimes (sized, in Java's slime chunks), magma
  cubes, ghasts, wither skeletons (with Wither and Hunger), witches that throw
  splash potions, husks, drowned, and baby zombies and chickens.

### Player

- **Fullscreen:** a remembered **Fullscreen** option: Borderless (native
  fullscreen on macOS) or Exclusive at the monitor's native resolution. **F11**
  toggles it.
- **Third-person camera:** **F5** cycles first person and both third-person
  views; the camera stops at walls.
- **Player model:** a visible player with walking, swinging and sneaking
  animations, and your own body in split-screen and for hosted players.
- **Armor rendering:** worn armor shows on you, other players and mobs, with
  the enchantment glint.
- **View bobbing and hurt tilt** while walking and when damaged.
- **Swimming and crawling:** sprint in water to swim, and fit through
  one-block gaps by crawling. Hitboxes follow the pose.

### Audio and visuals

- **Procedural music:** generated, situational tracks for the Overworld, caves,
  water, creative, the Nether, the End and the credits, using Java's timing.
  Mix it with the new Music volume slider.
- **Particles:** smoke, flames, bubbles, drips, splashes, block-break debris,
  explosions, potion swirls and more, in a single draw call. Choose All,
  Decreased or Minimal in Options.

### Rules and commands

- **Difficulty:** Peaceful to Hard, affecting mob damage, hunger and poison.
- **Game modes:** Adventure, Spectator and Hardcore join Survival and Creative.
- **Game rules:** 17 typed rules, such as `keepInventory` and `doDaylightCycle`.
- **More commands with tab completion:** the console (**/**, **T** or backtick)
  completes commands and arguments with **Tab**, and accepts Java-style
  syntax for `/give`, `/gamerule`, `/difficulty`, `/gamemode`, `/effect`
  and more.

### Engine

- **Block IDs are now 16-bit** and chunks use compact byte, palette or 16-bit
  storage, so the game has room for thousands of block states.
  Worlds from 0.2.0 load unchanged.
- **2048 texture layers**, up from 256, with a fallback for GPUs that only
  support the default array-layer limit.
- **Performance:** light emission is precomputed per block state and terrain
  columns are cached across a chunk stack. This roughly halved meshing time after the new
  content arrived. Measured on an Apple M5 with a release build,
  `--bench --rd 8`: generation 0.21 ms per chunk, light and mesh 0.66 ms per
  dense chunk, and 1,576 chunks meshed in 0.17 s while streaming at 256 blocks.
  Generated terrain is identical to before the optimization.

### Split-screen and agents

- **Gamepad split-screen** has full controls, menus and shared audio for up to
  three extra players beside the keyboard player.
- **Hosted agents can sleep** and respawn at their own beds.

### Known limitations

- Villages, villagers, redstone, piglins and hoglins, Nether biomes, trading
  and LAN joining aren't in yet.
- The Ender Dragon can't be respawned, and the End has no chorus trees, cities,
  shulkers or elytra. Wither skeletons drop a skull item that can't be placed,
  so the Wither can't be summoned.
- Mobs aren't saved with a world, so they respawn when you reload it.
- Slimeballs only come from slimes. Brown and black dye can't be made, and the
  coloured beds don't rotate.
- Potions work on you; mobs only react to instant splash effects. Witches don't
  inspect their target.
- Fishing has cod, salmon and some junk and treasure only. Bobbers don't hook mobs.
- Player poses don't cover riding, elytra, bows or offhand items.
- Commands accept `@s` and `@p` only. `/locate` finds strongholds, fortresses,
  bastion remnants, mineshafts and biomes.
- Credits don't play for split-screen players or hosted agents.
- Dungeon books and loot have no enchantments or discs, and Frost Walker doesn't
  freeze water.
- World generation is VoxelCraft's own: worlds aren't seed-compatible with
  Minecraft, and several structures are simplified versions of Java's.

## Upgrading and saves

Worlds from 0.2.0 load in 0.3.0. Back up your worlds before upgrading anyway:
older versions can't open 0.3.0 worlds. Versions before 0.2.0 can't read the
furnace records and discard their saved contents.

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
