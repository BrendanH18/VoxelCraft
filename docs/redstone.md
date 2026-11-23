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

Updates follow Java 26.3's order (`world::redstone_order`, v0.17 Track A,
[record](verification/2026-11-23-v0.17-update-order.md)). Every edit is
`Level.setBlock`: the old block's `affectNeighborsAfterRemoval`, the new
block's `onPlace`, `updateNeighborsAt` (west, east, down, up, north, south),
the comparator notification, then shape updates (west, east, north, south,
down, up). Neighbour updates run on Java's depth-first
`CollectingNeighborUpdater` stack, synchronously, with Java's limit of
1,000,000 chained updates per run (later ones are dropped). Wire follows
`DefaultRedstoneWireEvaluator`: power set without neighbour updates, then
updates around the wire and its six neighbours in Java `HashSet` order.
Diodes, comparators, torches, levers, buttons, plates, lamps and observers
make Java's update calls with Java's flags. Doors, trapdoors, gates, lamps
and hoppers change without neighbour updates. Observers react to shape
updates from their watched face; attached switches, torches, plates and
wire break on the shape update from their support.

Each tick collects every due scheduled tick (at most 65,536), ordered by
trigger tick, priority and sub-tick order, then runs them; ticks scheduled
meanwhile wait for a later tick. Repeaters schedule EXTREMELY_HIGH when
the block in front is a diode not facing back, else VERY_HIGH turning off
and HIGH turning on, and skip if already collected (`willTickThisTick`).
Comparators schedule HIGH or NORMAL; torches, lamps and observers NORMAL.
Block events (`World::queue_block_event`) are deduplicated and run at the
end of the tick, after the random ticks, as in `ServerLevel.tick`. Idle
circuits perform no block scans, allocate no memory and produce no work.

Changes from outside a ported Java behaviour keep a few compatibility
notifications (`flags::COMPAT`): container, sensor and cart notifications
run at the start of the next tick, before the clock advances; newly placed
blocks without a ported placement check check themselves then; and
unported strong sources update around the block they power. The
[record](verification/2026-11-23-v0.17-update-order.md#differences-from-java)
lists these and the other differences: derived repeater locks and wire
connections, and non-ticking cells parking their updates. Timing uses Java
game ticks: torch inversion 2, repeater delay 2/4/6/8, comparator 2, lamp
off 4, stone button 20, wooden button 30. Scheduled repeater rising edges
stretch short input pulses. Only side-facing repeaters and comparators lock
repeaters. Eight torch off-toggles within 60 ticks schedule a 160-tick tick,
and the torch relights on it once fewer than eight toggles remain in the
window.

Wire power, facing, delay and on/off states are in the chunk block data (`VXC2`, or `VXC3` once a saved state ID is 4096 or above; see [architecture](architecture.md#block-and-texture-ids)). A versioned
`redstone` dimension property (v2; v1 still loads) preserves queued work,
pending deadlines, block events, comparator output, torch toggle history,
and powered door/gate edges. Replaced
blocks invalidate their old ticks. Due ticks for unloaded blocks are parked by
chunk and resume on its load, retaining deadline, priority and insertion order
without a per-tick retry. Parked ticks still use the existing v1 relative-deadline
save records; overdue work runs on the next simulation tick after loading. The
pinned Java 26.3 `LevelTicks.removeContainer` removes chunks from active
scheduling while `LevelChunkTicks` retains their pending work; VoxelCraft keeps
its in-memory absolute deadlines rather than persisting a separate chunk ticker.
Old saves without the property remain readable. Connections are
recomputed from neighbours rather than saved.

Comparators use Java inventory fullness (floor(14 × average normalized slot
fullness) plus one for any nonempty slot), reading chests, furnaces and brewing
stands directly or through one conductor, and item frames on a conductor's
far side. A double chest reads as one 54-slot container from either half,
and a chest blocked from above (a conductor on either half) reads 0, as
Java's `ChestBlock.getAnalogOutputSignal` does. Container mutable access
wakes adjacent comparators (both halves of a double chest).
Wooden doors and fence gates respond to rising/falling power edges, preserving
manual operation while power stays unchanged. Powered TNT primes once.

## Reference contraptions

`tests/contraptions` builds sixteen reference machines (clocks, piston
doors, a flying machine, a sorter, a smelter, farms, a minecart storage
line, an autocrafter, a BUD switch and a T flip-flop) in a headless world,
records a per-tick trace, and repeats it after unloading the chunks, after
a save and fresh load, and after a restart mid-motion. Each build's Java
26.3 behaviour, the differences found and the tracks the ignored checks
wait for are in the [Track F record](verification/2026-11-23-v0.17-reference-contraptions.md).

## Java sources

Behaviour is checked against the Minecraft Java block implementations:

- [RedStoneWireBlock](https://raw.githubusercontent.com/mahtomedi/minecraft/main/src/main/java/net/minecraft/world/level/block/RedStoneWireBlock.java)
- [RedstoneTorchBlock](https://raw.githubusercontent.com/mahtomedi/minecraft/main/src/main/java/net/minecraft/world/level/block/RedstoneTorchBlock.java)
- [DiodeBlock](https://raw.githubusercontent.com/mahtomedi/minecraft/main/src/main/java/net/minecraft/world/level/block/DiodeBlock.java)
- [RepeaterBlock](https://raw.githubusercontent.com/mahtomedi/minecraft/main/src/main/java/net/minecraft/world/level/block/RepeaterBlock.java)
- [ComparatorBlock](https://raw.githubusercontent.com/mahtomedi/minecraft/main/src/main/java/net/minecraft/world/level/block/ComparatorBlock.java)

The decompiled source mirror has no pinned revision. The update order is
checked against the pinned 26.3 server jar instead:
`tools/extract_java_redstone_order_fixtures.py` extracts its orders and
constants and models seven contraptions, and `tests/redstone_order.rs`
compares the game's per-tick states and update logs with them.

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
dead and inactive players are excluded. Wooden and weighted plates also count
XP orbs (0.5 box), primed TNT, minecarts, thrown items, ender pearls and eyes,
splash potions, fireballs and fishing bobbers, as Java's `Entity` query does;
the touch box is Java's 1/16-inset column (x and z 1/16 to 15/16, height 0.25).
Bats (not implemented) would be excluded by `isIgnoringBlockTriggers`.
Reference: pinned Java 26.3 `BasePressurePlateBlock` static initializer
(`Block.column(14, 0, 4)`) and `PressurePlateBlock.getSignalStrength`;
wood uses `Entity`, stone uses `LivingEntity`. Regression coverage includes
orb edge overlap, TNT, potions, bobbers and fireballs. Entity contact boxes
use the bottom-centred sizes in 26.3 `EntityTypes` (arrows/orbs 0.5, small
fireballs 0.3125, large fireballs 1, other implemented projectiles/items 0.25).

Iron doors and iron trapdoors open only with power. Oak trapdoors permit manual
operation and respond to power edges. Arrow hits activate target blocks for 20
ticks, other callers' projectile hits for 8; face-centre distance gives 1–15
power and hits during the active pulse are ignored. Arrows clip shaped models
instead of the entire occupied cell. Ore illuminates to level 9 when mined,
used, stepped on or hit by arrows, and goes dark on a random tick like Java's
`RedStoneOreBlock.randomTick` (no fixed expiry; lighting a lit ore schedules
nothing); stone/deepslate ore harvest and fortune rules remain.

Daylight detectors update every 20 ticks and invert on use. They follow
`DaylightDetectorBlock.updateSignalStrength`: propagated sky light at the
detector minus the shared `skyDarken`, then, unless inverted, scaled by the
cosine of the sun angle eased 20% toward noon or midnight and rounded
(`simulation::lighting::daylight_detector_power`). Dimensions without
skylight read 0, so an inverted detector there gives 15. Chests, furnaces and brewing stand contents notify comparators
on player mutations and processing completion. Container UI slot changes must

Daylight detectors update every 20 ticks and invert on use. They use VoxelCraft's
existing day clock and heightmap exposure; sun-angle easing and diffuse sky
light beneath transparent roofs are approximated. In sky-less dimensions they
produce no power. Chests, furnaces and brewing stand contents notify comparators
on player mutations and processing completion, and a brewing stand also when it
loads blaze powder into its fuel bar (`BrewingStandBlockEntity.serverTick`). Container UI slot changes must
use the mutable world accessor to issue those notifications.

Every core component has its vanilla shaped/shapeless recipe; target crafting
adds hay bales (nine wheat, reversible). Every Java 26.3 button, pressure
plate, trapdoor, door and fence gate exists (all woods, stone, polished
blackstone, iron and copper; see [component inventory](#component-inventory-v017-track-e2)). Door hinges,
waterlogging and floor/ceiling switch horizontal orientation are simplified. Hay bales multiply landing damage by
0.2f before flooring, as in Java 26.3 `HayBlock.fallOn` and
`LivingEntity.calculateFallDamage` (pinned in `building.json`, including threshold
cases; all three placement axes are covered by the shared player environment).
Ordinary-player and performance acceptance remain open. Bare block-item IDs use the existing encoded block-item namespace;
no material item IDs are reassigned.

Additional sources: [ButtonBlock](https://raw.githubusercontent.com/mahtomedi/minecraft/main/src/main/java/net/minecraft/world/level/block/ButtonBlock.java),
[PressurePlateBlock](https://raw.githubusercontent.com/mahtomedi/minecraft/main/src/main/java/net/minecraft/world/level/block/PressurePlateBlock.java),
[WeightedPressurePlateBlock](https://raw.githubusercontent.com/mahtomedi/minecraft/main/src/main/java/net/minecraft/world/level/block/WeightedPressurePlateBlock.java),
[TargetBlock](https://raw.githubusercontent.com/mahtomedi/minecraft/main/src/main/java/net/minecraft/world/level/block/TargetBlock.java),
[DaylightDetectorBlock](https://raw.githubusercontent.com/mahtomedi/minecraft/main/src/main/java/net/minecraft/world/level/block/DaylightDetectorBlock.java).

## Pistons and observers

Pistons follow Java 26.3's `PistonBaseBlock`, `PistonStructureResolver`,
`MovingPistonBlock` and `PistonMovingBlockEntity` (v0.17 Track B, code in
`world::piston`; [record](verification/2026-11-23-v0.17-pistons.md)).

**Structures.** A neighbour change or placement runs `checkIfExtend`: power
from any side but the front, or quasi-connectivity through the cell above,
queues an extend event when the resolver can move the structure; losing
power queues a retract event. The resolver moves at most twelve blocks over
the whole assembly. Slime and honey drag the sticky-to-each-other blocks
beside them (honey never holds slime), and the moved list keeps Java's
order, including the reordering when a dragged line runs into blocks
already listed. Each block's reaction is its Java `PushReaction` with
`isPushable`'s checks: unbreakable blocks, `IMMOVEABLE` blocks (obsidian,
anvils, respawn anchors, ...), extended pistons and every block-entity block
stay put; `POPPED` blocks break and drop; glazed terracotta is `PUSH` (pushed,
never pulled or dragged). `tests/pistons.rs` compares the class of every Java
block VoxelCraft registers against `tests/fixtures/java/26.3/pistons.json`.
Unloaded cells and the build limits stop a push.

**Block events.** Piston events go on the level's Java block-event queue
(`Level.blockEvent`; see [Signal engine](#signal-engine)): equal events merge,
they run at the end of the tick after scheduled and random ticks
(`runBlockEvents`), events queued while it runs included, wait in unloaded
chunks, and are saved with the `redstone` property. Each runs only if its
cell still holds that kind of piston (`doBlockEvent`). A move edits cells
with Java's `setBlock` flags and sends `moveBlocks`' neighbour and shape
updates in Java's order (the vacated cells in `HashMap` order), so contraptions
see Java's update order. `triggerEvent` cancels an extension that lost power, turns a
retraction that found power again into a plain state change, completes an
arm still extending at once (`finalTick`), and pulls with a sticky piston only
when the block two ahead is a `PUSH_PULL` block or a piston. A falling edge
while the block ahead is still extending with progress below 0.5, in the
tick it last moved, or during block ticks and events, is a drop event: the
sticky piston leaves its block behind (short-pulse spitting).

**Moving cells.** A moving cell is the `moving_piston` block (state 1470)
with a block entity holding the moved state, direction, extending and
source flags and progress. Java's `facing` and `type` properties of that
block are read from the block entity, which always matches them. Block
entities tick after block events in creation order, gaining 0.5 progress a
tick and landing the tick after they reach 1, with `updateFromNeighbourShapes`
(stairs and walls reshape; a waterlogged block lands dry; a head whose base
is gone lands as air). Their collision shape is the moved block's (a head for
the arm, plus the extended base while a base retracts) offset by progress,
so it crosses into the neighbouring cell. An explosion that catches a moving
cell drops its moved block. Block entities tick right after the block events
(`World::tick_moving_pistons`), which ends Java's `handlingTick`. The free-block renderer interpolates moving
blocks and draws retracting bases in place.

**Entities.** Each moving-block tick records what its shape sweeps. After
the world tick, players (host, controllers and CLI agents), mobs, items,
experience orbs, minecarts and boats, and primed TNT are moved by that sweep plus 0.01
through the shared collision path, with moving blocks going the same way
ignored (`NOCLIP`) and Java's limit of 0.51 blocks per axis per tick, so
entities pushed into walls stop against them. Slime sets the movement axis
of the velocity of every entity in its swept area to one block per tick;
honey moving sideways carries grounded entities standing on it by the
progress step; an entity left inside a retracting base's cell is moved out of
its front. Riders move with their vehicle.

**Saves.** The `automation` dimension property (v2) stores each moving cell's
block entity, its progress and the previous tick's progress, so a world
saved mid-motion continues exactly. Older v1
two-tick motion records migrate on load.

**Observers** watch shape updates from their front face, delay two game
ticks, then emit strength 15 from their rear for two ticks. Repeated changes
during a pending/on pulse do not retrigger it. Cells turning into moving
pistons and landing send those shape updates, and a powered observer that
arrives with no pending tick (pushed by a piston, or placed) turns off at
once (`ObserverBlock.onPlace`). Container-only mutations do not count as
block state changes. Fire ageing does: `FireBlock.tick` writes the new age
with flags 260, which still update neighbour shapes, so an observer facing
fire sees it age (v0.17 Track E2).

Known differences from Java: entity pushes run once per
tick after the world tick instead of inside each block entity's tick, so an
entity's later pushes in a tick see every moving block already advanced;
pushed entities don't step up; the short piston-head shape is not modelled;
honey's `isSupportedBy` is approximated by standing on the moving cell's top;
projectiles, falling blocks, End crystals and decorations (item frames,
paintings, armor stands) are not pushed (Java pushes them, and breaks
hanging entities); Java saves the previous progress and
replays one step after a reload, VoxelCraft resumes exactly; Java
loads chunks a push reaches, VoxelCraft stops at unloaded ones; and LAN
clients don't receive moving-block entities, so they see moving cells empty. Directional
devices use the nearest look direction, including vertical placement,
through the same host/pad/CLI placement path.

Sources: [PistonStructureResolver](https://raw.githubusercontent.com/mahtomedi/minecraft/main/src/main/java/net/minecraft/world/level/block/piston/PistonStructureResolver.java),
[PistonBaseBlock](https://raw.githubusercontent.com/mahtomedi/minecraft/main/src/main/java/net/minecraft/world/level/block/piston/PistonBaseBlock.java),
[ObserverBlock](https://raw.githubusercontent.com/mahtomedi/minecraft/main/src/main/java/net/minecraft/world/level/block/ObserverBlock.java),
and the pinned Java 26.3 server jar's `PistonMovingBlockEntity`,
`MovingPistonBlock`, `PistonHeadBlock` and `Entity.limitPistonMovement`.

## Inventory automation

Dispensers and droppers store nine slots, hoppers five. They reuse the existing
chest-backed save records with unused padding, preserving old container saves.
Host and pad screens, quick moves, CLI chest commands, and comparator fullness
respect actual capacity. Replacing a storage block with another family spills
its contents once and creates empty storage.

Dispensers/droppers fire four game ticks after a rising power edge and select a
nonempty slot uniformly. Holding power does not repeatedly dispense. Droppers
insert one item into the front container through its facing side (retaining
it if full) or eject it. Following Java 26.3 `DefaultDispenseItemBehavior.spawnItem`,
dispensed and dropped items appear 0.7 blocks out (0.15625 lower, 0.125
vertically) at 0.2-0.3 blocks per tick plus `triangle(0.0172275 * 6)` noise
per axis with a 10-tick pickup delay.

Dispensers follow Java 26.3's dispense table (v0.17 Track C2,
[record](verification/2026-11-23-v0.17-dispensing.md)): `world::dispensing::behaviour`
resolves an item as `DispenserBlock.getDispenseMethod` does (the
`DispenseItemBehavior.bootStrap` registry, then any equippable item, then
`#sulfur_cube_swallowable`), checked both ways against
`tests/fixtures/java/26.3/dispensing.json`.

- Projectiles leave at their `DispenseConfig` power and uncertainty: arrows,
  eggs and snowballs 0.7 out and 0.1 up at 1.1 (uncertainty 6), experience
  bottles and splash and lingering potions at 1.375 (3), fire charges and
  wind charges a block out at 1.0 (6.67), rockets half a block out at 0.5 (1).
  A fire charge launches a small fireball, which ignites where it lands.
- Equippable items (armour, elytra, heads, carved pumpkins, shields,
  saddles, horse and nautilus armour, carpets, harnesses) go into the empty
  slot of the first entity in front that `canEquipWithDispenser` allows: a
  mob that can pick up loot, a tamed horse, donkey, mule, camel or llama (a
  saddle, armour or carpet), a grown pig, strider or happy ghast (saddle or
  harness), an armour stand, or a player; a mob keeps it on death and stops
  despawning. With nobody to take it the item pops out. A carved pumpkin
  first completes a snow, iron or copper golem base in front; a wither
  skeleton skull a Wither base; with no base and nobody to wear them they
  stay put.
- Shears harvest a full hive (three honeycomb, bees out), cut a lead or a
  fence knot's leads, or shear a sheep, mooshroom, bogged, sulfur cube or
  copper golem, losing one durability; a brush brushes an armadillo (16).
  Flint and steel primes a sulfur cube, sets fire, lights a campfire,
  candle or candle cake, or primes TNT, and wears even when it meets
  nothing (as in Java).
- Water, lava, powder snow and mob buckets empty in front and empty
  buckets fill, leaving the bucket in the same slot (`consumeWithRemainder`);
  glass bottles fill at a full hive (honey) or from water; a water bottle
  turns dirt, coarse dirt or rooted dirt into mud; glowstone charges a
  respawn anchor (a full one keeps it); honeycomb waxes copper; bone meal
  is spent only when it grew something; TNT is primed (kept without
  `tnt_explodes`); shulker boxes are placed.
- Boats go a block up onto water in front, or onto water below an air cell;
  minecarts onto a rail in front or over one (through the cart API); an
  armour stand stands in front turned the dispenser's way; a chest goes
  onto a tamed donkey, mule or llama. Otherwise each pops out.
- Spawn eggs don't exist yet (v0.18).

Behaviours that look for entities finish in `tick_automation_entities`
later in the same game tick, with the slot they came from; the app lends
its players through `tick_automation_entities_with`.

Hoppers push one item, then pull one from the container above in the same
eligible cycle, followed by eight game ticks of cooldown. An emptied
destination hopper receives cooldown `8 - k` as in Java's `tryMoveInItem`: seven
when a hopper source pushes into a hopper that already ran its tick this game
tick, otherwise eight. The scheduled-tick order stands in for Java's
block-entity ticker order, and a destination idling on cooldown is not
re-ticked, so it counts as not yet ticked. Power disables transfer and pickup.

Hoppers, droppers and hopper minecarts reach containers through Java 26.3's
faces (`world::container_sides`, checked against the same fixture): each
face's slots in Java's order, with each container's `canPlaceItem` and
`canTakeItem`. A furnace (smoker, blast furnace) takes input from above,
fuel from the sides and below (a second bucket never joins a bucket in the
fuel slot) and gives up its output, then an empty or water bucket from the
fuel slot, below. A brewing stand takes its ingredient from above, bottles
and blaze powder from the sides, and bottles and the ingredient from below,
and gives up bottles and an emptied glass bottle. Shulker boxes refuse
shulker boxes; a composter takes compostables from above and gives bone
meal below, and at level 7 still counts as a container. Jukeboxes take a
disc, chiseled bookshelves one book (written and writable books included)
per slot, and shelves anything. Stack components survive transfer, and
full/incompatible destinations consume nothing. Decorated pots hold no item
yet, and the crafter's faces are Track C1's.

With no container above, a hopper collects item entities whose boxes meet
Java's `Hopper.SUCK_AABB` (from 11/16 up the hopper to the top of the cell
above), ignoring player pickup delay, through a sorted cell index of item
entities built once per tick, not a walk over every item per hopper. A
full block above (other than a hive or nest) stops collection. A whole
entity stack can be collected at once; a stack only partly taken stays and
the next item is tried, as in Java. Active hoppers retry idle transfers each
tick.

Scheduled ticks, cooldowns, power edges, pending ejections and queued
dispenses survive saves.
The client calls `tick_automation_entities` between world rules and entity
physics; headless callers must invoke it with their entity collection too.
Only loaded containers transfer items. Round 2 rails, carts, tripwire and note
blocks are described below.

Sources: [HopperBlockEntity](https://raw.githubusercontent.com/mahtomedi/minecraft/main/src/main/java/net/minecraft/world/level/block/entity/HopperBlockEntity.java),
[DispenserBlock](https://raw.githubusercontent.com/mahtomedi/minecraft/main/src/main/java/net/minecraft/world/level/block/DispenserBlock.java),
[DropperBlock](https://raw.githubusercontent.com/mahtomedi/minecraft/main/src/main/java/net/minecraft/world/level/block/DropperBlock.java),
[DispenseItemBehavior](https://raw.githubusercontent.com/mahtomedi/minecraft/main/src/main/java/net/minecraft/core/dispenser/DispenseItemBehavior.java);
v0.17 Track C2 read the pinned 26.3 jar itself ([record](verification/2026-11-23-v0.17-dispensing.md)).

## Storage transactions

Every storage screen runs through `session::storage` (v0.17 Track D2): chests
(both halves of a double chest as one 54-slot container), trapped and copper
chests, barrels, shulker boxes, hoppers, dispensers, droppers, furnaces,
smokers, blast furnaces, brewing stands, chest and hopper carts (and the other
vehicles with storage) and the ender inventory. The desktop host, controller
seats and LAN peers reach it through the app's screen layer; headless callers
and the fuzz use it directly. It follows Java 26.3's
`AbstractContainerMenu.doClick`, `moveItemStackTo` and each menu's
`quickMoveStack`, pinned in `tests/fixtures/java/26.3/storage_transactions.json`:

- A click picks up, places, merges or swaps under each slot's rules (bottle
  slots hold one potion and keep its components; the furnace output only
  gives, half of it with the right button).
- A shift-click moves storage slots to the hotbar from the right, then the
  main grid from the bottom right, except a furnace's input and fuel, which go
  to the main grid from the top left first. From the inventory, chest-backed
  storage fills forwards; a furnace takes smeltables into its input, else fuel;
  a brewing stand takes blaze powder into its fuel slot and only tries the
  ingredient slot when the fuel slot took none, other ingredients only into
  the ingredient slot and potions one per empty bottle slot. Something a
  storage refuses stays put rather than moving between hotbar and grid.
- On the desktop, a number key over a slot swaps it with that hotbar slot
  (`SWAP`), as far as the slot's rules and limit allow, and the drop key over
  a storage slot throws one item or, with Ctrl, the stack (`THROW`).
- Breaking a container spills its contents whatever the `block_drops` rule
  says (`Containers.dropContents`); only the block's own loot follows the
  rule. A shulker box's contents ride in its item (its loot has no explosion
  condition); a furnace releases its stored experience either way.
- A hopper topping up a furnace's input keeps the cook going; only a
  different input restarts it (`AbstractFurnaceBlockEntity.setItem`).

`tests/storage_transactions.rs` drives seeded random sequences of these
operations, the CLI and headless LAN `chest` and `furnace` commands, pickups,
hoppers, droppers, dispensers, carts, survival mining, explosions and a chunk
unload or a save and reload, and checks after every step that every item and
component is conserved, that no stack breaks its limit or a slot rule, and
that each comparator shows Java's reading. New storage joins it as one entry
in its `KINDS` table. Not modelled: dragging a stack across slots
(`QUICK_CRAFT`), double-click collecting (`PICKUP_ALL`), number keys on
controllers and LAN clients, and a hopper under a rail (VoxelCraft rails need
a full block below, so no hopper drains a cart). See the
[verification record](verification/2026-11-23-v0.17-storage-fuzz.md).


## Crafter

v0.17 Track C1 (implemented, unverified;
[record](verification/2026-11-23-v0.17-crafter.md)). The crafter has
Java 26.3's 48 states (twelve `FrontAndTop` orientations, `triggered`,
`crafting`), its recipe (iron, a crafting table, redstone and a dropper),
1.5 hardness, 3.5 blast resistance, pickaxe speed without needing a tool,
and drops itself. Placement faces the front toward the player (the nearest
looking direction's opposite); a crafter facing up or down takes its top
from the player's horizontal facing, the others have `up` on top.
`triggered` comes from `hasNeighborSignal` at the cell, with no
quasi-connectivity; a crafter placed powered crafts four ticks later.

The nine slots are the chest-table entry dispensers use, so they save with
the chests, spill when the crafter is broken and reach hoppers, droppers,
LAN container sync and the CLI's `chest take/put`. Disabled slots and the
crafting countdown are the crafter's block entity, saved in the `crafters`
dimension property (Java's `disabled_slots` and
`crafting_ticks_remaining`); `triggered` is the block state. A slot can be
turned off or on only while it is empty (`CrafterBlockEntity.setSlotState`);
nothing goes into a disabled slot.

A rising edge sets `triggered` and schedules a craft four game ticks later
(`CRAFTING_TICK_DELAY`); a falling edge clears `triggered` and `crafting`.
The craft runs the game's recipe matcher on the 3×3 grid, including the
special recipes it knows (dyeing, tipped arrows, bundle and shulker box
dyeing, map cloning and extending, banner and book copying, shield
decoration, firework rockets, stars and fades). A result turns `crafting`
on for six block-entity ticks, leaves out of the front together with each
remainder (buckets, bottles, a copied book or banner): into the container
in front (one item at a time into another crafter), or as dropped items
with the dropper's speed and offset, the craft sound and white smoke. Then
every filled slot loses one item. No recipe plays the failure sound. Item
repair and the decorated pot's sherd faces are not in the matcher, so the
crafter can't make them either.

Hoppers and droppers fill the first enabled slot that is empty or holds a
stack no smaller than every later enabled slot's stack of the same item
(`canPlaceItem`), which spreads items smallest stack first. Comparators
read the number of slots that are filled or disabled. Every face reaches
all nine slots; `world::crafter::slots_for_face` is the seam for sided
transfers. The screen (desktop and controller) shows the grid with
disabled slots covered, the powered indicator and the crafting preview;
clicking an empty slot with an empty cursor turns it off, clicking a
disabled slot turns it on (and places the held stack). CLI agents use
`crafter toggle 0..8`, and `observe` shows a targeted crafter's slots,
toggles, power and preview.

Sources: `CrafterBlock`, `CrafterBlockEntity`, `CrafterMenu`, `CrafterSlot`
and `FrontAndTop` in the pinned 26.3 server's bytecode
([`crafter.json`](../tests/fixtures/java/26.3/crafter.json)).

## Rails and minecarts (round 2)

Normal rails retain ids 500–509. Powered, detector and activator rail states
use 1471–1506; 1469 is untouched. Special rails are straight or ascending only, as in
Java 26.3 (`PoweredRailBlock` and the detector/activator rails use `RAIL_SHAPE_STRAIGHT`;
only plain rails curve). Rail neighbour
updates choose straight, corner and ascending connections, remove unsupported
rails, and switch normal junction preference when powered. Both powered and
activator rails propagate along their own rail kind for eight additional rails
from a directly powered rail. Detector rails emit strength 15 while a cart
intersects their search box and recheck after 20 ticks; comparators read chest
and hopper cart fullness. Contents/occupancy changes notify comparators. The first overlapping container
cart supplies the comparator signal, including a cart straddling two detectors;
rideable/TNT carts do not mask container carts.
All rails render as alpha-tested detail planes, raised 1/16 block, with actual
one-block slopes and separate off/on textures. Vanilla rail and cart recipes
and base-state drops are registered.

Minecart items 761–764 and 1800 place only on rails. Cart motion runs at 20 Hz with
Java's 0.4-block/tick movement cap, track projection, curve direction changes,
slope gravity, occupied/empty/container friction, powered acceleration and
unpowered braking. Contacts follow `AbstractMinecart.push` and
`OldMinecartBehavior.pushAndPickupEntities` (v0.17 Track D1): a player or
mob whose box touches a minecart pushes it away and gets a quarter of that
push back; a rideable cart moving at least 0.1 blocks a tick takes in the
first mob its reach (its box grown 0.2 sideways) touches while it carries
nothing, never an iron golem or a bat, with no size limit, and shoves the
rest, players included (`Entity.push`); every minecart pushes the minecarts
its reach touches, but only when the line between them lies within the
pushed cart's facing (`pushOtherMinecart`'s 0.8 cosine of its yaw, which is
0 for a cart that has not moved yet). Both then keep a fifth of their speed
and share the average; a furnace cart keeps 95% of its own and hands the
other its speed. Boats keep the earlier contact push. Right-click
enters a rideable cart, the camera follows its seat, and sneak dismounts using
Java's `getDismountLocationForPassenger`: the eight cells around the cart in
`DismountHelper.offsetsForDirection` order (sideways first, relative to the rail
axis), at the heights 0, +1 and -1 for standing, crouching and swimming poses,
taking the first valid floor with a free player box. A cart in water (`isInWater`)
is capped at 0.2 blocks per tick, climbs slopes at one fifth of the pull,
loses a further 5% of its speed per tick and falls with gravity 0.005 instead
of 0.04 (`OldMinecartBehavior`, `getDefaultGravity`). A waterlogged block's
water counts, so a cart on a wet rail is in water. Every rail kind takes
the `waterlogged` state through the BLD-04f partners; shape and power changes
keep the water. Survival punches break carts into their variant
item and spill contents; creative removes the cart without dropping its item.

The furnace minecart (item 1800, `MinecartFurnace`, v0.17 Track D1) is
crafted from a furnace and a minecart, drops itself and carries nobody.
Right-clicking it with coal or charcoal (`#furnace_minecart_fuel`) adds 3600
ticks of fuel unless that would pass 32000, uses the item outside creative,
and sets its push to the horizontal line from the player's feet to the cart;
any other click is used and does nothing. Each tick on the rails its push
turns to its line of motion (`calculateNewPushAlong`) and its speed becomes
`0.8 × speed + push` (a tenth of that in water), then `0.98 × speed` without
a push, before the normal 0.96 slowdown; it moves at most half the normal
cap (0.2 blocks a tick, three quarters in water). It burns a tick of fuel
after each move and loses its push when the fuel runs out. While it has fuel
it shows a lit furnace mouth and puffs large smoke 0.8 above its feet one
tick in four. Its fuel and push are saved with it (Java `Fuel`, `PushX`,
`PushZ`) and sent to LAN guests, who see it lit.

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
fuse, player rider and a furnace cart's fuel and push. Mobs keep the cart they
sit in through the mob saves (villagers, wandering traders and golems too,
since v0.17 Track D1), and LAN guests receive each mob's vehicle. Loading
older worlds without these works. `--cart
x,y,z,rideable|chest|hopper|tnt|furnace|furnace_lit` places a test cart after
`--place` edits, for visual verification (`furnace_lit` with one coal's fuel
and no push); a boat or cushion item name (`red_cushion`) places that
vehicle instead, a cushion on the floor of the given cell.

A TNT cart lit by fire or lava gets Java's `nextInt(20) + nextInt(20)` fuse,
and smokes each tick while primed. Any burning projectile that breaks a TNT
cart primes it the same way instead of dropping it
(`shouldSourceDestroy`, `destroy`); a burning arrow still sets it off at once.
Blaze fireballs hit carts and boats for their 5 damage, breaking them; ghast
fireballs explode against them. A TNT cart's blast is centred on its feet and
is a TNT explosion; once the cart had been primed (an activator rail, fire, a
blast or a fuse already lit), it spares rails and the blocks under them: they
cost the blast's rays nothing and are never destroyed
(`getBlockExplosionResistance`, `shouldBlockExplode`). A cart that goes off
at once from an impact, a fall or a burning arrow without a lit fuse breaks
rails like any blast.

Moving minecarts roll (`entity.minecart.riding`, volume `0.7 × clamp(speed,
0, 0.5)`, pitch 1, since `MinecartSoundInstance`'s own `pitch` field shadows
the played one) and the local player's own cart plays its inside sound with
no distance falloff (`0.75 × min(speed, 1)`, the underwater variant while
their head is under water), both silent below 0.01 blocks a tick. They are
original procedural sounds played as overlapping half-second stretches.

Known gaps: textured cargo models (cargo is a coloured box). Arrows only burn
with Flame, not after flying through fire or lava. A LAN guest hears the
rolling sound the host plays but not its own inside sound. Mobs other than
bats and ridden mounts count as pushable; Java's other `isPushable`
exceptions (climbing, a digging warden, team rules) are not modelled. The
existing explosion system determines visibility and block destruction.
No experimental minecart-improvements behaviour is enabled.

Controller seats and CLI agents are agents: they mount a rideable cart through the
shared use path, follow its seat, and the descend/sneak input steps them out
(`agent::tests::controller_and_cli_agents_ride_carts_and_sneak_to_dismount`).
Controller menus open chest and hopper carts as `Tab::Vehicle`. Real-controller
play on a cart has not been checked on hardware.

Research used the Java implementations
[RailState](https://raw.githubusercontent.com/mahtomedi/minecraft/main/src/main/java/net/minecraft/world/level/block/RailState.java),
[PoweredRailBlock](https://raw.githubusercontent.com/mahtomedi/minecraft/main/src/main/java/net/minecraft/world/level/block/PoweredRailBlock.java),
[DetectorRailBlock](https://raw.githubusercontent.com/mahtomedi/minecraft/main/src/main/java/net/minecraft/world/level/block/DetectorRailBlock.java),
[AbstractMinecart](https://raw.githubusercontent.com/mahtomedi/minecraft/main/src/main/java/net/minecraft/world/entity/vehicle/AbstractMinecart.java),
[AbstractMinecartContainer](https://raw.githubusercontent.com/mahtomedi/minecraft/main/src/main/java/net/minecraft/world/entity/vehicle/AbstractMinecartContainer.java),
[MinecartHopper](https://raw.githubusercontent.com/mahtomedi/minecraft/main/src/main/java/net/minecraft/world/entity/vehicle/MinecartHopper.java)
and [MinecartTNT](https://raw.githubusercontent.com/mahtomedi/minecraft/main/src/main/java/net/minecraft/world/entity/vehicle/MinecartTNT.java).
This mirror is not pinned to Java 1.21; stable legacy rules were used and the
gaps above prevent a claim of complete Java parity. v0.17 Track D1 read the
pinned 26.3 server and client jars instead (`MinecartFurnace`,
`AbstractMinecart`, `OldMinecartBehavior`, `AbstractMinecartContainer`,
`MinecartTNT`, `VehicleEntity`, `MinecartSoundInstance`,
`RidingEntitySoundInstance`, `LocalPlayer.startRiding`), with the constants in
`tests/fixtures/java/26.3/minecarts.json`
([record](verification/2026-11-23-v0.17-d1-carts.md)).


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

A played note releases Java's `NoteParticle` 1.2 blocks above the block, tinted by
`note / 24` (three sines 120 degrees apart); a hook line plays Java's
`TripWireHookBlock.playSound` events at volume 0.4 (click on 0.6, click off 0.5,
attach 0.7, detach about 1.2) through the existing procedural click. The particle
class and tripwire sounds are client resources absent from the server jar, so
the note colour and lifetime follow the documented client behaviour.

A mob head above the note block sets its instrument (since v0.16): the note
plays the mob's sound at pitch 1 with no particle, as `NoteBlock` does for
`NoteBlockInstrument`'s mob-head entries; a player head without a
`note_block_sound` plays nothing. Known gaps: procedural instruments are
original approximations of the Java timbres. Substrate mapping covers the
sixteen canonical blocks and common existing wood/stone forms, but is not a
complete Java block-tag table. Java 26.3's tripwire models
(`tripwire_ns` and `tripwire_attached_ns` in the client jar) put the string at
the same height, 1.5/16, whether attached or not, and only use a different
strip of the texture; the game draws one texture for both.

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


## Component inventory (v0.17 Track E2)

[`redstone_components.json`](../tests/fixtures/java/26.3/redstone_components.json)
lists the 237 Java 26.3 blocks (75 classes) that emit power, are read by
comparators or react to power, extracted from the jar by
[`tools/extract_java_redstone_component_fixtures.py`](../tools/extract_java_redstone_component_fixtures.py).
[`world::redstone_catalogue`](../src/world/redstone_catalogue.rs) gives the
game's role for every block state from the same predicates the engine
dispatches on, and [`tests/redstone_components.rs`](../tests/redstone_components.rs)
compares the two both ways with no recorded exceptions: every Java
component resolves to a game block with the same source, strong, analog and
reads-power columns (or is a listed registry gap: the command, structure
and test blocks, and the crafter that v0.17 Track C1 adds), and every game
block state with a role is a Java component.

Comparators read these state-only outputs (`getAnalogOutputSignal`): cake
`(7 - bites) * 2`, candle cakes 14, a cauldron's level (lava 3, empty 0),
a respawn anchor `floor(charges / 4 * 15)` and an end portal frame 15 with
an eye. A decorated pot reads as a one-slot container and wakes adjacent
comparators when an item goes in. Beehives and bee nests give their honey
level, the composter its level, the copper golem statue its pose plus one,
the copper bulb 15 when lit, the jukebox its disc's output, the lectern its
page fraction, the creaking heart its state, shelves their filled-slot bits
from behind, chiseled bookshelves their last used slot plus one, and item
frames their rotation. Ender chests have no comparator output.

Scenarios cover every button's press length (from the registration's
constructor argument: 30 ticks for wood, 20 for stone and polished
blackstone), every door, trapdoor and fence gate following power, levers,
the redstone block's weak-only output, the lamp's 4-tick off delay, copper
bulbs toggling on rising edges with no delay (Java 26.3 `CopperBulbBlock`
schedules no tick), lightning rods' 8-tick pulse, targets, trapped chests,
jukeboxes, note blocks under mob heads, TNT, iron doors, redstone ore and
observers seeing fire age.

## Game events and sculk sensors (v0.17)

Sculk sensors and calibrated sculk sensors hear Java 26.3's game events
(`src/world/vibration.rs`, `src/world/sculk.rs`). The game's event table
(registry order, notification radii, `VIBRATION_FREQUENCY_FOR_EVENT` and the
`vibrations`, `warden_can_listen`, `shrieker_can_listen`,
`ignore_vibrations_sneaking` and `allay_can_listen` tags) is compared both
ways with `tests/fixtures/java/26.3/game_events.json`, which
`tools/extract_java_game_event_fixtures.py` reads from the pinned server jar,
together with the listener radii and timings and every Java method that posts
a game event.

Redstone posts what Java posts, at the block's centre:

| Source | Event | Java |
| --- | --- | --- |
| Lever use, wind charge | `block_activate` / `block_deactivate`, from the user | `LeverBlock.pull` |
| Button press (use, arrow, wind charge) | `block_activate`, from the user or arrow | `ButtonBlock.press` / `checkPressed` |
| Button release | `block_deactivate`, no source | `ButtonBlock.checkPressed` |
| Plate pressed / released | `block_activate` from the first entity found / `block_deactivate` with no source | `BasePressurePlateBlock.checkPressed` |
| Tripwire hook | `block_activate`, `block_deactivate`, `block_attach` or `block_detach` at both hooks, far hook first | `TripWireHookBlock.emitState` |
| Shears cutting string | `shear`, then `block_destroy` | `TripWireBlock.playerWillDestroy` |
| Door, gate or trapdoor opened by power | `block_open` / `block_close`, no source | `neighborChanged` |
| Trapdoor use | `block_open` / `block_close`, from the user | `TrapDoorBlock.playSound` |
| Daylight detector use | `block_change` naming its new state | `DaylightDetectorBlock.useWithoutItem` |
| Repeater or comparator use | nothing | — |
| Note block | `note_block_play`, from the player who tuned or struck it | `NoteBlock.playNote` |
| Bell | `block_change`, from whoever rang it | `BellBlock.attemptToRing` |
| TNT primed by power or fire | `prime_fuse`, no source | `TntBlock.prime` |
| Piston extends / retracts | `block_activate` naming the extended base / `block_deactivate` naming the moving piston | `PistonBaseBlock.triggerEvent` |
| Block a piston destroys | `block_destroy` naming it (a carpet dampens it) | `PistonBaseBlock.moveBlocks` |
| Empty dispenser | `block_activate` (droppers post nothing) | `DispenserBlock.dispenseFrom` |
| Dispensed bucket | `fluid_place` (and `entity_place` for a mob bucket) or `fluid_pickup` | `BucketItem`, `MobBucketItem` |
| Dispensed TNT | `entity_place` (no `prime_fuse`) | `DispenseItemBehavior` |
| Dispensed projectile | `projectile_shoot`, no owner | `Projectile.tick` |

The dispenser posts live in Track C2's `dispense` and the piston posts in
`piston_trigger_event` and `move_piston_blocks` (`world::piston::events`). Sensor timing is unchanged and matches Java: a candidate
is chosen a tick after it is heard (nearest, then highest frequency),
travels `floor(distance)` ticks, activates the sensor for 30 ticks (the
calibrated sensor 10) with power `max(1, 15 - floor(15 / radius * distance))`,
then cools down for 10. The calibrated sensor hears only the frequency its
back input signals, and doesn't power that side. Wool between event and
sensor occludes it when all six nudged rays meet wool; wool and carpet as the
event's block dampen it; sneaking hides the `ignore_vibrations_sneaking`
events. Details, the player and entity events, and the gaps:
[v0.17 sculk game events](verification/2026-11-23-v0.17-sculk-game-events.md).
