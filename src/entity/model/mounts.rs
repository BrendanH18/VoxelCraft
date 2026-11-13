//! Procedural Java-like equine proportions, coats, markings and equipment.
use super::{Cuboid, Mob, MobKind, Parts, cube, part};
use glam::Quat;
const COLORS: [[u8; 3]; 9] = [
    [236, 232, 218],
    [208, 177, 126],
    [159, 104, 57],
    [108, 66, 38],
    [40, 35, 31],
    [132, 126, 116],
    [72, 55, 43],
    [222, 218, 200],
    [94, 124, 68],
];
const fn bodies() -> [[Cuboid; 3]; 9] {
    let mut out = [[cube([0.; 3], [0.; 3], [0; 3], 0); 3]; 9];
    let mut i = 0;
    while i < 9 {
        out[i] = [
            cube([-5., 11., -10.], [5., 21., 9.], COLORS[i], 22),
            cube([-1.5, 14., -16.], [1.5, 21., -9.], [48, 38, 28], 20),
            cube([-1., 20., -9.], [1., 22., 10.], [44, 36, 26], 20),
        ];
        i += 1;
    }
    out
}
const BODIES: [[Cuboid; 3]; 9] = bodies();
const fn heads() -> [[Cuboid; 5]; 9] {
    let mut out = [[cube([0.; 3], [0.; 3], [0; 3], 0); 5]; 9];
    let mut i = 0;
    while i < 9 {
        out[i] = [
            cube([-2.5, -1., -1.], [2.5, 7., 4.], COLORS[i], 18),
            cube([-2.5, 4., 2.], [2.5, 9., 9.], COLORS[i], 18),
            cube([-2., 7., 1.], [-1., 11., 3.], COLORS[i], 14),
            cube([1., 7., 1.], [2., 11., 3.], COLORS[i], 14),
            cube([-2.6, 7., 5.], [2.6, 8., 6.], [30, 26, 22], 0),
        ];
        i += 1;
    }
    out
}
const HEADS: [[Cuboid; 5]; 9] = heads();
const fn legs() -> [[Cuboid; 2]; 9] {
    let mut out = [[cube([0.; 3], [0.; 3], [0; 3], 0); 2]; 9];
    let mut i = 0;
    while i < 9 {
        out[i] = [
            cube([-1.6, -11., -1.6], [1.6, 0., 1.6], COLORS[i], 18),
            cube([-1.8, -12., -1.8], [1.8, -9., 1.8], [50, 44, 38], 12),
        ];
        i += 1;
    }
    out
}
const LEGS: [[Cuboid; 2]; 9] = legs();
const EARS: &[Cuboid] =
    &[cube([-2., 10., 1.], [-1., 13., 3.], [110, 100, 86], 16), cube([1., 10., 1.], [2., 13., 3.], [110, 100, 86], 16)];
const CHESTS: &[Cuboid] =
    &[cube([-9., 10., -9.], [-5., 18., 0.], [130, 86, 40], 20), cube([5., 10., -9.], [9., 18., 0.], [130, 86, 40], 20)];
const SADDLE: &[Cuboid] = &[
    cube([-5.3, 20.7, -4.], [5.3, 22., 5.], [132, 72, 36], 18),
    cube([-6., 13., -1.], [-5.5, 21., 2.], [160, 146, 130], 8),
    cube([5.5, 13., -1.], [6., 21., 2.], [160, 146, 130], 8),
];
const PIG_SADDLE: &[Cuboid] = &[cube([-4.5, 13.8, -5.], [4.5, 15., 5.], [132, 72, 36], 16)];
const STRIDER_SADDLE: &[Cuboid] = &[cube([-6.3, 26., -6.3], [6.3, 27.5, 6.3], [132, 72, 36], 16)];
const MARKINGS: [[Cuboid; 2]; 4] = [
    [
        cube([-5.1, 13., -5.], [-5., 21., 5.], [240, 236, 224], 6),
        cube([5., 13., -5.], [5.1, 21., 5.], [240, 236, 224], 6),
    ],
    [
        cube([-5.1, 16., -10.], [-5., 21., 9.], [240, 236, 224], 12),
        cube([5., 16., -10.], [5.1, 21., 9.], [240, 236, 224], 12),
    ],
    [
        cube([-5.1, 16., -5.], [-5., 19., -1.], [240, 236, 224], 30),
        cube([5., 12., 3.], [5.1, 15., 8.], [240, 236, 224], 30),
    ],
    [cube([-5.1, 16., -5.], [-5., 19., -1.], [36, 30, 24], 30), cube([5., 12., 3.], [5.1, 15., 8.], [36, 30, 24], 30)],
];
/// Sentinel recoloured from the leather horse armor's `dyed_color` (else
/// `#A06540`) before emitting vertices, as `equipment/leather.json`'s
/// dyeable `horse_body` layer is tinted.
const LEATHER_HORSE: [u8; 3] = [1, 2, 4];
/// Leather, iron, gold, diamond, then copper and netherite (v0.17).
const fn armors() -> [[Cuboid; 3]; 6] {
    let colors = [LEATHER_HORSE, [210, 210, 215], [235, 194, 54], [60, 220, 210], [196, 112, 80], [80, 72, 78]];
    let mut out = [[cube([0.; 3], [0.; 3], [0; 3], 0); 3]; 6];
    let mut i = 0;
    while i < 6 {
        out[i] = [
            cube([-5.2, 11., -10.2], [-5., 20., 9.2], colors[i], 8),
            cube([5., 11., -10.2], [5.2, 20., 9.2], colors[i], 8),
            cube([-5.2, 21., -10.2], [5.2, 21.2, 9.2], colors[i], 8),
        ];
        i += 1;
    }
    out
}
const ARMOR: [[Cuboid; 3]; 6] = armors();
pub(super) fn pose(m: &Mob) -> Parts {
    let s = m.mount.as_ref().unwrap();
    let color = match m.kind {
        MobKind::SkeletonHorse => 7,
        MobKind::ZombieHorse => 8,
        MobKind::Horse => s.variant as usize,
        _ => 5,
    };
    let stride = m.limb_phase.sin() * m.limb_amp * 0.8;
    let head_y = match m.kind {
        MobKind::Donkey => 13.,
        MobKind::Mule => 14.,
        _ => 15.,
    };
    let head = Quat::from_rotation_y(-m.head_yaw) * Quat::from_rotation_x(-0.35 - m.head_pitch);
    let mut p = Parts::new();
    p.push(part(
        if m.kind == MobKind::SkeletonHorse { SKELETON_BODY } else { &BODIES[color] },
        [0.; 3],
        Quat::IDENTITY,
    ));
    for (x, z, phase) in [(-3.5, 7., stride), (3.5, 7., -stride), (-3.5, -8., -stride), (3.5, -8., stride)] {
        p.push(part(&LEGS[color], [x, 12., z], Quat::from_rotation_x(phase)));
    }
    p.push(part(&HEADS[color], [0., head_y, 6.], head));
    if matches!(m.kind, MobKind::Donkey | MobKind::Mule) {
        p.push(part(EARS, [0., head_y, 6.], head));
    }
    if s.marking > 0 && m.kind == MobKind::Horse {
        p.push(part(&MARKINGS[s.marking as usize - 1], [0.; 3], Quat::IDENTITY));
    }
    if s.chest {
        p.push(part(CHESTS, [0.; 3], Quat::IDENTITY));
    }
    equipment(m, &mut p);
    p
}
pub(super) fn equipment(m: &Mob, p: &mut Parts) {
    let Some(s) = m.mount.as_ref() else { return };
    if s.saddled() {
        p.push(part(
            match m.kind {
                MobKind::Pig => PIG_SADDLE,
                MobKind::Strider => STRIDER_SADDLE,
                _ => SADDLE,
            },
            [0.; 3],
            Quat::IDENTITY,
        ));
    }
    if let Some(stack) = s.slots[1] {
        use crate::item::ArmorMaterial;
        let index = match stack.item.body_armor() {
            Some((crate::equipment::BodyArmor::Horse, material)) => match material {
                ArmorMaterial::Leather => 0,
                ArmorMaterial::Iron => 1,
                ArmorMaterial::Gold => 2,
                ArmorMaterial::Diamond => 3,
                ArmorMaterial::Copper => 4,
                ArmorMaterial::Netherite => 5,
                _ => return,
            },
            _ => return,
        };
        p.push(part(&ARMOR[index], [0.; 3], Quat::IDENTITY));
    }
}

/// Llama coat colours: creamy, white, brown and gray.
const LLAMA_COATS: [[u8; 3]; 4] = [[221, 207, 178], [233, 233, 228], [128, 92, 60], [130, 128, 124]];
/// The trader llama's head and legs are drawn in this colour; wild llamas recolour it.
const LLAMA_FUR: [u8; 3] = [221, 207, 178];
/// Sentinel recoloured from the equipped carpet before emitting vertices.
const CARPET: [u8; 3] = [1, 2, 3];
const LY: f32 = super::trader_model::LLAMA_Y;
const LLAMA_BODY: &[Cuboid] = &[cube([-4.5, 14.0 * LY, -7.0], [4.5, 23.0 * LY, 7.0], LLAMA_FUR, 20)];
const LLAMA_DECOR: &[Cuboid] = &[
    cube([-4.7, 21.0 * LY, -7.2], [4.7, 23.3 * LY, 7.2], CARPET, 10),
    cube([-4.8, 15.5 * LY, -5.0], [-4.6, 21.0 * LY, 5.0], CARPET, 10),
    cube([4.6, 15.5 * LY, -5.0], [4.8, 21.0 * LY, 5.0], CARPET, 10),
];
const LLAMA_CHESTS: &[Cuboid] = &[
    cube([-7.5, 15.0 * LY, -6.0], [-4.5, 21.0 * LY, 2.0], [130, 86, 40], 20),
    cube([4.5, 15.0 * LY, -6.0], [7.5, 21.0 * LY, 2.0], [130, 86, 40], 20),
];
pub(super) fn llama(m: &Mob) -> Parts {
    use super::trader_model::{LLAMA_HEAD, LLAMA_LEG};
    let s = m.mount.as_ref();
    let swing = m.limb_phase.sin() * m.limb_amp * 0.9;
    let rx = Quat::from_rotation_x;
    let head = Quat::from_rotation_y(-m.head_yaw) * rx(-m.head_pitch);
    let mut p = Parts::new();
    p.push(part(LLAMA_BODY, [0.; 3], Quat::IDENTITY));
    p.push(part(LLAMA_HEAD, [0., 20. * LY, 5.], head));
    for (x, z, phase) in [(-3., 5., swing), (3., 5., -swing), (-3., -5., -swing), (3., -5., swing)] {
        p.push(part(LLAMA_LEG, [x, 14. * LY, z], rx(phase)));
    }
    if s.is_some_and(|s| s.slots[1].is_some()) {
        p.push(part(LLAMA_DECOR, [0.; 3], Quat::IDENTITY));
    }
    if s.is_some_and(|s| s.chest) {
        p.push(part(LLAMA_CHESTS, [0.; 3], Quat::IDENTITY));
    }
    p
}

const SAND: [u8; 3] = [219, 169, 95];
const CAMEL_BODY: &[Cuboid] = &[
    cube([-7.5, 20., -13.], [7.5, 30., 13.], SAND, 18),
    cube([-4.5, 30., -6.], [4.5, 35., 4.], [205, 152, 80], 18),
    cube([-1., 22., -14.], [1., 28., -13.], [160, 115, 60], 10),
];
const CAMEL_LEG: &[Cuboid] = &[
    cube([-1.5, -20., -1.5], [1.5, 0., 1.5], SAND, 16),
    cube([-1.8, -21., -1.8], [1.8, -18., 1.8], [130, 95, 55], 10),
];
const CAMEL_HEAD: &[Cuboid] = &[
    cube([-3., -2., -2.], [3., 13., 4.], SAND, 16),
    cube([-3.5, 9., 0.], [3.5, 15., 12.], SAND, 16),
    cube([-3.6, 12., 6.], [3.6, 13., 7.], [40, 32, 24], 0),
    cube([-4.5, 13., 1.], [-3.5, 14., 3.], [205, 152, 80], 10),
    cube([3.5, 13., 1.], [4.5, 14., 3.], [205, 152, 80], 10),
];
const CAMEL_SADDLE: &[Cuboid] = &[
    cube([-5., 35., -7.], [5., 36.2, 5.], [132, 72, 36], 16),
    cube([-7.7, 24., -2.], [-7.5, 35., 1.], [160, 146, 130], 8),
    cube([7.5, 24., -2.], [7.7, 35., 1.], [160, 146, 130], 8),
];
pub(super) fn camel(m: &Mob) -> Parts {
    let s = m.mount.as_ref();
    let sitting = s.is_some_and(|s| s.sit_ticks > 0);
    let swing = m.limb_phase.sin() * m.limb_amp * 0.7;
    let rx = Quat::from_rotation_x;
    let head = Quat::from_rotation_y(-m.head_yaw) * rx(-m.head_pitch);
    // Sitting: the body rests on folded legs.
    let drop = if sitting { -15. } else { 0. };
    let mut p = Parts::new();
    p.push(part(CAMEL_BODY, [0., drop, 0.], Quat::IDENTITY));
    p.push(part(CAMEL_HEAD, [0., 23. + drop, 12.], head));
    for (x, z, phase) in [(-4.5, 9., swing), (4.5, 9., -swing), (-4.5, -9., -swing), (4.5, -9., swing)] {
        if sitting {
            p.push(part(CAMEL_LEG, [x, 2., z + 9.], rx(std::f32::consts::FRAC_PI_2)));
        } else {
            p.push(part(CAMEL_LEG, [x, 21., z], rx(phase)));
        }
    }
    if s.is_some_and(|s| s.saddled()) {
        p.push(part(CAMEL_SADDLE, [0., drop, 0.], Quat::IDENTITY));
    }
    p
}

/// Llama coats and carpets, and dyed leather horse armor.
pub(super) fn colour(m: &Mob, c: &mut Cuboid) {
    let Some(s) = m.mount.as_ref() else { return };
    if c.color == LEATHER_HORSE {
        c.color = s.slots[1]
            .and_then(|stack| crate::inventory::components::dye_tint(&stack))
            .unwrap_or(crate::inventory::components::LEATHER_COLOR);
        return;
    }
    if m.kind != MobKind::Llama {
        return;
    }
    if c.color == LLAMA_FUR {
        c.color = LLAMA_COATS[s.variant as usize % 4];
    } else if c.color == CARPET {
        c.color = s.slots[1]
            .and_then(|stack| stack.item.block())
            .and_then(|b| b.carpet_color())
            .map_or([200, 200, 200], |d| d.sheep_rgb());
    }
}

/// Original exposed spine and ribs, in the same horse-shaped rig.
const SKELETON_BODY: &[Cuboid] = &[
    cube([-1., 19., -10.], [1., 21., 9.], [222, 218, 200], 12),
    cube([-4., 12., -8.], [-3., 20., -7.], [222, 218, 200], 12),
    cube([3., 12., -8.], [4., 20., -7.], [222, 218, 200], 12),
    cube([-4., 12., -3.], [-3., 20., -2.], [222, 218, 200], 12),
    cube([3., 12., -3.], [4., 20., -2.], [222, 218, 200], 12),
    cube([-4., 12., 2.], [-3., 20., 3.], [222, 218, 200], 12),
    cube([3., 12., 2.], [4., 20., 3.], [222, 218, 200], 12),
    cube([-4., 12., 7.], [4., 14., 8.], [222, 218, 200], 12),
    cube([-1., 14., -16.], [1., 20., -9.], [222, 218, 200], 12),
];

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{color::DyeColor, entity::Rng, inventory::Stack, item::Item, world::block::Block};

    /// The leather horse armor layer is drawn in the stack's `dyed_color`,
    /// `#A06540` undyed, and survives a save of the horse's slots.
    #[test]
    fn leather_horse_armor_is_drawn_in_its_dyed_colour() {
        use crate::inventory::components::{LEATHER_COLOR, apply_dyes};
        let mut mob = Mob::new(MobKind::Horse, glam::DVec3::ZERO, 0.);
        mob.mount = Some(crate::entity::mounts::State::new(MobKind::Horse, &mut Rng::new(1)).into());
        mob.mount.as_mut().unwrap().tame = true;
        let plain = Stack::new(Item::LEATHER_HORSE_ARMOR, 1);
        for (stack, want) in [
            (plain, LEATHER_COLOR),
            (apply_dyes(plain, &[DyeColor::Lime]).unwrap(), DyeColor::Lime.rgb()),
            (apply_dyes(plain, &[DyeColor::Red, DyeColor::Blue]).unwrap(), [173, 83, 152]),
        ] {
            mob.mount.as_mut().unwrap().slots[1] = Some(stack);
            mob.mount = Some(
                crate::entity::mounts::State::load(MobKind::Horse, &mob.mount.as_ref().unwrap().save()).unwrap().into(),
            );
            let parts = pose(&mob);
            assert!(parts.iter().any(|p| p.boxes.iter().any(|c| c.color == LEATHER_HORSE)));
            for source in ARMOR[0] {
                let mut emitted = source;
                colour(&mob, &mut emitted);
                assert_eq!(emitted.color, want);
            }
        }
        // Other horse armor keeps its own colour.
        mob.mount.as_mut().unwrap().slots[1] = Some(Stack::new(Item::IRON_HORSE_ARMOR, 1));
        let mut iron = ARMOR[1][0];
        colour(&mob, &mut iron);
        assert_eq!(iron.color, ARMOR[1][0].color);
    }

    #[test]
    fn every_llama_carpet_recolours_the_sentinel_after_equipment_reload() {
        let mut mob = Mob::new(MobKind::Llama, glam::DVec3::ZERO, 0.);
        mob.mount = Some(crate::entity::mounts::State::new(MobKind::Llama, &mut Rng::new(1)).into());
        mob.mount.as_mut().unwrap().tame = true;
        for dye in DyeColor::ALL {
            mob.mount.as_mut().unwrap().slots[1] = Some(Stack::new(Block::carpet(dye), 1));
            mob.mount = Some(
                crate::entity::mounts::State::load(MobKind::Llama, &mob.mount.as_ref().unwrap().save()).unwrap().into(),
            );
            let parts = llama(&mob);
            assert!(parts.iter().any(|p| std::ptr::eq(p.boxes, LLAMA_DECOR)));
            for &source in LLAMA_DECOR {
                let mut emitted = source;
                colour(&mob, &mut emitted);
                assert_eq!(emitted.color, dye.sheep_rgb());
                assert_ne!(emitted.color, CARPET);
            }
        }
    }
}
