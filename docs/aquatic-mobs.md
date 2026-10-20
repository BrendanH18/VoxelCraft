# Aquatic mobs (v0.6)

Cod, salmon, tropical fish, pufferfish, squid, glow squid, dolphins,
axolotls, guardians and elder guardians have animated original models,
procedural calls, collision, drops and shared simulation behavior. Their
names work with `/summon`, CLI spawning and screenshot `--spawn` options.

## Water and spawning

Water creatures move with Java 26.3's own move controls and water travel
(see the [movement evidence](verification/2026-10-19-swimmer-flyer-movement.md)).
Cod, salmon and tropical fish form schools of up to 8 (salmon 5) that follow
a leader; fish swim away from survival players within 8 blocks. Ordinary ocean creatures spawn in
water between Y=50 and sea level in their biome families; warm seas have
tropical fish and pufferfish, cold seas and rivers have salmon. Dolphins
and squid avoid frozen seas. Glow squid require darkness, Y below 30 and
stone/deepslate within five blocks below. Axolotls require lush cave water
with clay within five blocks below; lush caves also support tropical fish.
Guardians spawn only within monument bounds. Spawn attempts honor loaded
cells, player distance and local caps. Peaceful retains passive spawning.

Fish and squid suffocate after 15 seconds out of water. Axolotls dry out
after five minutes, while rain resets their timer. Dolphins breathe air,
seek the surface below 7 seconds of air, and dry out on land. Fish,
dolphins and guardians flop when stranded; squid fall. Guardians survive
out of water.

## Combat and effects

Guardian beams require uninterrupted sight of the same survival player:
normal guardians charge for four seconds, elders for three, after an
initial half-second windup. Damage includes separate magic and physical
components, and scales with difficulty. Stationary guardians retaliate
against melee attacks with two points of spike damage. Beams render while
charging. Elders apply five minutes of Mining Fatigue III within 50 blocks
every minute, including through walls. Fatigue affects keyboard, controller
and CLI mining; milk clears it through the existing effect system.

Pufferfish inflate near survival players, sting on contact and poison.
Nearby dolphins give swimming players Dolphin’s Grace, which affects only
water movement. Axolotls hunt fish, squid, guardians and drowned; injured
axolotls can play dead and regenerate. Guardians drop prismarine shards
and a weighted cod/crystal/empty pool. Elder player kills additionally drop
one wet sponge, unaffected by Looting. Burning cod and salmon cook.

## Buckets and saves

Use a water bucket on a fish or axolotl, then use its filled bucket to
release it. Keyboard, controllers and CLI `place` share the same action.
Capture respects block obstruction and reach. Filled buckets save the
creature’s appearance variant and health as an optional stack component;
old inventory strings still load. Released creatures persist across
unloading and save/load. Dispensers release the same payload and return an
empty bucket. In the Nether the water evaporates but the creature releases.

Tropical fish have twelve color variants; axolotls have their four ordinary
natural colors and a blue variant available through saved bucket data.
These original models approximate Java appearances. Exact tropical fish
pattern encoding and squid ink particles remain later work. Turtle helmets and
Turtle Master brewing are implemented on the assessment branch; see the
[turtle equipment evidence](verification/2026-10-12-turtle-equipment.md) for
tested mechanics and remaining player/performance acceptance.
The retained v0.7 implementation adds turtle breeding, nesting, egg hatching and loaded/near-player growth gating, plus axolotl breeding. See [animals](animals.md) and the assessment reports for verified scope and remaining differences.
Spawning follows Java's water spawn categories (see the
[spawn categories evidence](verification/2026-10-15-spawn-categories.md)).
Movement differences that remain: dolphins don't play with items, seek
treasure, follow boats or swim alongside players; turtles lack their travel
and go-to-water goals; guardians aren't held near their monument; and water
currents don't push swimmers. The
[movement evidence](verification/2026-10-19-swimmer-flyer-movement.md)
lists every remaining difference.

## References

- [Java 1.21.5 biome spawn data](https://github.com/misode/mcmeta/tree/1.21.5-data/data/minecraft/worldgen/biome).
- [Guardian loot data](https://raw.githubusercontent.com/misode/mcmeta/1.21.5-data/data/minecraft/loot_table/entities/guardian.json).
- [Elder guardian loot data](https://raw.githubusercontent.com/misode/mcmeta/1.21.5-data/data/minecraft/loot_table/entities/elder_guardian.json).
- [Mojang’s elder guardian overview](https://www.minecraft.net/en-us/article/meet-elder-guardian).
