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
