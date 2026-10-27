//! Crafting recipes of the v0.12 utility blocks (26.3 `data/minecraft/recipe`).

use super::{ANY_LOG, Recipe, b, shaped, shapeless};
use crate::item::Item;
use crate::world::{block::Block, gadgets, nether_biome_blocks as n, utilities as u};

/// Java's `#minecraft:logs`: every log and wood that exists here, and the
/// Nether stems and hyphae (`logs_that_burn` plus `crimson_stems` and
/// `warped_stems`).
pub(super) const LOGS: &[Item] = &{
    const STEMS: [Item; 8] = [
        b(n::CRIMSON_STEM),
        b(n::WARPED_STEM),
        b(n::STRIPPED_CRIMSON_STEM),
        b(n::STRIPPED_WARPED_STEM),
        b(n::CRIMSON_HYPHAE),
        b(n::WARPED_HYPHAE),
        b(n::STRIPPED_CRIMSON_HYPHAE),
        b(n::STRIPPED_WARPED_HYPHAE),
    ];
    let mut out = [Item(0); ANY_LOG.len() + STEMS.len()];
    let mut i = 0;
    while i < out.len() {
        out[i] = if i < ANY_LOG.len() { ANY_LOG[i] } else { STEMS[i - ANY_LOG.len()] };
        i += 1;
    }
    out
};

const STICK: &[Item] = &[Item::STICK];

pub(super) fn add(r: &mut Vec<Recipe>) {
    const IRON: &[Item] = &[Item::IRON_INGOT];
    const BRICK: &[Item] = &[Item::BRICK];
    const COALS: &[Item] = &[Item::COAL, Item::CHARCOAL];
    const SOUL_FIRE_BASE: &[Item] = &[b(Block::SOUL_SAND), b(n::SOUL_SOIL)];
    const OBSIDIAN: &[Item] = &[b(Block::OBSIDIAN)];
    const EYE: &[Item] = &[Item::EYE_OF_ENDER];
    const CHEST: &[Item] = &[b(Block::CHEST)];
    const HOOK: &[Item] = &[b(gadgets::HOOK)];
    const CRYING_OBSIDIAN: &[Item] = &[b(Block::CRYING_OBSIDIAN)];
    const GLOWSTONE: &[Item] = &[b(Block::GLOWSTONE)];
    r.extend([
        shaped(&["# #", "# #", "###"], &[('#', IRON)], b(u::CAULDRON), 1),
        shaped(&["# #", " # "], &[('#', BRICK)], b(u::FLOWER_POT), 1),
        // `decorated_pot_simple`: four bricks; sherds come later.
        shaped(&[" # ", "# #", " # "], &[('#', BRICK)], b(u::DECORATED_POT), 1),
        shaped(&[" S ", "SCS", "LLL"], &[('S', STICK), ('C', COALS), ('L', LOGS)], b(u::CAMPFIRE), 1),
        shaped(&[" S ", "S#S", "LLL"], &[('S', STICK), ('#', SOUL_FIRE_BASE), ('L', LOGS)], b(u::SOUL_CAMPFIRE), 1),
        shaped(&["###", "#E#", "###"], &[('#', OBSIDIAN), ('E', EYE)], b(u::ENDER_CHEST), 1),
        shapeless(&[CHEST, HOOK], b(u::TRAPPED_CHEST), 1),
        shaped(&["OOO", "GGG", "OOO"], &[('O', CRYING_OBSIDIAN), ('G', GLOWSTONE)], b(u::RESPAWN_ANCHOR), 1),
    ]);
    // The decorations: frames of sticks round leather, a painting round
    // any wool, and the armor stand's sticks on a smooth stone slab.
    const LEATHER: &[Item] = &[Item::LEATHER];
    const FRAME: &[Item] = &[Item::ITEM_FRAME];
    const GLOW_INK: &[Item] = &[Item::GLOW_INK_SAC];
    let wool: &'static [Item] = Box::leak(
        crate::color::DyeColor::ALL.iter().map(|&c| b(Block::wool(c))).collect::<Vec<_>>().into_boxed_slice(),
    );
    r.extend([
        shaped(&["###", "#X#", "###"], &[('#', STICK), ('X', LEATHER)], Item::ITEM_FRAME, 1),
        shapeless(&[FRAME, GLOW_INK], Item::GLOW_ITEM_FRAME, 1),
        shaped(&["###", "#X#", "###"], &[('#', STICK), ('X', wool)], Item::PAINTING, 1),
    ]);
    if let Some(slab) = Block::from_name("smooth stone slab") {
        let slab: &'static [Item] = Box::leak(Box::new([b(slab)]));
        r.push(shaped(&["///", " / ", "/_/"], &[('/', STICK), ('_', slab)], Item::ARMOR_STAND, 1));
    }
    // Eight chiseled stone bricks around an iron ingot. Chiseled stone
    // bricks come with the stone track; until they are registered the
    // lodestone is creative-only.
    if let Some(chiseled) = Block::from_name("chiseled stone bricks") {
        let ring: &'static [Item] = Box::leak(Box::new([b(chiseled)]));
        r.push(shaped(&["SSS", "S#S", "SSS"], &[('S', ring), ('#', IRON)], b(u::LODESTONE), 1));
    }
}
