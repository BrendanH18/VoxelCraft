//! Original box models for the v0.15 Track C mobs: happy ghasts with their
//! harness and goggles, nautiluses and zombie nautiluses with saddles and
//! armor, camel husks (the camel rig, dried) and parched (the skeleton rig,
//! sun-bleached and wrapped). Vertex colours only; no Mojang assets.
use super::*;

// ------------------------------------------------------------ happy ghast
const GHAST_SKIN: Rgb = [238, 236, 232];
const GHAST_SHADE: Rgb = [214, 212, 210];
const GHAST_EYE: Rgb = [42, 40, 48];
const GHAST_CHEEK: Rgb = [244, 178, 186];
/// Marker colour the harness straps are drawn in; [`colour`] dyes it.
const HARNESS: Rgb = [201, 0, 201];
const HARNESS_TRIM: Rgb = [86, 64, 46];
const LENS: Rgb = [120, 200, 230];
/// A 4-block body over nine short tentacles, facing +Z.
const GHAST_BODY: &[Cuboid] = &[
    cube([-30., 12., -30.], [30., 64., 30.], GHAST_SKIN, 10),
    cube([-30.2, 12., -30.2], [30.2, 14., 30.2], GHAST_SHADE, 8),
    // Closed, smiling eyes and rosy cheeks on the front face.
    cube([-14., 40., 30.], [-6., 42., 30.6], GHAST_EYE, 0),
    cube([6., 40., 30.], [14., 42., 30.6], GHAST_EYE, 0),
    cube([-20., 34., 30.], [-14., 37., 30.4], GHAST_CHEEK, 0),
    cube([14., 34., 30.], [20., 37., 30.4], GHAST_CHEEK, 0),
    cube([-4., 30., 30.], [4., 32., 30.6], GHAST_EYE, 0),
];
const GHAST_TENTACLE: &[Cuboid] = &[cube([-2., -12., -2.], [2., 0., 2.], GHAST_SHADE, 14)];
/// The harness: a dyed saddle frame on top and side straps.
const GHAST_HARNESS: &[Cuboid] = &[
    cube([-31., 60., -31.], [31., 64.4, 31.], HARNESS, 18),
    cube([-31., 20., -4.], [-30.4, 64., 4.], HARNESS, 18),
    cube([30.4, 20., -4.], [31., 64., 4.], HARNESS, 18),
    cube([-26., 64.4, -26.], [26., 65., 26.], HARNESS_TRIM, 22),
];
/// Goggles up on the forehead (no one aboard) or down over the eyes.
const GOGGLES_UP: &[Cuboid] = &[
    cube([-24., 50., 30.4], [24., 53., 31.2], HARNESS_TRIM, 10),
    cube([-16., 49., 30.8], [-4., 55., 32.], LENS, 0),
    cube([4., 49., 30.8], [16., 55., 32.], LENS, 0),
];
const GOGGLES_DOWN: &[Cuboid] = &[
    cube([-24., 39., 30.4], [24., 42., 31.2], HARNESS_TRIM, 10),
    cube([-16., 37., 30.8], [-4., 44., 32.], LENS, 0),
    cube([4., 37., 30.8], [16., 44., 32.], LENS, 0),
];
const TENTACLES: [[f32; 2]; 9] =
    [[-20., -20.], [0., -20.], [20., -20.], [-20., 0.], [0., 0.], [20., 0.], [-20., 20.], [0., 20.], [20., 20.]];

pub(super) fn happy_ghast(m: &Mob, time: f32) -> Parts {
    let mut p = parts![part(GHAST_BODY, [0.; 3], Quat::IDENTITY)];
    for (i, [x, z]) in TENTACLES.into_iter().enumerate() {
        // A slow sway, as `GhastModel.setupAnim`'s, kept level so the
        // tips stay on the box's floor.
        let sway = 1.5 * (time * 1.2 + i as f32 * 0.9).sin();
        p.push(part(GHAST_TENTACLE, [x, 12., z + sway], Quat::IDENTITY));
    }
    if m.mount.as_ref().is_some_and(|s| s.slots[1].is_some()) && !m.baby {
        p.push(part(GHAST_HARNESS, [0.; 3], Quat::IDENTITY));
        let down = m.happy_ghast.as_ref().is_some_and(|h| h.goggles_down);
        p.push(part(if down { GOGGLES_DOWN } else { GOGGLES_UP }, [0.; 3], Quat::IDENTITY));
    }
    p
}

// --------------------------------------------------------------- nautilus
/// Marker colours the variants recolour: shell, stripe, body.
const SHELL: Rgb = [222, 204, 172];
const STRIPE: Rgb = [166, 86, 52];
const SOFT: Rgb = [196, 132, 116];
const NAUTILUS_EYE: Rgb = [24, 24, 30];
const SADDLE: Rgb = [110, 66, 38];
/// Marker for worn nautilus armor; [`colour`] picks the material.
const NAUTILUS_ARMOR: Rgb = [3, 201, 3];
/// The coiled shell behind, the soft head and tentacles in front (+Z).
const NAUTILUS_SHELL: &[Cuboid] = &[
    cube([-5., 2., -8.], [5., 15.2, 4.], SHELL, 22),
    cube([-5.2, 6., -6.], [5.2, 7.5, 4.2], STRIPE, 10),
    cube([-5.2, 10., -7.], [5.2, 11.5, 4.2], STRIPE, 10),
    cube([-3., 8., -8.6], [3., 13., -8.], STRIPE, 12),
    // v0.18 Track 6 round 4: the hood's fold over the head.
    cube([-4.5, 12., 4.], [4.5, 13., 4.25], STRIPE, 0),
];
const NAUTILUS_HEAD: &[Cuboid] = &[
    cube([-4., 0., 0.], [4., 7., 6.], SOFT, 18),
    cube([-4.2, 4., 2.], [-3.6, 5.5, 3.5], NAUTILUS_EYE, 0),
    cube([3.6, 4., 2.], [4.2, 5.5, 3.5], NAUTILUS_EYE, 0),
    // v0.18 Track 6 round 4: eye glints and a beak between the tentacles.
    cube([-4.25, 5.0, 2.6], [-4.2, 5.4, 3.0], [230, 228, 220], 0),
    cube([4.2, 5.0, 2.6], [4.25, 5.4, 3.0], [230, 228, 220], 0),
    cube([-1.0, 2.0, 6.0], [1.0, 3.2, 6.6], [70, 46, 40], 0),
];
const NAUTILUS_TENTACLES: &[Cuboid] = &[
    cube([-3., 0., 0.], [-1.5, 2., 5.], SOFT, 16),
    cube([-0.75, 0., 0.], [0.75, 2., 6.], SOFT, 16),
    cube([1.5, 0., 0.], [3., 2., 5.], SOFT, 16),
];
const NAUTILUS_SADDLE: &[Cuboid] = &[
    cube([-4., 15.2, -5.], [4., 16., 2.], SADDLE, 20),
    cube([-5.3, 9., -1.], [-5., 15.2, 0.], SADDLE, 10),
    cube([5., 9., -1.], [5.3, 15.2, 0.], SADDLE, 10),
];
const NAUTILUS_PLATE: &[Cuboid] = &[cube([-5.6, 4., -8.6], [5.6, 14., 4.6], NAUTILUS_ARMOR, 24)];

pub(super) fn nautilus(m: &Mob, time: f32) -> Parts {
    let dash = m.nautilus.as_ref().is_some_and(|s| s.dashing);
    let wave = (time * if dash { 14.0 } else { 5.0 }).sin() * 0.25;
    let s = m.mount.as_ref();
    let mut p = parts![
        part(NAUTILUS_SHELL, [0.; 3], Quat::IDENTITY),
        part(NAUTILUS_HEAD, [0., 0., 4.], Quat::from_rotation_y(-m.head_yaw * 0.3)),
        part(NAUTILUS_TENTACLES, [0., 0., 10.], Quat::from_rotation_x(wave)),
    ];
    if s.is_some_and(|s| s.slots[1].is_some()) {
        p.push(part(NAUTILUS_PLATE, [0.; 3], Quat::IDENTITY));
    }
    if s.is_some_and(|s| s.saddled()) {
        p.push(part(NAUTILUS_SADDLE, [0.; 3], Quat::IDENTITY));
    }
    p
}

// ---------------------------------------------------------------- parched
const WRAP: Rgb = [196, 170, 120];
const WRAP_DARK: Rgb = [150, 118, 76];
/// Sun-bleached cloth wraps over the skeleton rig.
pub(super) const PARCHED_WRAPS: &[Cuboid] = &[
    cube([-4.3, 18., -2.3], [4.3, 22.5, 2.3], WRAP, 40),
    cube([-4.4, 23.5, -2.5], [4.4, 24.5, 2.5], WRAP_DARK, 30),
    cube([-3.9, 12., -2.2], [-0.4, 15., 2.2], WRAP, 36),
    cube([0.4, 11., -2.2], [3.9, 14., 2.2], WRAP_DARK, 36),
];

/// Recolours the markers: the harness dye, nautilus variants and armor,
/// the camel husk's dried hide and the parched's bleached bones.
pub(super) fn colour(m: &Mob, c: &mut Cuboid) {
    match m.kind {
        MobKind::HappyGhast if c.color == HARNESS => {
            c.color = m
                .mount
                .as_ref()
                .and_then(|s| s.slots[1])
                .and_then(|s| crate::content::harness::colour(s.item))
                .map_or([200, 200, 200], |d| d.sheep_rgb());
        }
        MobKind::Nautilus | MobKind::ZombieNautilus if c.color == NAUTILUS_ARMOR => {
            let material = m.mount.as_ref().and_then(|s| s.slots[1]).and_then(|s| s.item.body_armor()).map(|(_, m)| m);
            use crate::item::ArmorMaterial as A;
            c.color = match material {
                Some(A::Copper) => [196, 112, 72],
                Some(A::Gold) => [236, 204, 72],
                Some(A::Diamond) => [92, 214, 206],
                Some(A::Netherite) => [74, 66, 70],
                _ => [200, 204, 210],
            };
        }
        MobKind::ZombieNautilus => {
            // Temperate: grey-green rot. Warm (`zombie_nautilus_coral`): coral pink.
            let warm = m.mount.as_ref().is_some_and(|s| s.variant == 1);
            c.color = match (c.color, warm) {
                (SHELL, false) => [150, 158, 128],
                (SHELL, true) => [214, 128, 140],
                (STRIPE, false) => [86, 104, 72],
                (STRIPE, true) => [176, 64, 96],
                (SOFT, _) => [104, 132, 96],
                (other, _) => other,
            };
        }
        MobKind::CamelHusk if c.color != [28, 28, 22] => {
            let [r, g, b] = c.color;
            let f = |v: u8, k: f32, add: f32| (f32::from(v) * k + add).min(255.0) as u8;
            c.color = [f(r, 0.55, 30.0), f(g, 0.6, 32.0), f(b, 0.5, 22.0)];
        }
        MobKind::Parched if c.color == BONE => {
            c.color = [226, 214, 186];
            c.noise = 30;
        }
        _ => {}
    }
}

// ------------------------------------------------------------ sulfur cube
// v0.16 Track D: an original sulfur cube — a cloudy, sulfur-yellow jelly
// shell around a mustard core with a sleepy face; holding a block, the core
// gives way to the block itself.
const SULFUR_CORE: Rgb = [190, 164, 52];
const SULFUR_FACE: Rgb = [92, 70, 24];
const SULFUR_SHELL: Rgb = [226, 206, 88];
const SULFUR_CUBE: &[Cuboid] = &[
    cube([-2.8, 1.1, -2.8], [2.8, 6.7, 2.8], SULFUR_CORE, 40),
    cube([-2.0, 4.2, 2.8], [-0.7, 4.8, 2.9], SULFUR_FACE, 0),
    cube([0.7, 4.2, 2.8], [2.0, 4.8, 2.9], SULFUR_FACE, 0),
    cube([-0.6, 2.4, 2.8], [0.6, 3.0, 2.9], SULFUR_FACE, 0),
    // v0.18 Track 6 round 4: eye glints in the core.
    cube([-1.3, 4.4, 2.9], [-0.9, 4.7, 2.95], [250, 240, 190], 0),
    cube([1.3, 4.4, 2.9], [1.7, 4.7, 2.95], [250, 240, 190], 0),
    glass([-3.92, 0.0, -3.92], [3.92, 7.84, 3.92], SULFUR_SHELL, 50),
];
const SULFUR_SHELL_ONLY: &[Cuboid] = &[glass([-3.92, 0.0, -3.92], [3.92, 7.84, 3.92], SULFUR_SHELL, 50)];

pub(super) fn sulfur_cube(m: &Mob) -> Parts {
    // A lit cube flashes like TNT through the fuse's last ticks (see `build`).
    if m.sulfur_has_block() {
        parts![part(SULFUR_SHELL_ONLY, [0.0; 3], Quat::IDENTITY)]
    } else {
        parts![part(SULFUR_CUBE, [0.0; 3], Quat::IDENTITY)]
    }
}

/// The block a sulfur cube holds, textured, inside its shell.
pub(super) fn sulfur_block(m: &Mob, origin: Vec3, body: Quat, scale: f32, light: ([u8; 4], u8), out: &mut EntityMesh) {
    let Some(block) = m.sulfur_body().and_then(|s| s.item.block()) else { return };
    let held = crate::entity::player_model::Limb {
        pivot: Vec3::new(0.0, 3.92, 0.0),
        rot: Quat::IDENTITY,
        min: [-2.9; 3],
        max: [2.9; 3],
        uv: [0.0; 2],
    };
    crate::entity::player_model::textured_box(
        out,
        &held,
        0.0,
        origin,
        body,
        light,
        2,
        block.info().tex[0],
        scale,
        [255, 255, 255, 0],
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    fn mesh(m: &Mob) -> Vec<[f32; 3]> {
        let mut out = EntityMesh::flat();
        build(std::slice::from_ref(m), DVec3::new(0.0, 64.0, 0.0), Vec3::X, 100.0, 1.0, 1.0, &mut out);
        out.iter().map(|v| v.pos).collect()
    }

    #[test]
    fn harness_and_goggles_change_the_happy_ghast_and_babies_shrink() {
        let mut m = Mob::new(MobKind::HappyGhast, DVec3::new(10.0, 64.0, 0.0), 0.0);
        let bare = mesh(&m);
        m.mount.as_mut().unwrap().slots[1] =
            Some(crate::inventory::Stack::new(crate::content::harness::harness(crate::color::DyeColor::Red), 1));
        let harnessed = mesh(&m);
        assert!(harnessed.len() > bare.len());
        m.happy_ghast.as_mut().unwrap().goggles_down = true;
        assert_ne!(mesh(&m), harnessed);
        let mut baby = Mob::new(MobKind::HappyGhast, DVec3::new(10.0, 64.0, 0.0), 0.0);
        baby.baby = true;
        let top = mesh(&baby).iter().fold(f32::MIN, |h, p| h.max(p[1]));
        assert!((top - 4.0 * crate::entity::happy_ghast::BABY_SCALE as f32).abs() < 0.05, "{top}");
    }

    #[test]
    fn nautilus_gear_and_zombie_variants_draw() {
        let mut m = Mob::new(MobKind::Nautilus, DVec3::new(10.0, 64.0, 0.0), 0.0);
        let bare = mesh(&m);
        let s = m.mount.as_mut().unwrap();
        s.tame = true;
        s.slots[0] = Some(crate::inventory::Stack::new(crate::item::Item::SADDLE, 1));
        s.slots[1] = Some(crate::inventory::Stack::new(crate::equipment::DIAMOND_NAUTILUS_ARMOR, 1));
        assert!(mesh(&m).len() > bare.len());
        let mut z = Mob::new(MobKind::ZombieNautilus, DVec3::new(10.0, 64.0, 0.0), 0.0);
        let temperate = {
            let mut out = EntityMesh::flat();
            build(std::slice::from_ref(&z), DVec3::new(0.0, 64.0, 0.0), Vec3::X, 100.0, 1.0, 1.0, &mut out);
            out.iter().map(|v| v.color).collect::<Vec<_>>()
        };
        z.mount.as_mut().unwrap().variant = 1;
        let mut out = EntityMesh::flat();
        build(std::slice::from_ref(&z), DVec3::new(0.0, 64.0, 0.0), Vec3::X, 100.0, 1.0, 1.0, &mut out);
        assert_ne!(out.iter().map(|v| v.color).collect::<Vec<_>>(), temperate);
    }
}
