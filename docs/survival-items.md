# Everyday survival items

Append-only item IDs begin at 512; block states begin at 520.

Shears use Java's 238 durability, 15x leaves/cobweb speed and 5x wool speed.
Shearing a living sheep drops 1–3 wool and removes its fleece; a sheared
sheep cannot be sheared again and no longer drops wool on death.
Sheep drop wool in their current colour (see [colors](colors.md)). Grass-eating fleece regrowth is deferred.
Mobs currently do not persist across saves (the existing engine boundary).

Recipes are transcribed from the vanilla 1.21 JSON fixtures in
`docs/vanilla-recipes/`, fetched from
https://github.com/InventivetalentDev/minecraft-assets/tree/1.21/data/minecraft/recipe.
Java references:
- https://raw.githubusercontent.com/mahtomedi/minecraft/main/src/main/java/net/minecraft/world/item/ShearsItem.java
- https://raw.githubusercontent.com/mahtomedi/minecraft/main/src/main/java/net/minecraft/world/entity/animal/Sheep.java

Milk can be collected from cows with a bucket, including stacked empty
buckets. Creative keeps the bucket and gains one milk bucket. Drinking takes
32 ticks, works at full hunger, clears every effect, and returns the bucket. Mushroom stew restores 6 hunger/7.2 saturation and
returns its bowl. Beetroot and rabbit stews are deferred because their
ingredients are absent.

Brown/red mushrooms generate in caves, covered ground and swamps via an
isolated decoration pass. Random ticks spread with Java's 1/25 chance,
five-mushroom density limit and four-step random walk. They require a solid
surface and brightness below 13. Sky brightness uses the engine's vertical
exposure approximation, so outdoor shade is conservative and exposed swamp
mushrooms disappear. Huge mushrooms/mycelium/podzol are deferred.
- https://raw.githubusercontent.com/mahtomedi/minecraft/main/src/main/java/net/minecraft/world/item/MilkBucketItem.java
- https://raw.githubusercontent.com/mahtomedi/minecraft/main/src/main/java/net/minecraft/world/level/block/MushroomBlock.java

Snowballs stack to 16. The full snow block stands in for snow layers and drops
four snowballs; silk touch keeps the block, and four snowballs craft it back.
Thrown snowballs and eggs use Java's 1.5 blocks/tick, 0.03 gravity and a small
inaccuracy. They knock mobs back. A snowball deals 3 damage only to blazes.
Eggs hatch with Java's 1/8 chance, and 1/32 of those hatch four chicks. Chicks
are half-size babies that grow up after 24000 ticks. Grown chickens lay one egg
every 6000–12000 ticks. Host, pad and CLI players throw through the shared use path.
- https://raw.githubusercontent.com/mahtomedi/minecraft/main/src/main/java/net/minecraft/world/entity/projectile/Snowball.java
- https://raw.githubusercontent.com/mahtomedi/minecraft/main/src/main/java/net/minecraft/world/entity/projectile/ThrownEgg.java
- https://raw.githubusercontent.com/mahtomedi/minecraft/main/src/main/java/net/minecraft/world/entity/animal/Chicken.java

A compass points at world spawn: 32 icon frames, needle up when the holder
looks toward it, and a random wobble in the Nether and End. A clock uses 64
frames with noon as frame 0, matching Java's celestial angle, and wobbles
outside the Overworld. The wobble follows the Java client's `Wobbler`
(each game tick: velocity = (velocity + shortest delta to a fresh random target
* 0.1) * 0.8); the client classes are not in the server reference jar, so this
is written from the documented client behaviour and is unchecked against it. Both animate in the
inventory, the first-person hand and the third-person grip. An agent holding
one gets `compass` or `clock` on its observation.
- https://raw.githubusercontent.com/mahtomedi/minecraft/main/src/main/java/net/minecraft/world/item/CompassItem.java
- https://raw.githubusercontent.com/mahtomedi/minecraft/main/src/main/java/net/minecraft/world/item/ClockItem.java

Carrots and potatoes grow through Java's eight stages on farmland and drop
one crop when young, or one plus up to three (and fortune) bonus rolls of
4/7 when ripe. Ripe potatoes also drop a poisonous potato 2% of the time.
Zombies drop a carrot or a potato at 2.5% plus 1% per looting level.
Potatoes smelt into baked potatoes. A poisonous potato restores 2 hunger and
poisons for 5 seconds 60% of the time. Cake is a block of seven bites
(2 hunger and 0.4 saturation each) crafted from milk, sugar, egg and wheat;
the milk buckets come back. Pumpkin pie is pumpkin, sugar and egg.
- https://raw.githubusercontent.com/mahtomedi/minecraft/main/src/main/java/net/minecraft/world/level/block/CropBlock.java
- https://raw.githubusercontent.com/mahtomedi/minecraft/main/src/main/java/net/minecraft/world/level/block/CakeBlock.java

A fishing rod (durability 64, enchantability 1) casts one bobber and reels it
on the next use. The bite wait is Java's 100–600 ticks, minus 100 per Lure
level; rain can count a tick twice and a roof can skip one. Reeling during
the nibble rolls Java 26.3's `gameplay/fishing` table: junk (weight 10,
quality -2), treasure (5, quality 2) and fish (85, quality -1), with Luck of
the Sea adding quality times its level. Only treasure requires open water, so
closed water (failing the 5×4×5 check) still yields fish and junk. Fish are
cod 60, salmon 25, tropical fish 2 and pufferfish 13. Junk is the full
reference table, including lily pads, ten ink sacs, tripwire hooks, damaged
leather boots and rods, and bamboo when the hook is in a jungle biome as the line is reeled.
Treasure bows and rods are worn up to 25% and, with books, enchanted at 30
levels from `#on_random_loot` (curses, frost walker and mending included).
Cooked cod and salmon smelt from the raw fish. The hook bobs on Java's per-tick
spring and snags the first mob it touches (box widened by 0.3); it then rides the
mob at 80% of its height and reeling pulls the mob a tenth of the way to the
player per tick and costs five uses (items on the ground are not hookable yet).
The rows are checked against `tests/fixtures/java/26.3/fishing.json`.
- https://raw.githubusercontent.com/mahtomedi/minecraft/main/src/main/java/net/minecraft/world/entity/projectile/FishingHook.java
- Java 26.3 server `data/minecraft/loot_table/gameplay/fishing*.json`
