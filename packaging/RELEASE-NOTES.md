VoxelCraft 0.8.0 is about foundations. Mobs now find their way around the
world like Java's do, walking, swimming, flying and climbing along real paths
instead of heading straight at their goal. Natural spawning follows Java 26.3's
mob categories and biome spawn lists, light respects slabs and stairs, and a
new simulation distance setting keeps the world ticking around you. Death and
respawn, game rules, difficulty, absorption and knockback now match Java too.

VoxelCraft is an unofficial fan project, inspired by Minecraft Java Edition. It
is not affiliated with or endorsed by Mojang Studios or Microsoft, and it
contains no Minecraft code or assets.

Choose your download:

- **Windows 10/11 x64:** `VoxelCraft-0.8.0-windows-x64-Setup.exe`
- **Mac with Apple Silicon (M-series), macOS 13+:** `VoxelCraft-0.8.0-macos-apple-silicon.dmg`

## What's new since 0.7.0

### Mobs that find their way

- **Pathfinding:** mobs plan routes with Java 26.3's pathfinder, with each
  mob's step height, fall limit and path costs. Monsters chase you around
  walls, villagers open the doors on their way, and tempted, breeding, leashed
  and following animals walk to you instead of getting stuck.
- **Swimmers and fliers:** fish (in schools that follow a leader), dolphins,
  guardians, drowned, turtles, squid, axolotls, parrots, ghasts and blazes
  move with Java's swim, amphibious and fly navigation and move controls.
  Spiders keep climbing walls toward you, and idle drowned sink.
- **Striders** walk along lava paths, head back to lava when they're ashore,
  and stand in lava as they do in Java.
- **Dolphins** swim with players and give Dolphin's Grace, lead you to ocean
  ruins and shipwrecks when fed fish, fetch and toss items, leap, follow
  boats, avoid guardians and gang up on attackers.
- **Turtles** travel, return to their home beach, lay eggs and head back to
  the water, as in Java.
- Elder guardians stay near their monument; blazes alert each other when one
  is hit; ghasts and blazes fire at Java's cadence.

### Spawning

- Monsters, animals and water mobs spawn from Java 26.3's biome spawn lists
  and caps, counted around each player. Swamp
  huts, ocean monuments, pillager outposts and Nether fortresses spawn their
  own mobs.
- New chunks get their one-time herds of animals, with shared pack coats and
  colours and Java's chance of babies. Monsters despawn when you move far away,
  as in Java.

### World and settings

- **Simulation distance** (Options): crops, fire, fluids, redstone and mobs
  tick within this distance of you, and pause rather than reset beyond it.
  Pending redstone and falling blocks are kept when their chunks unload.
- **Light** is blocked by slabs, stairs, snow layers and farmland according to
  their shape, and the block outline matches each block's shape.
- **Game rules:** all 59 Java 26.3 game rules are known by their new and old
  names. `/gamerule` reports any rule whose system isn't in the game yet.
- New chunks with dungeons and amethyst geodes generate off the main thread,
  so exploring stutters less.

### Fixes: playing like Java 26.3

- **Death and respawn** work the same with mouse and keyboard, a controller,
  over LAN and for command agents. You drop your items and experience in every
  mode but Spectator unless `keepInventory` is on. Your vehicle is left behind,
  and controller and LAN players see the "Your home bed was missing" notice.
- **Combat:** sweeping attacks and knockback follow Java's impulses, and
  golden apples give absorption hearts with Java's cap and expiry.
- **Mobs:** mob goals follow Java 26.3's goal tables, including how often they
  follow, tempt, breed and flee; wolves no longer panic when hit, and axolotls
  don't panic when hurt.
- **Beds:** you stand up from a bed where Java puts you.
- **Saves:** mobs in unloaded chunks are kept with their chunk until you
  return, and the world saved mid-way through a piston push or repeater pulse
  reloads exactly as it was.

### Known limitations

- LAN is for VoxelCraft players on a trusted local network: there are no
  accounts or encryption, and it can't join Minecraft servers. Everyone stays
  in the host's dimension; when the host travels, the others follow. It has
  been tested between processes on one computer, not yet across Windows, Mac
  and Linux machines on a real network.
- Undead horses aren't in yet (they need lightning, which isn't in yet either).
- Bats, frogs, polar bears and some other mobs aren't in yet, so nothing spawns
  in the ambient category.
- Surface terrain has no overhangs or floating islands, and there is no deep
  dark, ancient city, glow lichen or cave spring yet.
- Buried treasure isn't generated, so there's no way to find the heart of the
  sea.
- Big dripleaves don't tilt, and pointed dripstone doesn't fall or drip.
- There are no soul lanterns, soul campfires, Soul Speed or spears, and no
  wither skeleton skull block, item frames, frosted ice (Frost Walker) or
  cauldrons.
- Raids aren't in yet. Lecterns, looms, cartography tables and stonecutters are
  decorative.
- Complex redstone contraptions that depend on Java's exact update order may
  behave differently.
- The Ender Dragon can't be respawned, and the End has no chorus trees, cities,
  shulkers or elytra. The Wither can't be summoned yet.
- Commands accept `@s` and `@p` only.
- Outposts, Nether fossils and bastions use VoxelCraft's own layouts, and
  swamp-hut cats don't spawn yet.
- Blazes on the ground still use VoxelCraft's own walking.
- Slabs, stairs and fences can't be waterlogged.
- Doors have no hinge side, buttons and levers can't go on floors or ceilings,
  and grindstones only stand on the floor.
- Spruce and jungle saplings don't grow 2x2 giant trees, and melon, pumpkin and
  beetroot crops aren't in yet.
- Bells don't swing, horse armor can't be dyed, strider jockeys don't spawn,
  boats ignore currents and bubble columns, and minecarts can't carry mobs
  across a save.
- Streaming in new terrain quickly can still cause an occasional long frame.
- Controller menus, beds and respawning, and LAN between different machines,
  haven't had a full manual test pass.
- World generation is VoxelCraft's own: worlds aren't seed-compatible with
  Minecraft.

## Upgrading and saves

Worlds from 0.7.0 and earlier load in 0.8.0; animal herds aren't doubled in
areas you've already explored. Back up your worlds before upgrading anyway:
older versions may not load everything in a world saved by 0.8.0. LAN players
all need 0.8.0.

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
