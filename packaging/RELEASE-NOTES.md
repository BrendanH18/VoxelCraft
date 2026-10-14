VoxelCraft 0.7.0 is about company and correctness. Open your world to LAN and
play with friends on the same network, tame and breed animals, saddle up horses,
camels and pigs, load llamas with chests, and sail boats across the sea. This
release also fixes more than ninety places where existing features behaved
differently from Minecraft Java Edition 26.3, from combat timing to cave
lighting, villagers and redstone.

VoxelCraft is an unofficial fan project, inspired by Minecraft Java Edition. It
is not affiliated with or endorsed by Mojang Studios or Microsoft, and it
contains no Minecraft code or assets.

Choose your download:

- **Windows 10/11 x64:** `VoxelCraft-0.7.0-windows-x64-Setup.exe`
- **Mac with Apple Silicon (M-series), macOS 13+:** `VoxelCraft-0.7.0-macos-apple-silicon.dmg`

## What's new since 0.6.0

### Open to LAN

- **Host from the pause menu:** Esc → Open to LAN, pick the game mode for
  joining players, Allow Cheats and PvP, then Start LAN World. Your world keeps
  running while your menus are open.
- **Join from the title screen:** Multiplayer lists games found on your
  network, or type an address into Direct Connect. Each computer keeps a local
  profile name, and rejoining with it brings back your inventory, position and
  bed.
- Players share the host's world and everything in it: mining and building,
  every screen (chests, furnaces, crafting, brewing, enchanting, anvils,
  grindstones, smithing, villager trading, and boat, minecart and mount
  storage), mobs, items, damage and death, sleeping and riding. You see each
  other's names, swings and mining cracks and hear each other's footsteps and
  fights. View distance follows each player's setting, up to 256 blocks.
- Chat with <kbd>T</kbd> and see who's on with <kbd>Tab</kbd>. The host can
  `/kick`, turn `/pvp` on or off (with Java's criticals, sweeps, enchantments
  and arrows) and `/save-all`. When the host goes through a portal, everyone
  follows.
- Save and Quit saves everyone and sends them back to the title screen.
- A world can only be open in one VoxelCraft window at a time.

### Tame and breed

- **Breeding:** feed two adults their favourite food and a baby appears, with
  hearts and a little experience. Cows, sheep, pigs, chickens, cats, wolves,
  foxes, rabbits, goats, horses, donkeys, llamas, camels, hoglins, striders,
  turtles and axolotls all breed. Feeding babies helps them grow up.
- Sheep regrow wool by eating grass and mix their colours in their lambs;
  chickens lay eggs, and a thrown egg sometimes hatches a chick.
- Turtles return to their home beach to lay eggs, which crack and hatch.
- **Wolves** (bones), **cats** (raw fish) and **parrots** (seeds) can be tamed.
  They sit and follow, wolves fight for you and wear dyeable collars, cats
  scare creepers and bring morning gifts, and parrots ride on your shoulder
  and mimic nearby monsters.
- **New animals:** wolves, foxes (sleep by day, steal items), parrots,
  rabbits and goats (watch out for rams).
- **Leads** (five string make two, as in Java 26.3) walk animals around and tie
  them to fences. **Name tags**, renamed at an anvil, name a mob and keep it from
  despawning.

### Ride

- **Boats and chest boats** for every wood, and bamboo rafts. Boats seat two
  (chest boats one, plus 27 stacks of storage), and ice is fast.
- **Horses, donkeys and mules** with Java's random health, speed and jump.
  Tame them by riding until they accept you, then saddle and steer; hold jump
  to charge a leap. Horse armor, and chests for donkeys and mules.
- **Llamas** in mountains and savannas carry chests sized by their strength,
  wear carpets, spit at whoever hits them (and at wolves), and follow a led
  llama in a caravan.
- **Camels** live in desert villages: saddle one to ride two players at once,
  dash forward, and step over 1.5-block ledges.
- **Pigs and striders** take a saddle; steer them with a carrot or warped
  fungus on a stick.

### New items

Boats and chest boats, saddle, carrot and warped fungus on a stick, leather,
iron, golden and diamond horse armor, golden carrot, lead, name tag, raw and
cooked rabbit, rabbit stew, rabbit hide and foot, raw and cooked mutton,
cookie, goat horns, and melon and beetroot seeds. Saddles, name tags and horse
armor turn up in dungeon, temple and fortress chests; saddles and name tags are
also fishing treasure.

### Copper gear

- **Copper tools** (pickaxe, shovel, axe, hoe and sword) and **copper armor**,
  crafted from copper ingots, with Java 26.3's durability, speed and defense.
- **Copper nuggets**: nine make an ingot. Iron, golden and copper tools, armor
  and horse armor melt down to a nugget in a furnace or blast furnace.
- Zombies and skeletons can now spawn wearing copper armor.

### Fixes: playing like Java 26.3

- **Combat:** attacks now charge up like Java's. Hitting before the charge
  finishes does less damage, and the crosshair shows the charge. Criticals,
  sweeps, sprint knockback, Haste and Mining Fatigue follow Java's rules, and
  entity reach is 3 blocks (5 in Creative). Crossbows gain Multishot, Piercing
  and Quick Charge, and bows and crossbows show arm poses.
- **Mining and building:** mining progress, tool speeds and correct tools follow
  Java's tool rules; shears drop the right loot; snow comes in layers one to
  eight high; saplings grow in two stages; barrels can face up or down; hay bales
  soften falls; Adventure players can use doors, buttons, levers and composters.
- **Lighting and spawning:** sky light now spreads properly into caves, so
  monsters spawn in dark caves by day and never in torch light, and crops and
  saplings grow from real light. Thunderstorms, daylight detectors and the
  spawn-point exclusion match Java, and the End and Nether have their proper
  ambient light.
- **Containers and reach:** chests, furnaces, trading and vehicle storage close
  when you walk out of range, and right-click actions work the same with a
  mouse, a controller, over LAN and for command agents.
- **Mobs and villagers:** villagers sleep in their beds, close doors behind them,
  farmers harvest and replant crops, and promotions take Java's short delay.
  Iron golems turn on players villagers dislike. Piglins celebrate a kill and
  zombified piglins keep their weapons; witches choose and drink potions like
  Java's; camels sit; horses in a herd share a coat; every mob can climb ladders
  and vines.
- **Fishing and food:** fishing uses Java 26.3's full loot tables, with luck;
  the hook can snag mobs; pufferfish and other foods give their full effects.
- **Redstone:** hoppers, pressure plates, dispensers (fire charges, bone meal),
  minecarts in water, note block particles and tripwire sounds follow Java more
  closely.
- **World:** jungle temples now have their lever puzzle and traps, strongholds
  use iron doors, structure chests use Java 26.3's loot tables, amethyst geodes
  generate, and Nether biome edges blend as in Java.
- **Performance:** large mob groups and basalt delta generation are faster, and
  lighting work is spread across ticks.

### Known limitations

- LAN is for VoxelCraft players on a trusted local network: there are no
  accounts or encryption, and it can't join Minecraft servers. Everyone stays
  in the host's dimension; when the host travels, the others follow. It has
  been tested between processes on one computer, not yet across Windows, Mac
  and Linux machines on a real network.
- Undead horses aren't in yet (they need lightning, which isn't in yet either).
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
- Mobs steer in straight lines with sidesteps rather than Java's pathfinding,
  and outposts, Nether fossils and bastions use VoxelCraft's own layouts.
- Light passes through slabs and stairs as if they were full blocks, and slabs,
  stairs and fences can't be waterlogged.
- Doors have no hinge side, buttons and levers can't go on floors or ceilings,
  and grindstones only stand on the floor.
- Spruce and jungle saplings don't grow 2x2 giant trees, and melon, pumpkin and
  beetroot crops aren't in yet.
- Bells don't swing, horse armor can't be dyed, strider jockeys don't spawn,
  boats ignore currents and bubble columns, and minecarts can't carry mobs
  across a save.
- Controller menus, beds and respawning, and LAN between different machines,
  haven't had a full manual test pass.
- World generation is VoxelCraft's own: worlds aren't seed-compatible with
  Minecraft.

## Upgrading and saves

Worlds from 0.6.0 load in 0.7.0, and the new animals appear as you explore. Worlds from 0.5.0 and earlier load too, with the terrain caveat
from 0.6.0. Back up your worlds before upgrading anyway: older versions can't
open 0.7.0 worlds.

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
