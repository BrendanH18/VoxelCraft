# Boats and riding

Craft a boat with five matching planks in a U (no shovel). Oak, spruce,
birch, jungle, acacia, dark oak, mangrove, cherry and pale oak boats are
available; bamboo makes a raft. Craft a chest variant with a boat and chest.

Use the item against water or an empty space over a surface. Use the boat
to sit; movement keys paddle and sneak dismounts. Ordinary boats have two
seats and pick up nearby small mobs. Chest boats have one seat and 27 storage
slots; sneak-use opens storage, as does use while riding. The keyboard
storage screen supports normal click and shift-click transfers. Boats share
the existing saved vehicle/rider plumbing with minecarts, including CLI and
controller riders. Hulls, wood colours and animated paddles are procedural.

Since v1.0 Track 7 boats run Java 26.3's `AbstractBoat` tick by tick
([record](verification/2026-11-25-v1.0-track-07-hostiles-mounts.md)): the
forward key paddles, left and right turn the boat (and the rider's view
with it), water friction 0.9 gives a top speed of 8 m/s, ice 2 blocks a
tick and blue ice about 3.6; currents carry boats, a bubble column throws a
boat up or drags it down after three seconds on it, and sixty ticks under
water eject the riders. A hit adds ten times its damage, breaking past 40
into the boat's own item. Java 26.3 boats and rafts survive
hard falls: `AbstractBoat.checkFallDamage` resets the accumulated distance on
landing without a damage/destroy/drop call. Ordinary and chest variants now
retain riders and cargo through hard landings; landing/save tests cover both
boat and raft forms.

Reference rules: [boats](https://minecraft.wiki/w/Boat),
[chest boats](https://minecraft.wiki/w/Boat_with_Chest),
[horses](https://minecraft.wiki/w/Horse), [striders](https://minecraft.wiki/w/Strider).

## Animal mounts

Use a saddle on an adult pig or strider, then use it to sit. Hold a carrot
on a stick for pigs or warped fungus on a stick for striders; use starts a
smooth boost lasting 7–49 seconds. Boosts use seven durability on a carrot
stick and one on a fungus stick. Striders retain their lava walking and
shivering slowdown. Sneak dismounts onto a nearby clear position away from lava.

Horses spawn in plains/savannas, donkeys also in meadows; mules can be
summoned. Use an empty hand to mount an untamed adult. Failed taming
attempts add five temper, making later attempts more likely to succeed.
Hearts indicate success. Wheat, sugar and apples heal and raise temper;
golden carrots and apples are stronger. Hay heals twenty health points.
Feeding foals speeds their twenty-minute growth.

A tamed horse needs a saddle for steering. Use horse armor to equip it;
leather armor crafts from seven leather, while iron/gold/diamond armor and
saddles come from structure loot. Saddles also craft from three leather and an
iron ingot, and can be fished up. Use a chest
on a tamed donkey or mule to add fifteen storage slots. Sneak-use or open
your inventory while riding to manage equipment/storage. The first slot is
a saddle; only horses accept armor in the second slot. Donkey/mule storage
starts in the third slot. Keyboard and controller menus enforce these rules;
CLI `chest take/put <slot>` accesses targeted or ridden vehicle storage.

Horse coats have seven colours and five marking patterns. Each has Java's
random health (15–30), speed (0.1125–0.3375 attribute), and jump (0.4–1.0)
distributions. Donkeys/mules use 0.175 speed and 0.5 jump. Hold jump to charge
and release to jump; the blue charge bar replaces XP and mount hearts replace
hunger while riding. Ridden horses, donkeys, mules and undead horses move by Java's
`travelRidden` on land (v1.0 Track 7): step height 1, the jump strength
attribute as the launch speed, a 0.4 forward leap on a running jump. A fall
hurts a horse `floor((fall − 6) × 0.5)`; only then does the rider take
their own fall damage for the same distance. Since v1.0 gap Track 3 the
same Java tick carries them through water and lava
([record](verification/2026-11-27-v1.0-gap-track-03-mounts-water.md)):
`travelInWater` pushes 0.02 a tick against a 0.8 drag (a top speed of
0.08 blocks a tick, 1.6 a second), the mount's float goal and
`#can_float_while_ridden` lift it to the surface, currents carry it, and
it climbs out where the bank is free. A skeleton horse has no float goal:
it walks the bottom with its 0.96 water drag, much faster than other
horses. The rider's jump charges only for a vehicle that can jump, and
starts at nothing on the press tick as Java's client does. Saddles, armor,
stats, temper, owner, riders and storage survive saves; equipment and cargo
spill when the animal dies.

Breeding uses the shared animal pipeline (`docs/animals.md`): tamed adult
horses and donkeys breed on golden carrots and apples, a horse and a donkey
make a mule, and mules are sterile. Foals inherit health, speed and jump with
Java's offspring formula.

## Llamas and camels

Llamas spawn in windswept hills and savanna plateaus with one of four coats
and a strength of 1–3 (1–5 one time in 25). Tame one like a horse by
mounting it with an empty hand; llamas tame within 30 temper. Wheat heals 2
and adds 3 temper, hay bales heal 10, add 6 and breed tamed adults. A chest
gives three slots per strength point and a carpet decorates its back.
Riders can't steer llamas. A llama you hit spits back once: Java's spit
projectile flies 1.5 blocks a tick and does 1 damage to whatever it hits.
Llamas also spit at wild wolves, which keep their distance. Lead a llama and
others within nine blocks fall in behind it as a caravan of up to ten, each
following the one ahead until it is ridden, led itself, or left 16 blocks
behind. Foals take a random strength up to the
stronger parent's (rarely one more) and a parent's coat.

Camels live in desert villages and need no taming: saddle one to steer it.
Two players can ride; the rear rider moves forward when the front one gets
off. Hold and release jump to dash forward (2.75 s cooldown; sprinting adds
speed while the dash is ready). Camels step up 1.5 blocks, sit down now and
then when idle, take 2.6 s to stand after a rider presses forward, eat and breed on cactus.

Since v1.0 gap Track 3 camels and camel husks ride on Java 26.3's own
`Camel` code ([record](verification/2026-11-27-v1.0-gap-track-03-mounts-water.md)):

- **Pose.** A camel keeps Java's pose clock (`LAST_POSE_CHANGE_TICK`):
  sitting down takes 40 ticks, standing up 52, and while it sits or changes
  pose it neither moves, turns for its rider nor dashes (`refuseToMove`).
  Sitting shrinks its box to 1.7 × 0.945 (a calf's to 0.95 × 0.425; standing
  calves are 0.95 × 1.4). Its brain's idle choice (`RandomStroll`,
  `RandomSitting(20)` or `DoNothing(30, 60)`, picked evenly among those
  that can start) sits it down after 20 seconds standing and up after 20
  sitting, only on the ground, out of water, unleashed and with no rider
  steering. Water or a hit stands it up at once; wanting to walk somewhere
  stands it up through the transition. The pose and its clock save; older
  saves load seated (if they were sitting) or partway up.
- **Riding.** Java has no right-click sit command (`Camel.mobInteract`):
  mounting a seated camel leaves it seated, and a saddled camel's rider
  stands it with forward input (`Camel.tickRidden`). Ridden, it walks at
  0.09 (Java's `travelRidden`, 2.2 blocks a second on stone), 0.1 faster
  while the rider sprints with the dash ready, steps up 1.5 blocks and
  swims and floats as horses do.
- **Dash.** Hold and release jump: the charge (`LocalPlayer`'s
  `jumpRidingScale`) grows only while the dash is ready and the camel is
  standing, and the released jump, from the ground, throws the camel
  `22.2222 × scale × 0.09` blocks a tick along its heading and
  `1.4285 × scale × 0.42` up, then waits 55 ticks.
- **Seats.** Java's `getPassengerAttachmentPoint`: a lone rider sits half a
  block forward of the middle; with two, the driver half a block forward
  and the other 0.7 back (an animal 0.2 further forward). The seat height
  follows the body through both transitions, and the model's body sinks
  and rises with it.

## Nautiluses

Tame a nautilus with a pufferfish (one in three), saddle it to steer it
under water, and dash with the jump key. Since v1.0 gap Track 3 a tame
adult nautilus (or zombie nautilus) has Java's mount inventory screen
(`NautilusInventoryMenu`): sneak-use it, or open the inventory while riding
it, to manage its saddle and nautilus armor slots. It opens on every input
path the horse screens do (keyboard, controller, LAN and the CLI's `chest
take/put`), and not while someone else rides it. Shift-clicking an item no
mount slot takes moves it between the hotbar and the main inventory, as
Java's `AbstractMountInventoryMenu` does for horses too. See the
[record](verification/2026-11-27-v1.0-gap-track-03-mounts-water.md).

## Undead horses (v0.15, implemented/unverified)

Skeleton horses come from thunderstorm traps: approach within ten blocks to
trigger four tamed horses with skeleton riders. Once their riders are gone,
use a saddle and ride through the ordinary horse controls. Summoned untamed
skeleton horses cannot be tamed by mounting. Zombie horses spawn in their
Java 26.3 biome monster lists with zombie riders, tame like horses and eat
red mushrooms. Both accept horse armor, never breed, and keep their saddles,
attributes, riders and trap timers through saves. Skeleton horses drop bones;
zombie horses drop rotten flesh. Original skeleton and green horse models use
the shared horse rig. See [evidence and differences](verification/2026-11-04-v0.15-lightning.md),
Natural zombie horsemen carry an iron spear (v1.0 Track 7), though no mob
uses a spear's charge yet; trap riders' bows and helmets are enchanted.

Current parity gaps:
horse coats/armor and sounds are procedural approximations;
camels' sit, stand and dash sounds aren't played; horse herds now share the first
member's coat while keeping independent markings, as Java 26.3
`Horse.finalizeSpawn`/`HorseGroupData` do. Saved coat/marking IDs are unchanged. Boat currents, bubble columns and
Java's boat statuses are followed since v1.0 Track 7; boat leashes remain future work. Offhand controls
await a player offhand system. Horse armor dyeing and Java's dedicated horse
inventory layout are not yet implemented.
