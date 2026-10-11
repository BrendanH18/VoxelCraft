# Animals (Java 1.21)

Right-click two adults with their food to breed them. The same `place` action
works for CLI agents and gamepad players. Cows and sheep eat wheat, pigs eat
carrots/potatoes/beetroots, chickens eat wheat/pumpkin/melon/beetroot seeds, foxes eat sweet/glow berries, rabbits eat carrots/golden carrots/dandelions, goats eat wheat, cats eat raw
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

## Companions and life cycles

Bones tame wolves, raw fish tame cats, and seeds tame parrots. Each bone/fish
has a 1/3 success chance, each seed 1/10. Owners use a non-food item or empty
hand to toggle sitting. Standing companions follow their owner and teleport
when separated by more than twelve blocks. Dye changes wolf/cat collars;
meat heals wolves before putting a healthy adult into love mode. Wolves defend
their owner and attack the owner's target (except creepers/ghasts). Tail angle
shows health; tamed wolves have forty health. Nearby cats scare creepers and
may bring a gift after their owner sleeps through the night.

Parrots imitate nearby hostile calls and perch on an owner's free shoulder;
jumping or entering water releases them. Cookies kill parrots. Foxes sleep
by day, pick up ground items and eat carried food; bred foxes trust the two
players who fed their parents. Rabbits hop and drop rabbit meat/hide, with a
10% rabbit-foot chance on a player kill. Goats jump, charge stationary players
and lose one of their two horns when a ram hits stone, logs, packed ice, or
iron/copper/emerald ore. Screaming goats have shorter ram cooldowns.

Sheep graze grass for two seconds, restoring wool and accelerating baby growth
by sixty seconds. `mobGriefing=false` preserves the grass while still restoring
wool. Pregnant turtles return to their imprinted beach, dig for ten seconds
and lay one to four eggs. Eggs progress through two cracked stages before
hatching; sand is required. Progress is guaranteed on a random tick between
21600 and 22560 day ticks, and otherwise has a 1/500 chance. Babies imprint the
nest and drop a scute on growing up. Egg clusters have individual shell boxes.

Sources: [Wolf](https://minecraft.wiki/w/Wolf), [Cat](https://minecraft.wiki/w/Cat),
[Parrot](https://minecraft.wiki/w/Parrot), [Fox](https://minecraft.wiki/w/Fox),
[Rabbit](https://minecraft.wiki/w/Rabbit), [Goat](https://minecraft.wiki/w/Goat).

## Leads and name tags

Craft two leads from four string and a slimeball. Use a lead on a passive
or neutral animal (farm animals, companions, golems, hoglins, striders,
dolphins, axolotls) to hold it; using the animal again takes the lead back.
Leashed animals follow when the rope passes six blocks and the lead snaps,
dropping itself, past ten. Use a fence while holding animals to tie every
lead within seven blocks to a knot; hitting or using the knot releases
them. Leashes and knots are saved with the animal.

Name an anvil-renamed name tag onto any mob to give it a nameplate and
stop it despawning. Name tags come from dungeon chests and fishing
treasure. Sources: [Lead](https://minecraft.wiki/w/Lead),
[Name Tag](https://minecraft.wiki/w/Name_Tag).
