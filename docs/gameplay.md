# Gameplay guide

[← Back to VoxelCraft](../README.md) · [Development tools](development.md)

## Controls

| Input | Action |
|---|---|
| Mouse | Look (click the window to capture the mouse) |
| W A S D | Move |
| Space | Jump / swim up / fly up — double-tap to toggle flying (creative) |
| Left Shift | Fly down |
| Left Ctrl or R | Sprint |
| F | Toggle flying (creative) |
| Left / right click | Break / place block (hold to repeat); left click on a mob attacks it; respawn on the death screen |
| Middle click | Pick block |
| E | Inventory (click to move stacks; creative shows the block palette) |
| G | Toggle survival / creative |
| 1–9, scroll wheel | Select hotbar slot |
| `[` / `]` | Decrease / increase render distance |
| T | Skip ahead 2 in-game hours |
| M | Mute / unmute sound (`--mute` starts muted, `--volume 0..1` sets the master volume) |
| V | Toggle vsync |
| F1 | Toggle HUD |
| F3 | Debug screen |
| F11 | Fullscreen |
| Esc | Pause menu (the game pauses): Back to Game, Options..., Save and Quit; Esc again goes back |

The world autosaves every two minutes and on exit. Switching away from
the window also pauses the game.

**Options** (Esc → Options...): render distance (2–32 chunks), field of
view (30–110°), mouse sensitivity (25–300%), master volume and vsync.
Changes apply immediately and are saved to `saves/options.txt` when you
leave the screen; `[`/`]` and V adjust render distance and vsync in game.
`--rd`, `--volume` and `--no-vsync` override the saved options for one
session.

![F3 debug screen](images/debug.jpg)

## World and survival features

- Infinite procedurally generated terrain: oceans, beaches, plains,
  forests, deserts, snowy tundra, taiga and mountains, with spaghetti caves,
  deep caverns, ores, oak and spruce trees and cacti
- Tall grass, dandelions and poppies (flowers grow in patches) on plains,
  forests and taiga, and dead bushes in deserts. Plants and torches are
  drawn as two crossed planes, break instantly, need soil (torches a full
  block) beneath them and pop off when it goes, and are washed away by
  water. Clicking tall grass with a block replaces it
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
- Sand and gravel fall when unsupported, as free-moving blocks that land
  on the first solid block (replacing plants and fluids in the way);
  knocking out a column's base drops the whole column
- Walking, swimming and flying with AABB collision
- Eight mobs with Minecraft-style animated box models (see [Mobs](#mobs)):
  pigs, cows, sheep and chickens wander in herds and panic when hit;
  zombies, skeletons, creepers and spiders hunt at night. Skeletons shoot
  arcing arrows that stick in blocks, creepers hiss, swell and explode
  (blowing a crater in the world), spiders climb walls. Mobs avoid tall
  drops, float in water, flash red when hurt and topple over when killed;
  killing one in survival puts its loot in your inventory. All mobs,
  arrows and explosion smoke are drawn in a single draw call
- Survival and creative modes: timed block breaking with crack overlay,
  drops, a 36-slot inventory with stacks, and a scrollable creative palette
- Crafting: a 2x2 grid in the survival inventory and a 3x3 grid at a
  crafting table (right-click it). Shaped recipes work anywhere in the
  grid and mirrored; click the result to craft one, and leftovers return
  to the inventory when the screen closes. Recipes: planks (from a log),
  sticks, crafting table, torches (coal or charcoal over a stick),
  sandstone, wool (from string), arrows, and pickaxes, shovels, axes,
  hoes and swords in wood, stone, iron, gold and diamond
- Furnaces (8 cobblestone in a ring): right-click to open; put something
  to smelt on top and fuel below. Each item takes 10 s; coal and charcoal
  burn 80 s, logs, planks and crafting tables 15 s, wooden tools 10 s and
  sticks 5 s. Smelts iron and gold ore into ingots, sand into glass,
  cobblestone into stone, logs into charcoal and raw meat into cooked
  meat. A burning furnace glows (light 13), keeps smelting with its
  screen closed while its chunk is loaded, and is saved with the world;
  breaking one returns its contents to your inventory
- Items beyond blocks, each with a procedurally drawn icon: sticks, coal,
  ingots, diamonds, food, mob materials, and pickaxes, shovels, axes, hoes
  and swords in five tiers with durability bars. Coal and diamond ore drop
  their items
- Survival health (no hunger): 10 hearts, fall damage (1 per block beyond
  3; water breaks falls), 15 s of air then drowning, natural regeneration
  after 4 s without damage, a red hurt flash and shaking hearts. Dying shows
  a death screen; clicking respawns at the world spawn with full health and
  the inventory kept (like `keepInventory`). Creative is immune to damage.
  Health and air are saved with the world
- Break, place and pick blocks, with a selection outline and hotbar
- Procedurally generated, mipmapped block textures — the game ships no assets
- Procedural sound, synthesized in code at startup (~25 ms): material-specific
  break/place/footstep sounds (stone, wood, dirt, grass, gravel, sand, snow,
  leaves, glass, water), jump and landing thuds, splashes and swimming,
  creeper hisses and explosions, bow twangs,
  inventory clicks, wind and cave ambience with dripping water, positional
  panning and distance falloff, and a muffled mix while underwater

## Mobs

| Mob | Health | Behaviour | Loot (survival kills) |
|---|---|---|---|
| Pig | 10 | wanders, idles, looks around; panics when hit | 1–3 raw porkchop |
| Cow | 10 | like pigs | 1–3 raw beef, 0–2 leather |
| Sheep | 8 | like pigs | 1 wool |
| Chicken | 4 | like pigs; flutters down instead of falling | 1 raw chicken, 0–2 feathers |
| Zombie | 20 | chases within 24 blocks, hits for 3 every second; burns in sunlight | 0–2 rotten flesh |
| Skeleton | 20 | keeps 5–10 blocks away, strafes, and shoots arrows (about 3 damage) when it can see you; burns in sunlight | 0–2 bones, 0–2 arrows |
| Creeper | 20 | walks up and lights a 1.5 s fuse within 3 blocks (kept lit within 7); explodes for up to 43 damage over 6 blocks, destroying blocks (not bedrock, obsidian or fluids) | 0–2 gunpowder |
| Spider | 16 | fast; climbs walls; hunts only in the dark or after being hit, bites for 2 | 0–2 string |

Animals spawn on sky-exposed grass in herds (up to 4 of each kind);
hostile mobs spawn on sky-exposed solid ground when daylight < 0.35 (up to
4 zombies and 3 of the others). Mobs spawn 24–64 blocks from the player
and despawn beyond 96 blocks or when their chunk unloads. Every mob burns
in lava. Player hits do 2–4 damage with knockback, at most every 0.5 s.
Hostile mobs ignore creative players.
