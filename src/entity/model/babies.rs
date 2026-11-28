//! v1.0 gap Track 7b: Java 26.3's separate baby models (`BabyChickenModel`,
//! `BabyPigModel`, `BabyCowModel`, `BabySheepModel`, `BabyWolfModel`,
//! `BabyVillagerModel`, `BabyZombieModel` and `BabyDrownedModel`), which
//! `AgeableMobRenderer` draws at full scale instead of the adult halved:
//! big heads, short legs. The main boxes take the proportions and pivots
//! of Java's baby meshes turned onto this game's axes (x kept, y up from
//! the feet, z forward); every box's colour, texture pattern and detail is
//! this game's own, in the adults' palettes. They animate as their adults'
//! Java models do (the walk position already runs three times as fast for
//! babies). No Mojang geometry file, texture or UV is used.

use super::*;

/// The kinds Java 26.3 gives a baby model of its own (every baby layer in
/// `LayerDefinitions`), drawn at full scale. v1.0 gap Track 7c added the
/// other kinds' ([`super::baby_rigs`]). The ghastling is Java's adult happy
/// ghast scaled by 0.2375 (`HappyGhastModel.BABY_TRANSFORMER`), as this
/// game draws it; other babies (none in Java 26.3) draw their adult halved.
pub(super) fn has_model(kind: MobKind) -> bool {
    use MobKind::*;
    matches!(
        kind,
        Chicken
            | Pig
            | Cow
            | Mooshroom
            | Sheep
            | Wolf
            | Villager
            | Zombie
            | Husk
            | Drowned
            | Armadillo
            | Axolotl
            | Bee
            | Camel
            | Cat
            | Ocelot
            | Dolphin
            | Donkey
            | Mule
            | Horse
            | SkeletonHorse
            | ZombieHorse
            | Fox
            | Goat
            | Llama
            | TraderLlama
            | Panda
            | PolarBear
            | Rabbit
            | Squid
            | GlowSquid
            | Turtle
            | Hoglin
            | Zoglin
            | Piglin
            | ZombifiedPiglin
            | Strider
            | ZombieVillager
            | Sniffer
            | Nautilus
    )
}

// ---------------------------------------------------------------- chick

/// A chick's down: paler and warmer than the hen's feathers.
const DOWN: Rgb = [250, 226, 132];
const DOWN_SHADE: Rgb = [232, 198, 98];
/// `BabyChickenModel`: one 4-pixel body with the beak on its front, stick
/// legs and stubby flat wings.
const CHICK_BODY: &[Cuboid] = &[
    cube([-2.0, -1.75, -3.25], [2.0, 2.25, 0.75], DOWN, 16).with(Pattern::Feather),
    cube([-1.0, -0.75, 0.75], [1.0, 0.25, 1.75], BEAK, 8),
    cube([-1.0, -0.75, 0.75], [1.0, -0.55, 1.8], [200, 128, 30], 0),
    // Eyes with a glint, a fluffy crown and a paler belly.
    cube([-1.6, 0.75, 0.75], [-0.9, 1.5, 0.8], [26, 24, 22], 0),
    cube([0.9, 0.75, 0.75], [1.6, 1.5, 0.8], [26, 24, 22], 0),
    cube([-1.45, 1.2, 0.8], [-1.2, 1.45, 0.83], [244, 240, 228], 0),
    cube([1.2, 1.2, 0.8], [1.45, 1.45, 0.83], [244, 240, 228], 0),
    cube([-0.5, 2.25, -1.5], [0.5, 2.75, -0.5], DOWN_SHADE, 8),
    cube([-1.5, -1.8, -2.5], [1.5, -1.75, 0.5], [252, 238, 176], 0),
];
const CHICK_LEG: &[Cuboid] = &[
    cube([-0.25, -2.0, -0.25], [0.25, 0.0, 0.25], CHICKEN_FEET, 6),
    cube([-0.5, -2.0, -0.25], [0.5, -1.8, 0.75], CHICKEN_FEET, 6),
];
const CHICK_WING_R: &[Cuboid] = &[cube([0.0, -0.3, -1.0], [1.0, 0.0, 1.0], DOWN_SHADE, 10)];
const CHICK_WING_L: &[Cuboid] = &[cube([-1.0, -0.3, -1.0], [0.0, 0.0, 1.0], DOWN_SHADE, 10)];

// ---------------------------------------------------------------- piglet

/// `BabyPigModel`: a 7 x 6 x 9 body, a 7 x 6 x 6 head with a 3 x 2 snout
/// and 2-pixel legs.
const PIGLET_BODY: &[Cuboid] = &[cube([-3.5, -3.0, -4.5], [3.5, 3.0, 4.5], PIG_SKIN, 22).with(Pattern::Hide)];
const PIGLET_HEAD: &[Cuboid] = &[
    cube([-3.5, -1.0, -1.0], [3.5, 5.0, 5.0], PIG_SKIN, 22).with(Pattern::Hide),
    cube([-1.5, 0.0, 5.0], [1.5, 2.0, 6.0], PIG_SNOUT, 14).with(Pattern::Hide),
    cube([-1.0, 0.6, 6.0], [-0.3, 1.4, 6.05], DARK, 0),
    cube([0.3, 0.6, 6.0], [1.0, 1.4, 6.05], DARK, 0),
    // Big eyes with a glint, pink cheeks and floppy ear tips.
    cube([-3.0, 2.5, 5.0], [-1.8, 3.7, 5.05], DARK, 0),
    cube([1.8, 2.5, 5.0], [3.0, 3.7, 5.05], DARK, 0),
    cube([-2.6, 3.2, 5.05], [-2.2, 3.6, 5.08], WHITE, 0),
    cube([2.2, 3.2, 5.05], [2.6, 3.6, 5.08], WHITE, 0),
    cube([-3.5, 0.8, 5.0], [-2.3, 1.6, 5.03], [242, 136, 146], 0),
    cube([2.3, 0.8, 5.0], [3.5, 1.6, 5.03], [242, 136, 146], 0),
    cube([-3.5, 5.0, 1.0], [-2.0, 5.6, 3.0], PIG_LEG, 12),
    cube([2.0, 5.0, 1.0], [3.5, 5.6, 3.0], PIG_LEG, 12),
];
const PIGLET_LEG: &[Cuboid] = &[
    cube([-1.0, -2.0, -1.0], [1.0, 0.0, 1.0], PIG_LEG, 22).with(Pattern::Hide),
    cube([-1.0, -2.0, -1.0], [1.0, -1.4, 1.0], PIG_HOOF, 10),
];

// ---------------------------------------------------------------- calf

/// `BabyCowModel`: an 8 x 6 x 12 body on 6-pixel legs, a 6 x 6 x 5 head
/// with nubs of horn and a 4 x 3 muzzle.
const CALF_BODY: &[Cuboid] = &[
    cube([-4.0, 6.0, -6.0], [4.0, 12.0, 6.0], COW_HIDE, 24).with(Pattern::Fur),
    cube([-4.1, 7.0, -1.0], [-3.9, 11.0, 3.0], COW_PATCH, 10).with(Pattern::Fur),
    cube([3.9, 8.0, -5.0], [4.1, 11.0, -1.0], COW_PATCH, 10).with(Pattern::Fur),
    cube([-2.0, 11.9, -4.0], [1.0, 12.1, 0.0], COW_PATCH, 10).with(Pattern::Fur),
];
const CALF_HEAD: &[Cuboid] = &[
    cube([-3.0, -1.5, 0.0], [3.0, 4.5, 5.0], COW_HIDE, 24).with(Pattern::Fur),
    cube([-2.0, -1.5, 5.0], [2.0, 1.5, 6.0], COW_NOSE, 12).with(Pattern::Hide),
    cube([-1.4, -0.6, 6.0], [-0.6, 0.2, 6.05], [96, 60, 56], 0),
    cube([0.6, -0.6, 6.0], [1.4, 0.2, 6.05], [96, 60, 56], 0),
    cube([-0.8, 1.5, 5.0], [0.8, 4.5, 5.05], COW_PATCH, 8),
    cube([3.0, 3.5, 2.8], [4.0, 5.5, 3.8], HORN, 8),
    cube([-4.0, 3.5, 2.8], [-3.0, 5.5, 3.8], HORN, 8),
    cube([-2.8, 2.0, 5.0], [-1.6, 3.2, 5.05], DARK, 0),
    cube([1.6, 2.0, 5.0], [2.8, 3.2, 5.05], DARK, 0),
    cube([-2.4, 2.7, 5.05], [-2.0, 3.1, 5.08], WHITE, 0),
    cube([2.0, 2.7, 5.05], [2.4, 3.1, 5.08], WHITE, 0),
];
const CALF_LEG: &[Cuboid] = &[
    cube([-1.5, -6.0, -1.5], [1.5, 0.0, 1.5], COW_HIDE, 24).with(Pattern::Fur),
    cube([-1.5, -6.0, -1.5], [1.5, -5.0, 1.5], COW_HOOF, 12),
];

// ---------------------------------------------------------------- lamb

/// `BabySheepModel`: a 6 x 4 x 9 fleece on 5-pixel legs and a 5-pixel
/// head with a tuft of wool on top.
const LAMB_BODY: &[Cuboid] = &[cube([-3.0, -2.0, -4.5], [3.0, 2.0, 4.5], WOOL, 34).with(Pattern::Wool)];
const LAMB_HEAD: &[Cuboid] = &[
    cube([-2.5, -0.5, -1.5], [2.5, 4.5, 3.5], SHEEP_SKIN, 14).with(Pattern::Hide),
    cube([-2.7, 3.0, -1.7], [2.7, 4.8, 2.0], WOOL, 34).with(Pattern::Wool),
    cube([-2.0, 1.5, 3.5], [-1.0, 2.5, 3.55], DARK, 0),
    cube([1.0, 1.5, 3.5], [2.0, 2.5, 3.55], DARK, 0),
    cube([-1.75, 2.1, 3.55], [-1.4, 2.45, 3.58], WHITE, 0),
    cube([1.4, 2.1, 3.55], [1.75, 2.45, 3.58], WHITE, 0),
    cube([-0.7, 0.0, 3.5], [0.7, 0.7, 3.55], [204, 140, 138], 0),
];
const LAMB_LEG: &[Cuboid] = &[
    cube([-1.0, -5.0, -1.0], [1.0, 0.0, 1.0], SHEEP_SKIN, 14).with(Pattern::Hide),
    cube([-1.0, -5.0, -1.0], [1.0, -4.0, 1.0], SHEEP_HOOF, 10),
];

// ---------------------------------------------------------------- pup

/// `BabyWolfModel`: a 6 x 4 x 8 body on 3-pixel legs, a 6 x 5 x 5 head
/// with a short muzzle and the ears on top, a 6-pixel tail.
const PUP_BODY: &[Cuboid] = &[
    cube([-3.0, -2.0, -4.0], [3.0, 2.0, 4.0], super::animals::FUR, 24).with(Pattern::Fur),
    cube([-3.2, -2.0, 1.0], [3.2, 2.4, 4.0], super::animals::MANE, 26).with(Pattern::Fur),
];
const PUP_HEAD: &[Cuboid] = &[
    cube([-3.0, -1.75, -2.0], [3.0, 3.25, 3.0], super::animals::FUR, 18).with(Pattern::Fur),
    cube([-1.5, -1.75, 3.0], [1.5, 0.25, 5.0], super::animals::MUZZLE, 16).with(Pattern::Fur),
    cube([-0.7, -0.6, 5.0], [0.7, 0.25, 5.05], [35, 33, 32], 0),
    cube([-3.0, 3.25, 0.0], [-1.0, 5.25, 1.0], super::animals::FUR, 20).with(Pattern::Fur),
    cube([1.0, 3.25, 0.0], [3.0, 5.25, 1.0], super::animals::FUR, 20).with(Pattern::Fur),
    cube([-2.5, 3.6, 1.0], [-1.5, 4.9, 1.05], [96, 78, 74], 0),
    cube([1.5, 3.6, 1.0], [2.5, 4.9, 1.05], [96, 78, 74], 0),
    cube([-2.5, 0.6, 3.0], [-1.3, 1.9, 3.05], [30, 28, 25], 0),
    cube([1.3, 0.6, 3.0], [2.5, 1.9, 3.05], [30, 28, 25], 0),
    cube([-2.15, 1.4, 3.05], [-1.75, 1.8, 3.08], [220, 220, 210], 0),
    cube([1.75, 1.4, 3.05], [2.15, 1.8, 3.08], [220, 220, 210], 0),
];
const PUP_LEG: &[Cuboid] = &[cube([-1.0, -3.0, -1.0], [1.0, 0.0, 1.0], super::animals::FUR, 18).with(Pattern::Fur)];
const PUP_TAIL: &[Cuboid] = &[cube([-1.0, -1.0, -6.0], [1.0, 1.0, 0.0], super::animals::FUR, 20).with(Pattern::Fur)];
const PUP_COLLAR: &[Cuboid] = &[cube([-3.3, -2.1, 3.0], [3.3, 2.6, 4.2], super::animals::COLLAR, 0)];

// ---------------------------------------------------------------- villager child

const CHILD_SKIN: Rgb = [177, 131, 101];
const CHILD_ROBE: Rgb = [114, 78, 50];
/// `BabyVillagerModel`: an 8 x 8 x 7 head with the nose, a 4 x 5 x 3 body
/// in a robe, arms folded in front and 3-pixel legs.
const CHILD_BODY: &[Cuboid] = &[
    cube([-2.2, 2.0, -1.7], [2.2, 8.0, 1.7], CHILD_ROBE, 30).with(Pattern::Cloth),
    cube([-2.25, 2.0, -1.75], [2.25, 2.6, 1.75], [74, 50, 33], 12),
    cube([-2.22, 5.0, -1.72], [2.22, 5.5, 1.72], [66, 44, 28], 10),
];
const CHILD_HEAD: &[Cuboid] = &[
    cube([-4.0, 0.0, -3.5], [4.0, 8.0, 3.5], CHILD_SKIN, 24).with(Pattern::Hide),
    cube([-1.0, 0.0, 3.5], [1.0, 2.0, 4.5], [162, 117, 86], 22).with(Pattern::Hide),
    cube([-3.0, 3.0, 3.5], [-2.0, 4.5, 3.55], [236, 232, 224], 0),
    cube([-2.0, 3.0, 3.5], [-1.0, 4.5, 3.6], [49, 103, 43], 0),
    cube([1.0, 3.0, 3.5], [2.0, 4.5, 3.6], [49, 103, 43], 0),
    cube([2.0, 3.0, 3.5], [3.0, 4.5, 3.55], [236, 232, 224], 0),
    cube([-3.0, 5.0, 3.5], [3.0, 5.6, 3.6], [62, 43, 28], 0),
];
/// The folded arms: two forearms and the bar of the sleeves across them.
const CHILD_HANDS: &[Cuboid] = &[
    cube([-1.0, -1.5, -0.15], [1.0, 2.5, 1.85], CHILD_ROBE, 28).with(Pattern::Cloth),
    cube([5.0, -1.5, -0.15], [7.0, 2.5, 1.85], CHILD_ROBE, 28).with(Pattern::Cloth),
];
const CHILD_ARM_BAR: &[Cuboid] = &[
    cube([-2.0, -1.0, -1.0], [2.0, 1.0, 1.0], CHILD_ROBE, 28).with(Pattern::Cloth),
    cube([-1.0, -1.0, 1.0], [1.0, 0.6, 1.05], CHILD_SKIN, 24),
];
const CHILD_LEG: &[Cuboid] = &[cube([-1.0, -2.5, -1.0], [1.0, 0.5, 1.0], [68, 49, 36], 28).with(Pattern::Cloth)];

// ---------------------------------------------------------------- baby zombie

/// `BabyZombieModel` (and `BabyDrownedModel`, the same mesh): a 6-pixel
/// head on a 4 x 5 x 2 body, 5-pixel arms and 4-pixel legs, in each
/// zombie's own palette.
macro_rules! baby_zombie {
    ($body:ident, $leg:ident, $arm:ident, $head:ident, $skin:expr, $shirt:expr, $pants:expr) => {
        const $body: &[Cuboid] = &[
            cube([-2.0, 4.0, -1.0], [2.0, 9.0, 1.0], $shirt, 40).with(Pattern::Cloth),
            cube([0.5, 5.0, 1.0], [1.5, 6.5, 1.05], $skin, 36).with(Pattern::Rot),
            cube([-2.0, 4.0, 1.0], [2.0, 4.5, 1.05], ZOMBIE_PANTS, 0),
        ];
        const $leg: &[Cuboid] = &[
            cube([-1.0, -4.0, -1.0], [1.0, 0.0, 1.0], $pants, 40).with(Pattern::Cloth),
            cube([-1.0, -4.0, -1.0], [1.0, -3.2, 1.0], ZOMBIE_SHOES, 30),
        ];
        const $arm: &[Cuboid] = &[
            cube([-1.0, -1.5, -1.0], [1.0, 0.5, 1.0], $shirt, 40).with(Pattern::Cloth),
            cube([-1.0, -4.5, -1.0], [1.0, -1.5, 1.0], $skin, 36).with(Pattern::Rot),
        ];
        const $head: &[Cuboid] = &[
            cube([-3.0, 0.25, -3.0], [3.0, 6.25, 3.0], $skin, 36).with(Pattern::Rot),
            cube([-2.5, 2.75, 3.0], [-0.8, 4.0, 3.05], ZOMBIE_EYES, 0),
            cube([0.8, 2.75, 3.0], [2.5, 4.0, 3.05], ZOMBIE_EYES, 0),
            cube([-1.6, 2.75, 3.05], [-0.9, 3.45, 3.08], [150, 170, 120], 0),
            cube([0.9, 2.75, 3.05], [1.6, 3.45, 3.08], [150, 170, 120], 0),
            cube([-1.5, 1.2, 3.0], [1.5, 1.7, 3.05], ZOMBIE_EYES, 10),
        ];
    };
}
baby_zombie!(
    BABY_ZOMBIE_BODY,
    BABY_ZOMBIE_LEG,
    BABY_ZOMBIE_ARM,
    BABY_ZOMBIE_HEAD,
    ZOMBIE_SKIN,
    ZOMBIE_SHIRT,
    ZOMBIE_PANTS
);
baby_zombie!(
    BABY_HUSK_BODY,
    BABY_HUSK_LEG,
    BABY_HUSK_ARM,
    BABY_HUSK_HEAD,
    [150, 128, 92],
    [118, 100, 70],
    [86, 72, 50]
);
baby_zombie!(
    BABY_DROWNED_BODY,
    BABY_DROWNED_LEG,
    BABY_DROWNED_ARM,
    BABY_DROWNED_HEAD,
    [104, 164, 152],
    [62, 118, 124],
    [48, 82, 120]
);

/// The baby zombie's parts' pivots: body, left and right leg, left and
/// right arm (as Java's turned through the origin, before
/// [`Parts::java_side`]) and head.
const BABY_ZOMBIE_LEG_AT: f32 = 4.0;
const BABY_ZOMBIE_ARM_AT: [f32; 3] = [3.0, 8.5, 0.0];
const BABY_ZOMBIE_HEAD_AT: f32 = 8.75;

/// The baby model's parts, or `None` for a kind drawn as its adult halved.
pub(super) fn pose(m: &Mob, time: f32, alpha: f32, head: Quat) -> Option<Parts> {
    if !has_model(m.kind) {
        return None;
    }
    let rx = Quat::from_rotation_x;
    let rz = Quat::from_rotation_z;
    // `QuadrupedModel` and the humanoids: `cos(pos * 0.6662) * 1.4 * speed`.
    let swing = walk_cos(m) * motion::LEG_SWING;
    use super::baby_rigs as b;
    Some(match m.kind {
        // v1.0 gap Track 7c.
        MobKind::Hoglin | MobKind::Zoglin => b::hoglin::pose(m),
        MobKind::Goat => b::goat::pose(m),
        MobKind::PolarBear => b::polar_bear::pose(m),
        MobKind::Panda => b::panda::pose(m, time),
        MobKind::Llama | MobKind::TraderLlama => b::llama::pose(m),
        MobKind::Camel => b::camel::pose(m, alpha),
        MobKind::Fox => b::fox::pose(m, time),
        MobKind::Cat | MobKind::Ocelot => b::feline::pose(m),
        MobKind::Rabbit => b::rabbit::pose(m, alpha),
        MobKind::Horse | MobKind::SkeletonHorse | MobKind::ZombieHorse | MobKind::Donkey | MobKind::Mule => {
            b::equine::pose(m, time, alpha)
        }
        MobKind::Turtle => b::turtle::pose(m),
        MobKind::Axolotl => b::axolotl::pose(m, alpha),
        MobKind::Bee => b::bee::pose(m, time, alpha),
        MobKind::Squid | MobKind::GlowSquid => b::squid::pose(m, alpha),
        MobKind::Dolphin => b::dolphin::pose(m, time),
        MobKind::Armadillo => b::armadillo::pose(m),
        MobKind::Piglin | MobKind::ZombifiedPiglin => b::piglin::pose(m, time),
        MobKind::Strider => b::strider::pose(m, time),
        MobKind::ZombieVillager => b::zombie_villager::pose(m, time),
        MobKind::Sniffer => b::snifflet::pose(m, time),
        MobKind::Nautilus => b::nautilus::pose(m, time),
        MobKind::Chicken => {
            // `ChickenModel.setupAnim`; the baby's head is its body, which
            // doesn't turn to look. Its wings (Java's right one on +X)
            // droop as they flap.
            let flap = super::flap_angle(m, alpha);
            parts![
                part(CHICK_BODY, [0.0, 3.75, 1.25], Quat::IDENTITY),
                part(CHICK_LEG, [-1.0, 2.0, -0.5], rx(swing)),
                part(CHICK_LEG, [1.0, 2.0, -0.5], rx(-swing)),
                part(CHICK_WING_R, [2.0, 4.0, 0.0], rz(-flap)),
                part(CHICK_WING_L, [-2.0, 4.0, 0.0], rz(flap)),
            ]
        }
        MobKind::Pig => quadruped(
            PIGLET_BODY,
            [0.0, 5.0, -0.5],
            PIGLET_HEAD,
            [0.0, 5.0, 2.0],
            head,
            PIGLET_LEG,
            [2.5, 2.0, 3.0, -4.0],
            swing,
        ),
        // `MushroomCowRenderer` draws the calf model (no mushrooms on a
        // baby); `passive::colour` paints its hide.
        MobKind::Cow | MobKind::Mooshroom => {
            quadruped(CALF_BODY, [0.0; 3], CALF_HEAD, [0.0, 10.5, 5.0], head, CALF_LEG, [2.5, 6.0, 3.5, -3.5], swing)
        }
        MobKind::Sheep => quadruped(
            LAMB_BODY,
            [0.0, 7.0, -0.5],
            LAMB_HEAD,
            [0.0, 8.5, 2.5],
            head,
            LAMB_LEG,
            [2.0, 5.0, 2.0, -3.0],
            swing,
        ),
        MobKind::Wolf => {
            // `WolfModel.setupAnim` as the adult's here: sitting folds the
            // legs, the tail droops with health and wags with the walk.
            let a = m.animal.as_deref();
            let sitting = a.is_some_and(|a| a.sit_pose || a.sleeping);
            let leg = |s: f32| rx(if sitting { 1.2 } else { swing * s });
            let tail_pitch = 1.0 - m.health / m.max_health() * 1.2;
            let wag = if m.ai != Ai::Chase { swing } else { 0.0 };
            let mut p = parts![
                part(PUP_BODY, [0.0, 5.0, 0.0], Quat::IDENTITY),
                part(PUP_HEAD, [0.0, 5.75, 4.0], head),
                // Right front, left front, right hind, left hind.
                part(PUP_LEG, [-1.5, 3.0, 3.0], leg(-1.0)),
                part(PUP_LEG, [1.5, 3.0, 3.0], leg(1.0)),
                part(PUP_LEG, [-1.5, 3.0, -3.0], leg(1.0)),
                part(PUP_LEG, [1.5, 3.0, -3.0], leg(-1.0)),
                part(PUP_TAIL, [0.0, 5.0, -3.0], Quat::from_rotation_y(-wag) * rx(tail_pitch)),
            ];
            if a.is_some_and(|a| a.owner.is_some()) {
                p.push(part(PUP_COLLAR, [0.0, 5.0, 0.0], Quat::IDENTITY));
            }
            p
        }
        MobKind::Villager => {
            // `VillagerModel`: legs at half stride; the arms stay folded.
            let mut p = parts![
                part(CHILD_BODY, [0.0; 3], Quat::IDENTITY),
                part(CHILD_LEG, [-1.0, 2.5, 0.0], rx(-swing * 0.5)),
                part(CHILD_LEG, [1.0, 2.5, 0.0], rx(swing * 0.5)),
                part(CHILD_HEAD, [0.0, 8.0, 0.0], head),
                part(CHILD_HANDS, [-3.0, 5.1, 0.96], rx(-std::f32::consts::FRAC_PI_3)),
                part(CHILD_ARM_BAR, [0.0, 5.6, 1.82], rx(-std::f32::consts::FRAC_PI_3)),
            ];
            p.java_side(1..3);
            p
        }
        _ => {
            let (body, leg, arm, head_box) = match m.kind {
                MobKind::Husk => (BABY_HUSK_BODY, BABY_HUSK_LEG, BABY_HUSK_ARM, BABY_HUSK_HEAD),
                MobKind::Drowned => (BABY_DROWNED_BODY, BABY_DROWNED_LEG, BABY_DROWNED_ARM, BABY_DROWNED_HEAD),
                _ => (BABY_ZOMBIE_BODY, BABY_ZOMBIE_LEG, BABY_ZOMBIE_ARM, BABY_ZOMBIE_HEAD),
            };
            // `ZombieModel` as the adult's (a baby holding something keeps
            // its arms down); its parts in the humanoid order so armor and
            // the held item attach.
            let mut h = zombie_limbs(m, time, m.main_hand().is_none());
            let swim = super::mob_anim_lerp(alpha, m.anim.o_swim, m.anim.swim);
            let head = if swim > 0.0 {
                drowned_swim(&mut h, swim, age_ticks(time));
                Quat::from_rotation_y(-m.head_yaw)
            } else {
                head
            };
            let [ax, ay, az] = BABY_ZOMBIE_ARM_AT;
            let mut p = parts![
                part(body, [0.0; 3], h.body()),
                part(leg, [-1.0, BABY_ZOMBIE_LEG_AT, 0.0], rx(h.left_leg)),
                part(leg, [1.0, BABY_ZOMBIE_LEG_AT, 0.0], rx(h.right_leg)),
                part(arm, h.arm_at([-ax, ay, az]), h.left_arm.quat()),
                part(arm, h.arm_at([ax, ay, az]), h.right_arm.quat()),
                part(head_box, [0.0, BABY_ZOMBIE_HEAD_AT, 0.0], head),
            ];
            p.java_side(0..5);
            if swim > 0.0 {
                let tip = swim * (-10.0 - head_x(m).to_degrees());
                p.turn_about(Vec3::new(0.0, m.shape().height as f32 * 8.0, 0.0), rx(-tip.to_radians()));
            }
            p
        }
    })
}

/// A `QuadrupedModel` baby: the body, the head looking, and four legs at
/// `[x, y, front z, hind z]`, the right hind and left front on the walk's
/// cosine and the other two against it (the right on -X).
#[allow(clippy::too_many_arguments)]
fn quadruped(
    body: &'static [Cuboid],
    body_at: [f32; 3],
    head_box: &'static [Cuboid],
    head_at: [f32; 3],
    head: Quat,
    leg: &'static [Cuboid],
    [x, y, front, hind]: [f32; 4],
    swing: f32,
) -> Parts {
    let rx = Quat::from_rotation_x;
    parts![
        part(body, body_at, Quat::IDENTITY),
        part(leg, [-x, y, hind], rx(swing)),
        part(leg, [x, y, hind], rx(-swing)),
        part(leg, [-x, y, front], rx(-swing)),
        part(leg, [x, y, front], rx(swing)),
        part(head_box, head_at, head),
    ]
}

/// v1.0 gap Track 7b: armor on a baby zombie, as Java's baby armor (the
/// adult's pieces scaled by `HumanoidModel.BABY_TRANSFORMER`: the body and
/// limbs halved, the head three quarters) on the baby's own pivots: the
/// head piece, drawn at `HEAD_ARMOR` of the mob's scale, and the rest at
/// `BODY_ARMOR`, each limb as long as the baby's own.
pub(super) const HEAD_ARMOR: f32 = 0.75;
pub(super) const BODY_ARMOR: f32 = 0.5;

pub(super) fn armor(
    parts: &[Part],
) -> ([crate::entity::player_model::Limb; 6], [crate::entity::player_model::Limb; 6]) {
    let at = |p: &Part, k: f32, min: [f32; 3], max: [f32; 3], uv: [f32; 2]| crate::entity::player_model::Limb {
        pivot: p.pivot / k,
        rot: p.rot,
        min,
        max,
        uv,
    };
    let arm = |p: &Part, uv| at(p, BODY_ARMOR, [-2.0, -10.0, -2.0], [2.0, 2.0, 2.0], uv);
    let leg = |p: &Part, uv| at(p, BODY_ARMOR, [-2.0, -8.0, -2.0], [2.0, 0.0, 2.0], uv);
    let body = [
        at(&parts[5], BODY_ARMOR, [-4.0, 0.0, -4.0], [4.0, 8.0, 4.0], [0.0, 0.0]),
        at(&parts[0], BODY_ARMOR, [-4.0, 8.0, -2.0], [4.0, 18.0, 2.0], [16.0, 16.0]),
        arm(&parts[4], [40.0, 16.0]),
        arm(&parts[3], [32.0, 48.0]),
        leg(&parts[2], [0.0, 16.0]),
        leg(&parts[1], [16.0, 48.0]),
    ];
    let mut head = body;
    head[0] = at(&parts[5], HEAD_ARMOR, [-4.0, 0.0, -4.0], [4.0, 8.0, 4.0], [0.0, 0.0]);
    (head, body)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn bounds(m: &Mob) -> (Vec3, Vec3) {
        let mut out = EntityMesh::flat();
        build(std::slice::from_ref(m), DVec3::new(0.0, 64.0, 0.0), Vec3::X, 100.0, 1.0, &mut out);
        out.iter().fold((Vec3::splat(f32::MAX), Vec3::splat(f32::MIN)), |(lo, hi), v| {
            let p = Vec3::from_array(v.pos);
            (lo.min(p), hi.max(p))
        })
    }

    /// The babies draw their own models at full scale, standing on the
    /// ground and about their (halved) boxes' height, with heads larger
    /// for their bodies than their adults' halved.
    #[test]
    fn babies_draw_their_own_models_on_the_ground() {
        for kind in MobKind::ALL.into_iter().filter(|&k| has_model(k)) {
            let mut m = Mob::new(kind, DVec3::new(10.0, 64.0, 0.0), 0.0);
            m.baby = true;
            m.age = crate::entity::animals::BABY_AGE;
            let (lo, hi) = bounds(&m);
            assert!(lo.y.abs() < 1e-3, "{kind:?} feet at {}", lo.y);
            let height = hi.y - lo.y;
            let adult = kind.shape().height as f32;
            assert!(height > adult * 0.3 && height < adult * 0.75, "{kind:?} {height} vs adult {adult}");
            let parts = pose(&m, 0.0, 1.0, Quat::IDENTITY).unwrap();
            assert!(parts.len() >= 5, "{kind:?}");
        }
    }

    /// A baby zombie's sword hand is its right (on -X), like the adult's.
    #[test]
    fn a_baby_zombie_holds_with_its_right_hand() {
        let m = {
            let mut m = Mob::new(MobKind::Zombie, DVec3::ZERO, 0.0);
            m.baby = true;
            m
        };
        let p = pose(&m, 0.0, 1.0, Quat::IDENTITY).unwrap();
        assert!(p[4].pivot.x < 0.0 && p[3].pivot.x > 0.0);
    }
}
