//! v0.18 Track 6: ground shadows under mobs, players, dropped items,
//! experience orbs, minecarts, boats, primed TNT, falling blocks, End
//! crystals and the dragon, as Java 26.3's `EntityRenderer.extractShadow` and
//! `ShadowFeatureRenderer` draw them (Entity Shadows option).
//!
//! Each caster darkens the top of every full-collision block under its
//! radius, one quad per block, fading with depth below its feet and with
//! distance from the camera (gone at 16 blocks) and scaled by the light
//! above the block. The quads are camera-relative triangles in the entity
//! mesh's translucent list (material [`SHADOW_MATERIAL`]); `entity.wgsl`
//! draws the disc. Nothing here touches hitboxes, simulation or light.
//! Radii and strengths are checked against
//! `tests/fixtures/java/26.3/entity_shadows.json`.

use glam::{DVec3, IVec3, Vec3};

use super::mob::{Mob, MobKind};
use super::model::{EntityMesh, EntityVertex};
use crate::physics::BlockSource;
use crate::world::block::RenderKind;

/// The Entity Shadows option: Java's `options.txt` key, label and default.
pub const OPTION_KEY: &str = "entityShadows";
pub const OPTION_LABEL: &str = "Entity Shadows";
pub const OPTION_DEFAULT: bool = true;

/// The entity shader's material number for a shadow quad.
pub const SHADOW_MATERIAL: u8 = 5;
/// `EntityRenderer.MAX_SHADOW_RADIUS`.
pub const MAX_RADIUS: f32 = 32.0;
/// Squared camera distance at which a full-strength shadow has faded out
/// (`1 - distSq / 256`).
pub const FADE_DISTANCE_SQ: f64 = 256.0;
/// `SHADOW_POWER_FALLOFF_Y`: strength lost per block below the feet.
pub const FALLOFF_PER_BLOCK: f32 = 0.5;
/// Blocks lit at this level or below get no shadow (`brightness > 3`).
pub const MIN_BRIGHTNESS: u8 = 3;
/// Java's `VIEW_OFFSET_Z_LAYERING` scale, which pulls the quads toward the
/// camera so they don't fight the block top they lie on.
const VIEW_OFFSET: f32 = 1.0 - 1.0 / 4096.0;

/// One shadow: interpolated feet, radius in blocks and strength 0..1.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Caster {
    pub pos: DVec3,
    pub radius: f32,
    pub strength: f32,
}

/// The ground under a shadow, as `extractShadowPiece` reads it.
pub trait Ground {
    /// For the air cell `p` above a possible receiver: the brightness factor
    /// (`Lightmap.getBrightness` of the raw local brightness at `p`) when the
    /// block below `p` is drawn, has a full-block collision shape and `p` is
    /// lit above [`MIN_BRIGHTNESS`]; `None` when nothing is drawn there.
    fn receiver(&self, p: IVec3) -> Option<f32>;
}

/// `Lightmap.getBrightness(dimensionType, level)`: the curved light level,
/// lifted toward full by the dimension type's `ambient_light`.
pub fn brightness(level: u8, ambient: f32) -> f32 {
    let v = f32::from(level) / 15.0;
    let curved = v / (4.0 - 3.0 * v);
    curved + (1.0 - curved) * ambient
}

impl Ground for crate::world::World {
    fn receiver(&self, p: IVec3) -> Option<f32> {
        let below = self.block(p - IVec3::Y)?;
        if below.kind() == RenderKind::Invisible || below == crate::world::redstone_blocks::MOVING {
            return None;
        }
        // Inside an opaque block the light is 0 (or the piece is hidden in
        // an emitting one), so skip the light lookup Java would make.
        if self.block(p).is_none_or(|b| b.is_opaque()) {
            return None;
        }
        let level = self.raw_brightness(p, self.sky_darken()).unwrap_or_else(|| self.block_light(p));
        if level <= MIN_BRIGHTNESS || !crate::physics::full_collision(self, p - IVec3::Y) {
            return None;
        }
        Some(brightness(level, self.generator.dimension.ambient_light()))
    }
}

/// The shadow radius each mob's Java renderer is built with (the constant
/// its constructor passes on, or the field it sets).
pub fn base_radius(kind: MobKind) -> f32 {
    use MobKind::*;
    match kind {
        Shulker => 0.0,
        TropicalFish => 0.15,
        Tadpole => 0.14,
        Pufferfish => 0.2,
        Bat => 0.25,
        Slime | MagmaCube | SulfurCube => 0.25,
        Chicken | Cod | Endermite | Frog | Parrot | Rabbit | Silverfish | Vex => 0.3,
        Allay | Armadillo | Bee | Cat | Fox | Ocelot | Salmon => 0.4,
        Axolotl | Blaze | Bogged | Breeze | CopperGolem | Creeper | Drowned | Enderman | Evoker | Guardian | Husk
        | Illusioner | Parched | Piglin | PiglinBrute | Pillager | Skeleton | SnowGolem | Stray | Strider
        | Villager | Vindicator | WanderingTrader | Witch | WitherSkeleton | Wolf | Zombie | ZombieVillager
        | ZombifiedPiglin => 0.5,
        CaveSpider => 0.56,
        Creaking => 0.6,
        Camel | CamelHusk | Cow | Dolphin | GlowSquid | Goat | Hoglin | IronGolem | Llama | TraderLlama | Mooshroom
        | Nautilus | ZombieNautilus | Pig | Sheep | Squid | Turtle | Zoglin => 0.7,
        Donkey | Horse | Mule | SkeletonHorse | ZombieHorse | Phantom => 0.75,
        Spider => 0.8,
        Panda | PolarBear | Warden => 0.9,
        Wither => 1.0,
        Ravager | Sniffer => 1.1,
        ElderGuardian => 1.2,
        Ghast => 1.5,
        HappyGhast => 2.0,
    }
}

/// `LivingEntity.getAgeScale` for a baby: 0.5 unless the entity overrides it.
pub fn baby_age_scale(kind: MobKind) -> f32 {
    match kind {
        MobKind::Turtle => 0.3,
        MobKind::HappyGhast => 0.2375,
        MobKind::Goat => 0.55,
        MobKind::Dolphin => 0.65,
        MobKind::Armadillo | MobKind::Camel => 0.6,
        _ => 0.5,
    }
}

/// A mob's shadow radius, with the renderer overrides Java applies:
/// slimes, magma cubes and sulfur cubes by size, pufferfish by puff state,
/// and every other mob by its age scale (with the villager's, turtle's and
/// strider's extra baby factors). No attribute scale exists here, so
/// `LivingEntityRenderer`'s `* scale` is 1.
pub fn mob_radius(m: &Mob) -> f32 {
    let baby = m.baby || m.age < 0;
    let radius = match m.kind {
        k if k.is_cube() => 0.25 * f32::from(m.size),
        MobKind::Pufferfish => 0.1 + 0.1 * m.aquatic.as_ref().map_or(0.0, |a| f32::from(a.puff)),
        k => {
            let r = base_radius(k) * if baby { baby_age_scale(k) } else { 1.0 };
            match k {
                MobKind::Villager | MobKind::Strider if baby => r * 0.5,
                MobKind::Turtle if baby => r * 0.83,
                _ => r,
            }
        }
    };
    radius.min(MAX_RADIUS)
}

/// Dropped items and experience orbs (`ItemEntityRenderer`, `ExperienceOrbRenderer`).
pub const ITEM: (f32, f32) = (0.15, 0.75);
/// Players (`AvatarRenderer`).
pub const PLAYER_RADIUS: f32 = 0.5;
/// Primed TNT and falling blocks (`TntRenderer`, `FallingBlockRenderer`).
pub const BLOCK_RADIUS: f32 = 0.5;
/// End crystals (`EndCrystalRenderer`).
pub const CRYSTAL_RADIUS: f32 = 0.5;
/// The Ender Dragon (`EnderDragonRenderer`), whose renderer keeps the
/// default-sized shadow however big the dragon is.
pub const DRAGON_RADIUS: f32 = 0.5;
/// Minecarts of every kind (`AbstractMinecartRenderer`).
pub const MINECART_RADIUS: f32 = 0.7;
/// Boats and rafts (`AbstractBoatRenderer`).
pub const BOAT_RADIUS: f32 = 0.8;

/// A minecart, boat or cushion's shadow radius; cushions (`CushionRenderer`) have none.
pub fn cart_radius(kind: super::minecart::CartKind) -> f32 {
    use super::minecart::CartKind;
    match kind {
        CartKind::Boat { .. } => BOAT_RADIUS,
        CartKind::Cushion { .. } => 0.0,
        _ => MINECART_RADIUS,
    }
}

/// A falling block's shadow: `pos` is its minimum corner.
pub fn falling_block(pos: DVec3) -> Caster {
    Caster { pos: pos + DVec3::new(0.5, 0.0, 0.5), radius: BLOCK_RADIUS, strength: 1.0 }
}

/// A player's shadow at its interpolated feet.
pub fn player(pos: DVec3) -> Caster {
    Caster { pos, radius: PLAYER_RADIUS, strength: 1.0 }
}

/// The shadow casters among `mobs` within the fade distance of `camera`:
/// drawn and not invisible.
pub fn mob_casters(mobs: &[Mob], camera: DVec3, alpha: f64, out: &mut Vec<Caster>) {
    for m in mobs {
        if m.trader.as_ref().is_some_and(|t| t.invisible)
            || m.effects.has(crate::simulation::effects::Effect::Invisibility)
        {
            continue;
        }
        let pos = m.previous_pos.lerp(m.pos, alpha);
        if pos.distance_squared(camera) >= FADE_DISTANCE_SQ {
            continue;
        }
        out.push(Caster { pos, radius: mob_radius(m), strength: 1.0 });
    }
}

/// Appends every caster's shadow quads to `out` (camera-relative).
pub fn build<G: Ground + ?Sized>(ground: &G, casters: &[Caster], camera: DVec3, out: &mut EntityMesh) {
    for c in casters {
        push(ground, c, camera, out);
    }
}

/// `extractShadow` for one caster.
pub fn push<G: Ground + ?Sized>(ground: &G, c: &Caster, camera: DVec3, out: &mut EntityMesh) {
    let radius = c.radius.min(MAX_RADIUS);
    if radius <= 0.0 {
        return;
    }
    let power = ((1.0 - c.pos.distance_squared(camera) / FADE_DISTANCE_SQ) * f64::from(c.strength)) as f32;
    if power <= 0.0 {
        return;
    }
    let r = f64::from(radius);
    // Never negative: a faint shadow still reaches the block under its feet.
    let depth = f64::from((power / 0.5 - 1.0).clamp(0.0, radius));
    let (x0, x1) = ((c.pos.x - r).floor() as i32, (c.pos.x + r).floor() as i32);
    let (z0, z1) = ((c.pos.z - r).floor() as i32, (c.pos.z + r).floor() as i32);
    let (y0, y1) = ((c.pos.y - depth).floor() as i32, c.pos.y.floor() as i32);
    for z in z0..=z1 {
        for x in x0..=x1 {
            for y in y0..=y1 {
                let p = IVec3::new(x, y, z);
                let Some(light) = ground.receiver(p) else { continue };
                let at_depth = power - (c.pos.y - f64::from(y)) as f32 * FALLOFF_PER_BLOCK;
                let alpha = (at_depth * 0.5 * light).clamp(0.0, 1.0);
                if alpha > 0.0 {
                    quad(out, p, c, radius, alpha, camera);
                }
            }
        }
    }
}

/// `ShadowFeatureRenderer.prepare`: the block top under `p`, its disc
/// coordinates running from 0 to 1 across the shadow's diameter.
fn quad(out: &mut EntityMesh, p: IVec3, c: &Caster, radius: f32, alpha: f32, camera: DVec3) {
    let rel = (p.as_dvec3() - c.pos).as_vec3();
    let (x0, x1, z0, z1) = (rel.x, rel.x + 1.0, rel.z, rel.z + 1.0);
    let u = |x: f32| -x / 2.0 / radius + 0.5;
    let base = (c.pos - camera).as_vec3();
    let a = (alpha * 255.0).round() as u8;
    let corner = |x: f32, z: f32| EntityVertex {
        pos: ((base + Vec3::new(x, rel.y, z)) * VIEW_OFFSET).to_array(),
        uv: [u(x), u(z)],
        color: [0, 0, 0, a],
        light: [255, 255, 0, 0],
        torch: [0, SHADOW_MATERIAL, 0, 0],
    };
    // Wound counter-clockwise seen from above, like a box's top face.
    let (v00, v01, v11, v10) = (corner(x0, z0), corner(x0, z1), corner(x1, z1), corner(x1, z0));
    out.translucent_mut().extend_from_slice(&[v00, v01, v11, v00, v11, v10]);
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Flat ground at y = 0 (the cells at y = 1 are the air above it), lit `level`.
    struct Flat(u8);
    impl Ground for Flat {
        fn receiver(&self, p: IVec3) -> Option<f32> {
            (p.y == 1 && self.0 > MIN_BRIGHTNESS).then(|| brightness(self.0, 0.0))
        }
    }

    fn quads(c: Caster, camera: DVec3, ground: &impl Ground) -> Vec<EntityVertex> {
        let mut mesh = EntityMesh::flat();
        push(ground, &c, camera, &mut mesh);
        // A drawn mesh keeps them with the blended triangles, drawn last.
        let mut drawn = EntityMesh::default();
        push(ground, &c, camera, &mut drawn);
        assert!(drawn.is_empty() && drawn.translucent() == &mesh[..]);
        mesh.to_vec()
    }

    #[test]
    fn a_shadow_lies_on_the_block_tops_under_its_radius() {
        let c = Caster { pos: DVec3::new(0.5, 1.0, 0.5), radius: 0.7, strength: 1.0 };
        let v = quads(c, DVec3::new(0.5, 1.0, 0.5), &Flat(15));
        // 0.5 ± 0.7 spans columns -1, 0 and 1 on both axes.
        assert_eq!(v.len(), 9 * 6);
        assert!(v.iter().all(|v| (v.pos[1] - 0.0).abs() < 1e-3 && v.torch[1] == SHADOW_MATERIAL));
        // Full strength at the feet, full light: 0.5 alpha.
        assert!(v.iter().all(|v| v.color[3] == 128));
        // The disc's centre is at uv (0.5, 0.5), under the feet.
        let centre = v.iter().find(|v| v.pos[0].abs() < 0.6 && v.pos[2].abs() < 0.6 && v.uv[0] > 0.0);
        assert!(centre.is_some());
    }

    #[test]
    fn shadows_fade_with_camera_distance_height_and_darkness() {
        let at = |y: f64, cam: f64, level| {
            let c = Caster { pos: DVec3::new(0.5, y, 0.5), radius: 0.5, strength: 1.0 };
            quads(c, DVec3::new(0.5 + cam, y, 0.5), &Flat(level)).first().map(|v| v.color[3])
        };
        assert_eq!(at(1.0, 0.0, 15), Some(128));
        assert!(at(1.0, 8.0, 15).unwrap() < 128);
        assert_eq!(at(1.0, 16.0, 15), None, "gone 16 blocks away");
        // Lifted half a block: power 1 - 0.25 at the block top below.
        assert_eq!(at(1.5, 0.0, 15), Some(96));
        // Too high above the ground: the scan doesn't reach it.
        assert_eq!(at(2.6, 0.0, 15), None);
        assert!(at(1.0, 0.0, 8).unwrap() < at(1.0, 0.0, 15).unwrap());
        assert_eq!(at(1.0, 0.0, 3), None, "no shadow at light 3 and below");
    }

    #[test]
    fn invisible_and_zero_radius_casters_draw_nothing() {
        let c = Caster { pos: DVec3::new(0.5, 1.0, 0.5), radius: 0.0, strength: 1.0 };
        assert!(quads(c, DVec3::ZERO, &Flat(15)).is_empty());
        let mut out = Vec::new();
        let mut m = Mob::new(MobKind::Pig, DVec3::new(0.5, 1.0, 0.5), 0.0);
        m.previous_pos = m.pos;
        mob_casters(std::slice::from_ref(&m), DVec3::ZERO, 1.0, &mut out);
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].radius, 0.7);
        m.effects.add(crate::simulation::effects::Effect::Invisibility, 0, 200);
        out.clear();
        mob_casters(std::slice::from_ref(&m), DVec3::ZERO, 1.0, &mut out);
        assert!(out.is_empty());
    }

    #[test]
    fn baby_and_sized_mobs_scale_their_shadows() {
        let mob = |kind, baby: bool| {
            let mut m = Mob::new(kind, DVec3::ZERO, 0.0);
            m.baby = baby;
            m
        };
        assert_eq!(mob_radius(&mob(MobKind::Cow, true)), 0.35);
        assert_eq!(mob_radius(&mob(MobKind::Villager, true)), 0.125);
        assert!((mob_radius(&mob(MobKind::Turtle, true)) - 0.7 * 0.3 * 0.83).abs() < 1e-6);
        let mut slime = mob(MobKind::Slime, false);
        slime.size = 4;
        assert_eq!(mob_radius(&slime), 1.0);
    }
}
