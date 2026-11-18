//! Java 26.3's spyglass (`SpyglassItem`, v0.15 Track B, item 1556): held
//! up with use, it zooms the first-person view to a tenth of the field of
//! view (`AbstractClientPlayer.getFieldOfViewModifier` returns 0.1 outright
//! while scoping, ignoring the FOV effects option) and draws the scope
//! overlay (`Gui.renderSpyglassOverlay`). Use lasts up to 1200 ticks.

use crate::item::{Item, ItemInfo, ItemKind, Sprite};

pub const SPYGLASS: Item = Item(1556);

/// `SpyglassItem.USE_DURATION`.
pub const USE_DURATION: u32 = 1200;
/// `SpyglassItem.ZOOM_FOV_MODIFIER`.
pub const ZOOM_FOV_MODIFIER: f32 = 0.1;
/// `Hud.extractSpyglassOverlay`'s scope: a square `min(width, height)`
/// times `scopeScale` on a side, black around it. Each frame `scopeScale`
/// moves `0.5 * deltaTicks` of the way to 1.125; it starts at 0.5 whenever
/// the player isn't scoping.
pub const SCOPE_EASE_PER_TICK: f32 = 0.5;
pub const SCOPE_START: f32 = 0.5;
pub const SCOPE_TARGET: f32 = 1.125;

pub(crate) fn item_info(item: Item) -> Option<ItemInfo> {
    (item == SPYGLASS).then_some(ItemInfo {
        name: "spyglass",
        kind: ItemKind::Material,
        max_stack: 1,
        sprite: Sprite::Spyglass,
    })
}

/// The overlay's scale a frame of `ticks` game ticks after `scale`
/// (`Mth.lerp(0.5 * deltaTicks, scopeScale, 1.125)`).
pub fn ease_scope(scale: f32, ticks: f32) -> f32 {
    scale + (SCOPE_TARGET - scale) * SCOPE_EASE_PER_TICK * ticks
}
