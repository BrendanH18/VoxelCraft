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
| Esc | Release mouse (press again to save and quit) |

The world autosaves every two minutes and on exit.

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
- Mobs with Minecraft-style animated box models: pigs wander, graze and
  look around, and panic when hit; zombies spawn at night, chase and hit
  survival players, jump 1-block ledges, sidestep obstacles and burn in
  sunlight. Mobs avoid tall drops, float in water, flash red when hurt and
  topple over when killed. All mobs are drawn in a single draw call
- Survival and creative modes: timed block breaking with crack overlay,
  drops, a 36-slot inventory with stacks, and a scrollable creative palette
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
  inventory clicks, wind and cave ambience with dripping water, positional
  panning and distance falloff, and a muffled mix while underwater

## Mobs

| | Pig | Zombie |
|---|---|---|
| Health | 10 | 20 |
| Spawns | on sky-exposed grass | on sky-exposed solid ground when daylight < 0.35 |
| Cap | 12 | 8 |
| Behaviour | wanders, idles, looks around; panics when hit | chases survival players within 24 blocks, hits for 3 every second; burns in sunlight |

Mobs spawn 24–64 blocks from the player and despawn beyond 96 blocks or
when their chunk unloads. Player hits do 2–4 damage with knockback, at most
every 0.5 s.
