# Java colours

The 16 dye items use Java DyeColor IDs and textureDiffuseColor RGB values.
Recipes follow vanilla 1.21 JSON, including all alternate mixing recipes.
Existing sources: poppy → red, dandelion → yellow, blue orchid → light blue,
lapis lazuli → blue, bone meal → white, smelted cactus → green (1 XP).

Brown and black dyes currently require creative inventory or `--give`:
there are no jungle cocoa pods/beans, squid/ink sacs or wither roses.
Gray and light gray mixing consequently also need creative black dye.
Other flowers, beetroot, sea pickles and pitcher plants are absent; their
recipes are deferred until those ingredients exist. No substitute sources.

Sources:
- https://raw.githubusercontent.com/mahtomedi/minecraft/main/src/main/java/net/minecraft/world/item/DyeColor.java
- https://github.com/InventivetalentDev/minecraft-assets/tree/1.21/data/minecraft/recipe

Wool and carpets have all 16 colours. Beds use matching wool in their recipe;
`bed` remains the saved/default red bed, with `red_bed` as an alias.
Sheep use Java's natural distribution, can be dyed and sheared with craftable
shears, and drop their own colour of wool. Sheared sheep drop no wool.
The shared interaction works for mouse, gamepad and CLI `place` actions.
Coloured beds share placement, breaking, sleep and respawn rules.

Current engine limits: beds still use the existing unrotated two-half model
and neighbour matching (no stored facing); mobs are transient rather than
saved, and sheep grazing/wool regrowth, breeding and mutton are deferred.

- https://raw.githubusercontent.com/mahtomedi/minecraft/main/src/main/java/net/minecraft/world/entity/animal/Sheep.java
- https://raw.githubusercontent.com/mahtomedi/minecraft/main/src/main/java/net/minecraft/world/item/DyeItem.java

Stained glass is translucent and solid, so it sorts with water. Panes join like
iron bars, including to glass. Java's six badlands terracotta colours keep IDs
85-90; the other ten are new. Plain terracotta still comes from smelting clay.
Generated terracotta bands can wait for further badlands work. Stained glass
and panes drop only with silk touch. Panes use the glass textures rather than
extra layers.

Concrete powder is a falling block that turns into concrete on contact with
water, including while falling. Glazed terracotta smelts from stained
terracotta (0.1 XP) and stores Java's four horizontal facings; each colour
has its own pattern, rotated in the texture layers.

- https://raw.githubusercontent.com/mahtomedi/minecraft/main/src/main/java/net/minecraft/world/level/block/ConcretePowderBlock.java
- https://raw.githubusercontent.com/mahtomedi/minecraft/main/src/main/java/net/minecraft/world/level/block/GlazedTerracottaBlock.java
