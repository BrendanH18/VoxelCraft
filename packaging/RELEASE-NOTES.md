VoxelCraft 0.17.0 is the redstone and automation update. **Redstone:**
signals now travel in Java's update order, so contraptions built from Java
designs behave the way they do in Java. **Pistons:** slime and honey blocks
pull their neighbours along, so flying machines fly, and pistons push mobs
and players. **The crafter:** a block that crafts on a redstone pulse.
**And more:** dispensers that handle every item as Java does, the furnace
minecart, hoppers with Java's timing, sculk sensors that hear everything
Java's hear, and workstations that match Java. If you have 0.10.0 or later,
the title screen offers this update; choose **Update and Restart**. If
you're coming from 0.9.0 or earlier, install it by hand once, as below, and
it updates itself from then on.

VoxelCraft is an unofficial fan project, inspired by Minecraft Java Edition. It
is not affiliated with or endorsed by Mojang Studios or Microsoft, and it
contains no Minecraft code or assets.

Choose your download:

- **Windows 10/11 x64:** `VoxelCraft-0.17.0-windows-x64-Setup.exe`
- **Mac with Apple Silicon (M-series), macOS 13+:** `VoxelCraft-0.17.0-macos-apple-silicon.dmg`

## What's new in 0.17.0

### Redstone in Java's order

- Blocks update their neighbours in Java's order and directions, and
  repeaters, comparators and observers take their turns as Java's do.
  Block update detectors, observer clocks, hopper clocks, item sorters and
  T flip-flops built from Java designs work, and keep working after you
  leave the area, save or restart.
- Every redstone component was checked against Java's: comparators read
  every container correctly, and observers notice fire burning.

### Pistons

- Slime and honey blocks move the blocks stuck to them, so flying machines
  and block-moving doors work. Every block moves, breaks or refuses to move
  as it does in Java: pumpkins and melons break off, for example.
- Pistons push mobs and players. Slime launches them, and honey carries
  them along.
- A short pulse makes a sticky piston spit its block out, as in Java.
- A moving block survives a save part way through its move.

### The crafter

- Craft with redstone: fill the crafter's grid by hand or with hoppers and
  give it a pulse, and it crafts one item and pushes it out of its face,
  into a container if there is one. Click an empty slot to switch it off,
  so hoppers fill only the slots you want. A comparator reads how many
  slots are filled.

### Dispensers, droppers and hoppers

- Dispensers handle items the way Java's do: they put armour, heads,
  saddles and carpets on mobs and players, shear, place shulker boxes and
  pumpkins, fill and empty buckets and bottles, use bone meal and flint and
  steel, light TNT, place boats and minecarts, and more.
- Hoppers pick up items from the area Java's do and move items on Java's
  timing, and every container takes items from the sides Java's do.

### Carts

- The furnace minecart: fuel it with coal or charcoal and it pushes itself
  and the carts in front of it.
- Rails can sit on hoppers, so a hopper under the track empties hopper and
  chest minecarts as they pass.
- Carts push and pick up mobs the way Java's do, slow down on waterlogged
  rails, and roll with their own sounds. A lit TNT minecart doesn't destroy
  the rails it sits on, and burning arrows and fireballs set it off.
- Villagers, wandering traders and golems stay in their cart when you save.

### Containers and workstations

- Furnaces, brewing stands, enchanting tables, anvils, grindstones and
  smithing tables follow Java's rules and costs. Hoppers no longer restart a
  furnace's cooking each time they top up its input.
- Four cases where moving items between containers, hoppers and carts lost
  items or missed a comparator update are fixed.
- On a keyboard, a number key over a container slot swaps it with that
  hotbar slot, and the drop key throws the item from the slot under the
  pointer.

### Sculk sensors

- Sculk sensors and the warden hear the same actions as in Java, including
  redstone, pistons, dispensers, projectiles, containers and doors.

### Farms

- Sugar cane grows at Java's rate: it was growing four times too fast.
  Observer and piston sugar cane, pumpkin and melon farms work.

## What's new in 0.16.0 (if you're coming from 0.15.0 or earlier)

### The outer End (new worlds only)

- End cities and End ships on the outer islands, laid out by Java's rules,
  with their treasure chests, shulkers and `/locate`. Each ship carries an
  elytra in an item frame and a dragon head on its bow.
- Chorus plants and flowers grow in the highlands; chorus fruit teleports
  you, and popped chorus fruit makes purpur blocks, pillars, stairs and
  slabs. End rods.
- Shulkers peek, shoot Levitation bullets, teleport, and multiply when hit
  by their own bullets. They drop shulker shells.

### Flight and storage

- All 17 shulker boxes keep their contents when broken, can be dyed and
  washed in a cauldron, give comparator output and can't be nested.
- Elytra glide with Java's flight physics, wear out, are mended with
  phantom membrane, and hurt you if you fly into a wall.
- Firework stars and rockets in every shape, colour and fade, with trails
  and twinkles. Rockets boost elytra flight and can be fired from a
  crossbow.

### The Ender Dragon, again

- Place four End crystals on the exit portal to summon the dragon again,
  through every stage of Java's ritual. Later kills give 500 experience
  and open a new End gateway. The ritual survives saving mid-way.
- The dragon now appears at Java's height, and new worlds' End uses Java's
  spikes, gateways and exit podium.

### The Wither and beacons

- Every mob head and skull, on the floor or on a wall. Wear one to make
  its mob notice you later. Charged creepers blow heads off, wither
  skeletons drop their skulls, and note blocks play the head's mob.
- Build the Wither in any orientation. It charges up, fires skulls,
  armours itself at half health, breaks blocks and drops a Nether star.
  Mobs it kills leave wither roses.
- Beacons with pyramids of every size, coloured beams, primary and
  secondary powers and their own screen. Dispensers place wither skeleton
  skulls where they complete a Wither.

### New worlds: generation 5

- Poplar forests with poplar wood, boats and three colours of leaves;
  shrubs, golden dandelions and shelf mushrooms.
- Sulfur caves with cinnabar, sulfur, sulfur spikes and potent sulfur,
  whose geysers throw you into the air and whose gas causes Nausea. Sulfur
  springs. The sulfur cube, which can be caught in a bucket and eats the
  blocks you feed it.
- Abandoned camps with their loot and the abandoned camp map.
- Strongholds, mineshafts and fortresses built piece by piece the way Java
  builds them, with infested blocks in strongholds and mountains;
  silverfish hide in stone and wake their friends.
- Shipwrecks and ocean monuments rebuilt from Java's rules, with three
  elder guardians in every monument.
- Nether gold ore.

### The conduit

- Build a conduit frame from prismarine to get Conduit Power underwater:
  water breathing, faster mining and clear sight. A full frame attacks
  hostile mobs nearby.

### Fixes

- LAN guests can save long books without being disconnected, and each LAN
  guest hears its own raid horn.
- A dragon ritual started just before saving carries on after reloading.

## What's new in 0.15.0 (if you're coming from 0.14.0 or earlier)

### Raids and illagers

- Vindicators, evokers (with their fangs and vexes, and the blue sheep they
  turn red), ravagers and illusioners (by command only, as in Java).
  Pillagers aim and reload their crossbows the way Java's do.
- Mobs hold weapons and tools and wear armor that count in a fight, and
  mobs that can pick up loot equip what they find.
- Pillager patrols spawn in daylight, led by a captain carrying the
  ominous banner. Pillager captains drop an ominous bottle; drinking it
  gives Bad Omen, which becomes Raid Omen in a village and starts a raid.
- Raids follow Java's waves for each difficulty, with a bonus wave, riders
  on ravagers and the raid bar. Villagers run to shelter when the bell
  rings. Win and you get Hero of the Village: lower prices and gifts from
  the villagers. The `raids` and `spawn_patrols` game rules work.

### Villages and villagers (new worlds only for buildings)

- Villages are built from Java's full set of building pools: profession
  houses with their job sites, decorations, animal pens and abandoned
  villages.
- Villagers trade Java 26.3's default offers, with Java's prices, uses,
  restocks and levelling. They follow Java's daily schedules, gather at the
  meeting bell, spread gossip and remember a cure.
- The wandering trader sells Java's sets, except five offers whose items
  aren't in the game yet.

### Woodland mansions and maps

- Woodland mansions generate in dark forests in new worlds, with their
  rooms, vindicators and evokers, allays in the jail cells, and their loot.
- In worlds created in 0.15.0, the master cartographer sells the woodland
  mansion map. Desert pyramid, warm ocean ruin, buried mineshaft and buried
  ancient city maps exist too.
- Trial chambers and ancient cities have Java's full set of room kinds in
  new worlds.

### Workstations and books

- Stonecutters cut stone blocks one step at a time.
- Looms weave all of Java's banner patterns, including the ten pattern
  items. Shields carry banner patterns, cauldrons wash banners, and raid
  captains carry the ominous banner.
- Cartography tables copy, zoom out and lock maps.
- Write in a book and quill, sign it, and put it on a lectern to read and
  turn pages; a lectern powers a comparator.
- The spyglass zooms in.

### New mobs

- Happy ghasts grow from dried ghasts kept in water. Put a harness on one
  and up to four players can ride it.
- Nautiluses and zombie nautiluses swim in the oceans and take nautilus
  armor. Camel husks roam deserts with their riders, and the parched shoot
  weakness arrows.
- Chickens, cows and pigs come in warm and cold variants. Warm chickens lay
  brown eggs and cold chickens lay blue eggs.

### Items and effects

- Armor trims: the 18 smithing templates and every trim material.
- Spectral arrows make their target glow.
- Frost Walker freezes water into frosted ice.
- Glowing, Levitation, Health Boost, Wind Charged, Weaving, Oozing and
  Infested all act, and the four new potions (Wind Charging, Weaving,
  Oozing and Infestation) can be brewed. Every Java brewing recipe matches.

### Fixes

- A LAN guest's book written from the off-hand is saved in the off-hand.

## What's new in 0.14.0 (if you're coming from 0.13.0 or earlier)

### Structures (new worlds only)

Worlds created in 0.14.0 use a new structure generator. Structures are
placed where Java places them, from Java's pools, and filled with Java's
loot, but the buildings are VoxelCraft's own designs, so a Java seed
won't give you the same village.

- **Villages** in all five styles are assembled piece by piece from town
  centres, streets, houses, farms, pens and lamps. The ground is shaped
  to meet them instead of leaving houses on pillars.
- **Pillager outposts** and **bastion remnants** are rebuilt the same way,
  with their guards, and outposts keep two allays in their cage.
- **New structures:** Overworld and Nether fossils, desert wells, ruined
  portals (in the Overworld and the Nether), buried trail ruins, buried
  treasure on beaches (with the heart of the sea), warm and cold ocean
  ruins with drowned, **ancient cities** deep in the deep dark, and
  **trial chambers** underground.
- **Maps:** empty and filled maps, explorer and buried treasure maps in
  structure chests, and cartographers who sell explorer maps, including
  ones to ocean monuments and trial chambers.
- Newly generated chests in every world fill the way Java fills them.

### Weather and lightning

- Thunderstorms strike lightning. It starts fires, charges creepers, turns
  pigs into zombified piglins, villagers into witches and mooshrooms
  between red and brown, and cleans the age off copper. Lightning rods
  draw strikes and give a redstone pulse.
- Skeleton horse traps appear in storms. Skeleton and zombie horses can
  be ridden.
- Snow piles up in layers during snowfall.

### Farming and bees

- **Melons, pumpkins and beetroots** grow from seeds, with stems that
  attach to their fruit.
- **Bees, bee nests and beehives:** bees pollinate flowers, help crops
  grow and make honey. Collect honey bottles and honeycomb (use a
  campfire to calm them). Honeycomb **waxes copper** and signs, and
  candles can now be crafted.
- **Torchflowers and pitcher plants** grow from seeds that sniffers dig up.

### Animals and mobs

- **New animals:** bats, frogs (three kinds, with tadpoles, frogspawn and
  froglights), pandas with their personalities, polar bears, armadillos
  (brush them for scutes and make **wolf armor**), mooshrooms and
  suspicious stew, ocelots, and village and swamp-hut cats.
- **Copper golems:** build one from a carved pumpkin on a copper block.
  It sorts items out of copper chests into chests nearby and turns into a
  statue as it ages; scrape it with an axe to wake it.
- **The creaking** guards pale oak trees at night, freezes while you look
  at it, and can only be killed by breaking its creaking heart.
  **Eyeblossoms** open at night, and resin makes a new brick family.
- **Allays** collect the items you give them, follow note blocks and
  dance to jukeboxes. A dancing allay given an amethyst shard duplicates.
- **Sniffers** hatch from eggs found by brushing suspicious sand in warm
  ocean ruins.
- **New hostiles:** strays, bogged, phantoms (if you go too long without
  sleep), the **warden** and the **breeze**. Zombies turn into drowned
  underwater and husks into zombies.
- **Brushes** uncover suspicious sand and gravel, which hold pottery
  sherds and other finds.

### The deep dark

- **Sculk sensors** hear vibrations (wool muffles them) and give a
  redstone signal; calibrated sensors listen for one kind.
- **Sculk shriekers** warn you, bring Darkness and, in ancient cities,
  summon the warden.
- **Sculk catalysts** spread sculk where mobs die.
- **The warden** follows sound and smell, attacks with a sonic boom, and
  digs back down if left alone.
- **Ancient cities** hold echo shards, the **recovery compass**, Swift
  Sneak books, disc fragments and music discs.
- **Jukeboxes** play all 22 music discs (with VoxelCraft's own tunes),
  power comparators and make allays dance.

### Trial chambers

- **Trial spawners** call waves that grow with the number of players and
  then cool down for half an hour. **Ominous** trials start when you
  enter with Bad Omen, which becomes Trial Omen; drink an **ominous
  bottle** to get it.
- **Vaults** give each player their own reward for a **trial key**;
  ominous vaults take ominous keys.
- **The breeze** jumps, slides and shoots wind charges, and drops breeze
  rods. Throw **wind charges** to launch yourself.

### Weapons

- **The mace** (heavy core and breeze rod): hit while falling for a smash
  attack that resets your fall. Density, Breach and Wind Burst
  enchantments.
- **Tridents:** throw them, and enchant them with Loyalty, Riptide,
  Channeling or Impaling. Drowned can carry and throw them.
- **Spears** in every material: jab, or hold to charge, and the Lunge
  enchantment.

### Brewing and trades

- Brewing now matches Java's recipes: fermented spider eyes, Weakness,
  Night Vision from golden carrots, Leaping from rabbit's feet, every
  corrupted potion, and **Invisibility** (mobs see an invisible player
  only up close). Slow Falling, tipped arrows and lingering potions are in.
- Nine villager and wandering-trader offers that named the wrong item
  are back.

### Mob gear and new items

- **Mobs wear and drop their gear.** Every mob has Java's eight equipment
  slots with Java's drop chances: zombies can spawn holding an iron sword,
  spear or shovel, armor is enchanted on harder difficulties, Looting raises
  the odds, and what a mob picked up or was given is always dropped.
- **Zombie reinforcements.** Hurt zombies, husks, drowned and zombie
  villagers can call others to help, and about one in twenty is a leader
  with more health that can break doors. Every mob rolls Java's follow-range
  bonus.
- **Endermites** sometimes appear where an ender pearl lands, despawn after
  two minutes, and are hunted by endermen.
- **Chainmail armor** from armorer trades, **copper and netherite horse
  armor**, and **nautilus armor** items (nautiluses come later).
- **Bundles** in seventeen colours, with Java's weight rule, village loot
  and dyeing. **Harnesses** in sixteen colours can be crafted and dyed;
  happy ghasts to wear them come later.
- **Bottle o' Enchanting**: throw it for experience, fire it from a
  dispenser, buy it from clerics or find it in chests.
- **Dyed leather armor**: mix dyes on leather armor at a crafting table and
  wash it off in a cauldron.

### Effects, damage and enchantments

- **Eleven more status effects**: Health Boost, Glowing, Levitation,
  Conduit Power, Hero of the Village, Raid Omen, Wind Charged, Weaving,
  Oozing, Infested and Breath of the Nautilus, with `/effect` support.
  Effects can now be infinite, ambient or hidden, and stack the way Java's
  do. Some (such as Raid Omen) wait for the features that use them.
- **Damage types** follow Java's 51 types and their tags, so armor,
  protection enchantments, Fire Resistance and the damage game rules apply
  to exactly the damage they do in Java, and death messages match.
- All of Java's **enchantment definitions**, with their item and exclusivity
  rules (Soul Speed still isn't in).
- The `forgive_dead_players` and `universal_anger` game rules now work for
  every neutral mob.

### Performance

- Mobs are drawn in one batch on the GPU, and crowded scenes with mobs
  run faster.
- Mobs looking up the biome they're in no longer rebuild terrain, which
  cuts long frames while new land streams in.

## What's new in 0.13.0 (if you're coming from 0.12.0 or earlier)

### New terrain (new worlds only)

- The surface is no longer a heightmap: cliffs overhang, arches and
  overhangs form, and mountain peaks are jagged. Plains and oceans stay
  gentle. Steep faces and the undersides of overhangs show stone (or
  sandstone under sand), and badlands bands run through the cliffs.
- Caves and underground lakes follow the new shape, and caves open onto
  the surface.
- Worlds created before 0.13.0 keep the terrain they had, including the
  parts nobody has explored yet. Only worlds created in 0.13.0 or later get
  the new terrain.

### Underground

- **The deep dark:** far under mountainous ground, a cave biome of sculk,
  sculk veins, sculk catalysts, sculk sensors and sculk shriekers. They
  can be mined and placed, but the sensors and shriekers don't react yet:
  the warden comes in 0.16.
- **Glow lichen** grows on cave walls and gives off light. Shears collect
  it, and bone meal spreads it.
- **Water and lava springs** pour from cave and cliff walls.
- **Large ore veins:** copper through granite and iron through tuff deep
  underground, with blocks of raw copper and raw iron. Dripstone caves
  have bigger copper ore.
- All of this is generated in new worlds only, but the blocks themselves
  work everywhere.

### Trees and plants

New worlds have:

- **Giant spruce and pine trees** in old growth taigas, with podzol and
  coarse dirt around them, and mossy cobblestone boulders.
- **Giant jungle trees** with vines up the trunk and cocoa pods.
- Leaf litter, wildflowers, bushes, firefly bushes, short and tall dry
  grass, cactus flowers and fallen logs.

In every world, four spruce or jungle saplings in a square grow one of the
new giant trees.

### Bubble columns

- Water above soul sand pushes you up, and water above a magma block pulls
  you down. Inside a column you don't run out of air. Columns form in the
  world wherever there's water above magma.

### Off-hand and shields

- **The off-hand slot:** press **F** to swap the held item into your
  off-hand. It shows beside the hotbar and in your inventory. Torches,
  blocks, food and buckets work from it.
- **Flying in creative has moved from F to R.** If you never changed the
  key, your options move it for you.
- **Shields** (planks and an iron ingot): hold right click to block melee
  hits, arrows and explosions from the front. A blocked axe hit disables
  the shield for five seconds.
- **The totem of undying**, held in either hand, saves you from death,
  except in the void or from `/kill`. There's no way to get one in
  survival yet: evokers come with raids in 0.15.

### Fire

- How easily blocks catch fire and burn away now follows Java for more
  blocks: bookshelves, lecterns, composters, hay bales and mangrove roots
  burn, saplings, mushrooms and lily pads don't, and vines and dried kelp
  catch more slowly.

## What's new in 0.12.0 (if you're coming from 0.11.0 or earlier)

### Wood

- Logs, stems and the bamboo block lie along the axis you place them on, and
  an axe strips every log, wood and bamboo block. All nine `*_wood` blocks and
  their stripped forms are in.
- Trapdoors, buttons and pressure plates for every wood, and the polished
  blackstone button and plate.
- **Signs and hanging signs** of all 12 woods, standing, on walls and
  hanging. Write on both sides, colour the text with dyes and make it glow
  with glow ink. Controller players write with an on-screen keyboard, and
  LAN players get the same editor as the host.
- **Shelves** of every wood (three items, swapped with your hotbar, and
  chained three wide) and the **chiseled bookshelf**.

### Stone and minerals

- Polished tuff and tuff bricks; deepslate bricks and tiles; quartz, smooth,
  cut and chiseled sandstone and red sandstone; red nether bricks; end stone
  bricks; mossy stone brick shapes; blocks of coal and diamond; tinted glass.
- Every stairs, slab and wall of those families, and 99 stonecutter recipes
  (the stonecutter screen itself comes later).
- **Slime blocks:** you, mobs and dropped items bounce on them, and walking
  over one is slow.
- Calcite, deepslate and smooth stone stairs, slabs and walls and polished
  granite, diorite and andesite walls aren't in Java, so they can no longer be
  crafted. Blocks you already placed stay.

### Copper

- Every copper block, cut copper, stairs, slab, chiseled copper, grate, bulb,
  door, trapdoor, bars, chain, lantern, chest, lightning rod and the copper
  torch, in all four stages of age. Copper ages slowly; an axe scrapes it
  back. Waxing comes with bees.

### Colour

- **Banners** in 16 colours, standing and on walls (patterns come with the
  loom).
- **Candles** in 17 colours, up to four to a block, lit with flint and steel
  or a Flame arrow, and candle cakes. Candles can't be crafted until bees
  bring honeycomb.
- Wool and concrete stairs and slabs, the straw bed and the 16 cushions you
  can sit on.

### Around the house

- **Item frames** and glow item frames, **paintings** and **armor stands**.
  Arrows knock items out of frames and break frames, paintings, armor
  stands and cushions, and wear down boats and minecarts.
- **Cauldrons** hold water, lava and powder snow, fill from buckets,
  bottles, rain, snow and dripping stalactites, and put out a burning player
  (using up a level).
- **Campfires** and soul campfires cook, hurt, send up smoke signals and can
  be put out and relit.
- Flower pots, decorated pots, coral wall fans, falling and dripping pointed
  dripstone and tilting big dripleaves.
- **Lodestones:** use a compass on one and it points there, in any
  dimension, until the lodestone is broken. **Respawn anchors** charged with
  glowstone set your spawn in the Nether and explode anywhere else.
- **Chests, trapped chests and copper chests** pair into double chests, and
  every player has their own **ender chest**.

### Java's data

- Every recipe, drop, mining speed, fuel, compostable and repair material
  for the blocks and items in the game now matches Java 26.3, apart from
  dyeing leather, repairing in the crafting grid and copying banners.
- Explosions follow Java: blast resistance shields what's behind it, blocks
  drop by their loot rules, and items and experience lying in the blast are
  destroyed.

### Fixes

- A minimised or hidden game window no longer grows by gigabytes of memory
  while chunks load.
- Dropped items slide on ice as far as they do in Java.

## What's new in 0.11.0 (if you're coming from 0.10.0 or earlier)

### Title screen and worlds

- Java's title menu: **Singleplayer**, **Multiplayer**, **Options...** and
  **Quit Game**, with the version and the update button.
- **Select World** lists each world with its name, folder, when it was last
  played, its game mode and the version that last saved it, with a search
  box. Opening a world saved by a newer version warns you first and offers
  a backup.
- **Edit World** renames a world, makes a backup, opens the world and
  backups folders and copies the seed. **Re-Create** starts a new world
  with the same settings, and deleting a world asks first.
- **Create New World** has Java's Game and World tabs.
- Multiplayer and Direct Connection join LAN games.

### Options

- Java's **Options** screen and its **Video Settings**, **Music & Sounds**,
  **Controls**, **Mouse Settings**, **Key Binds** and **Accessibility
  Settings** screens, from both the title screen and the pause menu.
- **Key Binds:** rebind every keyboard and mouse action. Conflicting keys
  are highlighted, and you can reset one key or all of them.
- **GUI Scale** (Auto or a fixed size) changes the menus and HUD at once.
- **Music & Sounds** has a slider for each of Java's sound categories:
  music, jukebox and note blocks, weather, blocks, hostile and friendly
  creatures, players and ambient sounds, under the master volume.
- New options: Brightness, **Max Framerate** (120 by default, as in Java),
  Sneak and Sprint as Hold or Toggle, Scroll Sensitivity, Discrete
  Scrolling, Invert Mouse X and Y, Distortion Effects, FOV Effects, Damage
  Tilt and Java's difficulty lock (saved with each world).
- Render Distance is shown in Java's 16-block chunks.
- **Changed keys:** Sprint is Left Ctrl only (R no longer sprints), / or T
  opens the console (the backtick no longer does), and only [ and ] change
  render distance (- and = no longer do). Rebind any of them in Key Binds.

### Menus everywhere

- Mouse, keyboard (Tab, the arrow keys, Enter and Escape) and a controller
  (D-pad, A and B) reach every button, slider and text box, with visible
  hover and focus.
- Text boxes support selecting, word jumps, copy and paste.
- When a world can't be opened, saved, backed up, renamed or deleted, a
  screen says what happened and what to do. So does a failure to save your
  options when you close Options, leave a world or choose Skip This Version;
  a failure when quitting or pressing F11 is only logged.

## What's new in 0.10.0 (if you're coming from 0.9.0 or earlier)

### Updates in game

- When a new version is published, the title screen shows an **Update to…**
  button in the bottom-right corner. It opens the release notes with
  **Update and Restart**, **Release Page**, **Later** and **Skip This
  Version**.
- **Update and Restart** downloads the new version, checks that it was
  signed by VoxelCraft's release key, backs up all your worlds and options,
  installs it and reopens the game. Nothing downloads until you choose to
  update.
- Backups go to the `backups` folder beside your `saves` folder; the three
  most recent are kept.
- The game asks GitHub for the latest release when the title screen opens.
  It sends only the game's version, nothing about you or your worlds. Turn
  this off with **Options →
  Check for Updates** (from the next launch).

### Safer saves

- A world whose save file can't be read is no longer replaced by a fresh
  world: the game goes back to the title screen and says what went wrong.
- If saving fails, the game tells you instead of only writing it to the log.

## Known limitations

- Structures are VoxelCraft's own designs laid out the way Java lays them
  out, not Java's buildings. Water from a nearby cave can pour into an
  ancient city.
- On steep ground some villages and outposts are smaller or don't appear,
  because a building is left out rather than stood on a pillar.
- Mineshaft chest minecarts and the stronghold biome search aren't in yet.
- Archaeology covers suspicious sand and gravel in ocean ruins only; trail
  ruins have no suspicious blocks yet, and decorated pots can't carry
  sherd patterns.
- Brushing has no sounds of its own, and other LAN players don't see the
  find rising out of the block.
- Other LAN players don't see the raid bar. Witches in a raid don't heal
  raiders, and raids add nothing to statistics.
- The new mobs and villagers use simpler brains than Java's, and many of
  them borrow other mobs' sounds. Item frames don't show map images.
- The Host, Raiser, Shaper and Wayfinder armor trim templates have no
  survival source yet: trail ruins have no suspicious gravel loot.
- The Wither's flight and attack timing are simpler than Java's.
- Dispensers can't put armour or heads on other LAN players.
- Music discs can be heard 24 blocks away (Java: 64), and other music
  doesn't fade while a disc plays.
- Some Java options aren't in yet because nothing in the game uses them:
  clouds, biome blend, entity distance and shadows, mipmaps, menu blur,
  resource packs, language, chat and skin options, the attack indicator,
  directional audio, the sound output device and voice volume.
- There are no world icons. Create New World has no More tab, and Edit World
  can't optimize or export a world. Multiplayer lists LAN games only.
- Difficulty stays on the Options screen; Java 26.3 moved it to the pause
  menu.
- Updating in game works on Windows x64 and Apple Silicon Macs, for copies
  installed by the installer or dragged into a folder you can change, such as
  Applications. A Mac app run straight from the disk image shows a link to the
  release page instead.
- On Windows, if a new version installs but won't start, reinstall it from the
  release page; your worlds are in the backup. On a Mac, a new version that
  doesn't start is replaced by the old one automatically.
- LAN is for VoxelCraft players on a trusted local network: there are no
  accounts or encryption, and it can't join Minecraft servers. Everyone stays
  in the host's dimension; when the host travels, the others follow. It has
  been tested between processes on one computer, not yet across Windows, Mac
  and Linux machines on a real network.
- Many blocks and mobs borrow the nearest existing sound; their own sounds
  come with the sound review.
- Mobs and dropped items treat scaffolding and powder snow as plain blocks.
- Walls built in older versions show their posts until a neighbouring block
  changes.
- The off-hand item isn't drawn in your hand, and bows can't be drawn from
  the off-hand.
- There is no Soul Speed.
- Pistons don't push arrows, falling blocks, End crystals, item frames,
  paintings or armour stands.
- Anvils can't rename items yet: the name box isn't on the anvil screen.
- You can't drag a stack across slots to share it out, or double-click to
  collect a stack.
- Sculk sensors don't hear a few actions yet, such as leads, shearing
  some mobs, goat horns and spawners.
- Commands accept `@s` and `@p` only.
- Bells don't swing, strider jockeys don't spawn and boats ignore currents.
- Streaming in new terrain quickly can still cause an occasional long frame
  on an Apple M5; that work is planned for 0.18.
- World generation is VoxelCraft's own: worlds aren't seed-compatible with
  Minecraft.

## Upgrading and saves

Players on 0.10.0 or later get this version from the title screen. Install
it by hand if you're on 0.9.0 or earlier; later versions then arrive in game.
Worlds and options from 0.16.0 and earlier load in 0.17.0, and everything
new in 0.17.0 works in them: it changes no terrain. Worlds keep the terrain
and structures they were created with, so End cities and ships, poplar
forests, sulfur caves, abandoned camps and the rebuilt strongholds,
mineshafts, fortresses, shipwrecks and monuments appear only in worlds
created in 0.16.0 or later. Redstone in an old world switches to Java's
update order when it loads, so a contraption that relied on the old order
may behave differently. Once 0.17.0 has saved a world, don't open it in an
older version: older versions can't read chunks holding the new blocks,
and lose the new carts and items. Back up your worlds before
upgrading from 0.8.0, and don't open a world saved by 0.9.0 or later in
0.8.0. LAN players all need the same version.

## Installing

These builds have **no verified publisher signature**; Mac downloads are also
**not notarized**. Windows may show a SmartScreen warning. If you trust the
download, choose **More info → Run anyway** when available. Windows Smart App
Control and managed-device policies can block unsigned apps outright.

On Mac, drag the app into Applications and try launching it once. If blocked,
open **System Settings → Privacy & Security → Open Anyway**, then confirm Open.
See [Apple's instructions](https://support.apple.com/en-us/102445).

To install this version over an older one, quit the game, then run the new
Windows installer over the old one, or replace the app in Applications on Mac.
Worlds, options and logs live in `%LOCALAPPDATA%\VoxelCraft` on Windows and
`~/Library/Application Support/VoxelCraft` on Mac, and updating or
uninstalling keeps them.

See the bundled **INSTALL.txt / READ ME FIRST.txt** for controls, migration
from source-build saves and troubleshooting. Checksums are in `SHA256SUMS.txt`,
signed in `SHA256SUMS.txt.sig`.

This game is still in development. Report problems with your OS, GPU, steps
to reproduce and `voxelcraft.log` through
[GitHub Issues](https://github.com/BrendanH18/VoxelCraft/issues).
