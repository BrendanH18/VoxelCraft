VoxelCraft 0.9.0 is about building and moving like Java. Slabs, stairs,
doors, buttons, levers, walls and lanterns now have Java's full set of
states, almost everything can be waterlogged, and you move with Java 26.3's
own physics: the same walking, sprinting, jumping, sneaking, swimming and
climbing. Scaffolding, powder snow and honey blocks are new.

VoxelCraft is an unofficial fan project, inspired by Minecraft Java Edition. It
is not affiliated with or endorsed by Mojang Studios or Microsoft, and it
contains no Minecraft code or assets.

Choose your download:

- **Windows 10/11 x64:** `VoxelCraft-0.9.0-windows-x64-Setup.exe`
- **Mac with Apple Silicon (M-series), macOS 13+:** `VoxelCraft-0.9.0-macos-apple-silicon.dmg`

## What's new since 0.8.0

### Moving like Java

- **Java 26.3 physics:** walking (4.3 blocks a second), sprinting (5.6) and
  sneaking (1.3), the jump arc, step-up onto slabs and stairs, ice and soul
  sand, and sneaking stopping you at edges all follow Java tick for tick.
- **Sprinting** works as in Java: press sprint or double-tap forward, and it
  keeps going until you stop, run into a wall, get hungry or use an item.
  Eating, drinking and drawing a bow slow you down.
- **Crouching and crawling:** a 1.5-block gap makes you crouch and a 1-block
  gap makes you crawl, without holding sneak.
- **Water and lava:** swimming, sinking and lava movement match Java, and
  flowing water now pushes you along.
- **Ladders and vines** climb at Java's speed, and sneaking holds on.
  Cobwebs and sweet berry bushes slow you, and beds bounce you unless you
  sneak.
- The field of view widens smoothly while sprinting or flying.

### New blocks

- **Scaffolding:** place it onto scaffolding to build outward or upward. It
  falls when it's more than 7 blocks from support, collapses when you break
  what holds it up, lets you stand on top, climb inside and sneak down
  through it, and can be waterlogged.
- **Powder snow and the powder snow bucket:** you sink in and slowly freeze,
  with frozen hearts and a frost overlay. Leather boots let you walk on top,
  and any leather armour stops the freezing.
- **Honey block and honey bottle:** honey slows you down, halves your jumps,
  lets you slide slowly down its sides and softens falls. Honey bottles cure
  poison.
- **Lanterns and soul lanterns**, standing or hanging.

### Building

- **Slabs** can go on top or double up, and **stairs** turn upside down and
  form corners, with matching light and collision.
- **Doors** have left and right hinges and open by themselves when placed
  next to power. **Buttons and levers** go on floors and ceilings, and
  **grindstones** on walls and ceilings.
- **Waterlogging:** slabs, stairs, fences, walls, panes, bars, chains,
  trapdoors, rails, ladders, chests, leaves, lanterns, scaffolding and more
  hold water, which flows through their open sides as in Java.
- **Walls** keep their posts the way Java does down a stack, and fences,
  walls and panes connect by Java's rules.
- **Redstone** dust, repeaters, comparators and pressure plates stand on any
  block with a solid top, such as top slabs and scaffolding.

## Known limitations

- LAN is for VoxelCraft players on a trusted local network: there are no
  accounts or encryption, and it can't join Minecraft servers. Everyone stays
  in the host's dimension; when the host travels, the others follow. It has
  been tested between processes on one computer, not yet across Windows, Mac
  and Linux machines on a real network.
- Honey can't be collected in survival yet: bees and beehives aren't in.
  Powder snow doesn't generate in the world yet, and there are no cauldrons.
- Mobs and dropped items treat scaffolding and powder snow as plain blocks.
- Walls built in older versions show their posts until a neighbouring block
  changes.
- Undead horses aren't in yet (they need lightning, which isn't in yet either).
- Bats, frogs, polar bears and some other mobs aren't in yet, so nothing spawns
  in the ambient category.
- Surface terrain has no overhangs or floating islands, and there is no deep
  dark, ancient city, glow lichen or cave spring yet.
- Buried treasure isn't generated, so there's no way to find the heart of the
  sea.
- Big dripleaves don't tilt, and pointed dripstone doesn't fall or drip.
- There are no soul campfires, Soul Speed, spears, slime blocks or bubble
  columns, and no wither skeleton skull block, item frames or frosted ice
  (Frost Walker).
- Raids aren't in yet. Lecterns, looms, cartography tables and stonecutters are
  decorative.
- Complex redstone contraptions that depend on Java's exact update order may
  behave differently.
- The Ender Dragon can't be respawned, and the End has no chorus trees, cities,
  shulkers or elytra. The Wither can't be summoned yet.
- Commands accept `@s` and `@p` only.
- Outposts, Nether fossils and bastions use VoxelCraft's own layouts, and
  swamp-hut cats don't spawn yet.
- Spruce and jungle saplings don't grow 2x2 giant trees, and melon, pumpkin and
  beetroot crops aren't in yet.
- Bells don't swing, horse armor can't be dyed, strider jockeys don't spawn,
  boats ignore currents and bubble columns, and minecarts can't carry mobs
  across a save.
- Streaming in new terrain quickly can still cause an occasional long frame,
  and the new blocks haven't had a frame-time benchmark yet.
- World generation is VoxelCraft's own: worlds aren't seed-compatible with
  Minecraft.

## Upgrading and saves

Worlds from 0.8.0 and earlier load in 0.9.0. Back up your worlds before
upgrading, and don't open a world saved by 0.9.0 in 0.8.0: once it uses any
of the new block states, 0.8.0 can't read its terrain and would overwrite it.
LAN players all need 0.9.0.

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
