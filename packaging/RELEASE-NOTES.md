VoxelCraft 0.6.0 rebuilds the Overworld. The world now reaches from y=-64 to
y=319, with over fifty biomes, towering mountains, cheese caves and underground
lakes, lush and dripstone caves, coral reefs and kelp forests, temples and
shipwrecks to raid, and seas full of fish, dolphins, axolotls and guardians.

VoxelCraft is an unofficial fan project, inspired by Minecraft Java Edition. It
is not affiliated with or endorsed by Mojang Studios or Microsoft, and it
contains no Minecraft code or assets.

Choose your download:

- **Windows 10/11 x64:** `VoxelCraft-0.6.0-windows-x64-Setup.exe`
- **Mac with Apple Silicon (M-series), macOS 13+:** `VoxelCraft-0.6.0-macos-apple-silicon.dmg`

## What's new since 0.5.0

### A taller, Java 1.18-style Overworld

- **Build from y=-64 to y=319.** Bedrock sits at the bottom, deepslate fills
  everything below y=0, and ores are at Java's heights (diamonds peak deep down).
- **Over fifty biomes** picked by temperature, humidity, continentalness,
  erosion and weirdness, following Java's biome builder: deep, cold, warm and
  frozen oceans, mushroom fields, stony shores, sunflower plains, flower forests,
  dark forests, pale gardens, old growth birch, pine and spruce taigas,
  mangrove swamps, bamboo and sparse jungles, savanna plateaus, wooded and
  eroded badlands, meadows, cherry groves, groves, snowy slopes, ice spikes,
  windswept hills, and jagged, frozen and stony peaks. `/locate biome` and F3
  use Java's names.
- **New shapes:** continents and deep oceans, plateaus, mountains above y=200,
  winding river valleys and flat swamps.
- **Caves:** cheese caverns, spaghetti and noodle tunnels, rare ravines, and
  aquifers: underground lakes at their own levels, flooded caves near the sea
  and lava below y=-54.
- **Lush caves** with moss, azaleas, dripleaves, glow berries, spore blossoms
  and clay pools, under azalea trees whose roots reach down; **dripstone caves**
  with stalactites and stalagmites.
- **Oceans:** kelp forests, seagrass, coral reefs with fans and sea pickles,
  icebergs of packed and blue ice.
- **New trees:** dark oak, pale oak with hanging moss, mangrove with roots,
  cherry, fancy oak, mega spruce and pine, tall birch, azalea, huge mushrooms,
  jungle trees with vines and cocoa.

### New blocks and items

- About 170 new block states: podzol, coarse and rooted dirt, mycelium, mud and
  mud bricks, moss and pale moss, snow layers, blue ice, dripstone and pointed
  dripstone, amethyst, smooth basalt, lush cave plants, kelp, seagrass, sea
  pickles, coral blocks, coral and fans (live and dead), prismarine family, sea
  lanterns, sponges, turtle eggs, new leaves and saplings, pale oak wood,
  mangrove roots, bamboo and its blocks, nine new flowers, six double plants,
  lily pads, vines, sweet berry bushes, cocoa and mushroom blocks.
- **Waterlogged plants:** kelp, seagrass, sea pickles and live coral hold water
  in their cell, so you can swim through them and breaking them leaves water.
- New items: glow berries, sweet berries, cocoa beans (brown dye at last), ink
  and glow ink sacs (black dye), dried kelp, prismarine shards and crystals,
  amethyst shards, turtle scutes, nautilus shells, heart of the sea, tropical
  fish and pufferfish, and buckets of fish and axolotls.
- Recipes for dyes from every new flower, prismarine and sea lanterns, mud
  bricks, bamboo planks, dried kelp, packed and blue ice, and mossy cobblestone.
- Plants grow on random ticks and with bone meal, berries can be picked, vines
  can be climbed, and double plants break together.

### Structures

- **Desert pyramids** with a hidden TNT-trapped treasure room, **jungle temples**
  with an arrow trap, **swamp huts** with witches, **igloos** with a secret
  basement, **pillager outposts**, **shipwrecks**, **ocean ruins** and **ocean
  monuments** with guardians, elder guardians, sponges and gold. Each has its
  own seeded loot, and `/locate structure` finds them.

### Mobs

- Cod, salmon, tropical fish, pufferfish, squid, glow squid, dolphins,
  axolotls, turtles, guardians, elder guardians and pillagers.
- Fish school, axolotls hunt and play dead, dolphins grant Dolphin's Grace,
  pufferfish poison, guardians fire beams and elder guardians cause Mining
  Fatigue. Catch fish and axolotls in a water bucket.
- Turtles live on beaches. Turtle eggs placed on sand hatch into babies that
  grow up and drop a scute.

### Engine

- Measured on an Apple M5 with a release build, `--bench --rd 8`: Overworld
  generation 0.39 ms per chunk and light and mesh 0.87 ms per dense chunk
  (0.26 and 0.68 ms in 0.5.0). Columns are now 12 chunks tall instead of 8.

### Known limitations

- Worlds from 0.5.0 and earlier load, but their edited chunks won't line up
  with the new terrain. The world list marks them as "pre-0.6 terrain"; start
  a new world to explore 0.6.
- Surface terrain has no overhangs or floating islands, and there is no deep
  dark, ancient city, glow lichen or cave spring yet.
- Animal breeding, taming and leads are still to come, so turtles don't lay
  eggs; turtle helmets and Turtle Master potions aren't in yet.
- Big dripleaves don't tilt, and pointed dripstone doesn't fall or drip.
- Structures are original builds in Java's style, not copies of its templates.
  Buried treasure isn't generated yet, so there's no way to find the heart of
  the sea.
- There are no soul lanterns, soul campfires or Soul Speed, and crossbows have
  no Multishot, Piercing or Quick Charge.
- Raids aren't in yet. Lecterns, looms, cartography tables and stonecutters are
  decorative.
- Complex redstone contraptions that depend on Java's exact update order may
  behave differently.
- The Ender Dragon can't be respawned, and the End has no chorus trees, cities,
  shulkers or elytra. The Wither can't be summoned yet.
- Commands accept `@s` and `@p` only.
- World generation is VoxelCraft's own: worlds aren't seed-compatible with
  Minecraft.

## Upgrading and saves

Worlds from 0.5.0 and earlier load in 0.6.0 (see above about terrain). Back up
your worlds before upgrading anyway: older versions can't open 0.6.0 worlds.

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
