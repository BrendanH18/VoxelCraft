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
an existing immunity window can start partway into the fall. Swimming,
crawling, riding and elytra poses remain separate roadmap work.

Reference client source (decompiled Java; these model/camera constants are
stable across the modern Java humanoid renderer):

- https://raw.githubusercontent.com/mahtomedi/minecraft/main/src/main/java/net/minecraft/client/model/HumanoidModel.java
- https://raw.githubusercontent.com/mahtomedi/minecraft/main/src/main/java/net/minecraft/client/model/PlayerModel.java
- https://raw.githubusercontent.com/mahtomedi/minecraft/main/src/main/java/net/minecraft/client/model/AnimationUtils.java
- https://raw.githubusercontent.com/mahtomedi/minecraft/main/src/main/java/net/minecraft/world/entity/WalkAnimationState.java
- https://raw.githubusercontent.com/mahtomedi/minecraft/main/src/main/java/net/minecraft/client/renderer/entity/player/PlayerRenderer.java
- https://minecraft.wiki/w/Third-person_view (direct retrieval blocked by robots.txt)
