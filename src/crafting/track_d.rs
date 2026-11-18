//! v0.15 Track D recipes (Java 26.3 `data/minecraft/recipe`):
//! `spectral_arrow.json` (four glowstone dust round an arrow make two) and
//! `<pattern>_armor_trim_smithing_template.json` (seven diamonds, the
//! template and its block make two templates).

use super::{Ingredient, Recipe, shaped};
use crate::content::trim::{copy_items, template};
use crate::inventory::components::TrimPattern;
use crate::item::Item;

pub(super) fn add(r: &mut Vec<Recipe>) {
    const GLOWSTONE: Ingredient = &[Item::GLOWSTONE_DUST];
    const ARROW: Ingredient = &[Item::ARROW];
    const DIAMOND: Ingredient = &[Item::DIAMOND];
    r.push(shaped(&[" # ", "#X#", " # "], &[('#', GLOWSTONE), ('X', ARROW)], Item::SPECTRAL_ARROW, 2));
    for pattern in TrimPattern::ALL {
        // The spire template's purpur block is a registry gap: its copy
        // recipe waits with it.
        let Some(copy) = copy_items(pattern) else { continue };
        let copy: &'static [Item] = Box::leak(copy.into_boxed_slice());
        let this: &'static [Item] = Box::leak(Box::new([template(pattern)]));
        r.push(shaped(&["#S#", "#C#", "###"], &[('#', DIAMOND), ('C', copy), ('S', this)], template(pattern), 2));
    }
}
