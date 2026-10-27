//! v0.12 colour track (BLD-01d), ids 24576–28671 (see the architecture
//! document's "v0.12 track reservations"): Java 26.3's wool and concrete
//! stairs and slabs, candles and candle cakes, plain banners and the straw
//! bed.
//!
//! Layout, one id per Java state (append-only):
//!
//! - 24576..=27135: stairs, 80 ids per family (16 wools, then 16 concretes,
//!   in [`DyeColor::ALL`] order), with
//!   `local = ((top * 5 + shape) * 2 + wet) * 4 + facing` as in
//!   [`super::slab_stair_states`]; local 0 (bottom, straight, dry, facing
//!   south) is the family's item.
//! - 27136..=27295: slabs, 5 ids per family: bottom dry (the item), bottom
//!   wet, top dry, top wet, double.
//! - 27296..=27567: candles, 16 ids per colour (plain, then the 16 dyes):
//!   `(candles - 1) * 4 + lit * 2 + wet`.
//! - 27568..=27601: candle cakes, 2 ids per colour: unlit, lit.
//! - 27616..=27871: standing banners, 16 rotations per colour.
//! - 27872..=27935: wall banners, 4 facings per colour (in `Facing::ALL` order).
//! - 27936..=27943: the straw bed: foot then head, 4 facings each.
//!
//! Stairs and slabs plug into `slab_stair_states` as families from
//! [`super::slab_stair_states::COLOUR_FIRST`], so they get its shapes,
//! halves, corners, waterlogging and placement.

use super::block::{Block, RenderKind, Shaped};
use super::shape::Boxes;
use crate::color::DyeColor;
use crate::item::{Item, ToolKind};

pub mod banner;
pub mod candle;
pub mod cushion;
pub mod straw_bed;
#[cfg(test)]
mod tests;
mod world;

pub(super) use world::settled_state;

pub use banner::{Banner, BannerMount};
pub use candle::Candle;

/// First id of the track's reservation.
pub const FIRST: u16 = 24576;
/// Last id of the track's reservation.
pub const RESERVED_LAST: u16 = 28671;

/// Wool then concrete stairs/slab families.
pub const SHAPE_FAMILIES: usize = 32;
pub const STAIRS_ORIGIN: u16 = FIRST;
pub const STAIRS_STRIDE: u16 = 80;
pub const SLABS_ORIGIN: u16 = STAIRS_ORIGIN + SHAPE_FAMILIES as u16 * STAIRS_STRIDE;
pub const SLABS_STRIDE: u16 = 5;
pub const SHAPES_END: u16 = SLABS_ORIGIN + SHAPE_FAMILIES as u16 * SLABS_STRIDE;
/// Plain candle, then the 16 dyed ones.
pub const CANDLE_COLOURS: u16 = 17;
pub const CANDLE_ORIGIN: u16 = SHAPES_END;
pub const CAKE_ORIGIN: u16 = CANDLE_ORIGIN + CANDLE_COLOURS * 16;
pub const CAKE_END: u16 = CAKE_ORIGIN + CANDLE_COLOURS * 2;
pub const BANNER_ORIGIN: u16 = 27616;
pub const WALL_BANNER_ORIGIN: u16 = BANNER_ORIGIN + 16 * 16;
pub const STRAW_BED_ORIGIN: u16 = WALL_BANNER_ORIGIN + 16 * 4;
/// One past the last registered id.
pub const END: u16 = STRAW_BED_ORIGIN + 8;
const _: () = assert!(SHAPES_END == 27296 && CAKE_END <= BANNER_ORIGIN && END - 1 <= RESERVED_LAST);

/// Texture layers, as offsets from one base the integrator may move (the
/// track's reservation is 2384–2575).
pub mod tex {
    pub const TEX: u16 = 2384;
    /// Candle wax per candle colour (plain, then the dyes).
    pub const WAX: u16 = TEX;
    pub const WICK: u16 = WAX + super::CANDLE_COLOURS;
    pub const WICK_LIT: u16 = WICK + 1;
    /// Flat item icons of the candles.
    pub const CANDLE_ICON: u16 = WICK_LIT + 1;
    /// Banner cloth per dye colour, then the pole and bar, then icons.
    pub const CLOTH: u16 = CANDLE_ICON + super::CANDLE_COLOURS;
    pub const POLE: u16 = CLOTH + 16;
    pub const BANNER_ICON: u16 = POLE + 1;
    /// Straw bed: the mattress's top, sides and underside, and the
    /// head's pillow.
    pub const STRAW_TOP: u16 = BANNER_ICON + 16;
    pub const STRAW_SIDE: u16 = STRAW_TOP + 1;
    pub const STRAW_BOTTOM: u16 = STRAW_TOP + 2;
    pub const STRAW_PILLOW: u16 = STRAW_TOP + 3;
    /// One past the last layer.
    pub const TEX_END: u16 = STRAW_PILLOW + 1;
    const _: () = assert!(TEX_END <= 2576);
}

/// Whether `id` is in the track's registered range.
#[inline(always)]
pub const fn owns(id: u16) -> bool {
    (id >= FIRST && id < CAKE_END) || (id >= BANNER_ORIGIN && id < END)
}

/// Whether `id` is one of the wool or concrete stairs and slab states.
#[inline(always)]
pub const fn owns_shape(id: u16) -> bool {
    id >= STAIRS_ORIGIN && id < SHAPES_END
}

/// The full block of stairs/slab family `i` (wools, then concretes).
pub const fn family_base(i: usize) -> Block {
    if i < 16 { Block::wool(DyeColor::ALL[i]) } else { Block::concrete(DyeColor::ALL[i - 16]) }
}

/// The item state (bottom, straight, dry, facing south) of family `i`'s stairs.
pub const fn family_stairs(i: usize) -> Block {
    Block(STAIRS_ORIGIN + i as u16 * STAIRS_STRIDE)
}

/// The item state (bottom, dry) of family `i`'s slab.
pub const fn family_slab(i: usize) -> Block {
    Block(SLABS_ORIGIN + i as u16 * SLABS_STRIDE)
}

/// Family and `slab_stair_states` local index of a stairs id.
pub const fn stairs_local(id: u16) -> Option<(u16, u16)> {
    if id >= STAIRS_ORIGIN && id < SLABS_ORIGIN {
        let i = id - STAIRS_ORIGIN;
        Some((i / STAIRS_STRIDE, i % STAIRS_STRIDE))
    } else {
        None
    }
}

/// Family and local index (bottom dry, bottom wet, top dry, top wet,
/// double) of a slab id.
pub const fn slab_local(id: u16) -> Option<(u16, u16)> {
    if id >= SLABS_ORIGIN && id < SHAPES_END {
        let i = id - SLABS_ORIGIN;
        Some((i / SLABS_STRIDE, i % SLABS_STRIDE))
    } else {
        None
    }
}

const COLOURS: [&str; 16] = [
    "white",
    "orange",
    "magenta",
    "light blue",
    "yellow",
    "lime",
    "pink",
    "gray",
    "light gray",
    "cyan",
    "purple",
    "blue",
    "brown",
    "green",
    "red",
    "black",
];

const WOOL_STAIRS: [&str; 16] = [
    "white wool stairs",
    "orange wool stairs",
    "magenta wool stairs",
    "light blue wool stairs",
    "yellow wool stairs",
    "lime wool stairs",
    "pink wool stairs",
    "gray wool stairs",
    "light gray wool stairs",
    "cyan wool stairs",
    "purple wool stairs",
    "blue wool stairs",
    "brown wool stairs",
    "green wool stairs",
    "red wool stairs",
    "black wool stairs",
];
const WOOL_SLABS: [&str; 16] = [
    "white wool slab",
    "orange wool slab",
    "magenta wool slab",
    "light blue wool slab",
    "yellow wool slab",
    "lime wool slab",
    "pink wool slab",
    "gray wool slab",
    "light gray wool slab",
    "cyan wool slab",
    "purple wool slab",
    "blue wool slab",
    "brown wool slab",
    "green wool slab",
    "red wool slab",
    "black wool slab",
];
const CONCRETE_STAIRS: [&str; 16] = [
    "white concrete stairs",
    "orange concrete stairs",
    "magenta concrete stairs",
    "light blue concrete stairs",
    "yellow concrete stairs",
    "lime concrete stairs",
    "pink concrete stairs",
    "gray concrete stairs",
    "light gray concrete stairs",
    "cyan concrete stairs",
    "purple concrete stairs",
    "blue concrete stairs",
    "brown concrete stairs",
    "green concrete stairs",
    "red concrete stairs",
    "black concrete stairs",
];
const CONCRETE_SLABS: [&str; 16] = [
    "white concrete slab",
    "orange concrete slab",
    "magenta concrete slab",
    "light blue concrete slab",
    "yellow concrete slab",
    "lime concrete slab",
    "pink concrete slab",
    "gray concrete slab",
    "light gray concrete slab",
    "cyan concrete slab",
    "purple concrete slab",
    "blue concrete slab",
    "brown concrete slab",
    "green concrete slab",
    "red concrete slab",
    "black concrete slab",
];
const CANDLES: [&str; 17] = [
    "candle",
    "white candle",
    "orange candle",
    "magenta candle",
    "light blue candle",
    "yellow candle",
    "lime candle",
    "pink candle",
    "gray candle",
    "light gray candle",
    "cyan candle",
    "purple candle",
    "blue candle",
    "brown candle",
    "green candle",
    "red candle",
    "black candle",
];
/// Java's display names: "Cake with White Candle".
const CANDLE_CAKES: [&str; 17] = [
    "cake with candle",
    "cake with white candle",
    "cake with orange candle",
    "cake with magenta candle",
    "cake with light blue candle",
    "cake with yellow candle",
    "cake with lime candle",
    "cake with pink candle",
    "cake with gray candle",
    "cake with light gray candle",
    "cake with cyan candle",
    "cake with purple candle",
    "cake with blue candle",
    "cake with brown candle",
    "cake with green candle",
    "cake with red candle",
    "cake with black candle",
];
const BANNERS: [&str; 16] = [
    "white banner",
    "orange banner",
    "magenta banner",
    "light blue banner",
    "yellow banner",
    "lime banner",
    "pink banner",
    "gray banner",
    "light gray banner",
    "cyan banner",
    "purple banner",
    "blue banner",
    "brown banner",
    "green banner",
    "red banner",
    "black banner",
];
/// Java's block ids (`white_wall_banner`); the item is the standing banner.
const WALL_BANNERS: [&str; 16] = [
    "white wall banner",
    "orange wall banner",
    "magenta wall banner",
    "light blue wall banner",
    "yellow wall banner",
    "lime wall banner",
    "pink wall banner",
    "gray wall banner",
    "light gray wall banner",
    "cyan wall banner",
    "purple wall banner",
    "blue wall banner",
    "brown wall banner",
    "green wall banner",
    "red wall banner",
    "black wall banner",
];

/// The colour words of a dye, as in block names.
pub const fn colour_name(c: DyeColor) -> &'static str {
    COLOURS[c as usize]
}

/// Texture of a wool or concrete family's full block.
const fn family_tex(i: usize) -> u16 {
    use super::block::tex;
    if i == 0 {
        tex::WOOL
    } else if i < 16 {
        tex::COLORED_WOOL + i as u16
    } else {
        tex::CONCRETE + (i - 16) as u16
    }
}

/// Name, render kind and per-face layers of every registered state.
pub const fn registry(id: u16) -> Option<(&'static str, RenderKind, [u16; 6])> {
    if let Some((family, local)) = stairs_local(id) {
        let f = family as usize;
        let name = if f < 16 { WOOL_STAIRS[f] } else { CONCRETE_STAIRS[f - 16] };
        let _ = local;
        return Some((name, RenderKind::Shaped, [family_tex(f); 6]));
    }
    if let Some((family, local)) = slab_local(id) {
        let f = family as usize;
        let name = if f < 16 { WOOL_SLABS[f] } else { CONCRETE_SLABS[f - 16] };
        // As `slab_stair_states::registry`: a bottom slab is a low cutout
        // block, a top slab a shaped half and a double slab a full cube.
        let kind = match local {
            0 | 1 => RenderKind::Cutout,
            2 | 3 => RenderKind::Shaped,
            _ => RenderKind::Opaque,
        };
        return Some((name, kind, [family_tex(f); 6]));
    }
    if let Some(c) = candle::decode(id) {
        return Some((CANDLES[c.colour as usize], RenderKind::Shaped, [tex::CANDLE_ICON + c.colour as u16; 6]));
    }
    if let Some((colour, _)) = candle::decode_cake(id) {
        use super::block::tex as t;
        return Some((
            CANDLE_CAKES[colour as usize],
            RenderKind::Shaped,
            [t::CAKE_SIDE, t::CAKE_SIDE, t::CAKE_TOP, t::CAKE_BOTTOM, t::CAKE_SIDE, t::CAKE_SIDE],
        ));
    }
    if let Some(b) = banner::decode(id) {
        let name = match b.mount {
            BannerMount::Ground(_) => BANNERS[b.colour as usize],
            BannerMount::Wall(_) => WALL_BANNERS[b.colour as usize],
        };
        return Some((name, RenderKind::Shaped, [tex::BANNER_ICON + b.colour as u16; 6]));
    }
    if straw_bed::decode(id).is_some() {
        let (side, top) = (tex::STRAW_SIDE, tex::STRAW_TOP);
        return Some(("straw bed", RenderKind::Shaped, [side, side, top, tex::STRAW_BOTTOM, side, side]));
    }
    None
}

/// Light emitted by each state: three per lit candle (`CandleBlock.LIGHT_EMISSION`
/// and the candle cake's `litBlockEmission(3)`).
pub const fn emission(id: u16) -> u8 {
    if let Some(c) = candle::decode(id) {
        return if c.lit { 3 * c.candles } else { 0 };
    }
    match candle::decode_cake(id) {
        Some((_, true)) => 3,
        _ => 0,
    }
}

/// The waterlogged partner of a candle (`CandleBlock` is a
/// `SimpleWaterloggedBlock`); stairs and slabs pair in `slab_stair_states`.
pub const fn water_pair(id: u16) -> Option<(u16, bool)> {
    if candle::decode(id).is_some() { Some((id ^ 1, id & 1 == 1)) } else { None }
}

/// The block items, recipes and pick-block use for a state.
pub fn base(b: Block) -> Option<Block> {
    if let Some(c) = candle::decode(b.0) {
        return Some(candle::candle(c.colour, 1, false, false));
    }
    if let Some((colour, _)) = candle::decode_cake(b.0) {
        return Some(candle::cake(colour, false));
    }
    if let Some(banner) = banner::decode(b.0) {
        return Some(banner::standing(banner.colour, 0));
    }
    if straw_bed::decode(b.0).is_some() {
        return Some(straw_bed::ITEM_BLOCK);
    }
    None
}

/// `Block::shaped` for the candle, cake, banner and bed states.
pub fn shaped(id: u16) -> Option<Shaped> {
    if !owns(id) || owns_shape(id) {
        return None;
    }
    Some(Shaped::Custom)
}

/// Hardness, preferred tool and harvest level (Java 26.3 `Blocks` and the
/// mineable tags). Stairs and slabs copy their full block.
pub fn mining(b: Block) -> Option<(f32, Option<ToolKind>, Option<u8>)> {
    let id = b.0;
    if !owns(id) {
        return None;
    }
    if owns_shape(id) {
        let base = super::slab_stair_states::family_base(b)?;
        return Some((base.hardness(), base.best_tool(), base.harvest_level()));
    }
    if candle::decode(id).is_some() {
        return Some((0.1, None, None));
    }
    if candle::decode_cake(id).is_some() {
        return Some((0.5, None, None));
    }
    if banner::decode(id).is_some() {
        return Some((1.0, Some(ToolKind::Axe), None));
    }
    if straw_bed::decode(id).is_some() {
        return Some((0.2, Some(ToolKind::Hoe), None));
    }
    None
}

/// What breaking a state yields (`None` for stairs and slabs, which drop
/// their item through the normal path). A candle drops one candle per
/// candle in the block (see [`drop_count`]); a candle cake its candle.
pub fn drop(b: Block) -> Option<Option<Item>> {
    if let Some(c) = candle::decode(b.0) {
        return Some(Some(Item::from_block(candle::candle(c.colour, 1, false, false))));
    }
    if let Some((colour, _)) = candle::decode_cake(b.0) {
        return Some(Some(Item::from_block(candle::candle(colour, 1, false, false))));
    }
    if let Some(banner) = banner::decode(b.0) {
        return Some(Some(Item::from_block(banner::standing(banner.colour, 0))));
    }
    if let Some((_, head)) = straw_bed::decode(b.0) {
        // Only the head drops the bed (`match_block` on `part=head`).
        return Some(head.then_some(Item::from_block(straw_bed::ITEM_BLOCK)));
    }
    None
}

/// How many of [`drop`]'s item a state yields.
pub fn drop_count(b: Block) -> u8 {
    candle::decode(b.0).map_or(1, |c| c.candles)
}

/// `FireBlock` flammability: wool stairs and slabs as wool, the straw bed.
pub fn fire_odds(b: Block) -> Option<(u8, u8)> {
    if let Some(base) = super::slab_stair_states::family_base(b).filter(|_| owns_shape(b.0)) {
        return Some(if base.wool_color().is_some() { (30, 60) } else { (0, 0) });
    }
    if straw_bed::decode(b.0).is_some() {
        return Some((30, 60));
    }
    owns(b.0).then_some((0, 0))
}

/// Java's `ignitedByLava`: banners and the straw bed.
pub fn ignited_by_lava(b: Block) -> bool {
    banner::decode(b.0).is_some() || straw_bed::decode(b.0).is_some()
}

/// Whether the state can rest on `below` (`canSurvive` for blocks that
/// only look down). Wall banners look at their wall; see
/// [`super::World::drop_unhung_colour_blocks`].
pub fn can_stay_on(b: Block, below: Block) -> Option<bool> {
    if candle::decode(b.0).is_some() {
        return Some(super::mount_states::supports_center(below, glam::IVec3::Y));
    }
    if candle::decode_cake(b.0).is_some() {
        return Some(below.is_solid());
    }
    match banner::decode(b.0) {
        Some(Banner { mount: BannerMount::Ground(_), .. }) => return Some(below.is_solid()),
        Some(_) => return Some(true),
        None => {}
    }
    owns(b.0).then_some(true)
}

/// Visible boxes of candles, candle cakes and the straw bed. Banners draw
/// as models, not in chunk meshes, so theirs are empty.
pub fn boxes(b: Block) -> Option<Boxes> {
    if let Some(c) = candle::decode(b.0) {
        return Some(candle::boxes(c));
    }
    if candle::decode_cake(b.0).is_some() {
        return Some(candle::cake_boxes());
    }
    if banner::decode(b.0).is_some() {
        return Some(Boxes::new());
    }
    if let Some((facing, head)) = straw_bed::decode(b.0) {
        return Some(straw_bed::boxes(facing, head));
    }
    None
}

/// Selection shapes (`getShape`) where they differ from [`boxes`].
pub fn outline(b: Block) -> Option<Boxes> {
    if let Some(c) = candle::decode(b.0) {
        return Some(candle::outline(c));
    }
    if candle::decode_cake(b.0).is_some() {
        return Some(candle::cake_outline());
    }
    banner::decode(b.0).map(banner::outline)
}

/// Collision shapes: Java's `getShape` for candles, candle cakes and the
/// straw bed, none for banners (`noCollision`).
pub fn collision(b: Block) -> Option<Boxes> {
    if banner::decode(b.0).is_some() {
        return Some(Boxes::new());
    }
    if let Some((facing, head)) = straw_bed::decode(b.0) {
        return Some(straw_bed::boxes(facing, head));
    }
    outline(b)
}

/// The layer box `index` of `b`'s [`boxes`] is drawn with, when it isn't
/// the block's own face layer: candle wax and wicks, the straw pillow.
#[inline]
pub fn box_layer(b: Block, index: usize) -> Option<u16> {
    if b.0 < CANDLE_ORIGIN || b.0 >= END {
        return None;
    }
    match straw_bed::decode(b.0) {
        Some((_, true)) if index == 1 => Some(tex::STRAW_PILLOW),
        Some(_) => None,
        None => candle::box_layer(b, index),
    }
}

/// Drawn as a flat sprite in inventories and in hand, like Java's candle
/// and banner items.
pub fn flat_icon(b: Block) -> bool {
    candle::decode(b.0).is_some() || banner::decode(b.0).is_some()
}

/// Item stack sizes that differ from 64: banners and the straw bed stack to 16.
pub fn max_stack(b: Block) -> Option<u8> {
    (banner::decode(b.0).is_some() || straw_bed::decode(b.0).is_some()).then_some(16)
}

/// Creative palette entries: stairs and slabs after each other per family,
/// then candles, banners and the straw bed.
pub fn palette_ids() -> impl Iterator<Item = u16> {
    (0..SHAPE_FAMILIES)
        .flat_map(|i| [family_stairs(i).0, family_slab(i).0])
        .chain((0..CANDLE_COLOURS as u8).map(|c| candle::candle(c, 1, false, false).0))
        .chain(DyeColor::ALL.into_iter().map(|c| banner::standing(c as u8, 0).0))
        .chain([straw_bed::ITEM_BLOCK.0])
}

/// The full block a stairs or slab family is cut from, for recipes.
pub fn family_of_base(base: Block) -> Option<usize> {
    (0..SHAPE_FAMILIES).find(|&i| family_base(i) == base)
}

/// Every candle item, plain first.
pub fn candle_items() -> impl Iterator<Item = Item> {
    (0..CANDLE_COLOURS as u8).map(|c| Item::from_block(candle::candle(c, 1, false, false)))
}

/// The state the item `raw` places at `ctx.at` for banners
/// (`StandingAndWallBlockItem.getPlacementState`): `None` if `raw` isn't a
/// banner, `Some(None)` if no direction can hold it.
pub fn placed(
    raw: Block,
    ctx: &crate::session::placement::PlaceContext,
    get: impl Fn(glam::IVec3) -> Option<Block>,
) -> Option<Option<Block>> {
    let colour = banner::decode(raw.0)?.colour;
    let survives = |b: Block| {
        let banner = banner::decode(b.0).expect("a banner state");
        get(banner::support(banner, ctx.at)).is_some_and(banner::holds)
    };
    let nearest = super::mount_states::context_looking(ctx);
    // `WallBannerBlock.getStateForPlacement`: the first horizontal look
    // direction whose opposite side holds it.
    let wall = nearest.iter().filter(|d| d.y == 0).find_map(|&d| {
        let state = banner::wall(colour, super::block::Facing::from_offset(-d)?);
        survives(state).then_some(state)
    });
    let horizontal = glam::Vec3::new(ctx.look.x, 0.0, ctx.look.z);
    let look = if horizontal.length_squared() > 1e-8 { horizontal } else { ctx.facing.offset().as_vec3() };
    let standing = banner::standing(colour, banner::rotation_for(banner::java_y_rot(look)));
    let state = nearest.iter().filter(|d| d.y <= 0).find_map(|&d| {
        let state = if d.y < 0 { Some(standing) } else { wall };
        state.filter(|&s| survives(s))
    });
    Some(state)
}

/// `PushReaction.POPPED` or `DESTROY`: candles, candle cakes (as cake) and
/// the straw bed break when a piston pushes them.
pub fn pops_on_push(b: Block) -> bool {
    candle::decode(b.0).is_some() || candle::decode_cake(b.0).is_some() || straw_bed::decode(b.0).is_some()
}
