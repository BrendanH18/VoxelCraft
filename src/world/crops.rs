//! Java 26.3 stems and beetroot. Original rendering lives in crop_textures.
//! Sources and differences: docs/verification/2026-11-04-v0.15-crops-bees.md.
use super::{
    World,
    block::{Block, Crop, Facing, RenderKind},
    shape::{Box16, Boxes},
};
use crate::{inventory::Stack, item::Item};
use glam::IVec3;

pub const FIRST: u16 = 10816;
pub const BEETROOT: u16 = FIRST + 24;
pub const LAST: u16 = BEETROOT + 3;
pub mod t {
    pub const BASE: u16 = 2992;
    pub const STEM: u16 = BASE;
    pub const ATTACHED: u16 = BASE + 8;
    pub const BEETROOT: u16 = BASE + 12;
    pub const END: u16 = BASE + 16;
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Fruit {
    Melon,
    Pumpkin,
}
impl Fruit {
    pub const fn seed(self) -> Item {
        match self {
            Self::Melon => Item::MELON_SEEDS,
            Self::Pumpkin => Item::PUMPKIN_SEEDS,
        }
    }
    pub const fn block(self) -> Block {
        match self {
            Self::Melon => Block::MELON,
            Self::Pumpkin => Block::PUMPKIN,
        }
    }
}
#[inline]
pub const fn owns(id: u16) -> bool {
    id >= FIRST && id <= LAST
}
pub const fn stem_block(fruit: Fruit, age: u8) -> Block {
    Block(FIRST + fruit as u16 * 12 + if age > 7 { 7 } else { age as u16 })
}
pub const fn attached(fruit: Fruit, facing: Facing) -> Block {
    Block(FIRST + fruit as u16 * 12 + 8 + facing as u16)
}
/// Fruit, age (7 for attached), and optional fruit-facing direction.
pub fn stem(b: Block) -> Option<(Fruit, u8, Option<Facing>)> {
    let rel = b.0.checked_sub(FIRST)?;
    if rel >= 24 {
        return None;
    }
    let fruit = if rel < 12 { Fruit::Melon } else { Fruit::Pumpkin };
    let local = rel % 12;
    Some((fruit, local.min(7) as u8, if local < 8 { None } else { Some(Facing::ALL[(local - 8) as usize]) }))
}
pub const fn registry(id: u16) -> (&'static str, RenderKind, [u16; 6]) {
    let local = (id - FIRST) % 12;
    let name = if id >= BEETROOT {
        "beetroots"
    } else if id < FIRST + 12 {
        if local < 8 { "melon stem" } else { "attached melon stem" }
    } else if local < 8 {
        "pumpkin stem"
    } else {
        "attached pumpkin stem"
    };
    let tex = if id >= BEETROOT {
        t::BEETROOT + id - BEETROOT
    } else if local < 8 {
        t::STEM + local
    } else {
        t::ATTACHED + local - 8
    };
    (name, RenderKind::Cross, [tex; 6])
}
pub fn outline(b: Block) -> Option<Boxes> {
    let (min, max) = if let Some((_, age, facing)) = stem(b) {
        if let Some(f) = facing {
            let bx = Box16 { min: [6, 0, 0], max: [10, 10, 10] }.turned(f.opposite());
            return Some(Boxes::from_box(bx));
        }
        ([7, 0, 7], [9, 2 + age * 2, 9])
    } else if let Some((Crop::Beetroot, age)) = b.as_crop() {
        ([0, 0, 0], [16, 2 + age * 2, 16])
    } else {
        return None;
    };
    Some(Boxes::from_box(Box16 { min, max }))
}
/// `supports_stem_fruit -> supports_vegetation -> substrate_overworld`.
pub fn supports_fruit(b: Block) -> bool {
    use super::overworld_blocks as ob;
    b.is_farmland()
        || matches!(
            b,
            Block::DIRT
                | Block::GRASS
                | Block::SNOWY_GRASS
                | ob::COARSE_DIRT
                | ob::ROOTED_DIRT
                | ob::PODZOL
                | ob::MYCELIUM
                | ob::MUD
                | ob::MUDDY_MANGROVE_ROOTS
                | ob::MOSS_BLOCK
                | ob::PALE_MOSS_BLOCK
        )
}
fn same_plant(a: Block, b: Block) -> bool {
    if let Some((fruit, _, None)) = stem(a) {
        return matches!(stem(b), Some((f, _, None)) if f == fruit);
    }
    a.as_crop().zip(b.as_crop()).is_some_and(|((a, _), (b, _))| a == b)
}
impl World {
    /// `CropBlock.getGrowthSpeed`: hydrated centre contributes 3, hydrated
    /// neighbours 3/4 each, dry farmland 1 and 1/4. Crowding halves it once.
    pub fn crop_growth_speed(&self, p: IVec3, plant: Block) -> f32 {
        let mut speed = 1.0;
        for dx in -1..=1 {
            for dz in -1..=1 {
                let b = self.get_block(p + IVec3::new(dx, -1, dz));
                let mut v = match b {
                    Some(Block::WET_FARMLAND) => 3.0,
                    Some(Block::FARMLAND) => 1.0,
                    _ => 0.0,
                };
                if dx != 0 || dz != 0 {
                    v /= 4.0;
                }
                speed += v;
            }
        }
        let same = |d| self.get_block(p + d).is_some_and(|b| same_plant(plant, b));
        let x = same(IVec3::X) || same(IVec3::NEG_X);
        let z = same(IVec3::Z) || same(IVec3::NEG_Z);
        if (x && z) || [-1, 1].into_iter().any(|x| [-1, 1].into_iter().any(|z| same(IVec3::new(x, 0, z)))) {
            speed /= 2.0;
        }
        speed
    }
    pub(super) fn tick_crop(&mut self, p: IVec3, b: Block) {
        let Some((crop, age)) = b.as_crop() else { return };
        // Java's `!= 0` gate: TWO thirds, not one third (brief corrected).
        if crop == Crop::Beetroot && self.one_in(3) {
            return;
        }
        if age < crop.max_age() && self.growth_light(p, false) >= 9 {
            let bound = (25.0 / self.crop_growth_speed(p, b)) as u64 + 1;
            if self.one_in(bound) {
                self.edit(p, Block::crop(crop, age + 1), false);
            }
        }
    }
    pub(super) fn tick_stem(&mut self, p: IVec3, b: Block) {
        let Some((fruit, age, None)) = stem(b) else { return };
        if self.growth_light(p, false) < 9 {
            return;
        }
        let bound = (25.0 / self.crop_growth_speed(p, b)) as u64 + 1;
        if !self.one_in(bound) {
            return;
        }
        if age < 7 {
            self.edit(p, stem_block(fruit, age + 1), false);
            return;
        }
        let facing = Facing::ALL[(self.roll() % 4) as usize];
        let q = p + facing.offset();
        if self.get_block(q) == Some(Block::AIR) && self.get_block(q - IVec3::Y).is_some_and(supports_fruit) {
            self.edit(q, fruit.block(), false);
            self.edit(p, attached(fruit, facing), false);
        }
    }
    pub(super) fn stem_bone_meal(&mut self, p: IVec3, b: Block) -> Option<bool> {
        let (fruit, age, facing) = stem(b)?;
        if facing.is_some() || age == 7 {
            return Some(false);
        }
        let age = (age + 2 + (self.roll() % 4) as u8).min(7);
        let grown = stem_block(fruit, age);
        self.edit(p, grown, true);
        // Java calls randomTick once on reaching age 7, including its chance.
        if age == 7 {
            self.tick_stem(p, grown);
        }
        Some(true)
    }
    pub(super) fn detach_stems_beside(&mut self, p: IVec3, old: Block, new: Block, sync: bool) {
        if old == new || !matches!(old, Block::MELON | Block::PUMPKIN) {
            return;
        }
        for f in Facing::ALL {
            let q = p - f.offset();
            if let Some((fruit, _, Some(facing))) = self.get_block(q).and_then(stem)
                && facing == f
                && new != fruit.block()
            {
                self.edit(q, stem_block(fruit, 7), sync);
            }
        }
    }
    pub(super) fn crop_drops(&mut self, b: Block, fortune: u64, out: &mut Vec<Stack>) {
        if let Some((fruit, age, _)) = stem(b) {
            let chance = (age as f32 + 1.0) / 15.0;
            let n = (0..3).filter(|_| self.next_float() < chance).count() as u8;
            if n > 0 {
                out.push(Stack::new(fruit.seed(), n));
            }
        } else if b.as_crop() == Some((Crop::Beetroot, 3)) {
            let n = 1 + (0..3 + fortune).filter(|_| self.next_float() < 0.571_428_6).count() as u8;
            out.push(Stack::new(Item::BEETROOT_SEEDS, n));
        } else if b == Block::MELON {
            let n = (3 + self.roll() % 5 + self.up_to(fortune)).min(9) as u8;
            if let Some(s) = out.first_mut() {
                s.count = n;
            }
        }
    }
}
#[cfg(test)]
mod tests;
