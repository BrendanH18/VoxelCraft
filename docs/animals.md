# Animals (Java 1.21)

Right-click two adults with their food to breed them. The same `place` action
works for CLI agents and gamepad players. Cows and sheep eat wheat, pigs eat
carrots/potatoes/beetroots, chickens eat wheat/pumpkin seeds, cats eat raw
cod/salmon, hoglins eat crimson fungus, striders eat warped fungus, turtles eat
seagrass, and axolotls eat buckets of tropical fish (returning a water bucket).

Love mode lasts 30 seconds. Partners look for each other within eight blocks,
court for three seconds, and produce a baby and 1–7 XP. Adults wait five minutes
before breeding again; babies grow for twenty minutes. Feeding a baby removes
10% of its remaining growth time, rounded to whole seconds. Sheep inherit a
craftable mix of their parents' colours, or one parent's colour. Axolotl babies
inherit a parent's variant with the rare 1/1200 blue mutation.

Farm animals and offspring persist when unloaded. Age, courtship, food cooldown,
sheep colour/shearing and chicken egg timer survive saving. Chickens lay every
5–10 minutes; thrown eggs hatch with probability 1/8, producing four chicks on a
further 1/32 roll. Baby animals drop neither ordinary loot nor experience.

## Extending the shared breeding API

Add the species to `MobKind::is_breedable` and its food predicate to
`MobKind::breeding_food`. `Mob::ready_to_breed`, `Entities::use_animal` and
`Entities::animal_child` supply age, feeding, cooldown, courtship and offspring.
Mounts can apply additional attribute inheritance in `animal_child` after the
shared baby setup. `animals::State` is optional, preserving existing mob saves.

Sources: [Breeding](https://minecraft.wiki/w/Breeding),
[Chicken](https://minecraft.wiki/w/Chicken),
[Sheep](https://minecraft.wiki/w/Sheep),
[Turtle](https://minecraft.wiki/w/Turtle),
[Axolotl](https://minecraft.wiki/w/Axolotl).
