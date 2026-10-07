# Gameplay guide

[← Back to VoxelCraft](../README.md) · [Development tools](development.md)

## Controls

| Input | Action |
|---|---|
| Mouse | Look (click the window to capture the mouse) |
| W A S D | Move |
| Space | Jump / swim up / fly up — double-tap to toggle flying (creative) |
| Left Shift | Sneak: walk slowly and quietly, never off an edge, and hold on to ladders / fly down |
| Left Ctrl or R | Sprint |
| F | Toggle flying (creative) |
| Left / right click | Break / place block (hold to repeat); left click on a mob attacks it; respawn on the death screen |
| Middle click | Pick block |
| Q / Ctrl+Q | Drop one of the selected item / the whole stack (with the inventory open: from the slot under the mouse) |
| E | Inventory (click to move stacks; creative shows the block palette) |
| Shift + click | In the inventory: move a stack between the open chest or furnace and the inventory (or between hotbar and main grid); on a crafting result, craft as many as fit |
| Right click on a door or fence gate | Open / close it |
| Shift + right click | Build against a crafting table, furnace, chest, door or gate instead of using it |
| Hold / release right click with a bow | Draw / shoot an arrow (needs arrows in survival) |
| Right click with flint and steel | Start a fire, light an obsidian portal frame or prime TNT |
| Right click with a bucket | Scoop up a water or lava source / pour it out again |
| Right click with a hoe | Till grass or dirt (with air above) into farmland |
| Right click with seeds / bone meal | Sow wheat on farmland / make a crop, sapling or patch of grass grow |
| G | Toggle survival / creative |
| 1–9, scroll wheel | Select hotbar slot |
| `[` / `]` | Decrease / increase render distance |
| /, T, backtick | Open slash command console (Enter runs, Esc closes, Up/Down history, Tab completion) |
| M | Mute / unmute sound (`--mute` starts muted, `--volume 0..1` sets the master volume) |
| V | Toggle vsync |
| F1 | Toggle HUD |
| F3 | Debug screen |
| F11 | Fullscreen |
| Esc | Pause menu (the game pauses): Back to Game, Options..., Save and Quit to Title; Esc again goes back |

The world autosaves every two minutes and on exit. Switching away from
the window also pauses offline play. See [save backups and version compatibility](releases.md#player-data-and-old-saves)
before upgrading or returning to an older build.
A host started with `--agent-listen` keeps
simulating while menus are open. See [hosted agent CLI and console commands](agents.md).

**Options** (Esc → Options...): render distance (2–32 chunks), field of
view (30–110°), mouse sensitivity (25–300%), master volume, vsync, Classic/Enhanced graphics and the FPS counter.
Changes apply immediately and are saved to `saves/options.txt` inside the
[per-user data folder](releases.md#player-data-and-old-saves) when you
leave the screen; `[`/`]` and V adjust render distance and vsync in game.
`--rd`, `--volume`, `--no-vsync` and `--graphics classic|enhanced` override the saved options for one
session. Enhanced is the default: sun/moon directional skylight, warm
sunsets, cool moonlight, and animated water normals with sky reflections
and sun/moon glints. Block lighting and ambient occlusion still illuminate
caves. Water reflects the sky rather than nearby objects; geometry shadows,
PBR materials and screen-space reflections remain future work. Classic
keeps the original lighting and water appearance.

A compact FPS counter is on by default at the top left. It averages
presented frames over half a second and stays readable in inventories and
menus. Options → FPS Counter hides it; F1 hides the HUD, and F3 shows the
full performance display instead of duplicating the compact counter.

![F3 debug screen](images/debug.jpg)

## World and survival features

- Infinite procedurally generated terrain in fourteen biomes: oceans,
  beaches, winding rivers, plains, oak and birch forests, swamps with
  shallow pools, deserts, terraced badlands striped with terracotta,
  savannas, jungles, snowy tundra, taiga and mountains. Cold seas and rivers
  freeze over (ice is slippery underfoot, and broken ice turns back into water). Grass and oak leaves take
  on the colour of their biome: murky in swamps, dry and yellow in savannas
  and badlands, vivid in jungles and cool in the snow, blending across
  biome borders. Clay (4 clay balls when
  broken) lines river and swamp beds. Spaghetti caves, deep caverns and ores lie below. Five kinds
  of tree grow there: oak, birch, spruce, acacia (leaning trunks under flat
  canopies) and jungle (tall trees, giant 2x2 trees and bushes), plus cacti
- Farming and growth, driven by Minecraft-style random block ticks in the
  chunks within 128 blocks (each block is picked about once a minute).
  Breaking tall grass sometimes drops wheat seeds; till grass or dirt with
  a hoe and sow them on the farmland. Farmland with water within 4 blocks
  turns dark and wet; wheat grows through 8 stages under open sky, about
  twice as fast on wet farmland, and ripe wheat drops wheat and 1-4 seeds
  (unripe wheat just its seed). Bare dry farmland turns back to dirt, and
  jumping or falling onto farmland can trample it. Leaves drop their tree's
  sapling (1 in 20, jungle 1 in 40; oak leaves also apples, 1 in 200);
  planted on grass or dirt, saplings grow into trees.
  Nether wart planted on soul sand ages through 4 stages (1 in 10 random
  ticks, in any light, ignoring bone meal); ripe wart drops 2-4, unripe 1.
  Sugar cane grows next to water up to 3 blocks tall, and must be planted
  on grass, dirt or sand beside water. Leaves that can't reach a log
  within 6 blocks decay a few seconds after a tree is felled. Grass spreads
  to lit dirt nearby and dies under blocks. Bone meal advances a crop 2-5
  stages, sometimes grows a sapling at once, and sprouts grass and flowers
  around a grass block. Crops and saplings grow with open sky or block light
  of at least 9, including torchlight under a roof. Crops sample their own
  cell; saplings sample the cell above. Skylight still uses an open-sky
  approximation.
- Tall grass, ferns, dandelions, poppies and blue orchids (flowers grow in
  patches), dead bushes in deserts and badlands, sugar cane along the
  water, pumpkins on the plains and melons in jungles. Plants and torches are
  drawn as two crossed planes, break instantly, need soil (torches a full
  block) beneath them and pop off when it goes, and are washed away by
  water. Clicking tall grass with a block replaces it
- Weather, like Minecraft: clear spells of one to five days alternate with
  rain lasting half a day to a day and a half. Rain falls as snow in cold
  biomes and above y = 150, and not at all in deserts, savannas and
  badlands. It greys and darkens the sky, hides the sun, moon and stars
  behind thicker clouds, stops at the first block overhead, waters
  farmland under open sky, keeps zombies and skeletons from burning, and
  can be heard drumming on the roof. The time of day and the weather are
  saved with the world
- Torches give off light level 14 (glowstone 15)
- Flood-fill sky and block light with smooth lighting and ambient occlusion
- Day/night cycle with a procedural sky: square sun and moon, sunset glow,
  rotating stars and drifting blocky clouds; distance and underwater fog
- Flowing water: falls, spreads up to 7 blocks toward the nearest drop,
  dries up without a source, and forms infinite sources; lowered surfaces
- Lava: flows like water but six times slower and only 3 blocks, glows
  (light 15), fills caves below y = 10, burns players (4 damage every
  0.5 s) and mobs, and can be swum through with an orange haze. Where lava
  meets water a source hardens into obsidian, flowing lava into
  cobblestone, and lava pouring onto water turns it to stone
- Fire: flint and steel lights empty space on a solid top or beside
  flammable blocks; successful uses spend one durability. Animated flames
  give off light level 15, spread through wood, leaves, wool and plants,
  and prime TNT with its normal four-second fuse. Fire burns out as it
  ages or loses its fuel; netherrack stays lit indefinitely, even in rain.
  Exposed rain and flowing water extinguish ordinary fire. Lava can start
  fires near combustible blocks. Players and mobs keep burning after
  leaving fire (8 seconds) or lava (15 seconds), until water or rain puts
  them out; zombified piglins resist both. Fire destroys dropped items,
  and burning blocks leave no loot. Punch fire to put it out, or replace
  it with a block. Fire ages survive saving and chunk reloads
- Sand and gravel fall when unsupported, as free-moving blocks that land
  on the first solid block (replacing plants and fluids in the way);
  knocking out a column's base drops the whole column
- Walking, swimming and flying with AABB collision
- Nine mobs with Minecraft-style animated box models (see [Mobs](#mobs)):
  pigs, cows, sheep and chickens wander in herds and panic when hit;
  zombies, skeletons, creepers and spiders hunt at night. Skeletons shoot
  arcing arrows that stick in blocks, creepers hiss, swell and explode
  (blowing a crater in the world), spiders climb walls. Mobs avoid tall
  drops, float in water, flash red when hurt and topple over when killed;
  killing one drops its loot. All mobs,
  arrows and explosion smoke are drawn in a single draw call
- Survival and creative modes: timed block breaking with crack overlay,
  drops, a 36-slot inventory with stacks, and a scrollable creative palette
- Crafting: a 2x2 grid in the survival inventory and a 3x3 grid at a
  crafting table (right-click it). Shaped recipes work anywhere in the
  grid and mirrored; click the result to craft one, and leftovers return
  to the inventory when the screen closes (thrown out if it's full). Click Recipes
  to browse layouts with the arrow buttons or scroll wheel; hover ingredients
  for names and alternatives, then copy the preview into your crafting grid.
  On narrower windows, Back closes the recipe overlay so you can craft.
  Recipes: planks (each log makes its own kind; any planks work in
  recipes), sticks, crafting table, chest (8 planks in a ring), torches
  (coal or charcoal over a stick), bread (3 wheat in a row), bone meal (from
  a bone), sandstone, wool (from string), clay and bricks (4 clay balls or
  bricks), melon slices (from a melon), bows (3 sticks and 3 string),
  arrows (flint, stick and feather; gravel drops flint 1 time in 10), and pickaxes, shovels, axes,
  hoes and swords in wood, stone, iron, gold and diamond
- Tools follow Minecraft's mining rules: a block takes its hardness x 1.5
  seconds to mine with something that can harvest it and x 5 otherwise,
  divided by the tool's speed when it's the right kind (pickaxe for stone
  and ores, shovel for dirt, sand and gravel, axe for wood). Stone and ores
  only drop with a pickaxe of a high enough tier: wood or gold for stone
  and coal, stone for iron, iron for gold and diamond, diamond for
  obsidian. Stone by hand takes 7.5 s; with a wooden pickaxe 1.1 s, down
  to 0.19 s with gold. Tools lose 1 durability per block (swords 2) and 1
  per hit (other tools 2), and break when worn out. Melee damage comes from
  the held item: a fist 1, swords 4 (wood, gold) to 7 (diamond), axes,
  pickaxes and shovels less. Hitting while falling is a critical hit for
  1.5x damage
- Slabs: three stone, cobblestone, planks, sandstone, bricks or nether
  bricks in a row make six half-height slabs that mine like their full
  block. Placing a slab on top of the same slab makes the full block
- Stairs: six of the same blocks in a staircase make four stairs (stone,
  cobblestone, planks, sandstone, bricks or nether bricks). The low step
  faces you as you place them. You walk up slabs and stairs without
  jumping (step height 0.6, as in Minecraft), and mobs do too. Slabs and
  stairs keep sky and torch light out, so a slab roof shades the room below
- Fences (planks, stick, planks in two rows: 3) join up with each other,
  gates and solid blocks, and are 1.5 blocks tall to anything trying to
  jump them, so they pen animals in. Fence gates (stick, planks, stick in
  two rows) open with a right-click, swinging away from you
- Doors (6 planks in two columns: 3) stand two blocks tall on a solid
  block; right-click to open or close them. Breaking either half breaks
  both
- Ladders (7 sticks in an H: 3) hang on the side of a solid block and fall
  off when it's removed. Walk into one or hold Space to climb, hold Shift
  to hold on; ladders break falls
- TNT (5 gunpowder and 4 sand in a checkerboard): light it with flint and
  steel and it hops out of its block, flashing white and swelling for 4
  seconds before a blast stronger than a creeper's (power 4). TNT caught in
  a blast goes off within a second and a half, so stacks chain. Explosions
  hurt through armor, knock you back and drop some of what they destroy
- Buckets (3 iron ingots in a V, stack to 16): right-click a water or lava
  source to fill one and right-click a block to pour it out next to it.
  Creative keeps its empty bucket. A lava bucket smelts for 1000 s and
  leaves the empty bucket behind in the furnace
- Bows: hold right-click to draw (fully drawn after 1 s; the bar under the
  crosshair turns gold and the view zooms in a little) and release to
  shoot. A full draw deals 6 damage plus a random critical bonus of up to
  4; weaker draws fly slower and hit softer. Arrows arc, knock mobs back,
  stick in blocks and drop out when the block is broken. Walk over a stuck
  arrow to pick it back up (arrows shot in creative can't be collected).
  Bows have 384 uses
- Furnaces (8 cobblestone in a ring): right-click to open; put something
  to smelt on top and fuel below. Each item takes 10 s; coal and charcoal
  burn 80 s, logs, planks, crafting tables and chests 15 s, wooden tools 10 s and
  sticks 5 s. Smelts iron and gold ore into ingots, sand into glass,
  cobblestone into stone, logs into charcoal, clay balls into bricks, clay
  into terracotta and raw meat into cooked meat. A burning furnace glows (light 13), keeps smelting with its
  screen closed while its chunk is loaded, and is saved with the world;
  breaking one drops its contents
- Items beyond blocks, each with a procedurally drawn icon: sticks, coal,
  ingots, diamonds, food, mob materials, and pickaxes, shovels, axes, hoes
  and swords in five tiers with durability bars. Coal and diamond ore drop
  their items
- Survival health: 10 hearts, fall damage (1 per block beyond 3; water
  breaks falls), 15 s of air then drowning, a red hurt flash and shaking
  hearts. Dying shows
  a death screen; everything you carried drops where you died, and clicking
  respawns at the world spawn with full health. Creative is immune to damage.
  Health and air are saved with the world
- Hunger, like Minecraft: 10 drumsticks plus a hidden saturation buffer
  (5 at spawn). Exhaustion from sprinting (0.1 per block), swimming (0.01
  per block), jumping (0.05, 0.2 sprinting), mining (0.005), attacking
  (0.1) and taking damage (0.1) costs a point of saturation, then food,
  every 4. Health regenerates half a heart every 4 s from 18 food (costing
  6 exhaustion), or every 0.5 s with a full bar and saturation left; an
  empty bar starves you down to half a heart. You can't sprint at 6 food or
  less. Hold right-click with food for 1.6 s to eat (not when full); the
  Eating indicator below the crosshair shows the bite's progress. The hunger
  bar shows on the right above the hotbar, with air bubbles above it.
  Creative players don't get hungry. Hunger is saved with the world
- Break, place and pick blocks, with a selection outline and hotbar
- Chests (8 planks in a ring): right-click to open 27 slots of storage
  above your inventory. Their contents are saved with the world and drop
  when the chest is broken. Chests and furnaces face you when placed
- Dropped items, like Minecraft's: mined blocks, mob loot, furnace
  contents, plants that pop off or wash away, and a third of what an
  explosion destroys drop as small spinning blocks or item icons (extra
  copies for bigger stacks). They fall, slide, float up in water, burn in
  lava, merge with matching stacks nearby, and vanish after five minutes
  (the timer stops while their chunk is unloaded). Walk over them to pick
  them up (thrown items wait 2 s first). Q drops the selected item, Ctrl+Q
  the stack; clicking off the inventory window throws the held stack (right
  click: one item). Items that don't fit when a crafting screen closes are
  thrown out. Dropped items are saved with the world
- Experience, like Java Edition: coal, diamond and nether quartz ore (when
  harvested with the right pickaxe), killed mobs (5 for monsters, 1-3 for
  animals) and smelting drop glowing orbs that shimmer between yellow and
  green. Furnaces store what they smelt (0.7 per iron ingot, 1 per gold
  ingot or diamond, 0.35 per cooked meat, 0.1 for most blocks) and release
  it when you take from the output slot or break the furnace. Orbs drift
  toward the nearest player within 8 blocks, merge with same-sized orbs,
  burn in lava and fire, and vanish after five minutes. You absorb one orb
  every two ticks, with a ding, and a fanfare plays every five levels. The
  level curve is Java's (7 points for level 1, then 2L+7, 5L-38 and 9L-158).
  The green bar above the hotbar shows progress and the level. Dying drops
  7 points per level (at most 100) and loses the rest; the death screen
  shows your score. `/xp add|set <n> [points|levels]` and `/xp query`
  change it from the console. Experience and orbs are saved with the world
- Status effects, as Java has them: speed and slowness (+20% / -15% speed
  per level), strength and weakness (+3 / -4 melee damage per level),
  instant health and damage (4 / 6, doubling per level), regeneration and
  poison (a point every 50 / 25 ticks, halving per level; poison never
  kills), fire resistance, water breathing, night vision (fades in its last
  10 seconds), jump boost (higher jumps, a block less fall damage per
  level) and slow falling (an eighth of the gravity, no fall damage).
  Icons show in the top right (harmful ones in a second row, blinking in
  their last 10 seconds) and with name and time beside the inventory.
  `/effect give <effect> [seconds] [amplifier]` and `/effect clear
  [effect]` use Java's ids; effects are saved, and death clears them.
- Potions: three glass in a V make three glass bottles, which fill from a
  water source (right click; the water stays). Hold right click to drink
  a potion (1.6 s, like eating, even when full); it applies its effect and
  leaves the bottle (creative keeps the potion). Java's 36 drinkable types
  whose effects exist are items, from the water bottle and the mundane,
  thick and awkward bases to long (extended) and strong (level II)
  variants with Java's durations; tooltips show the effect, level and
  time.
- Enchantments, with Java 1.21's levels, costs and effects: protection,
  fire/blast/projectile protection and feather falling (protection
  factors, capped at 80%), respiration (air lasts level + 1 times as
  long), aqua affinity, thorns (15% a level to hit back for 1-5 damage,
  using mainhand Looting), depth strider (half the bonus off the ground),
  sharpness (+0.5 a level + 0.5), smite and bane of arthropods
  (+2.5 a level against undead / spiders and silverfish), knockback, fire
  aspect (4 s of fire a level; melee kills cook meat even in water, and fire
  kills within five seconds of a player hit award XP), looting (up to one
  more of each eligible drop a level; sheep still drop one wool), sweeping
  edge (sword hits on the ground sweep mobs next to the target within three
  blocks of the player for 1 + level / (level + 1) times base damage, plus
  each mob's enchantment bonus; Fire Aspect also applies to sweeps),
  efficiency (level² + 1 mining speed), silk touch (the block itself, no
  ore experience; spawners still award XP), fortune (Java's ore, crop,
  glowstone, melon, gravel and leaf bonuses),
  unbreaking, power, punch, flame, infinity, mending (picked-up
  experience repairs held or worn gear, two durability a point), and the
  curses of binding (worn armor stays on outside creative) and vanishing
  (gone on death). Digging is five times slower with your eyes underwater
  (unless aqua affinity) and again off the ground, like Java. Enchanted
  items shimmer purple; tooltips list enchantments (curses in red).
  `/enchant <enchantment> [level]` enchants the held item (a book becomes
  an enchanted book).
- Enchanting tables: a book over two diamonds and four obsidian. Right
  click for Java's screen: put in an unenchanted tool, weapon, armor
  piece, bow or book and lapis lazuli for three offers. Each shows Java's
  rune words, its lapis price (1-3) and level cost; hovering shows one of
  the enchantments it will give. Costs follow Java's formula from the
  bookshelves two blocks out at the table's height or one up (up to 15,
  with air or plants between), so a full ring reaches 30 levels, and the
  enchantments are rolled with Java's algorithm and random generator from
  your enchantment seed, which changes each time you enchant. Taking
  offer n costs n levels and n lapis (creative is free); a book becomes an
  enchanted book. The table glows (light 7). Controller players have an
  enchanting tab, and agents `enchanting 1..3`.
- Anvils: three blocks of iron over an ingot over three ingots (a block
  of iron is nine ingots). They fall like sand and are placed broadside
  to you. Right click for Java's screen (without renaming): repair a tool
  or armor piece with its material (a quarter of its durability per
  item), merge two of the same item (both remainders plus 12%, and their
  enchantments: equal levels go up one, conflicting ones cost a level
  each and are dropped), or apply an enchanted book (half the price).
  Costs are Java's: each enchantment's anvil cost per level, plus the
  prior-work penalty both items carry, which doubles plus one with each
  use; at 40 levels survival says "Too Expensive!". Each survival use
  has a 12% chance to chip the anvil (chipped, damaged, then it breaks).
  Controller players have an anvil tab, and agents `anvil 1..9`.
- Ancient debris buried in the Nether, concentrated around y 16, needs a
  diamond pickaxe and drops itself (Fortune has no effect). Smelt it for
  Netherite scrap and 2 XP per block. Four scraps plus four gold ingots in
  any arrangement on a crafting table make one Netherite ingot; nine
  ingots make a block of Netherite, which crafts back into nine ingots.
  Debris and Netherite blocks resist explosions, and their dropped items,
  scrap and ingots survive fire and float in lava.
- Netherite tools and armor (Java's stats): tools last 2031 uses, dig at
  speed 9, harvest everything diamond can and hit one harder than diamond
  (a Netherite sword does 8); armor pieces last 407/592/555/481 hits, give
  diamond's armor points plus 3 toughness each (big hits get through less)
  and 0.1 knockback resistance each. Enchantability 15, repaired with
  Netherite ingots on an anvil, and dropped Netherite gear survives fire
  and lava. They can't be crafted: diamond gear is upgraded with a
  Netherite upgrade smithing template and an ingot. Seven diamonds around
  a template over netherrack copy it (two templates). Diamond armor now
  also has its toughness of 2 per piece.
- Lapis lazuli ore below y 32 (stone pickaxe or better) drops 4-9 lapis
  (with fortune's ore bonus) and 2-5 experience; nine make a block of
  lapis lazuli and back.
- Strongholds: 128 per world on Java's concentric rings (the first three
  1280-2816 blocks from the origin), sunk below sea level and built from
  Java's stronghold pieces: a spiral staircase, corridors, turns,
  stairways, crossing rooms (pillar, fountain or balcony with a chest),
  prison cells, five-way crossings, libraries with cobwebs and chests,
  chest corridors and one portal room, whose twelve End portal frames each
  hold an eye one time in ten, with a silverfish spawner on its stairs.
  Walls are randomly cracked or mossy stone bricks.
- Brewing: a blaze rod over three cobblestone makes a brewing stand.
  Right click it for Java's screen: blaze powder fuel (one powder brews 20
  times, shown by the bar), an ingredient slot and three bottle slots. A
  brew takes 20 seconds and changes every bottle the ingredient works on.
  Nether wart turns water bottles awkward; sugar, glistering melon slices
  (a melon slice in eight gold nuggets), spider eyes (one in three spiders
  drops one; eating it poisons you for 5 s) and blaze powder turn awkward
  potions into swiftness, healing, poison and strength (water bottles into
  mundane potions); glowstone dust makes level II potions (or thick
  potions from water). Sugar comes from sugar cane. Other Java ingredients
  (redstone, magma cream, ghast tears, golden carrots, rabbit's feet,
  pufferfish, phantom membranes, fermented spider eyes) don't exist yet.
- Procedurally generated, mipmapped block textures — the game ships no assets
- Procedural sound, synthesized in code at startup (~25 ms): material-specific
  break/place/footstep sounds (stone, wood, dirt, grass, gravel, sand, snow,
  leaves, glass, water), jump and landing thuds, splashes and swimming,
  creeper hisses and explosions, bow twangs,
  inventory clicks, wind and cave ambience with dripping water, positional
  panning and distance falloff, and a muffled mix while underwater

## The Nether

Build a frame of obsidian (pour a water bucket over lava source blocks,
then mine the obsidian with a diamond pickaxe) at least 4 wide and 5 tall around an
empty inside of 2x3 up to 21x21; the corners can be left out. Light it
with flint and steel (iron ingot and flint, shapeless; 64 uses) to fill it
with a swirling portal. Stand in it for 4 seconds (half a second in
creative) to travel to the Nether, and the same way back. Each block in
the Nether is eight in the overworld: the game looks for a portal within
16 blocks of the matching spot on the other side, and builds one (on an
obsidian ledge if there's nowhere to stand) if there isn't. Breaking any
part of a frame puts its portal out.

The Nether is a cavern world between a bedrock floor and roof: netherrack
cliffs and islands over a lava sea at y = 31, soul sand shores that drag
at your feet, gravel by the lava, quartz ore in the rock and glowstone
hanging from the ceilings. Nether fortresses (one per 432-block region,
laid out from Java's fortress pieces) rise from between y = 48 and 70:
nether brick bridges on pillars that reach down through the lava sea, small
fenced crossings, stair rooms and blaze spawner thrones, and through a
castle entrance hall, enclosed corridors with loot chests (one corner in
three: diamonds, iron, gold, golden gear, flint and steel, nether wart,
obsidian) and halls of nether wart on soul sand. There is no sky, weather
or day and night, just a steady dim glow in a red haze; water boils away,
and beds explode.
Netherrack smelts into nether bricks (four make a nether bricks block),
quartz ore drops nether quartz, and glowstone breaks into 2–4 glowstone
dust (four make a block). Nine gold nuggets make a gold ingot and back.

Each dimension keeps its own blocks, furnaces, chests and dropped items in
the save (the Nether in a `nether` folder inside the world's). Dying in
the Nether respawns you in the overworld. `--dimension nether` starts a
session there, as if you had just come through a portal.

## Mobs

| Mob | Health | Behaviour | Loot (player kills) |
|---|---|---|---|
| Pig | 10 | wanders, idles, looks around; panics when hit | 1–3 raw porkchop |
| Cow | 10 | like pigs | 1–3 raw beef, 0–2 leather |
| Sheep | 8 | like pigs | 1 wool |
| Chicken | 4 | like pigs; flutters down instead of falling | 1 raw chicken, 0–2 feathers |
| Zombie | 20 | chases within 24 blocks, hits for 3 every second; burns in sunlight | 0–2 rotten flesh |
| Skeleton | 20 | keeps 5–10 blocks away, strafes, and shoots arrows (about 3 damage) when it can see you; burns in sunlight | 0–2 bones, 0–2 arrows |
| Creeper | 20 | walks up and lights a 1.5 s fuse within 3 blocks (kept lit within 7); explodes for up to 43 damage over 6 blocks, destroying blocks (not bedrock, obsidian or fluids) | 0–2 gunpowder |
| Spider | 16 | fast; climbs walls; hunts only in the dark or after being hit, bites for 2 | 0–2 string |
| Zombified piglin | 20 | Nether only, in packs; ignores you until you hit one, then the whole pack within 32 blocks chases you for 30 s and strikes with gold swords for 5 | 0–1 rotten flesh, 0–1 gold nuggets |
| Enderman | 40 | 2.9 blocks tall; rare at night in the Overworld and Nether, common in the End. Neutral until hit or until you look it in the eyes (within 64 blocks), then screams, opens its jaw and hits for 7. Freezes while you watch it (up close it teleports away), teleports toward you from more than 16 blocks, dodges arrows by teleporting, is hurt by water and rain, and wanders off by teleporting in daylight | 0–1 ender pearls |
| Blaze | 20 | Nether fortresses: from spawner cages and inside fortress pieces in groups of 2-3. Hovers, glows while charging, then fires bursts of three fireballs (5 damage, setting you alight) and hits for 6 up close; hurt by water and rain, immune to fire and lava | 0–1 blaze rods |
| Silverfish | 8 | strongholds: from the portal room's spawner; small and quick, chases and nibbles for 1 | nothing |

Animals spawn on sky-exposed grass in herds (up to 4 of each kind);
hostile mobs spawn on sky-exposed solid ground when daylight < 0.35 (up to
4 zombies and 3 of the others). Inside Nether fortress pieces, Java's
fortress list also spawns blazes, packs of zombified piglins and groups
of skeletons, in any light. Like Java Edition's per-player mob caps,
these limits count mobs within 96 blocks of each player, so players far apart
each get their own. Mobs spawn 24–64 blocks from a player (never closer to
any player), despawn once every player is more than 96 blocks away or when
their chunk unloads, and hostile mobs chase the nearest survival player. Every mob burns
in lava. Player hits deal damage based on the held item, with knockback, at most
every 0.5 s: a fist deals 1, and swords deal 4–7 depending on their tier.
Hostile mobs ignore creative players.

Ender pearls (stack of 16) are thrown with **right click**, at most once a
second (a pale veil drains from the hotbar slot): they fly like Java's
(about 50 blocks at 45°), and where they hit a block or a mob you teleport,
taking 5 damage.

Eyes of ender (an ender pearl and blaze powder) find strongholds. Right
click in the overworld to release one: it drifts up to 12 blocks toward the
nearest stronghold, rising 8 (or sinking toward it once you're close),
hovers for four seconds, then drops back as an item four times in five and
shatters otherwise. Right click an empty End portal frame to put an eye in
it; when twelve frames with eyes ring a 3x3 hole, all facing in, the portal
opens with a deep rumble. Falling into an End portal takes you to the End's
obsidian platform at once; one in the End takes you back to your bed or
the world spawn. Agents use eyes with `place`.

Inventory search filters creative items by name. Type in the search field
(automatically focused in creative), or press Ctrl+F. In survival and containers,
matching stacks are highlighted without moving any slots. Matching ignores case,
accepts underscores and requires every search word. Enter leaves the field;
Esc closes the inventory. Agents can use `catalog [query]` for the same search.

The End is reached through a stronghold's End portal, or with
`/dimension end` or `--dimension end`. Return with `/dimension overworld`.
It includes a seeded central end-stone island, ten obsidian pillars, an arrival platform, a void gap and outer islands,
a static violet sky, no weather or natural Overworld/Nether mob spawning, and
its own `end/` save folder. Water works; beds explode. End stone is mineable
with a pickaxe and appears in the creative catalog.

The Ender Dragon (200 health) guards the central island. An End crystal burns
on every pillar, two of them inside iron bar cages, and the nearest one heals
the dragon a point every half second along a visible beam. Crystals blow up
(power 6) when hit, shot or caught in a blast; breaking the one healing the
dragon hurts it for 10. Like Java's dragon it circles the pillars, sometimes
swoops at a player to spit a fireball that leaves a spreading cloud of
harmful breath, and sometimes perches on the exit portal's pillar to breathe
fire, roar at anyone close, then take off or charge at them. Its wings shove
and hurt players, its head bites for 10, and it smashes through any block but
end stone, obsidian, bedrock, iron bars and portals. Head hits do full damage,
anywhere else a quarter (plus up to one); arrows glance off a perched dragon,
and bed or TNT blasts hurt it. The boss bar shows within 192 blocks of the
island's centre. On death it rises for ten seconds in beams of light,
spilling 12,000 experience (500 for later kills), then the bedrock exit
portal opens, with the dragon egg on its pillar after the first kill. The egg
teleports up to 15 blocks when hit or used, and falls like sand. The fight
(dragon health, crystals, kills) is saved with the End.

Each dragon kill also opens an End gateway at one of 20 spots in a ring 96
blocks out from the island's centre (in a seeded order): a gateway block in a
one-high gap between bedrock caps. Throw an ender pearl into it (or touch it)
to go through. The first trip finds the outer islands like Java does: 1024
blocks straight out, then the nearest chunk with land, ten blocks above the
highest ground there (building a small island if the void goes on). An exit
gateway is built there, linked back, and you land on the highest ground
beside it. The links are saved with the End.

Respawning the dragon, the gateway beam, chorus trees, cities, shulkers and
elytra are still roadmap work. Mojang's [End Highlands overview](https://www.minecraft.net/en-us/article/around-block--end-highlands)
describes the larger progression being built toward.
