# Client particles

The device-free `particles` module exposes a bounded request mailbox and a
16,384-particle FIFO pool. Velocities and lifetimes use 20 Hz game ticks;
previous/current positions interpolate separately for each camera. Expired
particles retain no storage. Pool updates and renderer staging never grow
or allocate after construction. Collision uses the engine's block shapes,
including slabs, stairs and fences, in bounded substeps.

The client renders camera-relative billboards in one instanced draw from a
fixed GPU buffer. Terrain particles resolve block icons at render time,
without depending on the block ID width or packed chunk quad format. Other
particles use original procedural 16-pixel sprites with eight age stages.
Light samples use authoritative block light and the existing skylight query.

Particles alpha-test and write depth before translucent water. Water thus
tints particles behind it and cannot paint over nearer particles. Fractional
particle alpha uses screen-door dithering instead of sorted alpha blending;
this is a deliberate simplification for ambient effect particles. The
skylight query has the same heightmap approximation as entities. No external
Minecraft textures or source code are included.

Reference rules: [Minecraft Wiki particles](https://minecraft.wiki/w/Particles),
[Java Particle](https://github.com/mahtomedi/minecraft/blob/main/src/main/java/net/minecraft/client/particle/Particle.java),
[Java ParticleEngine](https://github.com/mahtomedi/minecraft/blob/main/src/main/java/net/minecraft/client/particle/ParticleEngine.java),
[Java TerrainParticle](https://github.com/mahtomedi/minecraft/blob/main/src/main/java/net/minecraft/client/particle/TerrainParticle.java).
The decompiled reference repository predates 1.21; stable rules from it are
used as a baseline, not a claim of a complete 26.x implementation.

## Gameplay events and ambient emitters

Break, hit, combat, death, explosion, water entry and eye-shatter requests
are queued by the shared simulation (host, pad players and agents). The
client keeps one pool and one instanced draw, samples it around every
split-screen camera, and drops requests farther than Java's 32-block add
distance (128 for break, hit and explosions). Headless builds only see the
bounded mailbox.

Ambient work is client-only. Each camera tries 667 blocks at 16 and at 32
blocks, matching `ClientLevel.animateTick`, and skips that scan on Minimal.
Decreased still keeps two thirds of ordinary particles. Torches emit smoke
and flame. Smoke and large smoke follow Java `SmokeParticle`: quad size
about `0.1 * scale` after the 0.75 factor, grey (`rCol` 0.3–0.7) and
shrinking over their lifetime. Fire emits three large smokes when the block below can burn or
hold a solid top, otherwise two on each flammable face. Lava pops (1/100)
and ceiling drips (1/10, hanging then falling) follow `LavaFluid` and
`DripParticle`. Nether portals, End portals (smoke) and End gateways
(one portal particle per exposed face) follow their `animateTick` methods.
Enchanting tables send glyphs toward bookshelves with air between.

Rain tries `100 * strength²` columns within ten blocks (half on Decreased,
none on Minimal, half again without enhanced graphics) and uses smoke over
lava or fire. Players and agents enqueue `1 + width * 20` splashes and
bubbles when they enter water. Endermen, flying eyes of ender and dragon
breath are sampled from entities without touching mob RNG. Status effects
blend Java's potion colour into one swirl. A shattered eye plays the
80-particle portal ring from level event 2003.

Spawners still emit smoke and flame on the existing near-player roll
(about six times a second) so their RNG stream stays stable. Java's client
tick emits both every tick.

## Still open

Heart and angry particles have providers and no breeding or villager
behaviour to emit them. Also later: water dripping from ceilings, underwater
specks, bubble columns, sprint dust, fishing, campfires, spore blossoms,
sculk, sonic boom, splash and lingering potion bursts, item-break crumbs,
and mob water-entry splashes. Wall torches are not a separate block. Agent
hits on the dragon emit enchanted crits; falling critical hits stay on the
host attack path, which already applies the 1.5 damage.

## Providers and action events

Full-block destruction emits 4x4x4 crumbs; shaped blocks subdivide their
selection boxes in quarter-block steps, at least two per axis. Mining emits
one face-offset crumb per game tick with 0.2 velocity power and 0.6 size.
Destruction/hit requests bypass particle settings, as Java's direct terrain
particle path does. Removing blocks through the shared `World::set_block`
also covers agent and controller mining.

The options menu persists All/Decreased/Minimal. Normal particles are all,
2/3, or none; important requests receive Java's 1/10 rescue on Minimal
followed by the 2/3 filter. Poof and explosion types override the limiter.
Combat uses three-tick crit/magic-crit emitters with sixteen unit-sphere
rejection samples per tick. Their centers predict motion from the hit
snapshot rather than tracking a permanent entity ID. Death emits twenty
poofs when the mob disappears after its death animation, not on the fatal
hit. Explosions replace cube smoke with six fullbright billboards per tick
for eight ticks (or a single small explosion below power 2).

Providers also exist for flame, smoke, lava, lava drip, splash, bubble,
portal, enchantment glyph, colored effect, dragon breath, heart and angry.
The provider lifetimes, friction, gravity and color curves follow the Java
classes. Water drops remove themselves on about half of ground contacts, matching
`WaterDropParticle`. Heart and angry are ready for future breeding and
villager behaviour.

Provider sources: [CritParticle](https://github.com/mahtomedi/minecraft/blob/main/src/main/java/net/minecraft/client/particle/CritParticle.java),
[TrackingEmitter](https://github.com/mahtomedi/minecraft/blob/main/src/main/java/net/minecraft/client/particle/TrackingEmitter.java),
[ExplodeParticle](https://github.com/mahtomedi/minecraft/blob/main/src/main/java/net/minecraft/client/particle/ExplodeParticle.java),
[HugeExplosionSeedParticle](https://github.com/mahtomedi/minecraft/blob/main/src/main/java/net/minecraft/client/particle/HugeExplosionSeedParticle.java),
[ParticleTypes](https://github.com/mahtomedi/minecraft/blob/main/src/main/java/net/minecraft/core/particles/ParticleTypes.java),
[LevelRenderer](https://github.com/mahtomedi/minecraft/blob/main/src/main/java/net/minecraft/client/renderer/LevelRenderer.java).
