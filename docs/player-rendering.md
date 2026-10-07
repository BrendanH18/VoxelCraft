# Player rendering parity notes

The classic model uses Java's 64x64 skin UV layout, 8x8x8 head, 8x12x4
body, 4x12x4 arms and legs, shoulder pivots at +/-5 and leg pivots at
+/-1.9 model pixels. PlayerRenderer's 0.9375 scale is applied after the
1/16 model-pixel conversion. Hat inflation is 0.5 pixels; jacket,
sleeves and trouser overlays use 0.25. The skin is original procedural
pixel art, generated once at renderer creation; no Mojang assets are used.

Walking uses displacement*4 capped at 1 and 0.4 speed smoothing per
20 Hz tick, accumulating the smoothed speed as limbSwing. Arms use
cos(limbSwing*0.6662+[pi,0])*amount and legs use the opposite phases
with amplitude 1.4. The attack torso twist, quartic swing easing and
idle arm wobble follow HumanoidModel/AnimationUtils. Sneaking uses
body pitch 0.5, arm pitch +0.4, and Java's 4.2/3.2/5.2 head/body/arm
pivot drops and 12.2/-4 leg placement. Render geometry uses Y-up,
Z-forward, reversing Java's Y and Z coordinates.

All players share the existing entity vertex batch: host, local controller
peers and CLI agents. Cutout skin and held block/item textures use the
same lighting, fog and red hurt overlay. Animation creates no intermediate
part Vec or texture allocation per frame. The existing app view setup
still allocates its player/view lists.

Known approximations: body yaw follows view yaw with Java-style smoothing
and an 85-degree head limit, rather than LivingEntity's full movement/
attack-dependent body control. Item use raises the right arm; specialized
bow/crossbow, offhand and left-handed poses are not yet represented.
Held shaped blocks use a cube and flat items a thin textured box rather
than the resource-pack display transforms and full silhouette extrusion.
Death time currently derives from Vitals.since_damage; fatal hits during
an existing immunity window can start partway into the fall. Sprint-swim, forced crawl (swim hitbox on land), and Java 1.14+ crouch
hitbox/eye height are shared via `entity/player_pose.rs` and drive physics,
camera bob suppression and HumanoidModel-style swimAmount animation. Riding
and elytra poses remain separate roadmap work.

Reference client source (decompiled Java; these model/camera constants are
stable across the modern Java humanoid renderer):

- https://raw.githubusercontent.com/mahtomedi/minecraft/main/src/main/java/net/minecraft/client/model/HumanoidModel.java
- https://raw.githubusercontent.com/mahtomedi/minecraft/main/src/main/java/net/minecraft/client/model/PlayerModel.java
- https://raw.githubusercontent.com/mahtomedi/minecraft/main/src/main/java/net/minecraft/client/model/AnimationUtils.java
- https://raw.githubusercontent.com/mahtomedi/minecraft/main/src/main/java/net/minecraft/world/entity/WalkAnimationState.java
- https://raw.githubusercontent.com/mahtomedi/minecraft/main/src/main/java/net/minecraft/client/renderer/entity/player/PlayerRenderer.java
- https://minecraft.wiki/w/Third-person_view (direct retrieval blocked by robots.txt)

## Third-person views

F5 cycles first/back/front. Each Bot stores its own session-only camera
mode; controller X cycles only that Bot's view (X keeps its menu behavior
inside screens). `--camera first|third|front` controls the host at startup.
The host's own model and each follower's own model are drawn in third
person; first-person hands are suppressed. Picking remains at the player's
eye and look direction, independent of the camera. Fog checks camera
fluid occupancy, not the player's displaced eye.

Camera offsets match Camera.setup: four blocks backward from interpolated
eyes, with front yaw +180 degrees and pitch negated. Eight probes start
at every +/-0.1 XYZ corner, ignoring fluids and clipping block shapes.
DDA traversal and fixed-size probe state need no allocations. The camera
also stops at unloaded terrain. A conservative ray-distance bound and
0.0001-block separation supplement Java's Euclidean hit-to-eye distance,
which can otherwise let a corner cross a wall at a shallow angle. Camera
collision uses the engine's existing visual/collision shape definitions;
shape differences from Java remain an engine-wide parity issue.

- https://raw.githubusercontent.com/mahtomedi/minecraft/main/src/main/java/net/minecraft/client/Camera.java

## Walking bob and hurt tilt

Walking bob uses GameRenderer.bobView's phase
`-(walkDist + (walkDist - walkDistO) * partialTick) * pi`, translation
`(sin(phase)*bob*0.5, -abs(cos(phase)*bob), 0)`, Z rotation
`sin(phase)*bob*3` degrees and X rotation
`abs(cos(phase-0.2)*bob)*5` degrees. walkDist advances by horizontal
movement *0.6. Bob strength approaches a target capped at 0.1 with 0.4
smoothing per 20 Hz tick, decaying in flight/air. The target uses actual
horizontal displacement per tick rather than Java's velocity (the engine's
velocity has different physics units); swimming suppression awaits the
separate swimming pose work.

Hurt tilt uses a ten-tick interval and `-sin(remaining^4*pi)*14` degrees,
conjugated by the damage-source yaw. Mob, projectile and explosion
knockback supply that yaw for both host and CLI/controller players.
Death roll is `40-8000/(min(deathTicks,20)+200)` degrees. Death ticks have
the Vitals timing approximation described above. Damage tilt strength
uses Java's default 100%; a separate strength slider is not implemented.

Each view supplies the combined hurt/bob matrix before its view rotation.
The hand cancels and reapplies the same matrix around its FOV correction,
so it bobs exactly once in its fixed 70-degree projection at all FOVs.
Equip, attack and eating animation remain. View Bobbing is saved in options,
defaults on, and affects both world and hand; hurt tilt remains when it is
off. The options layout contracts spacing to fit split-screen views.

## Armor

HumanoidArmorLayer draws the same posed boxes one pixel outside the body
for the helmet, chestplate and boots, and half a pixel outside for
leggings. Textures are generated once into the skin array (no Mojang
assets): leather is a grey sheet dyed `#A06540` with an undyed stitch
overlay, and chainmail, iron, gold, diamond and netherite are generic
panel patterns. Chainmail is rolled onto zombies and skeletons; it is not
a player item. Netherite helmets, chestplates, leggings and boots use
the same path. Enchanted pieces add a scrolling glint in the entity fragment
shader. Monster equipment follows Java's feet-first 15% roll at regional
difficulty 1, without held weapons or local-difficulty scaling.

- https://raw.githubusercontent.com/mahtomedi/minecraft/main/src/main/java/net/minecraft/client/renderer/entity/layers/HumanoidArmorLayer.java

The formulas were also checked against the decompiled **1.21.1** client:

- https://raw.githubusercontent.com/Soumeh/1.21.1-Deobfuscated/main/minecraft/src/net/minecraft/client/render/GameRenderer.java
- https://raw.githubusercontent.com/Soumeh/1.21.1-Deobfuscated/main/minecraft/src/net/minecraft/entity/player/PlayerEntity.java
- https://raw.githubusercontent.com/Soumeh/1.21.1-Deobfuscated/main/minecraft/src/net/minecraft/client/render/Camera.java
- https://raw.githubusercontent.com/Soumeh/1.21.1-Deobfuscated/main/minecraft/src/net/minecraft/client/render/entity/model/BipedEntityModel.java
