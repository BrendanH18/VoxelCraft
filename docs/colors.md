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
