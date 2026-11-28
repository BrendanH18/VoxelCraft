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
Dolphins swim alongside the nearest sprint-swimming player within 10 blocks
and give them Dolphin’s Grace, which affects only water movement. Feed a
dolphin a fish and it leads the way to the nearest ocean ruin or shipwreck.
Dolphins fetch and toss loose items in the water, leap from the surface,
follow boats that players steer, keep away from guardians, and a hit pod
turns on its attacker. An elder guardian sets a 16-block home where it first
acts, heads back when it leaves it, and won't target players with its beam
outside it. Axolotls hunt fish, squid, guardians and drowned; injured
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
Turtles travel far through the sea, head home to lay their eggs, walk back
into the water from land, and run for water when hurt. The
[movement evidence](verification/2026-10-19-swimmer-flyer-movement.md) and
[dolphin and turtle goal evidence](verification/2026-10-19-dolphin-turtle-goals.md)
list the remaining differences.

Since v1.0 gap Track 3
([record](verification/2026-11-27-v1.0-gap-track-03-mounts-water.md)):

- **Baby dolphins.** After the first dolphin of a natural pack, one in ten
  is a calf (`AgeableMobGroupData(0.1)`); a dolphin spawn egg used on a
  dolphin also makes one. Calves have their own smaller model and a box
  0.65 of the adult's, grow up in 20 minutes, and a fish takes a tenth off
  the time left instead of sending them after treasure. A golden dandelion
  stops (or restarts) a calf's growing. Calves don't fight back when hit.
- **Guardians** beam squid, glow squid and axolotls as well as players
  (`GuardianAttackSelector`): the nearest one more than 3 blocks away
  within 16, in sight and inside an elder's home. The beam hits a mob for
  6 (an elder's for 8) after the same charge.
- **Currents.** Flowing water pushes the mobs Java pushes: guardians and
  elder guardians, drowned that aren't swimming, and every land mob in the
  water, 0.014 blocks a tick along the flow (`CurrentAccumulator`). Fish,
  tadpoles, squid, dolphins, turtles, axolotls, frogs and nautiluses aren't
  pushed in Java 26.3 (`isPushedByFluid` is false for them), and aren't
  here.

Since v1.0 gap Track 3b
([record](verification/2026-11-27-v1.0-gap-track-03b-mounts-water.md)):

- **Baby dolphins** remember the growth they were fed through (Java's
  `ForcedAge`, saved) and sparkle green for two seconds after a fish. A
  calf breathes at its own eyes, just above its feet, so it needs a little
  more of the surface than an adult; it dries out by its whole body.
- **Guardians** don't spot a target at once: with none, they look every
  other tick and then only one time in five, about half a second on
  average. A beam that reaches a squid or axolotl hits it twice, as in
  Java: the magic part with a push, then only what the guardian's attack
  adds over it.
- **Currents** build up over a few ticks for land mobs, as in Java, to a
  steady 0.07 blocks a tick in water (Track 3 used 0.056), and flowing lava
  pushes mobs too, 0.0023 blocks a tick (0.007 in the Nether).

## References

- [Java 1.21.5 biome spawn data](https://github.com/misode/mcmeta/tree/1.21.5-data/data/minecraft/worldgen/biome).
- [Guardian loot data](https://raw.githubusercontent.com/misode/mcmeta/1.21.5-data/data/minecraft/loot_table/entities/guardian.json).
- [Elder guardian loot data](https://raw.githubusercontent.com/misode/mcmeta/1.21.5-data/data/minecraft/loot_table/entities/elder_guardian.json).
- [Mojang’s elder guardian overview](https://www.minecraft.net/en-us/article/meet-elder-guardian).

## Tadpoles and frogs (v0.15)

Implemented, unverified: tadpoles hatch from frogspawn, swim and flop on
land, suffocate out of water, take slimeballs and retain age in water
buckets. Their adult frog variant follows Java 26.3 biome tag selectors at
growth. Frogs use amphibious movement and lay spawn beside water after
breeding. See [animals](animals.md#frogs-and-tadpoles-v015-java-263) and
[the passive track record](verification/2026-11-04-v0.15-passive-mobs.md).
