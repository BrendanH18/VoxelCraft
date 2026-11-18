//! Java 26.3 banner patterns (v0.15 Track B): the ten banner pattern items
//! (ids 1619..=1628, `Items` registration order, which is also the
//! `#loom_patterns` order), the loom (`LoomMenu`), the ominous banner
//! (`Raid.getBannerComponentPatch`), washing a layer off in a cauldron
//! (`CauldronInteractions.bannerInteraction`), and the two grid recipes
//! that move patterns (`BannerDuplicateRecipe`, `ShieldDecorationRecipe`).
//!
//! Patterns live in the stack's [`Payload::Banner`]; a placed banner keeps
//! them in its block entity (`world::workstations`). The drawn designs are
//! original masks over Java's 20×40 flag ([`mask`]), checked against the
//! pattern list in `tests/fixtures/java/26.3/workstations.json`.

use crate::color::DyeColor;
use crate::inventory::Stack;
use crate::inventory::components::{BannerLayer, BannerPattern, BannerPatterns, Payload};
use crate::item::{Item, ItemInfo, ItemKind, Sprite};

/// The flower banner pattern; the other nine follow in `Items` order.
pub const ITEM_FIRST: u16 = 1619;
/// One past the bordure indented banner pattern.
pub const ITEM_END: u16 = ITEM_FIRST + 10;
const _: () = assert!(ITEM_END == 1629);

/// `BannerBlockEntity.MAX_PATTERNS`: the loom stops adding layers here and
/// banner duplication refuses banners with more.
pub const MAX_LAYERS: usize = 6;

/// Java's `Rarity` of a pattern item (`Items`): common items have none.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Rarity {
    Common,
    Uncommon,
    Rare,
}

/// One banner pattern item: Java id, our name, the pattern its
/// `provides_banner_patterns` tag (`pattern_item/<x>`) holds, and rarity.
pub struct PatternItem {
    pub java: &'static str,
    pub name: &'static str,
    pub pattern: &'static str,
    pub rarity: Rarity,
}

/// `Items.FLOWER_BANNER_PATTERN` .. `BORDURE_INDENTED_BANNER_PATTERN`. Java
/// names most of them "Banner Pattern"; ours keep the id so every name is
/// unique (`Item::from_name`).
pub const PATTERN_ITEMS: [PatternItem; 10] = [
    PatternItem {
        java: "flower_banner_pattern",
        name: "flower banner pattern",
        pattern: "flower",
        rarity: Rarity::Common,
    },
    PatternItem {
        java: "creeper_banner_pattern",
        name: "creeper banner pattern",
        pattern: "creeper",
        rarity: Rarity::Uncommon,
    },
    PatternItem { java: "skull_banner_pattern", name: "skull banner pattern", pattern: "skull", rarity: Rarity::Rare },
    PatternItem {
        java: "mojang_banner_pattern",
        name: "mojang banner pattern",
        pattern: "mojang",
        rarity: Rarity::Rare,
    },
    PatternItem {
        java: "globe_banner_pattern",
        name: "globe banner pattern",
        pattern: "globe",
        rarity: Rarity::Common,
    },
    PatternItem {
        java: "piglin_banner_pattern",
        name: "piglin banner pattern",
        pattern: "piglin",
        rarity: Rarity::Uncommon,
    },
    PatternItem { java: "flow_banner_pattern", name: "flow banner pattern", pattern: "flow", rarity: Rarity::Rare },
    PatternItem {
        java: "guster_banner_pattern",
        name: "guster banner pattern",
        pattern: "guster",
        rarity: Rarity::Rare,
    },
    PatternItem {
        java: "field_masoned_banner_pattern",
        name: "field masoned banner pattern",
        pattern: "bricks",
        rarity: Rarity::Common,
    },
    PatternItem {
        java: "bordure_indented_banner_pattern",
        name: "bordure indented banner pattern",
        pattern: "curly_border",
        rarity: Rarity::Common,
    },
];

/// `#minecraft:no_item_required` in its tag order: what the loom offers
/// with no pattern item (`LoomMenu.getSelectablePatterns`).
pub const NO_ITEM_REQUIRED: [&str; 32] = [
    "square_bottom_left",
    "square_bottom_right",
    "square_top_left",
    "square_top_right",
    "stripe_bottom",
    "stripe_top",
    "stripe_left",
    "stripe_right",
    "stripe_center",
    "stripe_middle",
    "stripe_downright",
    "stripe_downleft",
    "small_stripes",
    "cross",
    "straight_cross",
    "triangle_bottom",
    "triangle_top",
    "triangles_bottom",
    "triangles_top",
    "diagonal_left",
    "diagonal_up_right",
    "diagonal_up_left",
    "diagonal_right",
    "circle",
    "rhombus",
    "half_vertical",
    "half_horizontal",
    "half_vertical_right",
    "half_horizontal_bottom",
    "border",
    "gradient",
    "gradient_up",
];

/// The pattern item `i` of [`PATTERN_ITEMS`].
pub const fn item(i: usize) -> Item {
    Item(ITEM_FIRST + i as u16)
}

pub fn is_pattern_item(item: Item) -> bool {
    (ITEM_FIRST..ITEM_END).contains(&item.0)
}

fn entry(item: Item) -> Option<&'static PatternItem> {
    PATTERN_ITEMS.get(item.0.checked_sub(ITEM_FIRST)? as usize)
}

/// The pattern a pattern item provides.
pub fn provides(item: Item) -> Option<BannerPattern> {
    BannerPattern::from_name(entry(item)?.pattern)
}

pub fn rarity(item: Item) -> Option<Rarity> {
    entry(item).map(|e| e.rarity)
}

pub fn every_item() -> impl Iterator<Item = Item> {
    (ITEM_FIRST..ITEM_END).map(Item)
}

pub(crate) fn item_info(item: Item) -> Option<ItemInfo> {
    let e = entry(item)?;
    Some(ItemInfo {
        name: e.name,
        kind: ItemKind::Material,
        max_stack: 1,
        sprite: Sprite::BannerPattern((item.0 - ITEM_FIRST) as u8),
    })
}

/// The colour of a banner item (`BannerItem.getColor`), standing or wall
/// state alike; `None` for anything else.
pub fn banner_color(item: Item) -> Option<DyeColor> {
    let b = crate::world::colour_blocks::banner::decode(item.block()?.0)?;
    Some(DyeColor::ALL[b.colour as usize])
}

/// The banner item of colour `c`.
pub fn banner_item(c: DyeColor) -> Item {
    Item::from_block(crate::world::colour_blocks::banner::standing(c as u8, 0))
}

/// The stack's `banner_patterns` and `base_color` (empty for none).
pub fn patterns(stack: &Stack) -> BannerPatterns {
    match stack.payload.map(|p| p.get()) {
        Some(Payload::Banner(b)) => b.clone(),
        _ => BannerPatterns::default(),
    }
}

/// The stack with these patterns (an empty set removes the component).
pub fn with_patterns(stack: Stack, patterns: BannerPatterns) -> Stack {
    Stack { payload: Payload::Banner(patterns).intern(), ..stack }
}

/// Whether this banner is the raid's ominous banner.
pub fn is_ominous(stack: &Stack) -> bool {
    banner_color(stack.item).is_some() && patterns(stack).ominous
}

/// `Raid.getOminousBannerInstance`: a white banner with eight layers, its
/// own name and uncommon rarity. Track E equips raid captains with it.
pub fn ominous_banner() -> Stack {
    let layer = |name: &str, color| BannerLayer { pattern: BannerPattern::from_name(name).unwrap(), color };
    let layers = vec![
        layer("rhombus", DyeColor::Cyan),
        layer("stripe_bottom", DyeColor::LightGray),
        layer("stripe_center", DyeColor::Gray),
        layer("border", DyeColor::LightGray),
        layer("stripe_middle", DyeColor::Black),
        layer("half_horizontal", DyeColor::LightGray),
        layer("circle", DyeColor::LightGray),
        layer("border", DyeColor::Black),
    ];
    with_patterns(Stack::new(banner_item(DyeColor::White), 1), BannerPatterns { base: None, layers, ominous: true })
}

/// `Raid.OMINOUS_BANNER_PATTERN_NAME`'s English text.
pub const OMINOUS_NAME: &str = "ominous banner";

/// Items in `#loom_dyes` (`#dyes`) with a `dye` component: the sixteen dyes.
pub fn loom_dye(item: Item) -> Option<DyeColor> {
    item.dye_color()
}

/// `LoomMenu.getSelectablePatterns`: the pattern item's patterns, or every
/// `#no_item_required` pattern without one.
pub fn selectable(pattern_item: Option<Stack>) -> Vec<BannerPattern> {
    match pattern_item {
        None => NO_ITEM_REQUIRED.iter().filter_map(|n| BannerPattern::from_name(n)).collect(),
        Some(s) => provides(s.item).into_iter().collect(),
    }
}

/// `LoomMenu.setupResultSlot`: one banner with `pattern` in the dye's
/// colour added as its top layer.
pub fn loom_result(banner: Stack, dye: Stack, pattern: BannerPattern) -> Option<Stack> {
    banner_color(banner.item)?;
    let color = loom_dye(dye.item)?;
    let mut p = patterns(&banner);
    p.layers.push(BannerLayer { pattern, color });
    Some(with_patterns(Stack { count: 1, ..banner }, p))
}

/// The loom's slots and choice after `LoomMenu.slotsChanged`.
#[derive(Clone, Debug, PartialEq, Eq, Default)]
pub struct Loom {
    /// What the pattern list shows.
    pub selectable: Vec<BannerPattern>,
    /// `selectedBannerPatternIndex` (`None` for -1).
    pub selected: Option<usize>,
}

impl Loom {
    /// `LoomMenu.slotsChanged` for these inputs, keeping the pattern chosen
    /// before when it's still offered. Returns the result slot.
    pub fn refresh(&mut self, banner: Option<Stack>, dye: Option<Stack>, pattern: Option<Stack>) -> Option<Stack> {
        let (Some(banner), Some(dye)) = (banner, dye) else {
            self.selectable.clear();
            self.selected = None;
            return None;
        };
        let previous = std::mem::take(&mut self.selectable);
        self.selectable = selectable(pattern);
        let display = if self.selectable.len() == 1 {
            self.selected = Some(0);
            Some(self.selectable[0])
        } else {
            match self.selected.and_then(|i| previous.get(i)) {
                None => {
                    self.selected = None;
                    None
                }
                Some(&chosen) => {
                    self.selected = self.selectable.iter().position(|&p| p == chosen);
                    self.selected.map(|_| chosen)
                }
            }
        };
        let display = display?;
        if patterns(&banner).layers.len() >= MAX_LAYERS {
            self.selected = None;
            return None;
        }
        loom_result(banner, dye, display)
    }

    /// `LoomMenu.clickMenuButton`: chooses pattern `i` of the list.
    pub fn choose(&mut self, i: usize, banner: Option<Stack>, dye: Option<Stack>) -> Option<Stack> {
        let &pattern = self.selectable.get(i)?;
        self.selected = Some(i);
        loom_result(banner?, dye?, pattern)
    }

    /// `onTake`: the banner and dye lose one each; the choice is forgotten
    /// once either runs out.
    pub fn take(&mut self, banner: &mut Option<Stack>, dye: &mut Option<Stack>) {
        for slot in [&mut *banner, &mut *dye] {
            if let Some(s) = slot {
                s.count -= 1;
                if s.count == 0 {
                    *slot = None;
                }
            }
        }
        if banner.is_none() || dye.is_none() {
            self.selected = None;
        }
    }
}

/// Which loom slot a shift-clicked stack goes to (`LoomMenu.quickMoveStack`):
/// 0 banners, 1 dyes, 2 pattern items.
pub fn loom_slot_for(item: Item) -> Option<usize> {
    if banner_color(item).is_some() {
        Some(0)
    } else if loom_dye(item).is_some() {
        Some(1)
    } else if is_pattern_item(item) {
        Some(2)
    } else {
        None
    }
}

/// `CauldronInteractions.bannerInteraction`: one banner with its top layer
/// washed off; `None` when it has no layers (the use falls through).
pub fn wash(stack: Stack) -> Option<Stack> {
    banner_color(stack.item)?;
    let mut p = patterns(&stack);
    p.layers.pop()?;
    Some(with_patterns(Stack { count: 1, ..stack }, p))
}

/// The two grid recipes that move patterns: their result when the grid's
/// stacks make one of them.
pub fn grid_special(occupied: &[Stack]) -> Option<Stack> {
    banner_duplicate(occupied).or_else(|| shield_decoration(occupied))
}

/// `BannerDuplicateRecipe`: a patterned banner (at most six layers) and a
/// plain one of the same colour make a copy; the original stays.
pub fn banner_duplicate(occupied: &[Stack]) -> Option<Stack> {
    if occupied.len() != 2 {
        return None;
    }
    let color = banner_color(occupied[0].item)?;
    if banner_color(occupied[1].item)? != color {
        return None;
    }
    let (mut source, mut target) = (None, None);
    for s in occupied {
        let n = patterns(s).layers.len();
        if n > MAX_LAYERS {
            return None;
        }
        let slot = if n > 0 { &mut source } else { &mut target };
        if slot.is_some() {
            return None;
        }
        *slot = Some(*s);
    }
    target?;
    source.map(|s| Stack { count: 1, ..s })
}

/// `ShieldDecorationRecipe`: a plain shield and a banner make the shield
/// with the banner's layers and its colour as `base_color`; the shield's
/// other components (damage, enchantments, name) stay.
pub fn shield_decoration(occupied: &[Stack]) -> Option<Stack> {
    if occupied.len() != 2 {
        return None;
    }
    let banner = occupied.iter().find(|s| banner_color(s.item).is_some())?;
    let shield = occupied.iter().find(|s| s.item == Item::SHIELD)?;
    if !patterns(shield).layers.is_empty() {
        return None;
    }
    let p = patterns(banner);
    let base = banner_color(banner.item);
    Some(with_patterns(Stack { count: 1, ..*shield }, BannerPatterns { base, layers: p.layers, ominous: false }))
}

/// The remainder `BannerDuplicateRecipe.getRemainingItems` leaves in the
/// patterned banner's slot: that banner, unchanged.
pub fn duplicate_remainder(occupied: &[Stack], stack: &Stack) -> Option<Stack> {
    (banner_duplicate(occupied).is_some() && !patterns(stack).layers.is_empty()).then_some(Stack { count: 1, ..*stack })
}

// ---- Drawing --------------------------------------------------------------

/// Java's flag is 20 pixels wide and 40 tall (`BannerFlagModel`).
pub const FLAG_W: f32 = 20.0;
pub const FLAG_H: f32 = 40.0;

/// How much of the flag pixel at (`x`, `y`) pattern `p` covers, 0..1, with
/// (0, 0) the top left as seen from the front. The designs are original:
/// each follows its pattern's heraldic name (Java's `block.minecraft.banner`
/// translations), not Mojang's textures.
pub fn mask(p: BannerPattern, x: f32, y: f32) -> f32 {
    let (cx, cy) = (x - 10.0, y - 20.0);
    let on = |b: bool| if b { 1.0 } else { 0.0 };
    let tooth = |t: f32, width: f32| ((t.rem_euclid(width)) - width / 2.0).abs();
    match p.name() {
        "base" => 1.0,
        "square_bottom_left" => on(x < 10.0 && y >= 27.0),
        "square_bottom_right" => on(x >= 10.0 && y >= 27.0),
        "square_top_left" => on(x < 10.0 && y < 13.0),
        "square_top_right" => on(x >= 10.0 && y < 13.0),
        "stripe_bottom" => on(y >= 27.0),
        "stripe_top" => on(y < 13.0),
        "stripe_left" => on(x < 7.0),
        "stripe_right" => on(x >= 13.0),
        "stripe_center" => on((7.0..13.0).contains(&x)),
        "stripe_middle" => on((14.0..26.0).contains(&y)),
        "stripe_downright" => on((y - 2.0 * x).abs() <= 8.0),
        "stripe_downleft" => on((y - 2.0 * (FLAG_W - x)).abs() <= 8.0),
        "small_stripes" => on(matches!(x as i32 % 5, 1 | 2)),
        "cross" => on((y - 2.0 * x).abs() <= 6.0 || (y - 2.0 * (FLAG_W - x)).abs() <= 6.0),
        "straight_cross" => on((8.0..12.0).contains(&x) || (18.0..22.0).contains(&y)),
        "triangle_bottom" => on(y >= 28.0 + cx.abs() * 1.2),
        "triangle_top" => on(y < 12.0 - cx.abs() * 1.2),
        "triangles_bottom" => on(y >= 34.0 + tooth(x, 5.0) * 1.6),
        "triangles_top" => on(y < 6.0 - tooth(x, 5.0) * 1.6),
        "diagonal_left" => on(y + 2.0 * x < FLAG_H),
        "diagonal_up_right" => on(y + 2.0 * x >= FLAG_H),
        "diagonal_up_left" => on(y >= 2.0 * x),
        "diagonal_right" => on(y < 2.0 * x),
        "circle" => on(cx * cx + cy * cy <= 25.0),
        "rhombus" => on(cx.abs() / 7.0 + cy.abs() / 12.0 <= 1.0),
        "half_vertical" => on(x < 10.0),
        "half_vertical_right" => on(x >= 10.0),
        "half_horizontal" => on(y < 20.0),
        "half_horizontal_bottom" => on(y >= 20.0),
        "border" => on(!(2.0..18.0).contains(&x) || !(2.0..38.0).contains(&y)),
        "curly_border" => {
            let d = x.min(FLAG_W - x).min(y).min(FLAG_H - y);
            let along = if x.min(FLAG_W - x) < y.min(FLAG_H - y) { y } else { x };
            on(d < 1.2 + tooth(along, 4.0) * 0.9)
        }
        "gradient" => 1.0 - y / FLAG_H,
        "gradient_up" => y / FLAG_H,
        "bricks" => {
            let row = (y / 4.0) as i32;
            let shift = if row % 2 == 0 { 0.0 } else { 3.0 };
            on(y.rem_euclid(4.0) >= 1.0 && (x + shift).rem_euclid(6.0) >= 1.0)
        }
        "globe" => {
            // A globe on a short stand: rim, equator, a meridian and two
            // parallels.
            let (gx, gy) = (x - 10.0, y - 17.0);
            let r = (gx * gx + gy * gy).sqrt();
            let stand = (gx.abs() < 1.0 && (24.0..27.0).contains(&y)) || (gx.abs() < 4.0 && (27.0..28.5).contains(&y));
            let grid = gy.abs() < 0.7
                || (gx / 3.2).powi(2) + (gy / 6.6).powi(2) >= 0.8 && (gx / 3.2).powi(2) + (gy / 6.6).powi(2) <= 1.2
                || (gy.abs() - 3.6).abs() < 0.6;
            on(r <= 7.0 && (r >= 6.0 || grid) || stand)
        }
        "creeper" => {
            // An original grinning face: square eyes over a stepped mouth.
            let (ix, iy) = (x as i32, y as i32);
            let eye =
                (4..8).contains(&ix) && (12..16).contains(&iy) || (12..16).contains(&ix) && (12..16).contains(&iy);
            let mouth = (8..12).contains(&ix) && (16..20).contains(&iy)
                || (6..14).contains(&ix) && (19..23).contains(&iy) && !(8..12).contains(&ix)
                || (6..8).contains(&ix) && (23..25).contains(&iy)
                || (12..14).contains(&ix) && (23..25).contains(&iy);
            on(eye || mouth)
        }
        "skull" => {
            // A skull over crossed bones.
            let (sx, sy) = (x - 10.0, y - 15.0);
            let head = (sx / 5.0).powi(2) + (sy / 4.6).powi(2) <= 1.0 || sx.abs() < 3.0 && (15.0..21.0).contains(&y);
            let socket = ((sx.abs() - 2.2).powi(2) + (sy - 0.5).powi(2)) < 1.8;
            let bone = |dx: f32, dy: f32| (dy - dx * 0.9).abs() < 1.0 && dx.abs() < 7.5;
            let (bx, by) = (x - 10.0, y - 26.0);
            on(head && !socket || bone(bx, by) || bone(-bx, by))
        }
        "flower" => {
            // An eight-petalled flower with an open centre.
            let r = (cx * cx + cy * cy).sqrt();
            let a = cy.atan2(cx);
            on(r <= 4.0 + 2.5 * (4.0 * a).cos().max(0.0) && r >= 1.6)
        }
        "mojang" => {
            // An original emblem: a ring around a knotted bar.
            let r = (cx * cx + cy * cy).sqrt();
            let knot = cx.abs() < 1.2 && cy.abs() < 6.0 || (cx.abs() - 2.4).abs() < 1.0 && cy.abs() < 2.5;
            on((5.5..7.0).contains(&r) || knot)
        }
        "piglin" => {
            // A broad snout with two nostrils and tusks.
            let (sx, sy) = (cx, y - 22.0);
            let snout = (sx / 5.5).powi(4) + (sy / 3.5).powi(4) <= 1.0;
            let nostril = (sx.abs() - 2.2).powi(2) + sy.powi(2) < 1.4;
            let tusk = (sx.abs() - 6.0).abs() < 0.8 && (25.0..29.0).contains(&y);
            on(snout && !nostril || tusk)
        }
        "flow" => {
            // A spiral current from the centre outward.
            let r = (cx * cx + cy * cy).sqrt();
            let a = cy.atan2(cx).rem_euclid(std::f32::consts::TAU);
            let arm = (r - a * 1.1).rem_euclid(std::f32::consts::TAU * 1.1);
            on(r <= 8.5 && arm < 2.6)
        }
        "guster" => {
            // A gust: three swept bands around a narrow eye.
            let r = (cx * cx + (cy * 0.7).powi(2)).sqrt();
            let a = cy.atan2(cx);
            let band = ((r * 0.9 + a * 1.5).sin()) > 0.45 && r <= 9.0;
            let eye = cx.abs() < 4.0 && cy.abs() < 0.9;
            on((band && r > 2.5) || eye)
        }
        _ => 0.0,
    }
}

/// The 4×4 ordered-dither threshold for texel (`x`, `y`), 0..1, so partial
/// coverage (the gradients) survives the renderer's alpha test.
pub fn dither(x: usize, y: usize) -> f32 {
    const BAYER: [[u8; 4]; 4] = [[0, 8, 2, 10], [12, 4, 14, 6], [3, 11, 1, 9], [15, 7, 13, 5]];
    (f32::from(BAYER[y % 4][x % 4]) + 0.5) / 16.0
}

/// Whether texel (`tx`, `ty`) of a `size`×`size` image of the flag's top
/// (`half` 0) or bottom (`half` 1) square shows pattern `p`.
pub fn texel(p: BannerPattern, half: u8, tx: usize, ty: usize, size: usize) -> bool {
    let scale = FLAG_W / size as f32;
    let x = (tx as f32 + 0.5) * scale;
    let y = (ty as f32 + 0.5) * scale + f32::from(half) * FLAG_W;
    mask(p, x, y) > dither(tx, ty)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_registered_pattern_draws_something_but_not_everything() {
        for p in BannerPattern::all() {
            let covered = (0..40)
                .flat_map(|y| (0..20).map(move |x| (x, y)))
                .filter(|&(x, y)| mask(p, x as f32 + 0.5, y as f32 + 0.5) > 0.5)
                .count();
            assert!(covered > 0, "{} draws nothing", p.name());
            if p.name() != "base" {
                assert!(covered < 800, "{} covers the whole flag", p.name());
            }
        }
    }

    fn banner(c: DyeColor) -> Stack {
        Stack::new(banner_item(c), 1)
    }

    fn pattern(name: &str) -> BannerPattern {
        BannerPattern::from_name(name).unwrap()
    }

    #[test]
    fn the_loom_keeps_its_choice_only_while_offered() {
        let (b, dye) = (Some(banner(DyeColor::Blue)), Some(Stack::new(DyeColor::Red.dye(), 4)));
        let mut loom = Loom::default();
        assert_eq!(loom.refresh(b, dye, None), None, "nothing chosen yet");
        assert_eq!(loom.selectable.len(), 32);
        let cross = loom.selectable.iter().position(|&p| p == pattern("cross")).unwrap();
        let made = loom.choose(cross, b, dye).unwrap();
        assert_eq!(patterns(&made).layers, [BannerLayer { pattern: pattern("cross"), color: DyeColor::Red }]);
        // A pattern item offers its one pattern, chosen at once.
        let globe = Some(Stack::new(item(4), 1));
        let made = loom.refresh(b, dye, globe).unwrap();
        assert_eq!(loom.selected, Some(0));
        assert_eq!(patterns(&made).layers[0].pattern, pattern("globe"));
        // Taking the item away: the globe isn't in the plain list, so nothing is chosen.
        assert_eq!(loom.refresh(b, dye, None), None);
        assert_eq!(loom.selected, None);
        // Without a dye the list empties.
        assert_eq!(loom.refresh(b, None, None), None);
        assert!(loom.selectable.is_empty());
    }

    #[test]
    fn six_layers_is_the_loom_limit_and_washing_removes_the_top() {
        let mut b = banner(DyeColor::White);
        for i in 0..MAX_LAYERS {
            b = loom_result(b, Stack::new(DyeColor::ALL[i].dye(), 1), pattern("border")).unwrap();
        }
        let mut loom = Loom::default();
        assert_eq!(loom.refresh(Some(b), Some(Stack::new(DyeColor::Red.dye(), 1)), Some(Stack::new(item(0), 1))), None);
        assert_eq!(loom.selected, None, "a full banner takes no more");
        let washed = wash(b).unwrap();
        assert_eq!(patterns(&washed).layers.len(), MAX_LAYERS - 1);
        assert_eq!(patterns(&washed).layers[..], patterns(&b).layers[..MAX_LAYERS - 1]);
        assert_eq!(wash(banner(DyeColor::Red)), None, "a plain banner isn't washed");
        assert_eq!(wash(Stack::new(Item::SHIELD, 1)), None, "shields aren't washed");
    }

    #[test]
    fn duplication_and_shield_decoration_follow_java() {
        let patterned =
            loom_result(banner(DyeColor::Lime), Stack::new(DyeColor::Black.dye(), 1), pattern("skull")).unwrap();
        assert_eq!(banner_duplicate(&[patterned, banner(DyeColor::Lime)]), Some(patterned));
        assert_eq!(banner_duplicate(&[patterned, banner(DyeColor::Red)]), None, "colours must match");
        assert_eq!(banner_duplicate(&[patterned, patterned]), None, "one must be plain");
        assert_eq!(banner_duplicate(&[ominous_banner(), banner(DyeColor::White)]), None, "more than six layers");
        let shield = shield_decoration(&[Stack::new(Item::SHIELD, 1), patterned]).unwrap();
        let p = patterns(&shield);
        assert_eq!((p.base, p.layers.len()), (Some(DyeColor::Lime), 1));
        assert_eq!(shield_decoration(&[shield, patterned]), None, "a decorated shield takes no more");
        // A plain banner still gives its colour.
        let plain = shield_decoration(&[Stack::new(Item::SHIELD, 1), banner(DyeColor::Cyan)]).unwrap();
        assert_eq!(patterns(&plain).base, Some(DyeColor::Cyan));
    }

    #[test]
    fn mirrored_pairs_cover_complementary_halves() {
        let p = |n| BannerPattern::from_name(n).unwrap();
        for (a, b) in [
            ("half_vertical", "half_vertical_right"),
            ("half_horizontal", "half_horizontal_bottom"),
            ("diagonal_left", "diagonal_up_right"),
            ("diagonal_up_left", "diagonal_right"),
        ] {
            for y in 0..40 {
                for x in 0..20 {
                    let (fx, fy) = (x as f32 + 0.5, y as f32 + 0.5);
                    assert_eq!(mask(p(a), fx, fy) + mask(p(b), fx, fy), 1.0, "{a}/{b} at {x},{y}");
                }
            }
        }
    }
}
