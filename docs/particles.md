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
