//! Java 26.3 acquisition recipes for hives and honeycomb blocks.
use super::{Recipe, b, shaped};
use crate::{item::Item, world::bees};
pub(super) fn add(r: &mut Vec<Recipe>) {
    r.push(shaped(&["PPP", "CCC", "PPP"], &[('P', super::PLANKS), ('C', &[Item::HONEYCOMB])], b(bees::HIVE), 1));
    r.push(shaped(&["CC", "CC"], &[('C', &[Item::HONEYCOMB])], b(bees::HONEYCOMB_BLOCK), 1));
}
