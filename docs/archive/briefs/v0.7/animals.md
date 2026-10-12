> Historical record, superseded on 2026-10-12. Old task instructions, priorities,
> reference versions and completion claims are not current guidance. See
> [the archive index](../../README.md) and [the current roadmap](../../../roadmap.md).

# v0.7 task: Breeding, taming, leads and name tags
Worktree: <assigned-worktree>  Branch: agent/animals
IDs: items 1000..=1039 (new append-only range; add a range const + assert like OVERWORLD_ITEM in src/item.rs), blocks 2150..=2199, texture layers 1750..=1799. MobKind: append Wolf, Fox, Parrot, Rabbit, Goat (and others you add) at the end.
Another agent is concurrently adding boats, saddles and horses/donkeys/mules/llamas/camels with horse taming (ranges 960..999). Don't implement those mounts; make your breeding API generic (e.g. MobKind -> breeding food predicate + baby spawn) so their horses plug in easily, and say how in your summary.

Read entity/mob.rs, entity/villager_breeding.rs (villagers already breed), entity/aquatic.rs (turtles: can't lay eggs yet), app/mobs.rs, entity/model*, render/entity.rs.

Deliver (match Java 1.21):
1. Love mode and breeding for all existing farm/passive animals: cow (wheat), sheep (wheat), pig (carrot/potato/beetroot), chicken (seeds), cat (raw cod/salmon), hoglin (crimson fungus), strider (warped fungus), turtle (seagrass -> lays eggs on its home beach; turtle egg block with hatching, trampling; babies imprint home), axolotl (bucket of tropical fish), plus your new mobs. Hearts particles, 5-minute adult cooldown, babies (20 min growth, feeding speeds it up 10%), 1-7 XP orb, follow-food-item tempt AI, mooshrooms if trivial. Sheep: graze grass to regrow wool, coloured-sheep inheritance. Chickens lay eggs every 5-10 min; thrown egg 1/8 chance chick.
2. Taming: wolves (bones; sit/follow/teleport to owner, attack owner's target, collar dyeing, health via tail angle, beg), cats (already exist: make tameable with raw fish, sit, gifts at morning, creeper/phantom scare), parrots (seeds; shoulder perching, dance near jukebox if any, imitate hostile sounds, cookies poison). Foxes (sweet berries, sleep by day, pick up items, trust from breeding), rabbits (carrots/dandelion; rabbit hide/foot drops if items missing), goats (wheat; ramming, horn drop if feasible). Spawning in correct biomes. Owners stored by stable player/profile ID.
3. Leads: item + recipe (4 string + 1 slimeball), attaching to mobs, 10-block leash physics, snapping beyond, fence knots on fence posts, rendering the rope, breaking knot drops lead. Name tags: item (loot only + anvil renaming already exists? integrate), naming prevents despawn, nameplate rendering, "Dinnerbone"/"jeb_" optional.
4. Everything saves/loads (age, love cooldown, owner, sitting, collar, lead attachment, name). Agents/pad players can feed, tame, leash and name.
