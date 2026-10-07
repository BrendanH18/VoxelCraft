# Everyday survival items

Append-only item IDs begin at 512; block states begin at 520.

Shears use Java's 238 durability, 15x leaves/cobweb speed and 5x wool speed.
Shearing a living sheep drops 1–3 wool and removes its fleece; a sheared
sheep cannot be sheared again and no longer drops wool on death.
All sheep currently have white wool. Grass-eating fleece regrowth is deferred.
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
