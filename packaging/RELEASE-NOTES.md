VoxelCraft 0.4.0 brings villages and redstone. Explore generated villages,
trade with villagers, protect them with iron golems, cure zombie villagers, and
build circuits, piston doors, item sorters and minecart railways.

VoxelCraft is an unofficial fan project, inspired by Minecraft Java Edition. It
is not affiliated with or endorsed by Mojang Studios or Microsoft, and it
contains no Minecraft code or assets.

Choose your download:

- **Windows 10/11 x64:** `VoxelCraft-0.4.0-windows-x64-Setup.exe`
- **Mac with Apple Silicon (M-series), macOS 13+:** `VoxelCraft-0.4.0-macos-apple-silicon.dmg`
- **Mac with Intel, macOS 13+:** `VoxelCraft-0.4.0-macos-intel.dmg`

## What's new since 0.3.0

### Villages and villagers

- **Villages:** generated in plains, desert, savanna, taiga and snowy plains,
  each in its own building style, with paths, farms, bells and loot chests.
- **Villagers:** 12 professions that claim workstations, beds and daily
  schedules (work, gather, sleep), and baby villagers that grow up.
  Leatherworkers aren't in yet.
- **Trading:** a Java-style trading screen with levels, XP, restocking,
  demand-based prices, and discounts from curing and reputation.
- **Breeding and gossip:** well-fed villagers with free beds have babies.
  Villagers remember how players treat them and share food with each other.
- **Iron and snow golems:** villages summon iron golems to defend them, and
  you can build both kinds from blocks and a carved pumpkin. Shears carve
  pumpkins.
- **Zombie villagers:** zombies hunt villagers and can infect them on Normal
  and Hard. Cure one with a splash potion of Weakness and a golden apple for
  big trade discounts.
- **Wandering trader:** visits now and then with two trader llamas and a
  random stock, drinks invisibility at night and leaves after a while.
- **Workstation blocks:** smokers, blast furnaces, barrels, grindstones,
  composters (turn plant matter into bone meal, hopper-friendly), bells
  (ring them by hand or with redstone to send villagers home) and more.

### Redstone

- **Core components:** redstone dust, torches, repeaters, comparators, levers,
  buttons, pressure plates, targets, daylight detectors, redstone blocks and
  lamps, iron doors and trapdoors.
- **Pistons and observers:** animated regular and sticky pistons, and
  observers that pulse on block changes.
- **Item automation:** dispensers, droppers and hoppers that move items
  between containers.
- **Rails and minecarts:** rails that connect into curves and slopes, powered,
  detector and activator rails, rideable minecarts, and chest, hopper and TNT
  minecarts.
- **Note blocks and tripwires:** 16 instruments chosen by the block below and
  25 pitches; tripwire hooks with string that trigger when something walks
  through.

### Polish

- The item search bar now shows only on the inventory and storage screens.

### Engine

- Measured on an Apple M5 with a release build, `--bench --rd 8`: generation
  0.22 ms per chunk and light and mesh 0.68 ms per dense chunk, unchanged
  from 0.3.0 with all the new content.

### Known limitations

- Raids, illagers, pillager outposts and farmer harvesting aren't in yet.
  Lecterns, looms, cartography tables and stonecutters are decorative.
- Piglins, hoglins, striders and Nether biomes are still to come.
- Rails don't hold water and carts aren't slowed by it. Minecart collisions
  and dismounting aren't exactly Java's, and mobs riding carts aren't saved.
  Split-screen players can't ride carts or open cart containers.
- Complex redstone contraptions that depend on Java's exact update order may
  behave differently.
- Note blocks have no note particles or mob-head sounds.
- The Ender Dragon can't be respawned, and the End has no chorus trees, cities,
  shulkers or elytra. The Wither can't be summoned yet.
- Commands accept `@s` and `@p` only.
- World generation is VoxelCraft's own: worlds aren't seed-compatible with
  Minecraft, and several structures are simplified versions of Java's.

## Upgrading and saves

Worlds from 0.3.0 and 0.2.0 load in 0.4.0. Back up your worlds before
upgrading anyway: older versions can't open 0.4.0 worlds. Versions before
0.2.0 can't read the furnace records and discard their saved contents.

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
