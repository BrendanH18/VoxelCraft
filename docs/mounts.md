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

Water gives buoyancy and momentum; ice and packed ice keep more momentum,
and blue ice keeps the most. Land travel is slow. Submerged boats eject
players. Punching breaks a boat into its item; a hard fall over three blocks
breaks it into three matching planks and two sticks, spilling its contents.

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
saddles come from structure loot. Saddles can also be fished up. Use a chest
on a tamed donkey or mule to add fifteen storage slots. Sneak-use or open
your inventory while riding to manage equipment/storage. The first slot is
a saddle; only horses accept armor in the second slot. Donkey/mule storage
starts in the third slot. Keyboard and controller menus enforce these rules;
CLI `chest take/put <slot>` accesses targeted or ridden vehicle storage.

Horse coats have seven colours and five marking patterns. Each has Java's
random health (15–30), speed (0.1125–0.3375 attribute), and jump (0.4–1.0)
distributions. Donkeys/mules use 0.175 speed and 0.5 jump. Hold jump to charge
and release to jump; the blue charge bar replaces XP and mount hearts replace
hunger while riding. Equines step over one-block obstacles and share their reduced fall damage
with riders. Saddles, armor,
stats, temper, owner, riders and storage survive saves; equipment and cargo
spill when the animal dies.

Breeding integration: `entity::mounts::breeding_food` exposes horse/donkey,
pig and strider food. Mules return an empty list because they are sterile.
The separate breeding feature should check adult/tame state before love mode.

## Llamas and camels

Llamas spawn in windswept hills and savanna plateaus with one of four coats
and a strength of 1–3 (1–5 one time in 25). Tame one like a horse by
mounting it with an empty hand; llamas tame within 30 temper. Wheat heals 2
and adds 3 temper, hay bales heal 10, add 6 and breed tamed adults. A chest
gives three slots per strength point and a carpet decorates its back.
Riders can't steer llamas. A llama you hit spits at you for 1 damage every
two seconds until it calms down. Foals take a random strength up to the
stronger parent's (rarely one more) and a parent's coat.

Camels live in desert villages and need no taming: saddle one to steer it.
Two players can ride; the rear rider moves forward when the front one gets
off. Hold and release jump to dash forward (2.75 s cooldown; sprinting adds
speed while the dash is ready). Camels step up 1.5 blocks, sit down now and
then when idle, take 2.6 s to stand when mounted, eat and breed on cactus.

Current parity gaps: llama caravans, llamas attacking wolves and spit as a
dodgeable projectile, camel sitting on command, and undead horses are not implemented;
horse coats/armor and sounds are procedural approximations; horse herd coats
are independently selected. Water-current steering/bubble columns and exact
Java land-status fall quirks for boats remain future work. Offhand controls
await a player offhand system. Horse armor dyeing and Java's dedicated horse
inventory layout are not yet implemented.
