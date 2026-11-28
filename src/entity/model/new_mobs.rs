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
const SADDLE: Rgb = [110, 66, 38];
/// Marker for worn nautilus armor; [`colour`] picks the material.
const NAUTILUS_ARMOR: Rgb = [3, 201, 3];

/// v1.0 gap Track 7d: the adult nautilus and zombie nautilus on
/// `NautilusModel.createBodyMesh`'s parts, facing as Java's do (owner,
/// 2026-11-27): the coiled shell leads, its flat face forward, and the
/// soft body and the mouth trail behind it, so a nautilus swims shell
/// first, as the baby (v1.0 gap Track 7c) already did. The saddle and
/// armor are `NautilusSaddleModel`'s and `NautilusArmorModel`'s shell
/// boxes grown by their deformations, and a warm zombie nautilus carries
/// `ZombieNautilusCoralModel`'s corals, hidden under armor. Colours,
/// stripes, eyes and the corals' look are this game's own.
mod adult {
    use std::f32::consts::{FRAC_PI_2, FRAC_PI_4};

    use super::super::keyframes::{Anim, Rot, Scale, ch, l};
    use super::super::rig::{Bone, ROOT, Rig, bone, jbox, jboxg};
    use super::*;

    pub const SHELL_PART: usize = 1;
    pub const BODY: usize = 2;
    pub const UPPER_MOUTH: usize = 3;
    pub const INNER_MOUTH: usize = 4;
    pub const LOWER_MOUTH: usize = 5;
    const SADDLE_PART: usize = 6;
    const ARMOR_PART: usize = 7;
    const CORALS: usize = 8;

    include!("keyframe_defs/nautilus.rs");

    const EYE: Rgb = [24, 24, 30];
    const GLINT: Rgb = [230, 228, 220];
    const YELLOW: Rgb = [226, 196, 64];
    const PINK: Rgb = [214, 108, 150];
    const BLUE: Rgb = [70, 100, 214];
    const RED: Rgb = [188, 50, 48];
    /// A coral fan: Java's flat `addBox` given a tenth of a pixel's depth.
    const fn fan(o: [f32; 3], s: [f32; 3], color: Rgb) -> Cuboid {
        let mut o = o;
        let mut s = s;
        let mut axis = 0;
        while axis < 3 {
            if s[axis] == 0.0 {
                o[axis] -= 0.05;
                s[axis] = 0.1;
            }
            axis += 1;
        }
        jbox(o, s, color, 0)
    }
    /// (Java writes the corals' quarter and eighth turns as 1.5708 and
    /// 0.7854.)
    static BONES: &[Bone] = &[
        bone(&[], ROOT, [0.0, 29.0, -6.0], [0.0; 3]),
        bone(
            &[
                jbox([-7.0, -10.0, -7.0], [14.0, 10.0, 16.0], SHELL, 22).with(Pattern::Mottle),
                jbox([-7.0, 0.0, -7.0], [14.0, 8.0, 20.0], SHELL, 22).with(Pattern::Mottle),
                // The coil's bands, as the baby's, on the shell's sides.
                jbox([-7.1, -7.5, 1.0], [14.2, 1.6, 8.0], STRIPE, 10),
                jbox([-7.1, -3.6, -3.0], [14.2, 1.4, 12.0], STRIPE, 10),
                jbox([-7.1, 2.4, 1.0], [14.2, 1.6, 12.0], STRIPE, 10),
                // The aperture's dark lip round the body, at the back.
                jbox([-6.0, 0.4, 12.95], [12.0, 7.2, 0.1], [92, 52, 40], 0),
            ],
            0,
            [0.0, -13.0, 5.0],
            [0.0; 3],
        ),
        bone(
            &[
                // Java's body box ends at its face plane (z 7), where the
                // mouth parts take over.
                jbox([-5.0, -4.51, -3.0], [10.0, 8.0, 10.0], SOFT, 18),
                jbox([-5.1, -2.6, 5.85], [0.1, 1.4, 1.0], EYE, 0),
                jbox([5.0, -2.6, 5.85], [0.1, 1.4, 1.0], EYE, 0),
                jbox([-5.15, -2.5, 5.95], [0.05, 0.5, 0.4], GLINT, 0),
                jbox([5.1, -2.5, 5.95], [0.05, 0.5, 0.4], GLINT, 0),
            ],
            0,
            [0.0, -8.5, 12.3],
            [0.0; 3],
        ),
        bone(&[jboxg([-5.0, -2.0, 0.0], [10.0, 4.0, 4.0], -0.001, SOFT, 16)], BODY as u8, [0.0, -2.51, 7.0], [0.0; 3]),
        bone(&[jbox([-3.0, -2.0, -0.5], [6.0, 4.0, 4.0], [150, 92, 84], 10)], BODY as u8, [0.0, -0.51, 7.5], [0.0; 3]),
        bone(&[jboxg([-5.0, -1.98, 0.0], [10.0, 4.0, 4.0], -0.001, SOFT, 16)], BODY as u8, [0.0, 1.49, 7.0], [0.0; 3]),
        // `NautilusSaddleModel`: the upper shell grown by 0.2, here a seat
        // on its top with straps down its sides.
        bone(
            &[
                jboxg([-5.0, -10.0, -4.5], [10.0, 0.0, 9.0], 0.2, SADDLE, 20),
                jboxg([-7.0, -10.0, -0.75], [0.0, 10.0, 1.5], 0.2, SADDLE, 10),
                jboxg([7.0, -10.0, -0.75], [0.0, 10.0, 1.5], 0.2, SADDLE, 10),
            ],
            SHELL_PART as u8,
            [0.0; 3],
            [0.0; 3],
        ),
        // `NautilusArmorModel`: the shell grown by a hundredth (here 0.15,
        // over the bands and under the saddle); plates over the upper shell
        // and the front of the lower.
        bone(
            &[
                jboxg([-7.0, -10.0, -7.0], [14.0, 10.0, 16.0], 0.15, NAUTILUS_ARMOR, 24),
                jboxg([-7.0, 0.5, -7.0], [14.0, 4.0, 16.0], 0.15, NAUTILUS_ARMOR, 24),
            ],
            SHELL_PART as u8,
            [0.0; 3],
            [0.0; 3],
        ),
        bone(&[], SHELL_PART as u8, [8.0, 4.5, -8.0], [0.0; 3]),
        bone(&[], CORALS as u8, [0.0, -11.0, 11.0], [0.0; 3]),
        bone(&[fan([-4.5, -3.5, 0.0], [6.0, 8.0, 0.0], YELLOW)], 9, [0.0, 0.0, 2.0], [0.0, -FRAC_PI_4, 0.0]),
        bone(&[fan([-4.5, -3.5, 0.0], [6.0, 8.0, 0.0], YELLOW)], 9, [0.0; 3], [0.0, FRAC_PI_4, 0.0]),
        bone(&[fan([-4.5, 4.5, 0.0], [6.0, 0.0, 8.0], PINK)], CORALS as u8, [-12.5, -18.0, 11.0], [0.0; 3]),
        bone(&[fan([-3.0, 0.0, -4.0], [6.0, 0.0, 8.0], PINK)], 12, [-1.5, 4.5, 4.0], [0.0, 0.0, FRAC_PI_2]),
        bone(&[], CORALS as u8, [-14.0, 0.0, 5.5], [0.0; 3]),
        bone(&[fan([-3.5, -5.5, 0.0], [5.0, 10.0, 0.0], BLUE)], 14, [0.0, 0.0, -2.0], [0.0, FRAC_PI_4, 0.0]),
        bone(&[fan([-3.5, -5.5, 0.0], [5.0, 10.0, 0.0], BLUE)], 14, [0.0; 3], [0.0, -FRAC_PI_4, 0.0]),
        bone(&[], CORALS as u8, [0.0; 3], [0.0; 3]),
        bone(&[fan([-2.5, -5.5, 0.0], [4.0, 10.0, 0.0], RED)], 17, [-0.5, -1.0, 1.5], [0.0, -0.829, 0.0]),
        bone(&[fan([-4.5, -5.5, 0.0], [6.0, 10.0, 0.0], RED)], 17, [0.0; 3], [0.0, FRAC_PI_4, 0.0]),
    ];

    /// `NautilusModel.setupAnim`: the body turns toward the look within 10
    /// degrees each way and `NautilusAnimation.SWIMMING` runs on the walk
    /// position plus a fifth of `ageInTicks` (`applyWalk(.., speed + 0.2, 2,
    /// 3)`), opening the mouth and pulsing the body (its scale keys).
    pub fn pose(m: &Mob, time: f32) -> Parts {
        let mut r = Rig::new(BONES);
        r.y_rot(BODY, m.head_yaw.to_degrees().clamp(-10.0, 10.0).to_radians());
        r.x_rot(BODY, (-m.head_pitch.to_degrees()).clamp(-10.0, 10.0).to_radians());
        SWIMMING.apply_walk(&mut r, m.limb_phase + age_ticks(time) / 5.0, m.limb_amp + 0.2, 2.0, 3.0);
        let s = m.mount.as_ref();
        let armored = s.is_some_and(|s| s.slots[1].is_some());
        r.shown[ARMOR_PART] = armored;
        r.shown[SADDLE_PART] = s.is_some_and(|s| s.saddled());
        // `ZombieNautilusCoralModel`: the warm variant's corals, hidden
        // under armor.
        r.shown[CORALS] = m.kind == MobKind::ZombieNautilus && s.is_some_and(|s| s.variant == 1) && !armored;
        r.parts()
    }
}

pub(super) fn nautilus(m: &Mob, time: f32) -> Parts {
    adult::pose(m, time)
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
        build(std::slice::from_ref(m), DVec3::new(0.0, 64.0, 0.0), Vec3::X, 100.0, 1.0, &mut out);
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
            build(std::slice::from_ref(&z), DVec3::new(0.0, 64.0, 0.0), Vec3::X, 100.0, 1.0, &mut out);
            out.iter().map(|v| v.color).collect::<Vec<_>>()
        };
        z.mount.as_mut().unwrap().variant = 1;
        let mut out = EntityMesh::flat();
        build(std::slice::from_ref(&z), DVec3::new(0.0, 64.0, 0.0), Vec3::X, 100.0, 1.0, &mut out);
        assert_ne!(out.iter().map(|v| v.color).collect::<Vec<_>>(), temperate);
    }

    /// v1.0 gap Track 7d: the adult nautilus leads with its shell and
    /// trails its body and mouth, as Java's (and the baby) do; its body
    /// pulses with `SWIMMING`'s scale keys; a warm zombie nautilus carries
    /// its corals unless armored.
    #[test]
    fn the_adult_nautilus_swims_shell_first() {
        let mut m = Mob::new(MobKind::Nautilus, DVec3::new(10.0, 64.0, 0.0), 0.0);
        let p = nautilus(&m, 0.0);
        assert_eq!(p.len(), 5, "shell, body and three mouth parts");
        // This game's models face +Z: the shell is ahead of the body.
        assert!(p[0].pivot.z > p[1].pivot.z + 5.0, "{} vs {}", p[0].pivot, p[1].pivot);
        // 250 ms into the swim at strength min((0 + 0.2) * 3, 1): the body
        // is a tenth longer, times 0.6.
        let p = nautilus(&m, 0.625);
        assert!((p[1].stretch.z_axis.z - 1.06).abs() < 1e-3, "{}", p[1].stretch);
        m.mount.as_mut().unwrap().slots[0] = Some(crate::inventory::Stack::new(crate::item::Item::SADDLE, 1));
        m.mount.as_mut().unwrap().tame = true;
        assert_eq!(nautilus(&m, 0.0).len(), 6, "the saddle");
        let mut z = Mob::new(MobKind::ZombieNautilus, DVec3::new(10.0, 64.0, 0.0), 0.0);
        assert_eq!(nautilus(&z, 0.0).len(), 5, "no corals on a temperate one");
        z.mount.as_mut().unwrap().variant = 1;
        assert_eq!(nautilus(&z, 0.0).len(), 13, "a warm one's eight coral fans");
        z.mount.as_mut().unwrap().slots[1] =
            Some(crate::inventory::Stack::new(crate::equipment::DIAMOND_NAUTILUS_ARMOR, 1));
        assert_eq!(nautilus(&z, 0.0).len(), 6, "armor hides the corals");
    }
}
