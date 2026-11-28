//! v1.0 gap Track 7e: villagers and zombie villagers as Java 26.3 draws
//! them. The adults stand on `VillagerModel`'s and `ZombieVillagerModel`'s
//! part trees (a 10-pixel head with its hanging nose, a 20-pixel robe over
//! the body, the villager's folded `arms` and the zombie's humanoid arms),
//! the villager scaled by `LayerDefinitions`' 0.9375 (`villagerLikeScale`).
//! `VillagerProfessionLayer` dresses them in layers: the biome type's outfit
//! (`type/<type>` for adults, `baby/<type>` for babies and the zombie
//! villager cub), then for adults with a job the profession's (none for
//! babies) and, unless a nitwit, the level's badge (stone, iron, gold,
//! emerald, diamond). The type's headwear is drawn only while
//! `typeHatVisible` holds: the profession's hat (the `villager.hat`
//! metadata of its texture: full for farmers, fishermen, fletchers,
//! librarians and shepherds, partial for butchers) is none, or partial
//! while the type's is not full (full for desert and snow villagers; the
//! zombie villager's type textures carry no metadata). The outfits are
//! this game's own boxes and colours, in the established style; the robe,
//! trim, belt, legs and accent colours are keys that [`colour`] replaces
//! with the type's palette (faded and greened for zombie villagers). No
//! Mojang geometry file, texture or UV is used.

use std::sync::LazyLock;

use super::rig::{Bone, ROOT, Rig, bone, face, jbox, jboxg};
use super::*;
use crate::entity::villager::Profession;

/// Java's seven `VillagerType`s, in this game's `map_variant` names.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) enum Type {
    Desert,
    Jungle,
    Plains,
    Savanna,
    Snow,
    Swamp,
    Taiga,
}

impl Type {
    pub const ALL: [Type; 7] =
        [Type::Desert, Type::Jungle, Type::Plains, Type::Savanna, Type::Snow, Type::Swamp, Type::Taiga];

    /// The type a `map_variant` names; `plains` for anything else, as
    /// `VillagerType` defaults.
    pub fn from_name(name: &str) -> Type {
        match name {
            "desert" => Type::Desert,
            "jungle" => Type::Jungle,
            "savanna" => Type::Savanna,
            "snow" => Type::Snow,
            "swamp" => Type::Swamp,
            "taiga" => Type::Taiga,
            _ => Type::Plains,
        }
    }
}

/// A villager-like mob's type (its biome's, `plains` until known).
pub(super) fn type_of(m: &Mob) -> Type {
    m.villager.as_ref().map_or(Type::Plains, |v| Type::from_name(v.variant()))
}

/// `VillagerMetadataSection.Hat`.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(super) enum Hat {
    None,
    Partial,
    Full,
}

/// The `villager.hat` metadata of `type/<type>.png`: only the villager's
/// desert and snow textures have any (`full`); the zombie villager's type
/// textures have none.
pub(super) fn type_hat(t: Type, zombie: bool) -> Hat {
    if !zombie && matches!(t, Type::Desert | Type::Snow) { Hat::Full } else { Hat::None }
}

/// The `villager.hat` metadata of `profession/<profession>.png`, the same
/// for villagers and zombie villagers.
pub(super) fn profession_hat(p: Profession) -> Hat {
    use Profession::*;
    match p {
        Farmer | Fisherman | Fletcher | Librarian | Shepherd => Hat::Full,
        Butcher => Hat::Partial,
        _ => Hat::None,
    }
}

/// `VillagerProfessionLayer`'s `typeHatVisible`.
pub(super) fn type_hat_visible(t: Type, p: Profession, zombie: bool) -> bool {
    let ph = profession_hat(p);
    ph == Hat::None || ph == Hat::Partial && type_hat(t, zombie) != Hat::Full
}

// ---------------------------------------------------------------- colours

/// Colour keys the outfit boxes carry, replaced by [`colour`].
pub(super) const ROBE: Rgb = [3, 1, 201];
pub(super) const TRIM: Rgb = [3, 1, 202];
pub(super) const BELT: Rgb = [3, 1, 203];
pub(super) const LEGS: Rgb = [3, 1, 204];
pub(super) const ACCENT: Rgb = [3, 1, 205];
pub(super) const ACCENT2: Rgb = [3, 1, 206];

/// The villager's skin and the zombie villager's.
pub(super) const SKIN: Rgb = [177, 131, 101];
pub(super) const NOSE: Rgb = [162, 117, 86];
pub(super) const ZOMBIE_SKIN: Rgb = [92, 148, 78];
const ZOMBIE_NOSE: Rgb = [70, 120, 58];

/// Robe, trim, belt, legs, accent and second accent of each type, in
/// [`Type::ALL`]'s order. Plains keeps the brown robe villagers had.
const PALETTES: [[Rgb; 6]; 7] = [
    // Desert: a sand robe with terracotta trim, a cream head wrap.
    [[208, 168, 106], [176, 76, 50], [132, 56, 40], [150, 114, 72], [232, 214, 168], [62, 156, 168]],
    // Jungle: leaf green over bark brown, bright leaves.
    [[78, 108, 50], [122, 88, 46], [92, 64, 34], [86, 62, 40], [118, 164, 58], [214, 92, 120]],
    // Plains: the brown robe, darker hem and belt.
    [[114, 78, 50], [74, 50, 33], [66, 44, 28], [68, 49, 36], [98, 66, 44], [150, 110, 64]],
    // Savanna: rust orange with yellow trim and a red band.
    [[176, 94, 50], [214, 178, 72], [110, 58, 36], [120, 82, 54], [190, 58, 44], [232, 220, 196]],
    // Snow: a blue coat with white fur and a red knitted crown.
    [[72, 94, 142], [236, 236, 238], [60, 46, 40], [88, 76, 70], [176, 54, 52], [236, 236, 238]],
    // Swamp: murky olive with purple trim and moss.
    [[78, 86, 62], [106, 72, 112], [52, 58, 40], [62, 58, 46], [92, 136, 58], [64, 92, 44]],
    // Taiga: dark brown with a tan fur shawl.
    [[94, 66, 44], [152, 120, 86], [56, 40, 28], [70, 52, 38], [148, 116, 84], [120, 92, 64]],
];

/// The wandering trader's blue robe and gold belt (its own texture, not a
/// type layer).
const TRADER: [Rgb; 6] = [[35, 70, 151], [51, 96, 176], [218, 173, 67], [40, 58, 112], [35, 70, 151], [158, 115, 83]];

fn key(c: Rgb) -> Option<usize> {
    (c[0] == 3 && c[1] == 1 && (201..=206).contains(&c[2])).then(|| usize::from(c[2] - 201))
}

/// A zombie villager's rotted copy of a colour: a third of the way to a
/// grey green, a little darker.
fn rotted(c: Rgb) -> Rgb {
    const ROT: [f32; 3] = [84.0, 104.0, 66.0];
    std::array::from_fn(|i| ((f32::from(c[i]) * 0.68 + ROT[i] * 0.32) * 0.9) as u8)
}

/// The outfit's key colours replaced by the mob's palette.
pub(super) fn colour(m: &Mob, c: &mut Cuboid) {
    let Some(i) = key(c.color) else { return };
    c.color = match m.kind {
        MobKind::WanderingTrader => TRADER[i],
        MobKind::ZombieVillager => rotted(PALETTES[type_of(m) as usize][i]),
        _ => PALETTES[type_of(m) as usize][i],
    };
}

// ---------------------------------------------------------------- the adults

/// `VillagerModel`'s parts, in [`villager`]'s output order.
pub(super) const HEAD: usize = 0;
pub(super) const BODY: usize = 2;
pub(super) const ARMS: usize = 3;
const RIGHT_LEG: usize = 4;
const LEFT_LEG: usize = 5;

/// The 8 x 10 x 8 head: green eyes with whites under a unibrow (this
/// game's face, set just under the hat's brim line).
const VILLAGER_HEAD: &[Cuboid] = &[
    jbox([-4.0, -10.0, -4.0], [8.0, 10.0, 8.0], SKIN, 24).with(Pattern::Hide),
    face([-3.0, -2.0], [-4.0, -3.0], -4.0, 0.1, [236, 232, 224]),
    face([-2.0, -1.0], [-4.0, -3.0], -4.0, 0.12, [49, 103, 43]),
    face([1.0, 2.0], [-4.0, -3.0], -4.0, 0.12, [49, 103, 43]),
    face([2.0, 3.0], [-4.0, -3.0], -4.0, 0.1, [236, 232, 224]),
    face([-3.0, 3.0], [-5.0, -4.2], -4.0, 0.1, [62, 43, 28]),
];
const VILLAGER_NOSE: &[Cuboid] = &[jbox([-1.0, -1.0, -6.0], [2.0, 4.0, 2.0], NOSE, 22).with(Pattern::Hide)];
/// The robe (`jacket`, 20 pixels from the shoulders, grown by 0.5) with a
/// darker hem and a belt; the body under it never shows.
const VILLAGER_ROBE: &[Cuboid] = &[
    jboxg([-4.0, 0.0, -3.0], [8.0, 20.0, 6.0], 0.5, ROBE, 30).with(Pattern::Cloth),
    jboxg([-4.0, 18.0, -3.0], [8.0, 2.5, 6.0], 0.55, TRIM, 12),
    jboxg([-4.0, 10.0, -3.0], [8.0, 1.0, 6.0], 0.53, BELT, 10),
];
/// The folded arms: two sleeves and the hands across between them.
const VILLAGER_ARMS: &[Cuboid] = &[
    jbox([-8.0, -2.0, -2.0], [4.0, 8.0, 4.0], ROBE, 28).with(Pattern::Cloth),
    jbox([4.0, -2.0, -2.0], [4.0, 8.0, 4.0], ROBE, 28).with(Pattern::Cloth),
    jbox([-4.0, 2.0, -2.0], [8.0, 4.0, 4.0], SKIN, 24).with(Pattern::Hide),
    // Cuffs where the hands come out.
    jboxg([-8.0, 4.5, -2.0], [4.0, 1.5, 4.0], 0.06, TRIM, 10),
    jboxg([4.0, 4.5, -2.0], [4.0, 1.5, 4.0], 0.06, TRIM, 10),
];
const VILLAGER_LEG: &[Cuboid] = &[jbox([-2.0, 0.0, -2.0], [4.0, 12.0, 4.0], LEGS, 28).with(Pattern::Cloth)];

static VILLAGER_BONES: &[Bone] = &[
    bone(VILLAGER_HEAD, ROOT, [0.0; 3], [0.0; 3]),
    bone(VILLAGER_NOSE, HEAD as u8, [0.0, -2.0, 0.0], [0.0; 3]),
    bone(VILLAGER_ROBE, ROOT, [0.0; 3], [0.0; 3]),
    bone(VILLAGER_ARMS, ROOT, [0.0, 3.0, -1.0], [-0.75, 0.0, 0.0]),
    bone(VILLAGER_LEG, ROOT, [-2.0, 12.0, 0.0], [0.0; 3]),
    bone(VILLAGER_LEG, ROOT, [2.0, 12.0, 0.0], [0.0; 3]),
];

/// `villagerLikeScale`: the villager's and wandering trader's meshes drawn
/// at 0.9375 about their feet.
pub(super) const VILLAGER_SCALE: f32 = 0.9375;

/// `VillagerModel.setupAnim`'s head: the look, or (v1.0 gap Track 7f)
/// while the villager is unhappy (`isUnhappy`, its `unhappyCounter`
/// above 0) the shake: `zRot = 0.3 * sin(0.45 * ageInTicks)` with the
/// head bowed to `xRot = 0.4`. Java's rotations; children too, as
/// `BabyVillagerModel` runs the same `setupAnim`.
pub(super) fn head_rotation(m: &Mob, time: f32) -> [f32; 3] {
    if m.villager.as_ref().is_some_and(|v| v.unhappy > 0) {
        [0.4, m.head_yaw, 0.3 * (0.45 * age_ticks(time)).sin()]
    } else {
        [head_x(m), m.head_yaw, 0.0]
    }
}

/// `VillagerModel.setupAnim`: the head looks (or shakes, unhappy), the
/// legs walk at half the humanoid stride, the arms stay folded. Then the
/// outfit layers.
pub(super) fn villager(m: &Mob, time: f32) -> Parts {
    let mut r = Rig::new(VILLAGER_BONES);
    r.rot[HEAD] = head_rotation(m, time);
    let leg = walk_cos(m) * motion::LEG_SWING * 0.5;
    r.x_rot(RIGHT_LEG, leg);
    r.x_rot(LEFT_LEG, -leg);
    let mut p = r.parts();
    let (head, body) = (p[HEAD], p[BODY]);
    if m.kind == MobKind::WanderingTrader {
        p.push(Part { boxes: TRADER_HOOD, ..head });
        p.push(Part { boxes: TRADER_PANEL, ..body });
    } else {
        let arms = [p[ARMS]];
        dress(m, &mut p, head, body, &arms, false);
    }
    p
}

// The zombie villager: a humanoid with the villager's head and robe.
const ZV_BODY: usize = 0;
const ZV_LEFT_LEG: usize = 1;
const ZV_RIGHT_LEG: usize = 2;
const ZV_LEFT_ARM: usize = 3;
const ZV_RIGHT_ARM: usize = 4;
const ZV_HEAD: usize = 5;

const ZV_HEAD_BOXES: &[Cuboid] = &[
    jbox([-4.0, -10.0, -4.0], [8.0, 10.0, 8.0], ZOMBIE_SKIN, 24).with(Pattern::Rot),
    jbox([-1.0, -3.0, -6.0], [2.0, 4.0, 2.0], ZOMBIE_NOSE, 22).with(Pattern::Rot),
    // Red eyes under a dark brow.
    face([-3.0, -1.0], [-4.0, -3.0], -4.0, 0.1, [160, 40, 36]),
    face([1.0, 3.0], [-4.0, -3.0], -4.0, 0.1, [160, 40, 36]),
    face([-3.0, 3.0], [-5.0, -4.2], -4.0, 0.1, [40, 28, 22]),
];
/// The robe grown by 0.05 (`ZombieVillagerModel`'s jacket), hem and belt.
const ZV_ROBE: &[Cuboid] = &[
    jboxg([-4.0, 0.0, -3.0], [8.0, 20.0, 6.0], 0.05, ROBE, 30).with(Pattern::Cloth),
    jboxg([-4.0, 18.0, -3.0], [8.0, 2.0, 6.0], 0.1, TRIM, 12),
    jboxg([-4.0, 10.0, -3.0], [8.0, 1.0, 6.0], 0.08, BELT, 10),
];
/// Humanoid arms: a sleeve with the rotted hand out of it.
const ZV_RIGHT_ARM_BOXES: &[Cuboid] = &[
    jbox([-3.0, -2.0, -2.0], [4.0, 8.0, 4.0], ROBE, 28).with(Pattern::Cloth),
    jbox([-3.0, 6.0, -2.0], [4.0, 4.0, 4.0], ZOMBIE_SKIN, 24).with(Pattern::Rot),
];
const ZV_LEFT_ARM_BOXES: &[Cuboid] = &[
    jbox([-1.0, -2.0, -2.0], [4.0, 8.0, 4.0], ROBE, 28).with(Pattern::Cloth),
    jbox([-1.0, 6.0, -2.0], [4.0, 4.0, 4.0], ZOMBIE_SKIN, 24).with(Pattern::Rot),
];

static ZV_BONES: &[Bone] = &[
    bone(ZV_ROBE, ROOT, [0.0; 3], [0.0; 3]),
    bone(VILLAGER_LEG, ROOT, [2.0, 12.0, 0.0], [0.0; 3]),
    bone(VILLAGER_LEG, ROOT, [-2.0, 12.0, 0.0], [0.0; 3]),
    bone(ZV_LEFT_ARM_BOXES, ROOT, [5.0, 2.0, 0.0], [0.0; 3]),
    bone(ZV_RIGHT_ARM_BOXES, ROOT, [-5.0, 2.0, 0.0], [0.0; 3]),
    bone(ZV_HEAD_BOXES, ROOT, [0.0; 3], [0.0; 3]),
];

/// `ZombieVillagerModel`, a `HumanoidModel`: the walk at full stride,
/// `animateZombieArms` and the idle bob, shaking while it is cured. The
/// parts are in the humanoid order the armor reads (body, left and right
/// leg, left and right arm, head), then the outfit layers.
pub(super) fn zombie_villager(m: &Mob, time: f32) -> Parts {
    let mut r = Rig::new(ZV_BONES);
    r.look(ZV_HEAD, m);
    let h = zombie_limbs(m, time, m.equipment.get(crate::entity::equipment::Slot::MainHand).is_none());
    r.y_rot(ZV_BODY, h.body_y);
    r.x_rot(ZV_LEFT_LEG, h.left_leg);
    r.x_rot(ZV_RIGHT_LEG, h.right_leg);
    r.rot[ZV_LEFT_ARM] = [h.left_arm.x, h.left_arm.y, h.left_arm.z];
    r.rot[ZV_RIGHT_ARM] = [h.right_arm.x, h.right_arm.y, h.right_arm.z];
    let mut p = r.parts();
    let (head, body) = (p[ZV_HEAD], p[ZV_BODY]);
    let arms = [p[ZV_RIGHT_ARM], p[ZV_LEFT_ARM]];
    dress(m, &mut p, head, body, &arms, true);
    if m.convert_left > 0.0 {
        p.transform(Vec3::new((time * 18.0).sin() * 1.5, 0.0, 0.0), Quat::IDENTITY);
    }
    p
}

/// `HumanoidArmorLayer` with `ZOMBIE_VILLAGER_ARMOR`
/// (`ZombieVillagerModel.createArmorLayerSet`): the humanoid's armor with
/// the helmet on the villager's taller head (Java's 8-pixel helmet box at
/// its top, from 2 to 10 pixels above the neck), the body's and legs'
/// boxes grown a further 0.1, on [`zombie_villager`]'s parts.
pub(super) fn zombie_villager_armor(parts: &[Part]) -> [crate::entity::player_model::Limb; 6] {
    let at = |p: &Part, min: [f32; 3], max: [f32; 3], uv: [f32; 2]| crate::entity::player_model::Limb {
        pivot: p.pivot,
        rot: p.rot,
        min,
        max,
        uv,
    };
    let leg = |p: &Part, uv| at(p, [-2.1, -12.1, -2.1], [2.1, 0.1, 2.1], uv);
    [
        at(&parts[ZV_HEAD], [-4.0, 2.0, -4.0], [4.0, 10.0, 4.0], [0.0, 0.0]),
        at(&parts[ZV_BODY], [-4.1, -12.1, -2.1], [4.1, 0.1, 2.1], [16.0, 16.0]),
        at(&parts[ZV_RIGHT_ARM], [-3.0, -10.0, -2.0], [1.0, 2.0, 2.0], [40.0, 16.0]),
        at(&parts[ZV_LEFT_ARM], [-1.0, -10.0, -2.0], [3.0, 2.0, 2.0], [32.0, 48.0]),
        leg(&parts[ZV_RIGHT_LEG], [0.0, 16.0]),
        leg(&parts[ZV_LEFT_LEG], [16.0, 48.0]),
    ]
}

/// `VillagerProfessionLayer` on an adult: the type's accents and (while
/// `typeHatVisible`) headwear, then a job's profession layer (v1.0 gap
/// Track 7f: its clothes on the robe, face, hat and arms; `arms` are the
/// villager's folded arms or the zombie's right and left arm) and, for all
/// but nitwits, the level badge. A profession that repaints the whole
/// robe (the cleric's and nitwit's) covers the type's touches on it, as
/// its texture's pixels cover the type's.
fn dress(m: &Mob, p: &mut Parts, head: Part, body: Part, arms: &[Part], zombie: bool) {
    let t = type_of(m);
    let (profession, level) = m.villager.as_ref().map_or((Profession::None, 1), |v| (v.profession, v.level));
    let fit = if zombie { &ZOMBIE_FIT } else { &VILLAGER_FIT };
    let mut push = |boxes: &'static [Cuboid], at: Part| {
        if !boxes.is_empty() {
            p.push(Part { boxes, ..at });
        }
    };
    if !covers_robe(profession) {
        push(&fit.accents[t as usize], body);
    }
    if type_hat_visible(t, profession, zombie) {
        push(&ADULT_HATS[t as usize], head);
    }
    if profession == Profession::None {
        return;
    }
    let j = profession as usize;
    push(&fit.jobs[j], body);
    push(&fit.faces[j], head);
    push(&fit.hats[j], head);
    for (boxes, arm) in fit.sleeves[j].iter().zip(arms) {
        push(boxes, *arm);
    }
    if profession != Profession::Nitwit {
        push(&fit.badges[usize::from(level.clamp(1, 5) - 1)], body);
    }
}

// ---------------------------------------------------------------- the layers

/// Box helper in this game's axes, relative to a part's pivot.
const fn b(min: [f32; 3], max: [f32; 3], color: Rgb, noise: u8) -> Cuboid {
    cube(min, max, color, noise)
}

/// The robe's outer faces (relative to the body's pivot at the neck): half
/// width, half depth and the top and bottom of the robe.
/// `g` is the robe's growth over the 8 x 20 x 6 body; zombie villagers
/// also have humanoid arms where villagers fold theirs.
#[derive(Clone, Copy)]
struct Robe {
    hx: f32,
    hz: f32,
    top: f32,
    bottom: f32,
    g: f32,
    zombie: bool,
}

const VILLAGER_ROBE_SIZE: Robe = Robe { hx: 4.5, hz: 3.5, top: 0.5, bottom: -20.5, g: 0.5, zombie: false };
const ZOMBIE_ROBE_SIZE: Robe = Robe { hx: 4.05, hz: 3.05, top: 0.05, bottom: -20.05, g: 0.05, zombie: true };

/// A robe's layers for every type, profession and level: the type's
/// touches, each profession's clothes on the robe (`jobs`), its face
/// pieces and hat on the head and its sleeves or gloves on each arm part,
/// and the level badges. Zombie villagers wear the professions' clothes
/// rotted, as their own `profession/` textures fade them.
struct Fit {
    accents: [Vec<Cuboid>; 7],
    jobs: [Vec<Cuboid>; 15],
    faces: [Vec<Cuboid>; 15],
    hats: [Vec<Cuboid>; 15],
    sleeves: [Vec<Vec<Cuboid>>; 15],
    badges: [Vec<Cuboid>; 5],
}

static VILLAGER_FIT: LazyLock<Fit> = LazyLock::new(|| fit(&VILLAGER_ROBE_SIZE));
static ZOMBIE_FIT: LazyLock<Fit> = LazyLock::new(|| fit(&ZOMBIE_ROBE_SIZE));

fn fit(r: &Robe) -> Fit {
    let rot = |v: Vec<Cuboid>| -> Vec<Cuboid> {
        if !r.zombie {
            return v;
        }
        v.into_iter().map(|c| if key(c.color).is_some() { c } else { Cuboid { color: rotted(c.color), ..c } }).collect()
    };
    let each = |f: &dyn Fn(Profession) -> Vec<Cuboid>| std::array::from_fn(|i| rot(f(Profession::ALL[i])));
    Fit {
        accents: Type::ALL.map(|t| accents(t, r)),
        jobs: each(&|p| job(p, r)),
        faces: each(&face_pieces),
        hats: each(&profession_hat_boxes),
        sleeves: std::array::from_fn(|i| sleeves(Profession::ALL[i], r).into_iter().map(rot).collect()),
        badges: std::array::from_fn(|i| badge(i, r)),
    }
}

/// Each type's own touches on the robe.
fn accents(t: Type, r: &Robe) -> Vec<Cuboid> {
    let Robe { hx, hz, top, bottom, .. } = *r;
    match t {
        Type::Plains => vec![],
        // A sash down the front and a coloured collar.
        Type::Desert => vec![
            b([-1.0, bottom + 2.5, hz], [1.0, -1.0, hz + 0.06], TRIM, 10),
            b([-hx - 0.05, -1.5, -hz - 0.05], [hx + 0.05, top + 0.05, hz + 0.05], TRIM, 10),
        ],
        // Leaf pads on the shoulders and a leafy hem.
        Type::Jungle => vec![
            b([-hx - 0.3, -2.5, -hz - 0.3], [-hx + 3.0, top + 0.3, hz + 0.3], ACCENT, 24).with(Pattern::Mottle),
            b([hx - 3.0, -2.5, -hz - 0.3], [hx + 0.3, top + 0.3, hz + 0.3], ACCENT, 24).with(Pattern::Mottle),
            b([-hx - 0.1, bottom + 2.5, hz], [hx + 0.1, bottom + 3.5, hz + 0.1], ACCENT, 18),
        ],
        // A beaded collar and red bands at the hem.
        Type::Savanna => vec![
            b([-hx - 0.05, -2.0, -hz - 0.05], [hx + 0.05, -1.0, hz + 0.05], TRIM, 8),
            b([-2.5, -3.2, hz], [-1.5, -2.2, hz + 0.1], ACCENT, 0),
            b([-0.5, -3.2, hz], [0.5, -2.2, hz + 0.1], ACCENT, 0),
            b([1.5, -3.2, hz], [2.5, -3.2 + 1.0, hz + 0.1], ACCENT, 0),
            b([-hx - 0.1, bottom + 3.5, -hz - 0.1], [hx + 0.1, bottom + 4.3, hz + 0.1], ACCENT, 10),
        ],
        // A thick fur collar and fur at the hem and cuffs of the coat.
        Type::Snow => vec![
            b([-hx - 0.5, -2.5, -hz - 0.5], [hx + 0.5, top + 0.6, hz + 0.5], TRIM, 18).with(Pattern::Wool),
            b([-hx - 0.3, bottom - 0.1, -hz - 0.3], [hx + 0.3, bottom + 2.6, hz + 0.3], TRIM, 18).with(Pattern::Wool),
            b([-0.6, -9.0, hz], [0.6, -3.0, hz + 0.08], BELT, 8),
        ],
        // Moss hanging off the shoulders and patches on the robe.
        Type::Swamp => vec![
            b([-hx - 0.2, -3.0, -hz - 0.2], [hx + 0.2, top + 0.2, hz + 0.2], ACCENT2, 26).with(Pattern::Mottle),
            b([-3.5, -16.0, hz], [-1.0, -13.0, hz + 0.08], ACCENT, 20),
            b([1.0, -7.0, hz], [3.5, -5.0, hz + 0.08], ACCENT, 20),
            b([-2.0, -14.0, -hz - 0.08], [2.5, -10.0, -hz], ACCENT, 20),
        ],
        // A fur shawl over the shoulders and back.
        Type::Taiga => vec![
            b([-hx - 0.4, -4.0, -hz - 0.4], [hx + 0.4, top + 0.5, hz + 0.4], ACCENT, 20).with(Pattern::Fur),
            b([-hx - 0.3, -8.0, -hz - 0.4], [hx + 0.3, -4.0, -hz + 1.0], ACCENT, 20).with(Pattern::Fur),
        ],
    }
}

/// The professions whose clothes repaint the whole robe and sleeves (the
/// cleric's vestments, the nitwit's green coat), covering the type's
/// touches on it.
fn covers_robe(p: Profession) -> bool {
    matches!(p, Profession::Cleric | Profession::Nitwit)
}

/// v1.0 gap Track 7f: each profession's clothes on the robe
/// (`profession/<profession>`), this game's own art after the
/// professions' looks: aprons, tabards, vests and vestments with their
/// pockets, tools and emblems. Depths are out of the robe's front (`d0`
/// to `d1`), under the level badge (from 0.1 out).
#[allow(clippy::too_many_lines)]
fn job(p: Profession, r: &Robe) -> Vec<Cuboid> {
    use Profession::*;
    let Robe { hx, hz, top, bottom, g, zombie } = *r;
    // A box on the robe's front, and one on its back.
    let f = |x: [f32; 2], y: [f32; 2], d: [f32; 2], c: Rgb, n: u8| {
        b([x[0], y[0], hz + d[0]], [x[1], y[1], hz + d[1]], c, n)
    };
    let k = |x: [f32; 2], y: [f32; 2], d: [f32; 2], c: Rgb, n: u8| {
        b([x[0], y[0], -hz - d[1]], [x[1], y[1], -hz - d[0]], c, n)
    };
    // The apron's lowest line, just above the hem.
    let lo = bottom + 3.0;
    const APRON: [f32; 2] = [0.04, 0.1];
    const DETAIL: [f32; 2] = [0.1, 0.16];
    // The robe, hem and belt repainted (grown over the type's own boxes).
    let repaint = |robe: Rgb, hem: Rgb, belt: Rgb| {
        vec![
            jboxg([-4.0, 0.0, -3.0], [8.0, 20.0, 6.0], g + 0.04, robe, 30).with(Pattern::Cloth),
            jboxg([-4.0, 18.0, -3.0], [8.0, if zombie { 2.0 } else { 2.5 }, 6.0], g + 0.09, hem, 12),
            jboxg([-4.0, 10.0, -3.0], [8.0, 1.0, 6.0], g + 0.07, belt, 10),
        ]
    };
    match p {
        None => vec![],
        // The nitwit's plain green coat.
        Nitwit => repaint([76, 122, 58], [54, 90, 42], [72, 54, 36]),
        // A long iron-grey smithing apron with a bib, straps and rivets.
        Armorer => {
            const C: Rgb = [74, 76, 82];
            vec![
                f([-3.4, 3.4], [lo, -9.6], APRON, C, 18).with(Pattern::Hide),
                f([-2.6, 2.6], [-9.6, -2.5], APRON, C, 18).with(Pattern::Hide),
                f([-2.6, -1.6], [-2.5, top], APRON, C, 14),
                f([1.6, 2.6], [-2.5, top], APRON, C, 14),
                k([-2.6, -1.6], [-8.0, top], APRON, C, 14),
                k([1.6, 2.6], [-8.0, top], APRON, C, 14),
                f([-2.4, -1.8], [-3.4, -2.8], DETAIL, [168, 168, 164], 0),
                f([1.8, 2.4], [-3.4, -2.8], DETAIL, [168, 168, 164], 0),
                f([-3.4, 3.4], [lo, lo + 0.7], DETAIL, [52, 54, 58], 8),
            ]
        }
        // A white apron with red butcher's stripes at its hem.
        Butcher => {
            const W: Rgb = [232, 230, 222];
            const R: Rgb = [178, 52, 48];
            vec![
                f([-3.4, 3.4], [lo, -9.6], APRON, W, 12),
                f([-2.8, 2.8], [-9.6, -1.5], APRON, W, 12),
                f([-2.0, -1.3], [-1.5, top], APRON, W, 10),
                f([1.3, 2.0], [-1.5, top], APRON, W, 10),
                f([-3.4, 3.4], [lo + 1.0, lo + 1.6], DETAIL, R, 6),
                f([-3.4, 3.4], [lo + 2.3, lo + 2.7], DETAIL, R, 6),
            ]
        }
        // A tan waistcoat with gold buttons, and a rolled map in a pouch
        // at the hip.
        Cartographer => {
            const V: Rgb = [196, 158, 100];
            const GOLD: Rgb = [224, 186, 74];
            vec![
                f([-hx + 0.4, -0.5], [-11.5, -1.0], APRON, V, 16).with(Pattern::Cloth),
                f([0.5, hx - 0.4], [-11.5, -1.0], APRON, V, 16).with(Pattern::Cloth),
                f([-0.5, 0.5], [-5.0, -1.0], APRON, [236, 228, 208], 8),
                f([0.6, 1.2], [-4.0, -3.4], DETAIL, GOLD, 0),
                f([0.6, 1.2], [-7.0, -6.4], DETAIL, GOLD, 0),
                b([hx, -14.0, -1.3], [hx + 0.9, -11.0, 1.3], [112, 78, 48], 14),
                b([hx + 0.15, -11.0, -0.6], [hx + 0.75, -7.2, 0.6], [234, 224, 190], 6),
                b([hx + 0.1, -9.4, -0.65], [hx + 0.8, -8.9, 0.65], [172, 56, 48], 0),
            ]
        }
        // Purple vestments with a gold-edged mantle and stole.
        Cleric => {
            const GOLD: Rgb = [214, 178, 76];
            let mut v = repaint([116, 56, 136], [84, 38, 100], GOLD);
            v.extend([
                b([-hx - 0.15, -3.0, -hz - 0.15], [hx + 0.15, top + 0.15, hz + 0.15], [88, 40, 106], 18)
                    .with(Pattern::Cloth),
                b([-hx - 0.2, -3.4, -hz - 0.2], [hx + 0.2, -2.9, hz + 0.2], GOLD, 6),
                f([-0.9, 0.9], [-9.0, -3.0], [0.15, 0.25], GOLD, 6),
                f([-0.9, 0.9], [lo, -12.0], [0.08, 0.16], GOLD, 6),
            ]);
            v
        }
        // Brown bib overalls with a pocket.
        Farmer => {
            const C: Rgb = [136, 98, 52];
            vec![
                f([-3.2, 3.2], [lo, -9.6], APRON, C, 18).with(Pattern::Cloth),
                f([-2.6, 2.6], [-9.6, -3.0], APRON, C, 18).with(Pattern::Cloth),
                f([-2.6, -1.7], [-3.0, top], APRON, C, 14),
                f([1.7, 2.6], [-3.0, top], APRON, C, 14),
                k([-2.6, -1.7], [-9.0, top], APRON, C, 14),
                k([1.7, 2.6], [-9.0, top], APRON, C, 14),
                f([-1.5, 1.5], [-7.4, -4.6], DETAIL, [168, 130, 70], 10),
                f([-2.4, -1.9], [-3.6, -3.1], DETAIL, [190, 190, 180], 0),
                f([1.9, 2.4], [-3.6, -3.1], DETAIL, [190, 190, 180], 0),
            ]
        }
        // A waxed yellow apron with a fish on it, and a neckerchief.
        Fisherman => {
            const C: Rgb = [176, 154, 70];
            vec![
                f([-3.2, 3.2], [lo, -9.6], APRON, C, 16),
                f([-2.6, 2.6], [-9.6, -3.5], APRON, C, 16),
                f([-hx + 0.3, hx - 0.3], [-2.0, top], [0.04, 0.12], [68, 96, 132], 10),
                f([-1.4, 1.4], [-3.5, -2.0], [0.04, 0.12], [68, 96, 132], 10),
                f([-1.8, 0.6], [-15.4, -13.8], DETAIL, [84, 140, 158], 6),
                f([0.6, 1.6], [-15.8, -13.4], DETAIL, [60, 108, 128], 6),
                f([-1.4, -1.0], [-14.8, -14.4], [0.16, 0.2], [30, 34, 36], 0),
            ]
        }
        // A green tunic over the chest, and a quiver of arrows on the back.
        Fletcher => {
            const C: Rgb = [76, 114, 58];
            const Q: Rgb = [120, 80, 46];
            vec![
                f([-hx + 0.2, hx - 0.2], [-10.5, -1.0], APRON, C, 18).with(Pattern::Cloth),
                f([-hx + 0.2, -1.0], [-1.0, top], APRON, C, 18),
                f([1.0, hx - 0.2], [-1.0, top], APRON, C, 18),
                f([-hx + 0.2, hx - 0.2], [-10.5, -9.9], DETAIL, [70, 52, 34], 6),
                k([0.6, 3.4], [-13.5, -2.0], [0.0, 2.2], Q, 16).with(Pattern::Hide),
                k([0.4, 3.6], [-3.0, -2.2], [0.0, 2.4], [92, 60, 36], 8),
                k([1.0, 1.6], [-2.2, -0.2], [1.0, 1.6], [236, 234, 226], 0),
                k([2.2, 2.8], [-2.2, -0.6], [0.6, 1.2], [194, 60, 50], 0),
                k([1.6, 2.2], [-2.2, 0.2], [1.4, 2.0], [236, 234, 226], 0),
            ]
        }
        // A tabard of cream with red edges and a red collar.
        Librarian => {
            const C: Rgb = [230, 224, 202];
            const R: Rgb = [150, 40, 50];
            vec![
                f([-2.6, 2.6], [lo - 1.0, -1.5], APRON, C, 12).with(Pattern::Cloth),
                f([-3.0, -2.6], [lo - 1.0, -1.5], APRON, R, 6),
                f([2.6, 3.0], [lo - 1.0, -1.5], APRON, R, 6),
                b([-hx - 0.1, -1.5, -hz - 0.1], [hx + 0.1, top + 0.1, hz + 0.1], R, 10),
                f([-1.6, -0.4], [-8.4, -3.4], DETAIL, [92, 58, 40], 8),
            ]
        }
        // A stitched leather apron with pockets.
        Leatherworker => {
            const C: Rgb = [152, 98, 56];
            const S: Rgb = [102, 64, 36];
            vec![
                f([-3.4, 3.4], [lo, -9.6], APRON, C, 22).with(Pattern::Hide),
                f([-2.8, 2.8], [-9.6, -1.5], APRON, C, 22).with(Pattern::Hide),
                f([-2.0, -1.3], [-1.5, top], APRON, S, 10),
                f([1.3, 2.0], [-1.5, top], APRON, S, 10),
                f([-3.4, -3.0], [lo, -9.6], DETAIL, S, 6),
                f([3.0, 3.4], [lo, -9.6], DETAIL, S, 6),
                f([-3.0, -0.6], [-16.0, -13.0], DETAIL, [130, 82, 46], 12),
                f([0.6, 3.0], [-16.0, -13.0], DETAIL, [130, 82, 46], 12),
            ]
        }
        // A stone-grey apron with a pocket and a chisel.
        Mason => {
            const C: Rgb = [178, 172, 162];
            vec![
                f([-3.4, 3.4], [lo, -9.6], APRON, C, 20).with(Pattern::Speckle),
                f([-2.6, 2.6], [-9.6, -3.0], APRON, C, 20).with(Pattern::Speckle),
                f([-2.6, -1.8], [-3.0, top], APRON, C, 12),
                f([1.8, 2.6], [-3.0, top], APRON, C, 12),
                f([-2.6, 0.6], [-16.4, -13.4], DETAIL, [118, 114, 108], 10),
                f([1.6, 2.2], [-15.8, -12.6], DETAIL, [124, 90, 56], 6),
                f([1.5, 2.3], [-12.6, -11.8], DETAIL, [190, 190, 186], 0),
            ]
        }
        // A woolly cape over the shoulders and a satchel at the hip.
        Shepherd => vec![
            b([-hx - 0.3, -5.0, -hz - 0.3], [hx + 0.3, top + 0.3, hz + 0.3], [226, 222, 208], 18).with(Pattern::Wool),
            f([-2.2, 2.2], [-6.2, -5.0], [0.2, 0.3], [226, 222, 208], 18).with(Pattern::Wool),
            b([-hx - 0.9, -14.5, -1.5], [-hx, -10.5, 1.5], [128, 92, 58], 14).with(Pattern::Hide),
            b([-hx - 1.0, -11.5, -1.6], [-hx, -10.5, 1.6], [104, 74, 46], 8),
            f([-3.4, -2.6], [-10.5, -5.0], [0.04, 0.12], [104, 74, 46], 8),
        ],
        // A near-black apron with a tool belt: a file and a hammer.
        Toolsmith => {
            const C: Rgb = [60, 52, 48];
            vec![
                f([-3.4, 3.4], [lo, -9.6], APRON, C, 16),
                f([-2.6, 2.6], [-9.6, -2.5], APRON, C, 16),
                f([-2.6, -1.8], [-2.5, top], APRON, C, 12),
                f([1.8, 2.6], [-2.5, top], APRON, C, 12),
                f([-hx + 0.2, hx - 0.2], [-12.4, -11.6], DETAIL, [104, 76, 50], 8),
                f([-3.2, -2.6], [-15.0, -11.6], [0.16, 0.24], [172, 172, 168], 0),
                f([2.2, 2.8], [-15.4, -11.6], [0.16, 0.24], [126, 92, 58], 6),
                f([1.6, 3.4], [-11.6, -10.6], [0.16, 0.26], [150, 150, 148], 4),
            ]
        }
        // A dark leather apron with an iron-studded hem.
        Weaponsmith => {
            const C: Rgb = [66, 52, 44];
            vec![
                f([-3.4, 3.4], [lo, -9.6], APRON, C, 18).with(Pattern::Hide),
                f([-2.6, 2.6], [-9.6, -2.0], APRON, C, 18).with(Pattern::Hide),
                f([-2.6, -1.8], [-2.0, top], APRON, C, 12),
                f([1.8, 2.6], [-2.0, top], APRON, C, 12),
                f([-3.4, 3.4], [lo, lo + 0.8], DETAIL, [116, 116, 120], 6),
                f([-2.6, -2.0], [lo + 1.6, lo + 2.2], DETAIL, [176, 176, 172], 0),
                f([-0.3, 0.3], [lo + 1.6, lo + 2.2], DETAIL, [176, 176, 172], 0),
                f([2.0, 2.6], [lo + 1.6, lo + 2.2], DETAIL, [176, 176, 172], 0),
            ]
        }
    }
}

/// The professions' pieces on the face (relative to the head's pivot at
/// the neck, the face at z 4): the armorer's welding mask, the
/// cartographer's monocle and the weaponsmith's eyepatch.
fn face_pieces(p: Profession) -> Vec<Cuboid> {
    match p {
        Profession::Armorer => vec![
            b([-4.15, 1.2, 4.0], [4.15, 7.6, 4.5], [64, 66, 70], 14),
            b([-3.3, 3.0, 4.5], [3.3, 4.4, 4.62], [36, 56, 60], 0),
            b([-3.9, 6.6, 4.5], [-3.3, 7.2, 4.6], [156, 156, 152], 0),
            b([3.3, 6.6, 4.5], [3.9, 7.2, 4.6], [156, 156, 152], 0),
            b([-4.15, 4.0, -4.15], [-4.0, 5.0, 4.0], [46, 42, 40], 6),
            b([4.0, 4.0, -4.15], [4.15, 5.0, 4.0], [46, 42, 40], 6),
            b([-4.15, 4.0, -4.15], [4.15, 5.0, -4.0], [46, 42, 40], 6),
        ],
        Profession::Cartographer => {
            const GOLD: Rgb = [226, 190, 80];
            vec![
                b([-3.5, 4.4, 4.0], [-0.5, 4.8, 4.25], GOLD, 0),
                b([-3.5, 2.2, 4.0], [-0.5, 2.6, 4.25], GOLD, 0),
                b([-3.5, 2.6, 4.0], [-3.1, 4.4, 4.25], GOLD, 0),
                b([-0.9, 2.6, 4.0], [-0.5, 4.4, 4.25], GOLD, 0),
                b([-3.45, 0.3, 4.0], [-3.15, 2.2, 4.14], [188, 150, 62], 0),
            ]
        }
        Profession::Weaponsmith => {
            const S: Rgb = [38, 32, 30];
            vec![
                b([0.7, 2.5, 4.0], [3.2, 4.7, 4.2], [26, 24, 24], 4),
                b([3.2, 4.3, 4.0], [4.06, 4.7, 4.12], S, 0),
                b([0.7, 4.7, 4.0], [1.5, 5.3, 4.12], S, 0),
                b([-4.06, 4.9, 4.0], [1.5, 5.3, 4.12], S, 0),
                b([4.0, 4.3, -4.06], [4.06, 4.7, 4.06], S, 0),
                b([-4.06, 4.9, -4.06], [-4.0, 5.3, 4.06], S, 0),
                b([-4.06, 4.5, -4.06], [4.06, 5.1, -4.0], S, 0),
            ]
        }
        _ => vec![],
    }
}

/// The professions' sleeves and gloves, one list for each arm part:
/// the villager's folded `arms` (two sleeves with the hands between), or
/// the zombie villager's right and left arm (a sleeve with the hand out of
/// it). The cleric's and nitwit's sleeves match their robes; smiths and
/// craftsmen wear gloves.
fn sleeves(p: Profession, r: &Robe) -> Vec<Vec<Cuboid>> {
    use Profession::*;
    let (sleeve, glove): (Option<(Rgb, Rgb)>, Option<Rgb>) = match p {
        Cleric => (Some(([116, 56, 136], [214, 178, 76])), Option::None),
        Nitwit => (Some(([76, 122, 58], [54, 90, 42])), Option::None),
        Armorer => (Option::None, Some([52, 52, 56])),
        Leatherworker => (Option::None, Some([124, 80, 44])),
        Mason => (Option::None, Some([130, 126, 118])),
        Weaponsmith => (Option::None, Some([58, 46, 38])),
        _ => (Option::None, Option::None),
    };
    if sleeve.is_none() && glove.is_none() {
        return vec![];
    }
    let mut out = if r.zombie { vec![vec![], vec![]] } else { vec![vec![]] };
    if let Some((c, cuff)) = sleeve {
        if r.zombie {
            for (arm, x) in out.iter_mut().zip([-3.0, -1.0]) {
                arm.push(jboxg([x, -2.0, -2.0], [4.0, 8.0, 4.0], 0.04, c, 28).with(Pattern::Cloth));
                arm.push(jboxg([x, 5.0, -2.0], [4.0, 1.0, 4.0], 0.08, cuff, 10));
            }
        } else {
            for x in [-8.0, 4.0] {
                out[0].push(jboxg([x, -2.0, -2.0], [4.0, 8.0, 4.0], 0.04, c, 28).with(Pattern::Cloth));
                out[0].push(jboxg([x, 4.5, -2.0], [4.0, 1.5, 4.0], 0.1, cuff, 10));
            }
        }
    }
    if let Some(c) = glove {
        if r.zombie {
            for (arm, x) in out.iter_mut().zip([-3.0, -1.0]) {
                arm.push(jboxg([x, 6.0, -2.0], [4.0, 4.0, 4.0], 0.05, c, 14).with(Pattern::Hide));
            }
        } else {
            out[0].push(jboxg([-4.0, 2.0, -2.0], [8.0, 4.0, 4.0], 0.05, c, 14).with(Pattern::Hide));
        }
    }
    out
}

/// The level badge on the belt: stone, iron, gold, emerald, diamond.
fn badge(level: usize, r: &Robe) -> Vec<Cuboid> {
    const COLOURS: [Rgb; 5] = [[126, 126, 122], [214, 214, 210], [246, 204, 64], [44, 186, 92], [96, 222, 214]];
    let hz = r.hz;
    vec![
        b([-1.4, -11.6, hz + 0.1], [1.4, -9.0, hz + 0.18], [60, 46, 34], 0),
        b([-1.0, -11.2, hz + 0.18], [1.0, -9.4, hz + 0.26], COLOURS[level], 10),
    ]
}

/// The adult head's hat shell (`hat`, the head grown by 0.51) and brim
/// (`hat_rim`, 16 x 16 at 5 to 6 pixels above the neck), which the
/// headwear stays within.
const H: f32 = 4.51;

/// Each type's headwear on the adult head (relative to its pivot at the
/// neck; the head is 8 x 10 x 8).
static ADULT_HATS: LazyLock<[Vec<Cuboid>; 7]> = LazyLock::new(|| {
    Type::ALL.map(|t| match t {
        Type::Plains | Type::Taiga => vec![],
        // A head wrap over the crown, back and sides, a band across the
        // brow and a gem.
        Type::Desert => vec![
            b([-H, 8.5, -H], [H, 10.0 + 0.51, H], ACCENT, 16).with(Pattern::Cloth),
            b([-H, 0.0, -H], [H, 8.5, -3.6], ACCENT, 16).with(Pattern::Cloth),
            b([-H, 1.0, -3.6], [-3.9, 8.5, H], ACCENT, 16).with(Pattern::Cloth),
            b([3.9, 1.0, -3.6], [H, 8.5, H], ACCENT, 16).with(Pattern::Cloth),
            b([-H, 5.5, 3.9], [H, 7.0, H], TRIM, 10),
            b([-0.7, 5.8, H], [0.7, 6.8, H + 0.1], ACCENT2, 0),
        ],
        // A leaf band with leaves sprouting on top and a flower.
        Type::Jungle => vec![
            b([-H, 6.2, -H], [H, 7.3, H], ACCENT, 20).with(Pattern::Mottle),
            b([-3.0, 10.0, -1.0], [-1.0, 11.6, 1.0], ACCENT, 20),
            b([0.5, 10.0, -0.5], [2.5, 11.0, 2.0], ACCENT, 20),
            b([2.4, 6.0, H], [3.6, 7.5, H + 0.1], ACCENT2, 0),
        ],
        // A red band with a bead in front and a feather at the back.
        Type::Savanna => vec![
            b([-H, 6.0, -H], [H, 7.2, H], ACCENT, 10),
            b([-0.6, 6.2, H], [0.6, 7.0, H + 0.1], TRIM, 0),
            b([3.6, 7.0, -4.2], [4.3, 11.5, -3.5], ACCENT2, 0),
        ],
        // A fur-trimmed knitted hat with ear flaps and a bobble.
        Type::Snow => vec![
            b([-4.6, 6.0, -4.6], [4.6, 8.0, 4.6], TRIM, 18).with(Pattern::Wool),
            b([-4.3, 8.0, -4.3], [4.3, 10.0 + 0.51, 4.3], ACCENT, 20).with(Pattern::Wool),
            b([-1.0, 10.51, -1.0], [1.0, 11.8, 1.0], TRIM, 16).with(Pattern::Wool),
            b([-4.6, 2.5, -1.5], [-4.0, 6.0, 1.5], TRIM, 18).with(Pattern::Wool),
            b([4.0, 2.5, -1.5], [4.6, 6.0, 1.5], TRIM, 18).with(Pattern::Wool),
        ],
        // A broad, low hat of reeds and moss on the brim line.
        Type::Swamp => vec![
            b([-6.5, 5.0, -6.5], [6.5, 6.0, 6.5], ACCENT, 22).with(Pattern::Mottle),
            b([-H, 6.0, -H], [H, 8.4, H], ACCENT2, 22).with(Pattern::Mottle),
            b([-H - 0.05, 6.0, -H - 0.05], [H + 0.05, 6.8, H + 0.05], TRIM, 8),
        ],
    })
});

/// The profession hats on the adult head (full: farmer, fisherman,
/// fletcher, librarian, shepherd; partial: butcher).
fn profession_hat_boxes(p: Profession) -> Vec<Cuboid> {
    match p {
        // A straw hat on the brim line.
        Profession::Farmer => vec![
            b([-6.5, 5.0, -6.5], [6.5, 6.0, 6.5], [213, 177, 88], 25),
            b([-H, 6.0, -H], [H, 9.0, H], [199, 159, 72], 25),
            b([-H - 0.05, 6.0, -H - 0.05], [H + 0.05, 6.8, H + 0.05], [150, 110, 50], 10),
        ],
        // A canvas bucket hat.
        Profession::Fisherman => vec![
            b([-5.6, 5.4, -5.6], [5.6, 6.2, 5.6], [150, 134, 98], 20),
            b([-H, 6.2, -H], [H, 9.6, H], [138, 122, 88], 20).with(Pattern::Cloth),
        ],
        // A green cap with a peak and a red feather.
        Profession::Fletcher => vec![
            b([-H, 7.0, -H], [H, 10.0 + 0.51, H], [70, 110, 60], 20).with(Pattern::Cloth),
            b([-3.0, 7.0, H], [3.0, 7.6, 7.0], [70, 110, 60], 20),
            b([3.9, 8.0, -3.5], [4.6, 12.5, -2.8], [214, 62, 50], 0),
        ],
        // A tall red cap with a gold band.
        Profession::Librarian => vec![
            b([-H, 7.0, -H], [H, 12.5, H], [128, 36, 46], 20).with(Pattern::Cloth),
            b([-H - 0.05, 7.0, -H - 0.05], [H + 0.05, 8.0, H + 0.05], [212, 180, 80], 10),
        ],
        // A soft wool cap with a short peak.
        Profession::Shepherd => vec![
            b([-4.6, 7.4, -4.6], [4.6, 10.8, 4.6], [230, 226, 214], 18).with(Pattern::Wool),
            b([-3.5, 7.4, 4.6], [3.5, 8.0, 6.2], [210, 204, 190], 14),
        ],
        // A red headband (a partial hat).
        Profession::Butcher => vec![b([-H, 6.4, -H], [H, 7.5, H], [182, 50, 46], 10)],
        _ => vec![],
    }
}

/// The wandering trader's hood (its own texture's hat) and the panel and
/// gold belt down its robe's front.
const TRADER_HOOD: &[Cuboid] = &[
    b([-4.6, 8.5, -4.6], [4.6, 10.6, 4.6], ROBE, 12).with(Pattern::Cloth),
    b([-4.6, 0.0, -4.6], [4.6, 8.5, -3.2], ROBE, 12).with(Pattern::Cloth),
    b([-4.6, 0.0, -3.2], [-3.6, 8.5, 4.6], ROBE, 12).with(Pattern::Cloth),
    b([3.6, 0.0, -3.2], [4.6, 8.5, 4.6], ROBE, 12).with(Pattern::Cloth),
];
const TRADER_PANEL: &[Cuboid] =
    &[b([-4.1, -17.0, 3.5], [4.1, -5.0, 3.6], TRIM, 12), b([-4.6, -14.0, -3.6], [4.6, -13.0, 3.7], BELT, 8)];

// ---------------------------------------------------------------- babies

/// The baby's hat shell (`BabyVillagerModel`'s and
/// `BabyZombieVillagerModel`'s `hat`, the 8 x 8 x 7 head grown by 0.3).
const BX: f32 = 4.3;
const BZ: f32 = 3.8;

/// Each type's headwear on a baby's head (relative to its pivot at the
/// neck), the adult's made small; babies wear no profession.
static BABY_HATS: LazyLock<[Vec<Cuboid>; 7]> = LazyLock::new(|| {
    Type::ALL.map(|t| match t {
        Type::Plains | Type::Taiga => vec![],
        Type::Desert => vec![
            b([-BX, 7.0, -BZ], [BX, 8.3, BZ], ACCENT, 16).with(Pattern::Cloth),
            b([-BX, 0.0, -BZ], [BX, 7.0, -3.0], ACCENT, 16).with(Pattern::Cloth),
            b([-BX, 1.0, -3.0], [-3.9, 7.0, BZ], ACCENT, 16).with(Pattern::Cloth),
            b([3.9, 1.0, -3.0], [BX, 7.0, BZ], ACCENT, 16).with(Pattern::Cloth),
            b([-BX, 5.7, 3.4], [BX, 6.8, BZ], TRIM, 10),
            b([-0.6, 5.9, BZ], [0.6, 6.6, BZ + 0.1], ACCENT2, 0),
        ],
        Type::Jungle => vec![
            b([-BX, 5.9, -BZ], [BX, 6.7, BZ], ACCENT, 20).with(Pattern::Mottle),
            b([-2.5, 8.0, -1.0], [-0.8, 9.2, 1.0], ACCENT, 20),
            b([0.8, 8.0, 0.0], [2.5, 8.9, 1.6], ACCENT, 20),
        ],
        Type::Savanna => {
            vec![b([-BX, 5.8, -BZ], [BX, 6.8, BZ], ACCENT, 10), b([-0.5, 6.0, BZ], [0.5, 6.6, BZ + 0.1], TRIM, 0)]
        }
        Type::Snow => vec![
            b([-4.4, 5.6, -3.9], [4.4, 7.0, 3.9], TRIM, 18).with(Pattern::Wool),
            b([-4.1, 7.0, -3.6], [4.1, 8.5, 3.6], ACCENT, 20).with(Pattern::Wool),
            b([-0.8, 8.5, -0.8], [0.8, 9.5, 0.8], TRIM, 16).with(Pattern::Wool),
        ],
        Type::Swamp => vec![
            b([-5.5, 5.6, -5.0], [5.5, 6.2, 5.0], ACCENT, 22).with(Pattern::Mottle),
            b([-BX, 6.2, -BZ], [BX, 7.8, BZ], ACCENT2, 22).with(Pattern::Mottle),
        ],
    })
});

/// `VillagerProfessionLayer` on a baby (a villager child or the zombie
/// villager cub): only the type's baby outfit, its headwear while
/// `typeHatVisible` (the profession's hat metadata still counts, though
/// the profession isn't drawn).
pub(super) fn dress_baby(m: &Mob, p: &mut Parts, head: Part) {
    let t = type_of(m);
    let profession = m.villager.as_ref().map_or(Profession::None, |v| v.profession);
    if type_hat_visible(t, profession, m.kind == MobKind::ZombieVillager) && !BABY_HATS[t as usize].is_empty() {
        p.push(Part { boxes: &BABY_HATS[t as usize], ..head });
    }
}

// ---------------------------------------------------------------- held item

/// `CrossedArmsItemLayer`: the villager's (or wandering trader's) main-hand
/// item, its shown trade, held up in front of the folded arms:
/// `translateToArms` (the root, then the `arms` part), a 0.75 turn about
/// x, a scale of 1.07, `translate(0, 0.13, -0.34)` and a half turn about x,
/// with the item's `ground` display transform
/// (`HoldingEntityRenderState` resolves it with `ItemDisplayContext.GROUND`).
#[allow(clippy::too_many_arguments)]
pub(super) fn crossed_arms_item(
    out: &mut EntityMesh,
    stack: crate::inventory::Stack,
    arms: &Part,
    origin: Vec3,
    body: Quat,
    scale: f32,
    light: ([u8; 4], u8),
) {
    use glam::Mat4;
    let hand = Mat4::from_translation(origin)
        * Mat4::from_quat(body)
        * Mat4::from_scale(Vec3::splat(scale / 16.0))
        * Mat4::from_translation(arms.pivot)
        * Mat4::from_quat(arms.rot)
        // Java's part frame: y down, z back.
        * Mat4::from_rotation_x(PI)
        * Mat4::from_scale(Vec3::splat(16.0))
        * Mat4::from_rotation_x(0.75)
        * Mat4::from_scale(Vec3::splat(1.07))
        * Mat4::from_translation(Vec3::new(0.0, 0.13, -0.34))
        * Mat4::from_rotation_x(PI);
    let display = crate::content::item_display::drawn(&stack);
    let transform = hand * crate::content::item_display::Context::Ground.matrix(&display);
    out.push_held(HeldItemModel {
        stack,
        transform,
        sky_light: f32::from(light.0[0]) / 255.0,
        block_light: f32::from(light.1) / 255.0,
        block: None,
        icon: None,
        tint: None,
    });
}

/// The item [`crossed_arms_item`] holds up: a villager's shown trade, or
/// whatever a wandering trader has in its main hand.
pub(super) fn crossed_arms_stack(m: &Mob) -> Option<crate::inventory::Stack> {
    match m.kind {
        MobKind::Villager => m.shown_trade_item(),
        MobKind::WanderingTrader => m.main_hand(),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hats_follow_javas_metadata() {
        use Profession::*;
        // No job, or a hatless one: the type's hat shows.
        assert!(type_hat_visible(Type::Desert, None, false));
        assert!(type_hat_visible(Type::Snow, Mason, false));
        // A full profession hat hides every type hat.
        for t in Type::ALL {
            assert!(!type_hat_visible(t, Farmer, false));
            assert!(!type_hat_visible(t, Librarian, true));
        }
        // The butcher's partial hat hides only a full type hat, which
        // zombie villagers' type textures never have.
        assert!(type_hat_visible(Type::Plains, Butcher, false));
        assert!(!type_hat_visible(Type::Desert, Butcher, false));
        assert!(type_hat_visible(Type::Desert, Butcher, true));
    }

    #[test]
    fn every_key_takes_a_palette_colour() {
        for t in Type::ALL {
            for k in [ROBE, TRIM, BELT, LEGS, ACCENT, ACCENT2] {
                let i = key(k).unwrap();
                assert!(key(PALETTES[t as usize][i]).is_none());
                assert!(key(rotted(PALETTES[t as usize][i])).is_none());
            }
        }
        assert_eq!(Type::from_name("snow"), Type::Snow);
        assert_eq!(Type::from_name("plains"), Type::Plains);
        assert_eq!(Type::from_name("unknown"), Type::Plains);
    }

    fn dressed(kind: MobKind, t: &str, p: Profession) -> Mob {
        let mut m = Mob::new(kind, DVec3::new(10.0, 64.0, 0.0), 0.0);
        let v = m.villager.as_mut().expect("villager data");
        v.map_variant = Some(t.into());
        v.profession = p;
        v.level = 3;
        m
    }

    fn has(parts: &Parts, boxes: &[Cuboid]) -> bool {
        parts.iter().any(|p| std::ptr::eq(p.boxes, boxes))
    }

    fn mesh(m: &Mob) -> EntityMesh {
        let mut out = EntityMesh::flat();
        assert_eq!(build(std::slice::from_ref(m), DVec3::new(0.0, 64.0, 0.0), Vec3::X, 100.0, 1.0, &mut out), 1);
        out
    }

    fn colours(m: &Mob) -> std::collections::HashSet<Rgb> {
        mesh(m).iter().map(|v| [v.color[0], v.color[1], v.color[2]]).collect()
    }

    /// The type's outfit and headwear, a job's apron, hat and level badge,
    /// combined as `VillagerProfessionLayer` combines them.
    #[test]
    fn outfits_layer_type_profession_and_level_as_java_does() {
        let desert = &ADULT_HATS[Type::Desert as usize][..];
        let plain = villager(&dressed(MobKind::Villager, "desert", Profession::None), 0.0);
        // Six parts, the desert accents and the head wrap.
        assert_eq!(plain.len(), 8);
        assert!(has(&plain, desert));
        // A farmer's full hat hides the wrap; the apron, hat and badge show.
        let farmer = villager(&dressed(MobKind::Villager, "desert", Profession::Farmer), 0.0);
        assert_eq!(farmer.len(), 10);
        assert!(!has(&farmer, desert));
        assert!(has(&farmer, &VILLAGER_FIT.hats[Profession::Farmer as usize]));
        assert!(has(&farmer, &VILLAGER_FIT.badges[2]));
        // A nitwit wears its coat and sleeves but no badge; plains
        // villagers no accents or hat.
        assert_eq!(villager(&dressed(MobKind::Villager, "plains", Profession::Nitwit), 0.0).len(), 8);
        // The butcher's partial hat hides a villager's full desert wrap but
        // not a zombie villager's (its type texture has no metadata).
        assert!(!has(&villager(&dressed(MobKind::Villager, "desert", Profession::Butcher), 0.0), desert));
        let mut zombie = dressed(MobKind::ZombieVillager, "desert", Profession::Butcher);
        assert!(has(&zombie_villager(&zombie, 0.0), desert));
        // The zombie's robe, its type's colours rotted; no key left over.
        let robe = PALETTES[Type::Desert as usize][0];
        let c = colours(&zombie);
        assert!(c.contains(&rotted(robe)) && !c.contains(&robe));
        assert!(c.iter().all(|&c| key(c).is_none()));
        zombie.villager.as_mut().unwrap().map_variant = Some("snow".into());
        assert!(colours(&zombie).contains(&rotted(PALETTES[Type::Snow as usize][0])));
        // Each type its own robe.
        for t in ["desert", "jungle", "plains", "savanna", "snow", "swamp", "taiga"] {
            let m = dressed(MobKind::Villager, t, Profession::Mason);
            assert!(colours(&m).contains(&PALETTES[Type::from_name(t) as usize][0]), "{t}");
        }
    }

    /// Babies wear only the type's baby outfit (no apron or badge) and
    /// its headwear.
    #[test]
    fn babies_wear_their_type_s_baby_outfit() {
        for kind in [MobKind::Villager, MobKind::ZombieVillager] {
            let mut m = dressed(kind, "snow", Profession::None);
            m.baby = true;
            m.age = crate::entity::animals::BABY_AGE;
            let c = colours(&m);
            let robe = PALETTES[Type::Snow as usize][0];
            assert!(c.contains(&if kind == MobKind::ZombieVillager { rotted(robe) } else { robe }), "{kind:?}");
            assert!(c.iter().all(|&c| key(c).is_none()), "{kind:?}");
            let parts = super::super::pose(&m, 0.0, 0.0);
            assert!(has(&parts, &BABY_HATS[Type::Snow as usize]), "{kind:?}");
            // The apron colours never show on a baby.
            m.villager.as_mut().unwrap().profession = Profession::Librarian;
            let parts = super::super::pose(&m, 0.0, 0.0);
            assert!(!has(&parts, &BABY_HATS[Type::Snow as usize]), "{kind:?}: a full profession hat hides it");
            assert!(!parts.iter().any(|p| {
                VILLAGER_FIT.jobs.iter().chain(ZOMBIE_FIT.jobs.iter()).any(|a| std::ptr::eq(p.boxes, &a[..]))
            }));
        }
    }

    /// v1.0 gap Track 7f: every profession's own clothes, on villagers and
    /// (rotted) on zombie villagers, layered as `VillagerProfessionLayer`
    /// layers them: over the type's outfit, under the level badge.
    #[test]
    fn every_profession_wears_its_own_clothes() {
        let mut seen = std::collections::HashSet::new();
        for p in Profession::ALL.into_iter().skip(1) {
            let j = p as usize;
            assert!(!VILLAGER_FIT.jobs[j].is_empty(), "{p:?}");
            // Each profession's clothes look different.
            let look: Vec<Rgb> = VILLAGER_FIT.jobs[j].iter().map(|c| c.color).collect();
            assert!(seen.insert(look), "{p:?}");
            let m = dressed(MobKind::Villager, "taiga", p);
            let parts = villager(&m, 0.0);
            assert!(has(&parts, &VILLAGER_FIT.jobs[j]), "{p:?}");
            // The clothes go on after the type's touches and before the badge.
            let at = |boxes: &[Cuboid]| parts.iter().position(|q| std::ptr::eq(q.boxes, boxes));
            let job = at(&VILLAGER_FIT.jobs[j]).unwrap();
            if !covers_robe(p) {
                assert!(at(&VILLAGER_FIT.accents[Type::Taiga as usize]).unwrap() < job, "{p:?}");
            }
            if p != Profession::Nitwit {
                assert!(job < at(&VILLAGER_FIT.badges[2]).unwrap(), "{p:?}");
            }
            // The zombie villager's are the same clothes, rotted.
            let mut z = dressed(MobKind::ZombieVillager, "taiga", p);
            z.villager.as_mut().unwrap().level = 3;
            let zparts = zombie_villager(&z, 0.0);
            assert!(has(&zparts, &ZOMBIE_FIT.jobs[j]), "{p:?}");
            let raw = VILLAGER_FIT.jobs[j][0].color;
            assert_eq!(ZOMBIE_FIT.jobs[j][0].color, rotted(raw), "{p:?}");
            let c = colours(&z);
            assert!(c.contains(&rotted(raw)) && c.iter().all(|&c| key(c).is_none()), "{p:?}");
        }
        assert!(VILLAGER_FIT.jobs[Profession::None as usize].is_empty());
        // The cleric's vestments cover the type's touches on the robe (the
        // snow coat's fur), not its headwear; its sleeves are purple, one
        // part on the folded arms and one on each zombie arm.
        let cleric = villager(&dressed(MobKind::Villager, "snow", Profession::Cleric), 0.0);
        assert!(!has(&cleric, &VILLAGER_FIT.accents[Type::Snow as usize]));
        assert!(has(&cleric, &ADULT_HATS[Type::Snow as usize]));
        assert_eq!(VILLAGER_FIT.sleeves[Profession::Cleric as usize].len(), 1);
        assert_eq!(ZOMBIE_FIT.sleeves[Profession::Cleric as usize].len(), 2);
        let z = zombie_villager(&dressed(MobKind::ZombieVillager, "snow", Profession::Cleric), 0.0);
        let sleeves = &ZOMBIE_FIT.sleeves[Profession::Cleric as usize];
        assert!(has(&z, &sleeves[0]) && has(&z, &sleeves[1]));
        // Face pieces: the armorer's mask, the cartographer's monocle, the
        // weaponsmith's patch, on the head.
        for p in [Profession::Armorer, Profession::Cartographer, Profession::Weaponsmith] {
            let parts = villager(&dressed(MobKind::Villager, "plains", p), 0.0);
            let face = parts.iter().find(|q| std::ptr::eq(q.boxes, &VILLAGER_FIT.faces[p as usize][..])).unwrap();
            assert_eq!(face.pivot, parts[HEAD].pivot, "{p:?}");
        }
        // Smiths' gloves cover the hands.
        assert!(!VILLAGER_FIT.sleeves[Profession::Armorer as usize].is_empty());
        assert!(VILLAGER_FIT.sleeves[Profession::Farmer as usize].is_empty());
    }

    /// v1.0 gap Track 7f: `VillagerModel.setupAnim` while `isUnhappy`: the
    /// head bows to 0.4 and rolls `0.3 * sin(0.45 * ageInTicks)`, on adults,
    /// children and wandering traders.
    #[test]
    fn unhappy_villagers_shake_their_heads() {
        let mut m = dressed(MobKind::Villager, "plains", Profession::Nitwit);
        m.head_pitch = 0.3;
        assert_eq!(head_rotation(&m, 1.0), [-0.3, 0.0, 0.0]);
        m.villager.as_mut().unwrap().unhappy = 40;
        let t = 3.0 * motion::TICK;
        let [x, y, z] = head_rotation(&m, t);
        assert_eq!((x, y), (0.4, 0.0));
        assert!((z - 0.3 * (0.45 * age_ticks(t)).sin()).abs() < 1e-6);
        // Posed: the head tilts with it, adult and child.
        let mut calm = dressed(MobKind::Villager, "plains", Profession::Nitwit);
        calm.head_pitch = 0.3;
        assert!(villager(&m, t)[HEAD].rot.angle_between(villager(&calm, t)[HEAD].rot) > 0.5);
        for c in [&mut m, &mut calm] {
            c.baby = true;
            c.age = crate::entity::animals::BABY_AGE;
        }
        let (a, b) = (super::super::pose(&m, t, 0.0), super::super::pose(&calm, t, 0.0));
        assert!(a[3].rot.angle_between(b[3].rot) > 0.5);
    }

    /// `CrossedArmsItemLayer`: the shown trade held up in front of the
    /// folded arms, at the chest.
    #[test]
    fn villagers_hold_their_shown_trade_up_in_front_of_their_arms() {
        let mut m = dressed(MobKind::Villager, "plains", Profession::Farmer);
        assert!(mesh(&m).held_items().is_empty());
        let stack = crate::inventory::Stack::new(crate::item::Item::EMERALD, 1);
        m.equipment.set(crate::entity::equipment::Slot::MainHand, Some(stack));
        let out = mesh(&m);
        assert_eq!(out.held_items().len(), 1);
        let centre = out.held_items()[0].transform.transform_point3(Vec3::splat(0.5));
        // Facing +X from x = 10: a few pixels in front of the chest.
        assert!((10.1..10.6).contains(&centre.x), "{centre}");
        assert!((0.8..1.3).contains(&centre.y), "{centre}");
        assert!(centre.z.abs() < 0.05, "{centre}");
    }

    /// `HumanoidArmorLayer` with `ZOMBIE_VILLAGER_ARMOR`: the adult wears its
    /// armor, the helmet on its taller head.
    #[test]
    fn adult_zombie_villagers_wear_armor() {
        let mut m = dressed(MobKind::ZombieVillager, "plains", Profession::None);
        let bare = mesh(&m).iter().count();
        m.equipment.wear_set(crate::entity::armor::ArmorKind::Iron, false);
        let out = mesh(&m);
        assert!(out.iter().count() > bare);
        let top = out.iter().map(|v| v.pos[1]).fold(f32::MIN, f32::max);
        // The 34-pixel model's helmet, a pixel proud of its 10-pixel head.
        assert!((2.12..2.25).contains(&top), "{top}");
    }
}
