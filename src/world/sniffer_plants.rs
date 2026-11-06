//! Java 26.3 ancient crops. State IDs are Track C's append-only reservation.
//! Source audit and remaining shape differences: docs/verification/2026-11-05-v0.15-sniffers.md.
use super::{
    World,
    block::{Block, RenderKind},
    shape::{Box16, Boxes},
};
use crate::item::Item;
use glam::IVec3;
pub const FIRST: u16 = 15104;
pub const TORCH_CROP: Block = Block(FIRST);
pub const TORCHFLOWER: Block = Block(FIRST + 2);
pub const POTTED_TORCHFLOWER: Block = Block(FIRST + 3);
pub const PITCHER_CROP: Block = Block(FIRST + 4);
pub const PITCHER_PLANT: Block = Block(FIRST + 14);
pub const LAST: u16 = FIRST + 15;
pub mod t {
    pub const BASE: u16 = 3440;
    pub const TORCH: u16 = BASE;
    pub const POTTED: u16 = BASE + 3;
    pub const PITCHER: u16 = BASE + 4;
    pub const FLOWER: u16 = BASE + 14;
    pub const END: u16 = BASE + 16;
}
pub const fn owns(id: u16) -> bool {
    id >= FIRST && id <= LAST
}
pub const fn pitcher(age: u8, upper: bool) -> Block {
    Block(PITCHER_CROP.0 + if age > 4 { 4 } else { age as u16 } + if upper { 5 } else { 0 })
}
pub fn pitcher_state(b: Block) -> Option<(u8, bool)> {
    let n = b.0.checked_sub(PITCHER_CROP.0)?;
    (n < 10).then_some(((n % 5) as u8, n >= 5))
}
pub fn torch_age(b: Block) -> Option<u8> {
    (TORCH_CROP.0..TORCH_CROP.0 + 2).contains(&b.0).then_some((b.0.wrapping_sub(TORCH_CROP.0)) as u8)
}
pub fn double(b: Block) -> Option<(Block, bool)> {
    if let Some((age, upper)) = pitcher_state(b) {
        return (age >= 3).then_some((pitcher(age, false), upper));
    }
    (PITCHER_PLANT.0..=LAST).contains(&b.0).then_some((PITCHER_PLANT, b.0 == LAST))
}
pub fn partner(lower: Block) -> Block {
    if let Some((age, _)) = pitcher_state(lower) { pitcher(age, true) } else { Block(PITCHER_PLANT.0 + 1) }
}
pub fn picked(b: Block) -> Option<Item> {
    if torch_age(b).is_some() {
        Some(Item::TORCHFLOWER_SEEDS)
    } else if pitcher_state(b).is_some() {
        Some(Item::PITCHER_POD)
    } else if (PITCHER_PLANT.0..=LAST).contains(&b.0) {
        Some(PITCHER_PLANT.into())
    } else {
        None
    }
}
pub fn drop(b: Block) -> Option<Option<Item>> {
    if torch_age(b).is_some() {
        Some(Some(Item::TORCHFLOWER_SEEDS))
    } else if let Some((age, upper)) = pitcher_state(b) {
        Some((!upper).then(|| if age == 4 { PITCHER_PLANT.into() } else { Item::PITCHER_POD }))
    } else if b.0 == LAST {
        Some(None)
    } else {
        None
    }
}
pub fn can_stay(b: Block, below: Block) -> Option<bool> {
    if torch_age(b).is_some() {
        Some(below.is_farmland())
    } else if let Some((age, upper)) = pitcher_state(b) {
        Some(if upper { pitcher_state(below).is_some_and(|(_, u)| !u) } else { below.is_farmland() && age <= 4 })
    } else if b == TORCHFLOWER || b == PITCHER_PLANT {
        Some(super::crops::supports_fruit(below))
    } else if b.0 == LAST {
        Some(below == PITCHER_PLANT)
    } else {
        None
    }
}
pub const fn registry(id: u16) -> (&'static str, RenderKind, [u16; 6]) {
    if id == POTTED_TORCHFLOWER.0 {
        let u = super::utilities::t::POT_SOIL;
        return (
            "potted torchflower",
            RenderKind::Shaped,
            [t::POTTED, t::POTTED, u, super::utilities::t::POT_BOTTOM, t::POTTED, t::POTTED],
        );
    }
    let (name, tex) = if id < TORCHFLOWER.0 {
        ("torchflower crop", t::TORCH + id - FIRST)
    } else if id == TORCHFLOWER.0 {
        ("torchflower", t::TORCH + 2)
    } else if id < PITCHER_PLANT.0 {
        ("pitcher crop", t::PITCHER + id - PITCHER_CROP.0)
    } else {
        ("pitcher plant", t::FLOWER + id - PITCHER_PLANT.0)
    };
    (name, RenderKind::Cross, [tex; 6])
}
pub fn outline(b: Block) -> Option<Boxes> {
    let (width, height) = if let Some(age) = torch_age(b) {
        (6, 6 + 4 * age)
    } else if let Some((age, upper)) = pitcher_state(b) {
        let height: u8 = [4, 15, 17, 28, 32][age as usize];
        (if age == 0 { 6 } else { 10 }, if upper { height.saturating_sub(17) } else { (height - 1).min(16) })
    } else if b == TORCHFLOWER {
        (6, 10)
    } else if (PITCHER_PLANT.0..=LAST).contains(&b.0) {
        (12, 16)
    } else {
        return None;
    };
    Some(Boxes::from_box(Box16 { min: [8 - width / 2, 0, 8 - width / 2], max: [8 + width / 2, height, 8 + width / 2] }))
}
impl World {
    fn pitcher_can_grow(&mut self, p: IVec3, age: u8) -> bool {
        age < 4
            && self.contains_y(p.y + 1)
            && self.growth_light(p, false) >= 8
            && (age + 1 < 3
                || self.get_block(p + IVec3::Y).is_some_and(|b| b == Block::AIR || pitcher_state(b).is_some()))
    }
    fn grow_pitcher(&mut self, p: IVec3, age: u8) -> bool {
        if !self.pitcher_can_grow(p, age) {
            return false;
        }
        // Write both halves before neighbour/support validation, as Java flag 2 does.
        self.edit(p, pitcher(age + 1, false), true);
        if age + 1 >= 3 {
            self.edit(p + IVec3::Y, pitcher(age + 1, true), true);
        }
        true
    }
    pub(super) fn tick_sniffer_crop(&mut self, p: IVec3, b: Block) {
        if let Some(age) = torch_age(b) {
            if self.one_in(3) || self.growth_light(p, false) < 9 {
                return;
            }
            let bound = (25.0 / self.crop_growth_speed(p, b)) as u64 + 1;
            if self.one_in(bound) {
                self.edit(p, if age == 0 { Block(TORCH_CROP.0 + 1) } else { TORCHFLOWER }, false);
            }
        } else if let Some((age, false)) = pitcher_state(b) {
            let bound = (25.0 / self.crop_growth_speed(p, b)) as u64 + 1;
            if age < 4 && self.one_in(bound) {
                self.grow_pitcher(p, age);
            }
        }
    }
    pub(super) fn sniffer_bone_meal(&mut self, p: IVec3, b: Block) -> Option<bool> {
        if let Some(age) = torch_age(b) {
            self.edit(p, if age == 0 { Block(TORCH_CROP.0 + 1) } else { TORCHFLOWER }, true);
            return Some(true);
        }
        if let Some((_, upper)) = pitcher_state(b) {
            let p = if upper { p - IVec3::Y } else { p };
            return Some(match self.get_block(p).and_then(pitcher_state) {
                Some((age, false)) => self.grow_pitcher(p, age),
                _ => false,
            });
        }
        (b == TORCHFLOWER || (PITCHER_PLANT.0..=LAST).contains(&b.0)).then_some(false)
    }
    pub(super) fn update_sniffer_plants(&mut self, p: IVec3) {
        for q in [p, p - IVec3::Y, p + IVec3::Y] {
            let Some(b) = self.get_block(q) else { continue };
            let Some((lower, upper)) = double(b) else { continue };
            let other = if upper { q - IVec3::Y } else { q + IVec3::Y };
            let want = if upper { lower } else { partner(lower) };
            if self.get_block(other).is_some_and(|b| {
                // DoublePlantBlock checks block type and opposite half, not age.
                if pitcher_state(want).is_some() { pitcher_state(b).is_none_or(|(_, u)| u == upper) } else { b != want }
            }) {
                self.edit(q, Block::AIR, true);
                if !upper {
                    self.spill_block(q, b);
                }
            }
        }
    }
}
#[cfg(test)]
mod tests;
