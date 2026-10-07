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
classes. Water drops currently expire deterministically on floor contact;
Java chooses randomly on contact. Heart and angry are ready for future
breeding/villager behavior.

Provider sources: [CritParticle](https://github.com/mahtomedi/minecraft/blob/main/src/main/java/net/minecraft/client/particle/CritParticle.java),
[TrackingEmitter](https://github.com/mahtomedi/minecraft/blob/main/src/main/java/net/minecraft/client/particle/TrackingEmitter.java),
[ExplodeParticle](https://github.com/mahtomedi/minecraft/blob/main/src/main/java/net/minecraft/client/particle/ExplodeParticle.java),
[HugeExplosionSeedParticle](https://github.com/mahtomedi/minecraft/blob/main/src/main/java/net/minecraft/client/particle/HugeExplosionSeedParticle.java),
[ParticleTypes](https://github.com/mahtomedi/minecraft/blob/main/src/main/java/net/minecraft/core/particles/ParticleTypes.java),
[LevelRenderer](https://github.com/mahtomedi/minecraft/blob/main/src/main/java/net/minecraft/client/renderer/LevelRenderer.java).
