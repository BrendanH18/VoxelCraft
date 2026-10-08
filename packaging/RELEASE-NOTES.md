VoxelCraft 0.5.0 brings the Nether to life. Explore crimson and warped
forests, soul sand valleys and basalt deltas, barter gold with piglins, fight
hoglins and piglin brutes in bastions, and watch striders cross the lava seas.

VoxelCraft is an unofficial fan project, inspired by Minecraft Java Edition. It
is not affiliated with or endorsed by Mojang Studios or Microsoft, and it
contains no Minecraft code or assets.

Choose your download:

- **Windows 10/11 x64:** `VoxelCraft-0.5.0-windows-x64-Setup.exe`
- **Mac with Apple Silicon (M-series), macOS 13+:** `VoxelCraft-0.5.0-macos-apple-silicon.dmg`
- **Mac with Intel, macOS 13+:** `VoxelCraft-0.5.0-macos-intel.dmg`

## What's new since 0.4.0

### Nether biomes

- **Five biomes:** nether wastes, crimson forest, warped forest, soul sand
  valley and basalt deltas, placed by Java's multi-noise biome source. Each has
  its own fog colour, ambient particles (spores and ash) and music. The F3
  screen and `/locate biome` show them.
- **Forests:** huge crimson and warped fungi, nylium floors, roots, sprouts,
  shroomlights, and weeping and twisting vines that grow.
- **Soul sand valleys and basalt deltas:** fossils, soul fire, basalt pillars
  and columns, and delta lava pools.
- **New blocks:** crimson and warped stems, hyphae, planks and every wood
  shape (stairs, slabs, fences, gates, doors, trapdoors, buttons and pressure
  plates), wart blocks, shroomlight, soul soil, soul fire and soul torches.
  Nether wood doesn't burn, like Java.
- **Bone meal:** spreads nylium onto netherrack, grows forest floor plants and
  vines, and grows a planted fungus into a huge fungus.
- Bone blocks now have Java's three orientations.

### Nether mobs

- **Piglins:** they attack players who wear no gold armor and get angry if you
  open chests or break gold blocks near them. Throw them gold to barter, using
  Java 1.21's bartering table. Outside the Nether they turn into zombified
  piglins after 15 seconds.
- **Bastion residents:** bastions come with piglins, piglin brutes that are
  always hostile and swing golden axes, and penned hoglins.
- **Hoglins and zoglins:** hoglins charge and toss players, avoid warped
  fungus and portals, and turn into zoglins outside the Nether. Zoglins attack
  everything.
- **Striders:** they walk on lava and shiver when out of it.
- **Biome spawning:** each biome spawns its own mobs, so hoglins live in
  crimson forests while warped forests hold only endermen (and striders on the lava).
- **Crossbow:** craft and load one, then fire. Crossbow piglins use them too.
- **Crying obsidian:** from bartering and bastion chests.

### Engine

- Measured on an Apple M5 with a release build, `--bench --rd 8`: Overworld
  generation 0.25 ms per chunk, Nether generation 0.21 ms per chunk, and light
  and mesh 0.68 ms per dense chunk, unchanged from 0.4.0.

### Known limitations

- Hoglins and striders can't be bred, and striders can't be ridden or
  saddled. Animal breeding, taming and leads are still to come.
- There are no soul lanterns, soul campfires or Soul Speed, and crossbows have
  no Multishot, Piercing or Quick Charge.
- Nether chunks saved by older versions keep their old terrain, so explored
  areas meet the new biomes at a visible edge.
- Raids, illagers, pillager outposts and farmer harvesting aren't in yet.
  Lecterns, looms, cartography tables and stonecutters are decorative.
- Rails don't hold water and carts aren't slowed by it. Minecart collisions
  and dismounting aren't exactly Java's, and mobs riding carts aren't saved.
  Split-screen players can't ride carts or open cart containers.
- Complex redstone contraptions that depend on Java's exact update order may
  behave differently.
- The Ender Dragon can't be respawned, and the End has no chorus trees, cities,
  shulkers or elytra. The Wither can't be summoned yet.
- Commands accept `@s` and `@p` only.
- World generation is VoxelCraft's own: worlds aren't seed-compatible with
  Minecraft, and several structures are simplified versions of Java's.

## Upgrading and saves

Worlds from 0.4.0, 0.3.0 and 0.2.0 load in 0.5.0. Back up your worlds before
upgrading anyway: older versions can't open 0.5.0 worlds. Versions before
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
