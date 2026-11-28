//! v1.0 gap track 6c: Java 26.3's item display transforms, for every item.
//!
//! An item model's `display` places it in each context it is drawn in
//! (`ItemDisplayContext`): a slot (`gui`), dropped (`ground`), an item frame
//! (`fixed`), the first- and third-person right hand and a shelf
//! (`on_shelf`). Java merges `display` per context down the model's parent
//! chain; the 42 distinct sets the 26.3 client's items resolve to are the
//! constants below, read from the client's `models/`
//! (`tools/extract_java_item_model_fixtures.py` records each item's set in
//! `tests/fixtures/java/26.3/item_models.json`, and
//! `tests/java_item_models.rs` checks [`of`] against it for every item).
//!
//! [`Context::matrix`] is `ItemTransform.apply` followed by the item
//! renderer's `translate(-0.5)`, from model space (a block is 0..1) to the
//! context's space. [`gui_shade`] is the GUI lighting of a model turned
//! that way (`Lighting.Entry.ITEMS_3D` and `ITEMS_FLAT`).

use glam::{EulerRot, Mat4, Quat, Vec3};

use crate::inventory::Stack;
use crate::item::Item;

/// A display transform (`ItemTransform`): rotation in degrees (Java's XYZ
/// order), translation in sixteenths, scale (uniform for every 26.3 item).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Transform {
    pub rotation: [f32; 3],
    pub translation: [f32; 3],
    pub scale: f32,
}

impl Transform {
    pub const fn new(rotation: [f32; 3], translation: [f32; 3], scale: f32) -> Self {
        Self { rotation, translation, scale }
    }

    /// `ItemTransform.apply`: translate, rotate, scale.
    pub fn matrix(&self) -> Mat4 {
        let [rx, ry, rz] = self.rotation.map(f32::to_radians);
        Mat4::from_translation(Vec3::from_array(self.translation) / 16.0)
            * Mat4::from_quat(Quat::from_euler(EulerRot::XYZ, rx, ry, rz))
            * Mat4::from_scale(Vec3::splat(self.scale))
    }
}

const fn t(rotation: [f32; 3], translation: [f32; 3], scale: f32) -> Transform {
    Transform::new(rotation, translation, scale)
}

/// Where an item is drawn (`ItemDisplayContext`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Context {
    Gui,
    Ground,
    /// An item frame.
    Fixed,
    FirstPersonRightHand,
    /// v1.0 gap track 6c: held by a player seen from outside or by a mob.
    ThirdPersonRightHand,
    /// v1.0 gap track 6c: on a shelf (`on_shelf`).
    OnShelf,
}

impl Context {
    /// The model-space to context-space matrix for `display`: Java's
    /// display transform, then `translate(-0.5, -0.5, -0.5)`.
    pub fn matrix(self, display: &Display) -> Mat4 {
        display.transform(self).matrix() * Mat4::from_translation(Vec3::splat(-0.5))
    }
}

/// A model's display transforms and GUI light.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Display {
    pub gui: Transform,
    pub ground: Transform,
    pub fixed: Transform,
    pub first_person: Transform,
    pub third_person: Transform,
    pub on_shelf: Transform,
    /// `gui_light: front` (lit flat from the front) rather than `side`.
    pub front_light: bool,
}

impl Display {
    pub fn transform(&self, context: Context) -> Transform {
        match context {
            Context::Gui => self.gui,
            Context::Ground => self.ground,
            Context::Fixed => self.fixed,
            Context::FirstPersonRightHand => self.first_person,
            Context::ThirdPersonRightHand => self.third_person,
            Context::OnShelf => self.on_shelf,
        }
    }
}

/// `item/generated`: flat sprites.
pub const GENERATED: Display = Display {
    gui: t([0.0, 0.0, 0.0], [0.0, 0.0, 0.0], 1.0),
    ground: t([0.0, 0.0, 0.0], [0.0, 2.0, 0.0], 0.5),
    fixed: t([0.0, 180.0, 0.0], [0.0, 0.0, 0.0], 1.0),
    first_person: t([0.0, -90.0, 25.0], [1.13, 3.2, 1.13], 0.68),
    third_person: t([0.0, 0.0, 0.0], [0.0, 3.0, 1.0], 0.55),
    on_shelf: t([0.0, 0.0, 0.0], [0.0, 0.0, 0.0], 1.0),
    front_light: true,
};
/// `block/block`.
pub const BLOCK: Display = Display {
    gui: t([30.0, 225.0, 0.0], [0.0, 0.0, 0.0], 0.625),
    ground: t([0.0, 0.0, 0.0], [0.0, 3.0, 0.0], 0.25),
    fixed: t([0.0, 0.0, 0.0], [0.0, 0.0, 0.0], 0.5),
    first_person: t([0.0, 45.0, 0.0], [0.0, 0.0, 0.0], 0.4),
    third_person: t([75.0, 45.0, 0.0], [0.0, 2.5, 0.0], 0.375),
    on_shelf: t([0.0, 180.0, 0.0], [0.0, 0.0, 0.0], 1.0),
    front_light: false,
};
/// `block/fence_inventory` and `block/wall_inventory`.
pub const FENCE_INVENTORY: Display = Display {
    gui: t([30.0, 135.0, 0.0], [0.0, 0.0, 0.0], 0.625),
    ground: t([0.0, 0.0, 0.0], [0.0, 3.0, 0.0], 0.25),
    fixed: t([0.0, 90.0, 0.0], [0.0, 0.0, 0.0], 0.5),
    first_person: t([0.0, 45.0, 0.0], [0.0, 0.0, 0.0], 0.4),
    third_person: t([75.0, 45.0, 0.0], [0.0, 2.5, 0.0], 0.375),
    on_shelf: t([0.0, 90.0, 0.0], [0.0, 0.0, 0.0], 1.0),
    front_light: false,
};
/// `block/template_fence_gate`.
pub const GATE: Display = Display {
    gui: t([30.0, 45.0, 0.0], [0.0, -1.0, 0.0], 0.8),
    ground: t([0.0, 0.0, 0.0], [0.0, 3.0, 0.0], 0.25),
    fixed: t([0.0, 0.0, 0.0], [0.0, 0.0, 0.0], 0.5),
    first_person: t([0.0, 45.0, 0.0], [0.0, 0.0, 0.0], 0.4),
    third_person: t([75.0, 45.0, 0.0], [0.0, 2.5, 0.0], 0.375),
    on_shelf: t([0.0, 180.0, 0.0], [0.0, 0.0, 0.0], 1.0),
    front_light: false,
};
/// `block/thin_block`: trapdoors, pressure plates, carpets, snow, the daylight detector.
pub const THIN_BLOCK: Display = Display {
    gui: t([30.0, 225.0, 0.0], [0.0, 0.0, 0.0], 0.625),
    ground: t([0.0, 0.0, 0.0], [0.0, 3.0, 0.0], 0.25),
    fixed: t([0.0, 0.0, 0.0], [0.0, 0.0, 0.0], 0.5),
    first_person: t([0.0, 45.0, 0.0], [0.0, 4.2, 0.0], 0.4),
    third_person: t([75.0, 45.0, 0.0], [0.0, 2.5, 2.0], 0.375),
    on_shelf: t([0.0, 180.0, 0.0], [0.0, 0.0, 0.0], 1.0),
    front_light: false,
};
/// The shelves' inventory models.
pub const SHELF: Display = Display {
    gui: t([30.0, 225.0, 0.0], [2.5, -1.5, 0.0], 0.625),
    ground: t([0.0, 0.0, 0.0], [0.0, 3.0, -1.375], 0.25),
    fixed: t([0.0, 0.0, 0.0], [0.0, 0.0, -4.0], 0.5),
    first_person: t([0.0, 135.0, 0.0], [0.0, -2.0, 0.0], 0.5),
    third_person: t([75.0, 135.0, 0.0], [-3.0, 1.0, 0.0], 0.4),
    on_shelf: t([0.0, 180.0, 0.0], [0.0, 0.0, 4.0], 1.0),
    front_light: false,
};
/// `block/stairs`.
pub const STAIRS: Display = Display {
    gui: t([30.0, 135.0, 0.0], [0.0, 0.0, 0.0], 0.625),
    ground: t([0.0, 0.0, 0.0], [0.0, 3.0, 0.0], 0.25),
    fixed: t([0.0, 0.0, 0.0], [0.0, 0.0, 0.0], 0.5),
    first_person: t([0.0, 45.0, 0.0], [0.0, 0.0, 0.0], 0.4),
    third_person: t([75.0, 45.0, 0.0], [0.0, 2.5, 0.0], 0.375),
    on_shelf: t([0.0, 180.0, 0.0], [0.0, 0.0, 0.0], 1.0),
    front_light: false,
};
/// `block/template_anvil`.
pub const ANVIL: Display = Display {
    gui: t([30.0, 225.0, 0.0], [0.0, 0.0, 0.0], 0.625),
    ground: t([0.0, 0.0, 0.0], [0.0, 3.0, 0.0], 0.25),
    fixed: t([0.0, 90.0, 0.0], [0.0, 0.0, 0.0], 0.5),
    first_person: t([0.0, 45.0, 0.0], [0.0, 0.0, 0.0], 0.4),
    third_person: t([75.0, 45.0, 0.0], [0.0, 2.5, 0.0], 0.375),
    on_shelf: t([0.0, 90.0, 0.0], [0.0, 0.0, 0.0], 1.0),
    front_light: false,
};
/// `item/handheld`: tools, sticks, rods and bones.
pub const HANDHELD: Display = Display {
    gui: t([0.0, 0.0, 0.0], [0.0, 0.0, 0.0], 1.0),
    ground: t([0.0, 0.0, 0.0], [0.0, 2.0, 0.0], 0.5),
    fixed: t([0.0, 180.0, 0.0], [0.0, 0.0, 0.0], 1.0),
    first_person: t([0.0, -90.0, 25.0], [1.13, 3.2, 1.13], 0.68),
    third_person: t([0.0, -90.0, 55.0], [0.0, 4.0, 0.5], 0.85),
    on_shelf: t([0.0, 0.0, 0.0], [0.0, 0.0, 0.0], 1.0),
    front_light: true,
};
/// `block/orientable` and the glazed terracotta, pumpkins, hives and looms that share its turn.
pub const ORIENTABLE: Display = Display {
    gui: t([30.0, 225.0, 0.0], [0.0, 0.0, 0.0], 0.625),
    ground: t([0.0, 0.0, 0.0], [0.0, 3.0, 0.0], 0.25),
    fixed: t([0.0, 0.0, 0.0], [0.0, 0.0, 0.0], 0.5),
    first_person: t([0.0, 135.0, 0.0], [0.0, 0.0, 0.0], 0.4),
    third_person: t([75.0, 45.0, 0.0], [0.0, 2.5, 0.0], 0.375),
    on_shelf: t([0.0, 180.0, 0.0], [0.0, 0.0, 0.0], 1.0),
    front_light: false,
};
/// `block/big_dripleaf`.
pub const BIG_DRIPLEAF: Display = Display {
    gui: t([30.0, 225.0, 0.0], [0.0, -2.0, 0.0], 0.625),
    ground: t([0.0, 0.0, 0.0], [0.0, 3.0, 0.0], 0.25),
    fixed: t([0.0, 0.0, 0.0], [0.0, 0.0, -1.0], 0.5),
    first_person: t([0.0, 0.0, 0.0], [1.13, 0.0, 1.13], 0.68),
    third_person: t([0.0, 0.0, 0.0], [0.0, 1.0, 0.0], 0.55),
    on_shelf: t([0.0, 180.0, 0.0], [0.0, 0.0, 0.0], 1.0),
    front_light: false,
};
/// `item/template_banner`.
pub const BANNER: Display = Display {
    gui: t([30.0, 20.0, 0.0], [0.0, -3.25, 0.0], 0.5325),
    ground: t([0.0, 0.0, 0.0], [0.0, 1.0, 0.0], 0.25),
    fixed: t([0.0, 180.0, 0.0], [0.0, 0.0, 0.0], 0.5),
    first_person: t([0.0, 90.0, 0.0], [0.0, 0.0, 0.0], 0.375),
    third_person: t([0.0, 90.0, 0.0], [0.0, 2.0, 0.5], 0.375),
    on_shelf: t([0.0, 0.0, 0.0], [0.0, 0.0, 0.0], 1.0),
    front_light: true,
};
/// `block/template_bed`.
pub const BED: Display = Display {
    gui: t([30.0, 340.0, 0.0], [2.0, 3.0, 0.0], 0.5325),
    ground: t([0.0, 180.0, 0.0], [0.0, 1.0, 2.0], 0.25),
    fixed: t([270.0, 180.0, 0.0], [0.0, 4.0, -2.0], 0.5),
    first_person: t([30.0, 340.0, 0.0], [0.0, 3.0, 0.0], 0.375),
    third_person: t([30.0, 340.0, 0.0], [0.0, 3.0, -2.0], 0.23),
    on_shelf: t([90.0, 0.0, 0.0], [0.0, 14.5, 2.5], 0.9375),
    front_light: false,
};
/// `item/chest` and `item/shulker_box` (special renderers).
pub const CHEST: Display = Display {
    gui: t([30.0, 45.0, 0.0], [0.0, 0.0, 0.0], 0.625),
    ground: t([0.0, 0.0, 0.0], [0.0, 3.0, 0.0], 0.25),
    fixed: t([0.0, 180.0, 0.0], [0.0, 0.0, 0.0], 0.5),
    first_person: t([0.0, 315.0, 0.0], [0.0, 0.0, 0.0], 0.4),
    third_person: t([75.0, 315.0, 0.0], [0.0, 2.5, 0.0], 0.375),
    on_shelf: t([0.0, 0.0, 0.0], [0.0, 0.0, 0.0], 1.0),
    front_light: false,
};
/// `item/bow`.
pub const BOW: Display = Display {
    gui: t([0.0, 0.0, 0.0], [0.0, 0.0, 0.0], 1.0),
    ground: t([0.0, 0.0, 0.0], [0.0, 2.0, 0.0], 0.5),
    fixed: t([0.0, 180.0, 0.0], [0.0, 0.0, 0.0], 1.0),
    first_person: t([0.0, -90.0, 25.0], [1.13, 3.2, 1.13], 0.68),
    third_person: t([-80.0, 260.0, -40.0], [-1.0, -2.0, 2.5], 0.9),
    on_shelf: t([0.0, 0.0, 0.0], [0.0, 0.0, 0.0], 1.0),
    front_light: true,
};
/// `item/brush`.
pub const BRUSH: Display = Display {
    gui: t([0.0, 0.0, 0.0], [0.0, 0.0, 0.0], 1.0),
    ground: t([0.0, 0.0, 0.0], [0.0, 2.0, 0.0], 0.5),
    fixed: t([0.0, 180.0, 0.0], [0.0, 0.0, 0.0], 1.0),
    first_person: t([0.0, -90.0, 25.0], [1.13, 3.2, 1.13], 0.68),
    third_person: t([0.0, 0.0, 45.0], [0.0, 4.0, 0.0], 0.9),
    on_shelf: t([0.0, 0.0, 0.0], [0.0, 0.0, 0.0], 1.0),
    front_light: true,
};
/// `block/calibrated_sculk_sensor_inactive`.
pub const CALIBRATED_SCULK_SENSOR: Display = Display {
    gui: t([30.0, 45.0, 0.0], [0.0, 0.0, 0.0], 0.625),
    ground: t([0.0, 0.0, 0.0], [0.0, 3.0, 0.0], 0.25),
    fixed: t([0.0, 0.0, 0.0], [0.0, 0.0, 0.0], 0.5),
    first_person: t([0.0, 45.0, 0.0], [0.0, 0.0, 0.0], 0.4),
    third_person: t([75.0, 45.0, 0.0], [0.0, 2.5, 0.0], 0.375),
    on_shelf: t([0.0, 180.0, 0.0], [0.0, 0.0, 0.0], 1.0),
    front_light: true,
};
/// `item/handheld_rod`.
pub const ROD: Display = Display {
    gui: t([0.0, 0.0, 0.0], [0.0, 0.0, 0.0], 1.0),
    ground: t([0.0, 0.0, 0.0], [0.0, 2.0, 0.0], 0.5),
    fixed: t([0.0, 180.0, 0.0], [0.0, 0.0, 0.0], 1.0),
    first_person: t([0.0, 90.0, 25.0], [0.0, 1.6, 0.8], 0.68),
    third_person: t([0.0, 90.0, 55.0], [0.0, 4.0, 2.5], 0.85),
    on_shelf: t([0.0, 0.0, 0.0], [0.0, 0.0, 0.0], 1.0),
    front_light: true,
};
/// `item/conduit`.
pub const CONDUIT: Display = Display {
    gui: t([30.0, 45.0, 0.0], [0.0, 0.0, 0.0], 1.0),
    ground: t([0.0, 0.0, 0.0], [0.0, 3.0, 0.0], 0.5),
    fixed: t([0.0, 180.0, 0.0], [0.0, 0.0, 0.0], 1.0),
    first_person: t([0.0, 315.0, 0.0], [0.0, 0.0, 0.0], 0.8),
    third_person: t([75.0, 315.0, 0.0], [0.0, 2.5, 0.0], 0.5),
    on_shelf: t([0.0, 0.0, 0.0], [0.0, 0.0, 0.0], 1.0),
    front_light: false,
};
/// `item/template_copper_golem_statue`.
pub const STATUE: Display = Display {
    gui: t([30.0, 45.0, 180.0], [0.0, 2.0, 0.0], 0.55),
    ground: t([0.0, 0.0, 180.0], [0.0, 0.0, 0.0], 0.35),
    fixed: t([0.0, 180.0, 180.0], [0.0, 3.0, 0.0], 0.5),
    first_person: t([0.0, -45.0, 180.0], [0.0, 5.0, 0.0], 0.5),
    third_person: t([0.0, -45.0, 180.0], [0.0, 5.0, 0.0], 0.5),
    on_shelf: t([0.0, 0.0, 180.0], [0.0, 8.0, 0.0], 1.0),
    front_light: false,
};
/// `item/template_skull`.
pub const SKULL: Display = Display {
    gui: t([30.0, 45.0, 0.0], [0.0, 3.0, 0.0], 1.0),
    ground: t([0.0, 0.0, 0.0], [0.0, 3.0, 0.0], 0.5),
    fixed: t([0.0, 180.0, 0.0], [0.0, 4.0, 0.0], 1.0),
    first_person: t([0.0, 0.0, 0.0], [0.0, 0.0, 0.0], 1.0),
    third_person: t([45.0, 45.0, 0.0], [0.0, 3.0, 0.0], 0.5),
    on_shelf: t([0.0, 0.0, 0.0], [0.0, 8.0, 0.0], 2.0),
    front_light: false,
};
/// `item/crossbow`.
pub const CROSSBOW: Display = Display {
    gui: t([0.0, 0.0, 0.0], [0.0, 0.0, 0.0], 1.0),
    ground: t([0.0, 0.0, 0.0], [0.0, 2.0, 0.0], 0.5),
    fixed: t([0.0, 180.0, 0.0], [0.0, 0.0, 0.0], 1.0),
    first_person: t([-90.0, 0.0, -55.0], [1.13, 3.2, 1.13], 0.68),
    third_person: t([-90.0, 0.0, -60.0], [2.0, 0.1, -3.0], 0.9),
    on_shelf: t([0.0, 0.0, 0.0], [0.0, 0.0, 0.0], 1.0),
    front_light: true,
};
/// `item/decorated_pot`.
pub const DECORATED_POT: Display = Display {
    gui: t([30.0, 45.0, 0.0], [0.0, 0.0, 0.0], 0.6),
    ground: t([0.0, 0.0, 0.0], [0.0, 1.0, 0.0], 0.25),
    fixed: t([0.0, 180.0, 0.0], [0.0, 0.0, 0.0], 0.5),
    first_person: t([0.0, 90.0, 0.0], [0.0, 0.0, 0.0], 0.375),
    third_person: t([0.0, 90.0, 0.0], [0.0, 2.0, 0.5], 0.375),
    on_shelf: t([0.0, 0.0, 0.0], [0.0, 0.0, 0.0], 1.0),
    front_light: true,
};
/// `item/dragon_head`.
pub const DRAGON_HEAD: Display = Display {
    gui: t([30.0, 45.0, 0.0], [-2.0, 2.0, 0.0], 0.6),
    ground: t([0.0, 0.0, 0.0], [0.0, 3.0, 0.0], 0.5),
    fixed: t([0.0, 180.0, 0.0], [0.0, 4.0, 0.0], 1.0),
    first_person: t([0.0, 0.0, 0.0], [0.0, 0.0, 0.0], 1.0),
    third_person: t([0.0, 180.0, 0.0], [0.0, -1.0, 2.0], 0.5),
    on_shelf: t([0.0, 0.0, 0.0], [0.0, 2.0, 0.0], 1.25),
    front_light: false,
};
/// `block/dried_ghast`.
pub const DRIED_GHAST: Display = Display {
    gui: t([30.0, 225.0, 0.0], [0.4, 1.6, 0.0], 0.8),
    ground: t([0.0, 0.0, 0.0], [0.0, 3.0, 0.0], 0.25),
    fixed: t([0.0, 0.0, 0.0], [0.0, 0.0, 0.0], 0.5),
    first_person: t([0.0, 45.0, 0.0], [0.0, 0.0, 0.0], 0.4),
    third_person: t([75.0, 45.0, 0.0], [0.0, 2.5, 0.0], 0.375),
    on_shelf: t([0.0, 180.0, 0.0], [0.0, 0.0, 0.0], 1.0),
    front_light: false,
};
/// `block/end_rod`.
pub const END_ROD: Display = Display {
    gui: t([30.0, 225.0, 0.0], [0.0, 0.0, 0.0], 0.625),
    ground: t([0.0, 0.0, 0.0], [0.0, 3.0, 0.0], 0.25),
    fixed: t([0.0, 0.0, 0.0], [0.0, 0.0, 0.0], 0.5),
    first_person: t([0.0, 45.0, 0.0], [0.0, 0.0, 0.0], 0.4),
    third_person: t([0.0, 0.0, 0.0], [0.0, 0.0, 0.0], 0.375),
    on_shelf: t([0.0, 180.0, 0.0], [0.0, 0.0, 0.0], 1.0),
    front_light: false,
};
/// `block/lightning_rod`.
pub const LIGHTNING_ROD: Display = Display {
    gui: t([30.0, 225.0, 0.0], [0.0, 0.0, 0.0], 0.625),
    ground: t([0.0, 0.0, 0.0], [0.0, 3.0, 0.0], 0.25),
    fixed: t([0.0, 0.0, 0.0], [0.0, 0.0, 0.0], 0.5),
    first_person: t([0.0, 45.0, 0.0], [0.0, 0.0, 0.0], 0.4),
    third_person: t([0.0, 0.0, 0.0], [0.0, 2.0, 0.5], 0.4),
    on_shelf: t([0.0, 180.0, 0.0], [0.0, 0.0, 0.0], 1.0),
    front_light: false,
};
/// `item/goat_horn`.
pub const GOAT_HORN: Display = Display {
    gui: t([0.0, 0.0, 0.0], [0.0, 0.0, 0.0], 1.0),
    ground: t([0.0, 0.0, 0.0], [0.0, 2.0, 0.0], 0.5),
    fixed: t([0.0, 180.0, 0.0], [0.0, 0.0, 0.0], 1.0),
    first_person: t([0.0, -90.0, 25.0], [1.13, 3.2, 1.13], 0.68),
    third_person: t([0.0, 180.0, 0.0], [0.0, 3.0, 1.0], 0.55),
    on_shelf: t([0.0, 0.0, 0.0], [0.0, 0.0, 0.0], 1.0),
    front_light: true,
};
/// `item/hanging_roots`.
pub const HANGING_ROOTS: Display = Display {
    gui: t([0.0, 0.0, 0.0], [0.0, 0.0, 0.0], 1.0),
    ground: t([0.0, 0.0, 0.0], [0.0, 2.0, 0.0], 0.5),
    fixed: t([0.0, 180.0, 0.0], [0.0, 0.0, 0.0], 1.0),
    first_person: t([0.0, -90.0, 25.0], [1.13, 0.0, 1.13], 0.68),
    third_person: t([0.0, 0.0, 0.0], [0.0, 0.0, 1.0], 0.55),
    on_shelf: t([0.0, 0.0, 0.0], [0.0, 0.0, 0.0], 1.0),
    front_light: true,
};
/// `block/heavy_core`.
pub const HEAVY_CORE: Display = Display {
    gui: t([30.0, 225.0, 0.0], [0.0, 3.0, 0.0], 1.0),
    ground: t([0.0, 0.0, 0.0], [0.0, 3.0, 0.0], 0.5),
    fixed: t([0.0, 180.0, 0.0], [0.0, 4.0, 0.0], 1.0),
    first_person: t([0.0, 0.0, 0.0], [0.0, 0.0, 0.0], 1.0),
    third_person: t([45.0, 45.0, 0.0], [0.0, 3.0, 0.0], 0.5),
    on_shelf: t([0.0, 0.0, 0.0], [0.0, 0.0, 0.0], 1.0),
    front_light: false,
};
/// `item/large_amethyst_bud`.
pub const LARGE_BUD: Display = Display {
    gui: t([0.0, 0.0, 0.0], [0.0, 2.0, 0.0], 1.0),
    ground: t([0.0, 0.0, 0.0], [0.0, 2.0, 0.0], 0.5),
    fixed: t([0.0, 0.0, 0.0], [0.0, 4.0, 0.0], 1.0),
    first_person: t([0.0, -90.0, 25.0], [0.0, 5.0, 0.0], 0.68),
    third_person: t([0.0, 0.0, 0.0], [0.0, 4.0, 1.0], 0.55),
    on_shelf: t([0.0, 0.0, 0.0], [0.0, 0.0, 0.0], 1.0),
    front_light: true,
};
/// `block/lectern`.
pub const LECTERN: Display = Display {
    gui: t([30.0, 225.0, 0.0], [0.0, 0.0, 0.0], 0.6),
    ground: t([0.0, 0.0, 0.0], [0.0, 3.0, 0.0], 0.25),
    fixed: t([0.0, 0.0, 0.0], [0.0, 0.0, 0.0], 0.5),
    first_person: t([0.0, 135.0, 0.0], [0.0, 0.0, 0.0], 0.4),
    third_person: t([75.0, 45.0, 0.0], [0.0, 2.5, 0.0], 0.375),
    on_shelf: t([0.0, 180.0, 0.0], [0.0, 0.0, 0.0], 1.0),
    front_light: false,
};
/// `item/mace`.
pub const MACE: Display = Display {
    gui: t([0.0, 0.0, 0.0], [0.0, 0.0, 0.0], 1.0),
    ground: t([0.0, 0.0, 0.0], [0.0, 2.0, 0.0], 0.5),
    fixed: t([0.0, 180.0, 0.0], [0.0, 0.0, 0.0], 1.0),
    first_person: t([0.0, -90.0, 25.0], [0.0, 3.0, 0.8], 0.9),
    third_person: t([0.0, -90.0, 55.0], [0.0, 4.0, 1.0], 1.0),
    on_shelf: t([0.0, 0.0, 0.0], [0.0, 0.0, 0.0], 1.0),
    front_light: true,
};
/// `item/medium_amethyst_bud`.
pub const MEDIUM_BUD: Display = Display {
    gui: t([0.0, 0.0, 0.0], [0.0, 2.0, 0.0], 1.0),
    ground: t([0.0, 0.0, 0.0], [0.0, 2.0, 0.0], 0.5),
    fixed: t([0.0, 0.0, 0.0], [0.0, 6.0, 0.0], 1.0),
    first_person: t([0.0, -90.0, 25.0], [0.0, 5.0, 0.0], 0.68),
    third_person: t([0.0, 0.0, 0.0], [0.0, 4.0, 1.0], 0.55),
    on_shelf: t([0.0, 0.0, 0.0], [0.0, 0.0, 0.0], 1.0),
    front_light: true,
};
/// `item/template_music_disc`.
pub const MUSIC_DISC: Display = Display {
    gui: t([0.0, 0.0, 0.0], [0.0, 0.0, 0.0], 1.0),
    ground: t([0.0, 0.0, 0.0], [0.0, 2.0, 0.0], 0.5),
    fixed: t([0.0, 180.0, 0.0], [-0.5, 0.0, 0.0], 1.0),
    first_person: t([0.0, -90.0, 25.0], [1.13, 3.2, 1.13], 0.68),
    third_person: t([0.0, 0.0, 0.0], [0.0, 3.0, 1.0], 0.55),
    on_shelf: t([0.0, 0.0, 0.0], [0.0, 0.0, 0.0], 1.0),
    front_light: true,
};
/// `item/pointed_dripstone` and the sulfur spike.
pub const DRIPSTONE: Display = Display {
    gui: t([0.0, 0.0, 0.0], [0.0, 0.0, 0.0], 1.0),
    ground: t([0.0, 0.0, 0.0], [0.0, 2.0, 0.0], 0.5),
    fixed: t([0.0, 180.0, 0.0], [0.0, 0.0, 0.0], 1.0),
    first_person: t([0.0, 100.0, 0.0], [0.0, -2.0, 0.0], 0.9),
    third_person: t([0.0, 100.0, 0.0], [-1.0, -1.0, 0.0], 0.9),
    on_shelf: t([0.0, 0.0, 0.0], [0.0, 0.0, 0.0], 1.0),
    front_light: true,
};
/// `block/shelf_mushroom`.
pub const SHELF_MUSHROOM: Display = Display {
    gui: t([30.0, 225.0, 0.0], [3.25, -3.0, 0.0], 1.0),
    ground: t([0.0, 0.0, 0.0], [0.0, 2.55, -1.35], 0.3),
    fixed: t([0.0, 0.0, 0.0], [0.0, -1.0, -6.75], 1.0),
    first_person: t([0.0, 45.0, 0.0], [0.0, 0.0, 0.0], 0.5),
    third_person: t([53.0, 0.0, 0.0], [0.0, 2.25, 0.25], 0.375),
    on_shelf: t([0.0, 180.0, 0.0], [0.0, 0.0, 4.0], 1.0),
    front_light: true,
};
/// `item/shield`.
pub const SHIELD: Display = Display {
    gui: t([15.0, -25.0, -5.0], [2.0, 3.0, 0.0], 0.65),
    ground: t([0.0, 0.0, 0.0], [2.0, 4.0, 2.0], 0.25),
    fixed: t([0.0, 180.0, 0.0], [-4.5, 4.5, -5.0], 0.55),
    first_person: t([0.0, 180.0, 5.0], [-10.0, 1.75, -10.0], 1.25),
    third_person: t([0.0, 90.0, 0.0], [10.0, 6.0, -4.0], 1.0),
    on_shelf: t([0.0, 0.0, 0.0], [11.0, 18.5, 8.7], 1.4),
    front_light: true,
};
/// `item/small_amethyst_bud`.
pub const SMALL_BUD: Display = Display {
    gui: t([0.0, 0.0, 0.0], [0.0, 2.0, 0.0], 1.0),
    ground: t([0.0, 0.0, 0.0], [0.0, 2.0, 0.0], 0.5),
    fixed: t([0.0, 0.0, 0.0], [0.0, 7.0, 0.0], 1.0),
    first_person: t([0.0, -90.0, 25.0], [0.0, 6.0, 0.0], 0.68),
    third_person: t([0.0, 0.0, 0.0], [0.0, 4.0, 1.0], 0.55),
    on_shelf: t([0.0, 0.0, 0.0], [0.0, 0.0, 0.0], 1.0),
    front_light: true,
};
/// `item/small_dripleaf`.
pub const SMALL_DRIPLEAF: Display = Display {
    gui: t([30.0, 225.0, 0.0], [0.0, 0.0, 0.0], 0.625),
    ground: t([0.0, 0.0, 0.0], [0.0, 3.0, 0.0], 0.25),
    fixed: t([0.0, 0.0, 0.0], [0.0, 0.0, 0.0], 0.5),
    first_person: t([0.0, 45.0, 0.0], [0.0, 3.2, 0.0], 0.4),
    third_person: t([0.0, 0.0, 0.0], [0.0, 4.0, 1.0], 0.55),
    on_shelf: t([0.0, 180.0, 0.0], [0.0, 0.0, 0.0], 1.0),
    front_light: false,
};
/// The straw bed.
pub const STRAW_BED: Display = Display {
    gui: t([30.0, 340.0, 0.0], [2.0, 3.0, 0.0], 0.5325),
    ground: t([0.0, 180.0, 0.0], [0.0, 1.0, 2.0], 0.25),
    fixed: t([270.0, 180.0, 0.0], [0.0, 4.0, -3.0], 0.5),
    first_person: t([30.0, 340.0, 0.0], [0.0, 3.0, 0.0], 0.375),
    third_person: t([30.0, 340.0, 0.0], [0.0, 3.0, -2.0], 0.23),
    on_shelf: t([90.0, 0.0, 0.0], [0.0, 14.5, 2.5], 0.9375),
    front_light: false,
};

/// v1.0 gap track 6d: `item/trident_in_hand`'s display (the hands; the
/// other contexts keep `item/trident`'s, which Java draws there).
pub const TRIDENT_IN_HAND: Display = Display {
    first_person: t([0.0, -90.0, 25.0], [-3.0, 17.0, 1.0], 1.0),
    third_person: t([0.0, 60.0, 0.0], [11.0, 17.0, -2.0], 1.0),
    ..GENERATED
};

/// v1.0 gap track 6e: `item/trident_throwing`'s display, which Java's
/// trident item definition draws while the trident is used: the in-hand
/// first-person transform, the third-person one turned point down behind
/// the raised arm.
pub const TRIDENT_THROWING: Display =
    Display { third_person: t([0.0, 90.0, 180.0], [8.0, -17.0, 9.0], 1.0), ..TRIDENT_IN_HAND };

/// v1.0 gap track 6d: `item/spyglass_in_hand`'s display: no parent, so the
/// first-person hand takes no transform and the third-person one 2/16 down.
pub const SPYGLASS_IN_HAND: Display = Display {
    first_person: t([0.0, 0.0, 0.0], [0.0, 0.0, 0.0], 1.0),
    third_person: t([0.0, 0.0, 0.0], [0.0, -2.0, 0.0], 1.0),
    ..GENERATED
};

/// v1.0 gap track 6d: the display a hand holds `stack` with: its in-hand
/// model's ([`TRIDENT_IN_HAND`], [`SPYGLASS_IN_HAND`]), else [`drawn`].
pub fn held(stack: &Stack) -> Display {
    if stack.item == crate::weapons::TRIDENT {
        TRIDENT_IN_HAND
    } else if stack.item == Item::SPYGLASS {
        SPYGLASS_IN_HAND
    } else {
        drawn(stack)
    }
}

/// The display transforms `stack` is drawn with: its item model's parent
/// chain in Java 26.3, chosen by the item (Java's model names follow the
/// item ids, as the game's item names do).
pub fn of(stack: &Stack) -> &'static Display {
    let item = stack.item;
    if item.bed_color().is_some() {
        return &BED;
    }
    if crate::content::banner_patterns::banner_color(item).is_some() {
        return &BANNER;
    }
    if item == Item::SHIELD {
        return &SHIELD;
    }
    if crate::item::drawn_3d(stack) { block_family(item.name()) } else { flat_family(item) }
}

/// [`of`] for the game's own model of `stack`. Java's copper golem statue
/// is an entity model, upside down until each of its transforms turns it
/// half about z; the game's statue boxes stand upright (Java's model turned
/// half about z through the block's centre), so they take the same
/// transforms without that half turn, which places them exactly as Java's.
pub fn drawn(stack: &Stack) -> Display {
    let mut display = *of(stack);
    if display == STATUE {
        for t in [
            &mut display.gui,
            &mut display.ground,
            &mut display.fixed,
            &mut display.first_person,
            &mut display.third_person,
            &mut display.on_shelf,
        ] {
            t.rotation[2] -= 180.0;
        }
    }
    display
}

/// A flat item's model: `item/generated` unless its own model or parent
/// says otherwise.
fn flat_family(item: Item) -> &'static Display {
    if item.as_tool().is_some() {
        return &HANDHELD;
    }
    if crate::world::jukebox::song_of(item).is_some() {
        return &MUSIC_DISC;
    }
    if item == Item::GOAT_HORN {
        return &GOAT_HORN;
    }
    match item.name() {
        "stick" | "bone" | "blaze rod" | "breeze rod" | "bamboo" | "debug stick" => &HANDHELD,
        "fishing rod" | "carrot on a stick" | "warped fungus on a stick" => &ROD,
        "bow" => &BOW,
        "crossbow" => &CROSSBOW,
        "brush" => &BRUSH,
        "mace" => &MACE,
        "hanging roots" => &HANGING_ROOTS,
        "small amethyst bud" => &SMALL_BUD,
        "medium amethyst bud" => &MEDIUM_BUD,
        "large amethyst bud" => &LARGE_BUD,
        "pointed dripstone" | "sulfur spike" => &DRIPSTONE,
        _ => &GENERATED,
    }
}

/// A 3D item's model, by its name.
fn block_family(name: &str) -> &'static Display {
    let ends = |suffix: &str| name.ends_with(suffix);
    match name {
        "straw bed" => &STRAW_BED,
        "conduit" => &CONDUIT,
        "decorated pot" => &DECORATED_POT,
        "dragon head" => &DRAGON_HEAD,
        "dried ghast" => &DRIED_GHAST,
        "end rod" => &END_ROD,
        "heavy core" => &HEAVY_CORE,
        "lectern" => &LECTERN,
        "shelf mushroom" => &SHELF_MUSHROOM,
        "big dripleaf" => &BIG_DRIPLEAF,
        "small dripleaf" => &SMALL_DRIPLEAF,
        "calibrated sculk sensor" => &CALIBRATED_SCULK_SENSOR,
        "snow layer" | "daylight detector" => &THIN_BLOCK,
        "bee nest" | "beehive" | "blast furnace" | "carved pumpkin" | "dispenser" | "dropper" | "furnace"
        | "jack o'lantern" | "loom" | "pumpkin" | "smoker" => &ORIENTABLE,
        _ if ends(" glazed terracotta") => &ORIENTABLE,
        _ if ends(" stairs") => &STAIRS,
        _ if ends(" fence gate") => &GATE,
        _ if ends(" fence") || ends(" wall") => &FENCE_INVENTORY,
        _ if ends(" trapdoor") || ends(" pressure plate") || ends(" carpet") => &THIN_BLOCK,
        _ if ends(" shelf") => &SHELF,
        _ if ends("anvil") => &ANVIL,
        _ if ends("chest") || ends("shulker box") => &CHEST,
        _ if ends(" head") || ends(" skull") => &SKULL,
        _ if ends("golem statue") => &STATUE,
        _ if ends("lightning rod") => &LIGHTNING_ROD,
        _ => &BLOCK,
    }
}

/// Java's two GUI lights (`Lighting`'s `DIFFUSE_LIGHT_0` (0.2, 1, -0.7) and
/// `DIFFUSE_LIGHT_1` (-0.2, 1, 0.7), normalised) turned by `ITEMS_3D`'s pose
/// (`scaling(1, -1, 1).rotateYXZ(1.0821041, 3.2375858, 0).rotateYXZ(-π/8,
/// 3π/4, 0)`) or `ITEMS_FLAT`'s (`rotationY(-π/8).rotateX(3π/4)`), in the
/// GUI pose's y-down frame.
pub fn gui_lights(front: bool) -> [Vec3; 2] {
    let pose = if front {
        Mat4::from_rotation_y(-std::f32::consts::FRAC_PI_8) * Mat4::from_rotation_x(std::f32::consts::FRAC_PI_4 * 3.0)
    } else {
        Mat4::from_scale(Vec3::new(1.0, -1.0, 1.0))
            * Mat4::from_rotation_y(1.082_104_1)
            * Mat4::from_rotation_x(3.237_585_8)
            * Mat4::from_rotation_y(-std::f32::consts::FRAC_PI_8)
            * Mat4::from_rotation_x(std::f32::consts::FRAC_PI_4 * 3.0)
    };
    [Vec3::new(0.2, 1.0, -0.7), Vec3::new(-0.2, 1.0, 0.7)].map(|l| pose.transform_vector3(l.normalize()))
}

/// The brightness Java's item shader gives a face turned to `normal` in a
/// slot (x right, y up, z toward the viewer): `minecraft_mix_light`, 0.6 of
/// both lights' diffuse plus 0.4 ambient, at most 1. The GUI pose flips y
/// (`GuiItemAtlas` scales y by -1), so the normal is flipped with it.
pub fn gui_shade(normal: Vec3, front: bool) -> f32 {
    let n = Vec3::new(normal.x, -normal.y, normal.z).normalize_or_zero();
    let [a, b] = gui_lights(front);
    (0.4 + 0.6 * (a.dot(n).max(0.0) + b.dot(n).max(0.0))).min(1.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_block_in_a_slot_is_lit_as_java_lights_it() {
        // `block/block`'s GUI turn shows the top, the east face on the left
        // and the north face on the right.
        let m = BLOCK.gui.matrix();
        let shade = |n: Vec3| gui_shade(m.transform_vector3(n), false);
        let (top, east, north) = (shade(Vec3::Y), shade(Vec3::X), shade(Vec3::NEG_Z));
        assert!((top - 1.0).abs() < 1e-3, "{top}");
        assert!((east - 0.6505).abs() < 1e-3, "{east}");
        assert!((north - 0.4).abs() < 1e-3, "{north}");
        let left = m.transform_vector3(Vec3::X);
        let right = m.transform_vector3(Vec3::NEG_Z);
        assert!(left.x < 0.0 && left.z > 0.0 && right.x > 0.0 && right.z > 0.0, "{left} {right}");
        // A flat sprite faces the viewer, fully lit.
        assert!((gui_shade(Vec3::Z, true) - 1.0).abs() < 1e-3);
        assert!((gui_shade(Vec3::NEG_Z, true) - 0.4).abs() < 1e-3);
    }
}
