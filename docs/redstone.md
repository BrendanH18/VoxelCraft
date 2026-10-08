# Redstone

Redstone simulation runs once per shared 50 ms game tick, including headless
worlds. Component block states occupy the append-only 1100–1599 allocation;
existing redstone dust (item 365) places wire. States share texture layers.

## Signal engine

Signals have integer strength 0–15. Strong outputs power one full conducting
block; weak outputs activate adjacent devices without conducting through it.
Wire excludes all wire emissions while reading external power, then takes the
maximum connected wire power minus one. Horizontal wires step up a conducting
block when their own ceiling is clear, and down beside nonconductors. Wire
strongly powers its supporting block and the horizontal solids it points at.
A redstone block is a weak source; a torch strongly powers the block above it.

An iterative, deduplicated FIFO handles neighbour notifications. An edit wakes
only a fixed two-block neighbourhood. A heap ordered by deadline, priority and
insertion sequence handles scheduled ticks. Idle circuits perform no block
scans, allocate no memory and produce no work. A 65,536-update budget carries
pathological feedback work to the next tick instead of recursing or hanging.

Each tick increments the redstone clock, drains queued notifications, processes
due scheduled ticks in priority order and drains notifications after each tick.
Repeaters turn off at very high priority and on at high priority. This differs
from Java's synchronous neighbour callback stack and its directional diode
priority heuristic; update-order-sensitive contraptions are not guaranteed.
Timing otherwise uses Java game ticks: torch inversion 2, repeater delay
2/4/6/8, comparator 2, lamp off 4, stone button 20, wooden button 30. Scheduled
repeater rising edges stretch short input pulses. Only side-facing repeaters
and comparators lock repeaters. Eight torch off transitions within 60 ticks
cause a 160-tick restart delay. Burnout history is bounded per torch.

Wire power, facing, delay and on/off states are in VXC2 block data. A versioned
`redstone` dimension property preserves queued work, pending deadlines,
comparator output, torch history/burnout, and powered door/gate edges. Replaced
blocks invalidate their old ticks. Unloaded scheduled blocks wait for loading;
this currently retries their deadlines every tick instead of Java chunk tick
containers. Old saves without the property remain readable. Connections are
recomputed from neighbours rather than saved.

Comparators use Java inventory fullness (floor(14 × average normalized slot
fullness) plus one for any nonempty slot), reading chests, furnaces and brewing
stands directly or through one conductor. Item frames and double chests are
not implemented. Container mutable access wakes adjacent comparators.
Wooden doors and fence gates respond to rising/falling power edges, preserving
manual operation while power stays unchanged. Powered TNT primes once.

## Java sources

Behaviour is checked against the Minecraft Java block implementations:

- [RedStoneWireBlock](https://raw.githubusercontent.com/mahtomedi/minecraft/main/src/main/java/net/minecraft/world/level/block/RedStoneWireBlock.java)
- [RedstoneTorchBlock](https://raw.githubusercontent.com/mahtomedi/minecraft/main/src/main/java/net/minecraft/world/level/block/RedstoneTorchBlock.java)
- [DiodeBlock](https://raw.githubusercontent.com/mahtomedi/minecraft/main/src/main/java/net/minecraft/world/level/block/DiodeBlock.java)
- [RepeaterBlock](https://raw.githubusercontent.com/mahtomedi/minecraft/main/src/main/java/net/minecraft/world/level/block/RepeaterBlock.java)
- [ComparatorBlock](https://raw.githubusercontent.com/mahtomedi/minecraft/main/src/main/java/net/minecraft/world/level/block/ComparatorBlock.java)

The decompiled source mirror has no pinned 1.21 revision; core rules above are
stable Java rules, not a claim of exact 1.21 update-order parity.

## Inputs, models and recipes

Host right-click and generic CLI `place` call the same device use methods;
gamepad views route through the host action path. Sneak-building bypasses use.
Dust can toggle an isolated cross into a saved dot; the dot supplies power only
below it. Mounted lever/buttons/torches, plates and diodes have compact shapes;
wire, switches, torches and plates do not collide with walking entities.
Connections include upward sides in chunk models; power shares sixteen tinted
textures instead of allocating a texture for every state/connection.

Stone plates detect living players/mobs only; oak plates detect entities,
including item stacks and arrows. Light plates count up to fifteen entities;
heavy plates return ceil(entity_count/10), up to 150 entities. A whole dropped
stack counts as one entity. Binary plates recheck after 20 ticks, weighted ones
after 10. Wooden buttons stay pressed while an arrow overlaps their shape,
rechecking after 30 ticks; stone buttons ignore arrows and release after 20.
Contact inputs reuse a map and query only entity bounding-box cells. Spectators,
dead and inactive players are excluded. Entities such as XP orbs, TNT and
projectiles other than arrows are not yet included in plate counts.

Iron doors and iron trapdoors open only with power. Oak trapdoors permit manual
operation and respond to power edges. Arrow hits activate target blocks for 20
ticks, other callers' projectile hits for 8; face-centre distance gives 1–15
power and hits during the active pulse are ignored. Arrows clip shaped models
instead of the entire occupied cell. Ore illuminates to level 9 when mined,
used, stepped on or hit by arrows. Its 30-tick expiry is a simplification of
Java random-tick expiry; stone/deepslate ore harvest and fortune rules remain.

Daylight detectors update every 20 ticks and invert on use. They use VoxelCraft's
existing day clock and heightmap exposure; sun-angle easing and diffuse sky
light beneath transparent roofs are approximated. In sky-less dimensions they
produce no power. Chests, furnaces and brewing stand contents notify comparators
on player mutations and processing completion. Container UI slot changes must
use the mutable world accessor to issue those notifications.

Every core component has its vanilla shaped/shapeless recipe; target crafting
adds hay bales (nine wheat, reversible). Only oak wooden buttons, plates and
trapdoors currently exist. Door hinges, waterlogging and floor/ceiling switch
horizontal orientation are simplified. Hay bale fall-damage reduction is not
yet added. Bare block-item IDs use the existing encoded block-item namespace;
no material item IDs are reassigned.

Additional sources: [ButtonBlock](https://raw.githubusercontent.com/mahtomedi/minecraft/main/src/main/java/net/minecraft/world/level/block/ButtonBlock.java),
[PressurePlateBlock](https://raw.githubusercontent.com/mahtomedi/minecraft/main/src/main/java/net/minecraft/world/level/block/PressurePlateBlock.java),
[WeightedPressurePlateBlock](https://raw.githubusercontent.com/mahtomedi/minecraft/main/src/main/java/net/minecraft/world/level/block/WeightedPressurePlateBlock.java),
[TargetBlock](https://raw.githubusercontent.com/mahtomedi/minecraft/main/src/main/java/net/minecraft/world/level/block/TargetBlock.java),
[DaylightDetectorBlock](https://raw.githubusercontent.com/mahtomedi/minecraft/main/src/main/java/net/minecraft/world/level/block/DaylightDetectorBlock.java).

## Pistons and observers

Pistons and sticky pistons support six directions, enforce the twelve-block
push limit before editing, and reject unloaded destinations, world boundaries,
unbreakable blocks, obsidian, netherite blocks, portals, spawners, all containers,
and extended/moving pistons. Fragile controls/plants break and drop normally.
Sticky retraction pulls one eligible block; normal retraction leaves it behind.
Power excludes the front and includes Java quasi-connectivity above the base.
Block events share the scheduled queue after neighbour propagation.

Motion lasts two game ticks. A saved `automation` dimension property records
moving states and their age; chunk meshes omit moving placeholders and the
existing free-block renderer interpolates them without a new frame collection.
Collision currently uses the destination block's full cell while moving. Java
entity displacement, slime/honey assemblies, short-pulse sticky block spitting,
and exact piston block-event arbitration are not implemented. Directional devices use the nearest look direction, including vertical
placement, through the same host/pad/CLI placement path.

Observers watch state changes on their front face, delay two game ticks, then
emit strength 15 from their rear for two ticks. Repeated changes during a
pending/on pulse do not retrigger it. Container-only mutations do not count as
block state changes.

Sources: [PistonStructureResolver](https://raw.githubusercontent.com/mahtomedi/minecraft/main/src/main/java/net/minecraft/world/level/block/piston/PistonStructureResolver.java),
[PistonBaseBlock](https://raw.githubusercontent.com/mahtomedi/minecraft/main/src/main/java/net/minecraft/world/level/block/piston/PistonBaseBlock.java),
[ObserverBlock](https://raw.githubusercontent.com/mahtomedi/minecraft/main/src/main/java/net/minecraft/world/level/block/ObserverBlock.java).

## Inventory automation

Dispensers and droppers store nine slots, hoppers five. They reuse the existing
chest-backed save records with unused padding, preserving old container saves.
Host and pad screens, quick moves, CLI chest commands, and comparator fullness
respect actual capacity. Replacing a storage block with another family spills
its contents once and creates empty storage.

Dispensers/droppers fire four game ticks after a rising power edge and select a
nonempty slot uniformly. Holding power does not repeatedly dispense. Droppers
insert one item into the front container (retaining it if full) or eject it.
Dispensers eject ordinary items, shoot recoverable unowned arrows, place or
collect water/lava source buckets, and prime TNT. Fire charge item 760 has its
vanilla three-item recipe and ignites the front cell; a travelling small
fireball and Java dispenser inaccuracy are not implemented. Other special
behaviours such as equipment, bonemeal and spawn eggs fall back to item drops.

Hoppers push one item, then pull one from the container above in the same
eligible cycle, followed by eight game ticks of cooldown. Empty destination
hoppers receive a cooldown; Java's directional seven-tick optimization is
simplified to eight. Power disables transfer and pickup. Furnace insertion
uses top input/side fuel and bottom extraction uses output or empty buckets;
brewing insertion uses top ingredient/side bottle or blaze powder. Stack
components survive transfer, and full/incompatible destinations consume
nothing. With no container above, a hopper collects item entities above its
cavity, ignoring player pickup delay. A whole entity stack can be collected at
once, as in Java. Active hoppers retry idle transfers each tick; item pickup
currently scans the entity list rather than a spatial entity index.

Scheduled ticks, cooldowns, power edges and pending ejections survive saves.
The client calls `tick_automation_entities` between world rules and entity
physics; headless callers must invoke it with their entity collection too.
Only loaded containers transfer items. Round 2 rails, carts, tripwire and note
blocks are described below.

Sources: [HopperBlockEntity](https://raw.githubusercontent.com/mahtomedi/minecraft/main/src/main/java/net/minecraft/world/level/block/entity/HopperBlockEntity.java),
[DispenserBlock](https://raw.githubusercontent.com/mahtomedi/minecraft/main/src/main/java/net/minecraft/world/level/block/DispenserBlock.java),
[DropperBlock](https://raw.githubusercontent.com/mahtomedi/minecraft/main/src/main/java/net/minecraft/world/level/block/DropperBlock.java),
[DispenseItemBehavior](https://raw.githubusercontent.com/mahtomedi/minecraft/main/src/main/java/net/minecraft/core/dispenser/DispenseItemBehavior.java).


## Rails and minecarts (round 2)

Normal rails retain ids 500–509. Powered, detector and activator rail states
use 1471–1506; 1469 is untouched. Special rails cannot curve. Rail neighbour
updates choose straight, corner and ascending connections, remove unsupported
rails, and switch normal junction preference when powered. Both powered and
activator rails propagate along their own rail kind for eight additional rails
from a directly powered rail. Detector rails emit strength 15 while a cart
intersects their search box and recheck after 20 ticks; comparators read chest
and hopper cart fullness. Contents/occupancy changes notify comparators.
All rails render as alpha-tested detail planes, raised 1/16 block, with actual
one-block slopes and separate off/on textures. Vanilla rail and cart recipes
and base-state drops are registered.

Minecart items 761–764 place only on rails. Cart motion runs at 20 Hz with
Java's 0.4-block/tick movement cap, track projection, curve direction changes,
slope gravity, occupied/empty/container friction, powered acceleration and
unpowered braking. Walking players and mobs push carts; nearby moving empty
rideable carts collect mobs; carts exchange momentum on contact. Right-click
enters a rideable cart, the camera follows its seat, and sneak dismounts to a
nearby collision-free position. Survival punches break carts into their variant
item and spill contents; creative removes the cart without dropping its item.

Chest carts expose 27 slots and hopper carts five slots through the existing
container screen, including normal click and quick-move operations. Hoppers
feeding or draining carts respect their eight-tick block-hopper cooldown.
Hopper carts pull from above or collect nearby dropped stacks each tick; a
powered activator disables collection until an unpowered activator rail is encountered.
As in Java, a hopper below drains the cart; the cart does not push into an
arbitrary chest below. TNT carts get an 80-tick activator fuse, a shortened fire
fuse, and detonate on a three-block fall or fast horizontal impact. Blasts use
the existing explosion event path with speed-dependent power.

A per-dimension `minecarts` property saves ids, variant, full-precision position
and velocity, yaw, contents with stack components, hopper enabled state, TNT
fuse and player rider. Loading older worlds without it works. `--cart
x,y,z,rideable|chest|hopper|tnt` places a test cart after `--place` edits, for
visual verification.

Known gaps: waterlogged rails/water cart slowdown, exact Java cart collision
alignment and safe dismount floor selection, saved mob passengers, rolling
cart audio, textured cargo models and TNT's special rail/support blast
protection are not implemented. Split-screen controller riding and cart
container menus have not been completed. Burning projectiles do not yet hit
carts. Fire uses a deterministic short fuse rather than Java's random sum.
The existing explosion system determines visibility and block destruction.
No experimental minecart-improvements behaviour is enabled.

Research used the Java implementations
[RailState](https://raw.githubusercontent.com/mahtomedi/minecraft/main/src/main/java/net/minecraft/world/level/block/RailState.java),
[PoweredRailBlock](https://raw.githubusercontent.com/mahtomedi/minecraft/main/src/main/java/net/minecraft/world/level/block/PoweredRailBlock.java),
[AbstractMinecart](https://raw.githubusercontent.com/mahtomedi/minecraft/main/src/main/java/net/minecraft/world/entity/vehicle/AbstractMinecart.java),
[AbstractMinecartContainer](https://raw.githubusercontent.com/mahtomedi/minecraft/main/src/main/java/net/minecraft/world/entity/vehicle/AbstractMinecartContainer.java),
[MinecartHopper](https://raw.githubusercontent.com/mahtomedi/minecraft/main/src/main/java/net/minecraft/world/entity/vehicle/MinecartHopper.java)
and [MinecartTNT](https://raw.githubusercontent.com/mahtomedi/minecraft/main/src/main/java/net/minecraft/world/entity/vehicle/MinecartTNT.java).
This mirror is not pinned to Java 1.21; stable legacy rules were used and the
gaps above prevent a claim of complete Java parity.


## Note blocks and tripwire (round 2)

Note-block states 1507–1556 store 25 pitches and their powered edge. Right-click
cycles the pitch, left-click plays it, and a rising redstone edge plays once;
a non-air block above prevents playback. Sixteen instruments are selected by
the substrate below. Bone blocks (1581) and packed ice (1582) supply xylophone
and chime; both have their vanilla recipes, and packed ice requires Silk Touch
to drop. Original synthesized sounds use the existing spatial audio mixer,
with instrument-specific timbre and octave plus semitone playback rate.

Hooks (1557–1572) mount on solid horizontal faces and string items place
tripwire (1573–1580). Opposing hooks connect across 1–40 string blocks, including
chunk boundaries. Entities crossing the wire power both hooks with strength
15; a hook strongly powers its supporting block. Occupancy rechecks every ten
game ticks. Cutting an armed, attached string holds a ten-tick alarm pulse;
shears disarm before removal and suppress that pulse. String always drops as
string. Hook facing, wire attachment/disarmed state, note pitch and powered
state are block states; pending alarm deadlines survive redstone saves.

Known gaps: note particles, mob-head instruments above the note block, and
tripwire attach/click sounds are absent. Procedural instruments are original
approximations of the Java timbres. Substrate mapping covers the sixteen
canonical blocks and common existing wood/stone forms, but is not a complete
Java block-tag table. Tripwire geometry does not yet visibly sag when detached.

Research: [NoteBlock](https://raw.githubusercontent.com/mahtomedi/minecraft/main/src/main/java/net/minecraft/world/level/block/NoteBlock.java),
[TripWireBlock](https://raw.githubusercontent.com/mahtomedi/minecraft/main/src/main/java/net/minecraft/world/level/block/TripWireBlock.java),
and [TripWireHookBlock](https://raw.githubusercontent.com/mahtomedi/minecraft/main/src/main/java/net/minecraft/world/level/block/TripWireHookBlock.java).

Validation: the requested fmt, release Clippy and release test gates pass with
both default features and `--no-default-features`. Tests cover rail connections,
slopes, support removal, the eight-rail power limit, cart motion/containers/riding,
variant save round trips, all sixteen instrument substrates, note rising edges,
wire entity contacts, cross-chunk alarm pulses and shears-safe cuts. Screenshot
fixtures are built near y=150 using the command-line placement and cart flags.

On the Apple M5, three final `--bench --rd 8` runs had median generation
0.213 ms/chunk, light+mesh 0.669 ms/chunk and streaming 0.20 s. Three alternating
baseline runs measured 0.227 and 0.727 ms/chunk; timings show no regression.
Local inspected captures: `target/redstone-review/rails-carts.png`,
`rails-close.png`, `notes-tripwire.png` and `tripwire-close.png` in that directory.
