//! Bastion remnants: four hand-authored layouts and Java chest pools.
//! Cached layouts use shared Java Nether-complex placement; no jigsaw assets.

use crate::item::Item;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum ChestKind {
    Treasure,
    Bridge,
    HoglinStable,
    Other,
}

/// A rational independent chance for one item in a bastion chest.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct LootChance {
    pub item: Item,
    pub count: u8,
    pub numerator: u8,
    pub denominator: u8,
}

/// Java 1.21's template pool: every treasure-room chest has one; bridge,
/// hoglin-stable and generic bastion chests have a one-in-ten chance.
pub const NETHERITE_UPGRADE: [(ChestKind, LootChance); 4] = [
    (ChestKind::Treasure, LootChance { item: Item::NETHERITE_UPGRADE, count: 1, numerator: 1, denominator: 1 }),
    (ChestKind::Bridge, LootChance { item: Item::NETHERITE_UPGRADE, count: 1, numerator: 1, denominator: 10 }),
    (ChestKind::HoglinStable, LootChance { item: Item::NETHERITE_UPGRADE, count: 1, numerator: 1, denominator: 10 }),
    (ChestKind::Other, LootChance { item: Item::NETHERITE_UPGRADE, count: 1, numerator: 1, denominator: 10 }),
];

pub fn netherite_upgrade(kind: ChestKind) -> LootChance {
    NETHERITE_UPGRADE.iter().find(|(chest, _)| *chest == kind).expect("all bastion chest kinds").1
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn upgrade_template_matches_java_bastion_tables() {
        for kind in [ChestKind::Bridge, ChestKind::HoglinStable, ChestKind::Other] {
            assert_eq!(
                netherite_upgrade(kind),
                LootChance { item: Item::NETHERITE_UPGRADE, count: 1, numerator: 1, denominator: 10 }
            );
        }
        assert_eq!(
            netherite_upgrade(ChestKind::Treasure),
            LootChance { item: Item::NETHERITE_UPGRADE, count: 1, numerator: 1, denominator: 1 }
        );
    }
}

use super::{
    block::{Block, Facing},
    chunk::{CHUNK_SIZE, CHUNK_SIZE_I, CHUNK_VOLUME},
    fortress::Feature,
    nether_complexes::{self, Complex, REGION, START_MAX},
    noise::hash3,
    structure::{Bounds, Oriented, Paint, Rng},
};
use glam::{IVec2, IVec3};
use rustc_hash::FxHashMap;
use std::sync::{Arc, Mutex};

/// Hand-authored reductions of Java's four start pools, rather than a jigsaw port.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Kind {
    Housing,
    Stables,
    Treasure,
    Bridge,
}
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum PieceKind {
    Housing,
    Stable,
    Court,
    Tower,
    Treasure,
    Bridge,
    Face,
}
#[derive(Clone, Debug)]
pub struct Piece {
    pub kind: PieceKind,
    pub bounds: Bounds,
    pub facing: Facing,
    size: IVec3,
    seed: u64,
    chests: Vec<(IVec3, ChestKind)>,
}
impl Oriented for Piece {
    fn bounds(&self) -> &Bounds {
        &self.bounds
    }
    fn facing(&self) -> Facing {
        self.facing
    }
}
#[derive(Debug)]
pub struct Bastion {
    pub kind: Kind,
    pub bounds: Bounds,
    pub pieces: Vec<Piece>,
}
impl Bastion {
    pub fn generate(seed: u64, start: IVec2, kind: Kind, facing: Facing) -> Self {
        // All pieces share the same local frame. Abutting floor edges and
        // doorways rotate together, including north/west mirrored frames.
        let frame = Piece {
            kind: PieceKind::Court,
            bounds: Bounds::oriented(IVec3::new(start.x, 33, start.y), IVec3::ZERO, IVec3::new(53, 38, 65), facing),
            facing,
            size: IVec3::ZERO,
            seed,
            chests: Vec::new(),
        };
        let mut pieces = Vec::new();
        let mut add = |k, offset: IVec3, size: IVec3, chests: &[(IVec3, ChestKind)]| {
            let a = frame.world(offset.x, offset.y, offset.z);
            let b = frame.world(offset.x + size.x - 1, offset.y + size.y - 1, offset.z + size.z - 1);
            pieces.push(Piece {
                kind: k,
                bounds: Bounds { min: a.min(b), max: a.max(b) },
                facing,
                size,
                seed: hash3(offset.x, offset.y, offset.z, seed),
                chests: chests.to_vec(),
            });
        };
        let v = IVec3::new;
        match kind {
            Kind::Housing => {
                add(PieceKind::Court, v(15, 0, 15), v(19, 5, 19), &[]);
                for x in [0, 34] {
                    for z in [0, 34] {
                        add(PieceKind::Housing, v(x, 0, z), v(15, 27, 15), &[(v(4, 9, 4), ChestKind::Other)]);
                    }
                }
                for z in [8, 34] {
                    add(PieceKind::Bridge, v(15, 0, z), v(19, 8, 7), &[]);
                }
                for x in [8, 34] {
                    add(PieceKind::Bridge, v(x, 0, 15), v(7, 8, 19), &[]);
                }
            }
            Kind::Stables => {
                add(
                    PieceKind::Stable,
                    v(0, 0, 0),
                    v(21, 22, 37),
                    &[(v(4, 9, 4), ChestKind::HoglinStable), (v(16, 17, 30), ChestKind::HoglinStable)],
                );
                add(PieceKind::Housing, v(32, 0, 0), v(21, 27, 37), &[(v(4, 9, 4), ChestKind::Other)]);
                add(PieceKind::Bridge, v(21, 0, 14), v(11, 8, 9), &[]);
            }
            Kind::Treasure => {
                add(
                    PieceKind::Treasure,
                    v(0, 0, 0),
                    v(33, 38, 33),
                    &[(v(15, 5, 15), ChestKind::Treasure), (v(17, 5, 15), ChestKind::Treasure)],
                );
                add(PieceKind::Bridge, v(13, 16, 33), v(7, 8, 15), &[]);
                for x in [0, 18] {
                    add(PieceKind::Tower, v(x, 8, 48), v(15, 27, 17), &[(v(4, 9, 4), ChestKind::Other)]);
                }
                add(PieceKind::Bridge, v(15, 16, 48), v(3, 8, 17), &[]);
            }
            Kind::Bridge => {
                add(PieceKind::Face, v(0, 0, 0), v(33, 31, 21), &[(v(16, 9, 8), ChestKind::Bridge)]);
                add(PieceKind::Bridge, v(12, 0, 21), v(9, 8, 32), &[]);
                add(PieceKind::Tower, v(9, 0, 53), v(15, 27, 12), &[(v(4, 9, 4), ChestKind::Other)]);
            }
        }
        let bounds = pieces
            .iter()
            .map(|p| p.bounds)
            .reduce(|a, b| Bounds { min: a.min.min(b.min), max: a.max.max(b.max) })
            .unwrap();
        Self { kind, bounds, pieces }
    }
}

pub struct Bastions {
    seed: u64,
    cache: Mutex<FxHashMap<IVec2, Arc<Bastion>>>,
    /// Java only starts bastions in biomes tagged `has_structure/bastion_remnant`.
    biomes: super::nether_biome::NetherBiomeSource,
}
impl Bastions {
    pub fn new(seed: u64) -> Self {
        Self {
            seed,
            cache: Mutex::new(FxHashMap::default()),
            biomes: super::nether_biome::NetherBiomeSource::new(seed),
        }
    }
    /// Java tests the biome at the centre of the start chunk (`start` is its
    /// corner + 2); basalt deltas reject the bastion and leave the region empty.
    fn biome_allows(&self, start: IVec2) -> bool {
        self.biomes.biome(start.x + 6, start.y + 6).has_bastions()
    }
    pub fn get(&self, region: IVec2) -> Option<Arc<Bastion>> {
        let (start, selected) = nether_complexes::placement(self.seed, region);
        if selected != Complex::Bastion || !self.biome_allows(start) {
            return None;
        }
        if let Some(b) = self.cache.lock().unwrap().get(&region) {
            return Some(b.clone());
        }
        let seed = hash3(region.x, 0, region.y, self.seed ^ 0xBA5710);
        let mut rng = Rng(seed);
        let kind = [Kind::Housing, Kind::Stables, Kind::Treasure, Kind::Bridge][rng.below(4) as usize];
        let b = Arc::new(Bastion::generate(seed, start, kind, Facing::ALL[rng.below(4) as usize]));
        let mut cache = self.cache.lock().unwrap();
        if cache.len() >= 256 {
            cache.clear();
        }
        Some(cache.entry(region).or_insert(b).clone())
    }
    fn near(&self, min: IVec2, max: IVec2) -> Vec<Arc<Bastion>> {
        let lo = (min - START_MAX - 65).div_euclid(IVec2::splat(REGION));
        let hi = (max + 65).div_euclid(IVec2::splat(REGION));
        let mut out = Vec::new();
        for z in lo.y..=hi.y {
            for x in lo.x..=hi.x {
                let region = IVec2::new(x, z);
                let (p, k) = nether_complexes::placement(self.seed, region);
                if k != Complex::Bastion
                    || (p - 65).cmpgt(max).any()
                    || (p + 65).cmplt(min).any()
                    || !self.biome_allows(p)
                {
                    continue;
                }
                if let Some(b) = self.get(region)
                    && b.bounds.min.x <= max.x
                    && b.bounds.max.x >= min.x
                    && b.bounds.min.z <= max.y
                    && b.bounds.max.z >= min.y
                {
                    out.push(b);
                }
            }
        }
        out
    }
    /// Bastions whose bounds come within `r` blocks of column `p`, for
    /// placing their resident mobs.
    pub fn around(&self, p: IVec2, r: i32) -> Vec<Arc<Bastion>> {
        self.near(p - r, p + r)
    }
    pub fn nearest(&self, p: IVec2) -> Option<IVec3> {
        let r = p.div_euclid(IVec2::splat(REGION));
        (-2..=2)
            .flat_map(|z| (-2..=2).map(move |x| r + IVec2::new(x, z)))
            .filter_map(|r| self.get(r))
            .map(|b| {
                IVec3::new((b.bounds.min.x + b.bounds.max.x) / 2, b.bounds.min.y, (b.bounds.min.z + b.bounds.max.z) / 2)
            })
            .min_by_key(|at| (IVec2::new(at.x, at.z) - p).as_i64vec2().length_squared())
    }
    fn paint_region(&self, blocks: &mut [Block], base: IVec3, top: IVec3) {
        if top.y < 33 || base.y > 70 {
            return;
        }
        let chunk = Bounds { min: base, max: top };
        for b in self.near(IVec2::new(base.x, base.z), IVec2::new(top.x, top.z)) {
            for piece in &b.pieces {
                if piece.bounds.intersects(&chunk) {
                    Paint { blocks, base, top, piece, open: &|_, _, _, _| false, pillar: Block::BLACKSTONE }
                        .bastion_piece();
                }
            }
        }
    }
    pub fn paint(&self, blocks: &mut [Block; CHUNK_VOLUME], base: IVec3) {
        self.paint_region(blocks, base, base + IVec3::splat(CHUNK_SIZE_I - 1));
    }
    pub fn column_at(&self, base: IVec3, mut terrain: [Block; CHUNK_SIZE]) -> [Block; CHUNK_SIZE] {
        self.paint_region(&mut terrain, base, base + IVec3::Y * (CHUNK_SIZE_I - 1));
        terrain
    }
    pub fn features(&self, cpos: IVec3) -> Vec<(IVec3, Feature)> {
        let base = cpos * CHUNK_SIZE_I;
        let chunk = Bounds { min: base, max: base + IVec3::splat(CHUNK_SIZE_I - 1) };
        let mut out = Vec::new();
        if chunk.max.y < 33 || chunk.min.y > 70 {
            return out;
        }
        for b in self.near(IVec2::new(base.x, base.z), IVec2::new(chunk.max.x, chunk.max.z)) {
            for piece in &b.pieces {
                for &(local, kind) in &piece.chests {
                    let p = piece.world(local.x, local.y, local.z);
                    if chunk.contains(p) {
                        out.push((p, Feature::BastionChest(hash3(local.x, local.y, local.z, piece.seed), kind)));
                    }
                }
            }
        }
        out
    }
}

const BLACK: Block = Block::BLACKSTONE;
const BRICKS: Block = Block::POLISHED_BLACKSTONE_BRICKS;
const AIR: Block = Block::AIR;
impl Paint<'_, Piece> {
    fn bastion_piece(&mut self) {
        let IVec3 { x: w, y: h, z: d } = self.piece.size;
        self.fill(0, 0, 0, w - 1, h - 1, d - 1, AIR);
        self.fill(0, 0, 0, w - 1, 2, d - 1, BLACK);
        match self.piece.kind {
            PieceKind::Court => {
                self.fill(7, 3, 7, 11, 3, 11, Block::SOUL_SAND);
                self.fill(7, 4, 7, 11, 4, 11, Block::nether_wart(3));
            }
            PieceKind::Bridge => {
                self.fill(0, 2, 0, w - 1, 2, d - 1, BRICKS);
                self.fill(0, 4, 0, 0, 5, d - 1, BLACK);
                self.fill(w - 1, 4, 0, w - 1, 5, d - 1, BLACK);
                // Lateral bridges keep two open ends instead of sealed rails.
                if w > d {
                    self.fill(0, 4, 1, 0, 5, d - 2, AIR);
                    self.fill(w - 1, 4, 1, w - 1, 5, d - 2, AIR);
                    self.fill(0, 4, 0, w - 1, 5, 0, BLACK);
                    self.fill(0, 4, d - 1, w - 1, 5, d - 1, BLACK);
                }
            }
            _ => {
                self.shell(w, h, d);
                if self.piece.kind == PieceKind::Treasure {
                    self.treasure(w, d);
                } else {
                    self.floors(w, h, d);
                }
                if self.piece.kind == PieceKind::Stable {
                    for y in [3, 11, 19] {
                        for z in (6..d - 3).step_by(8) {
                            self.fill(2, y, z, 7, y + 2, z, crate::world::nether_blocks::shape_id(0, 5));
                            self.fill(w - 8, y, z, w - 3, y + 2, z, crate::world::nether_blocks::shape_id(0, 5));
                        }
                    }
                    self.fill(w / 2, 2, 3, w / 2, 2, d - 4, Block::LAVA);
                }
                if self.piece.kind == PieceKind::Face {
                    // Bridge bastion's piglin face: eyes, projecting snout and tusks.
                    self.fill(8, 13, d - 1, 24, 26, d - 1, Block::POLISHED_BLACKSTONE);
                    for x in [10, 21] {
                        self.fill(x, 22, d - 1, x + 1, 23, d - 1, Block::GOLD_BLOCK);
                    }
                    self.fill(13, 16, d - 2, 19, 20, d - 1, Block::CHISELED_POLISHED_BLACKSTONE);
                    for x in [12, 20] {
                        self.fill(x, 14, d - 1, x, 18, d - 1, Block::GILDED_BLACKSTONE);
                    }
                }
            }
        }
        for &(p, _) in &self.piece.chests {
            self.fill(p.x, p.y - 1, p.z, p.x, p.y - 1, p.z, BRICKS);
            self.fill(p.x, p.y, p.z, p.x, p.y, p.z, self.facing(Block::CHEST, Facing::South));
        }
    }
    fn shell(&mut self, w: i32, h: i32, d: i32) {
        self.fill(0, 3, 0, 1, h - 1, d - 1, BLACK);
        self.fill(w - 2, 3, 0, w - 1, h - 1, d - 1, BLACK);
        self.fill(2, 3, 0, w - 3, h - 1, 1, BLACK);
        self.fill(2, 3, d - 2, w - 3, h - 1, d - 1, BLACK);
        for y in (4..h).step_by(4) {
            for x in (2..w - 2).step_by(3) {
                for z in [0, d - 1] {
                    let roll = hash3(x, y, z, self.piece.seed) % 12;
                    let b = match roll {
                        0 if y > 10 => AIR,
                        1 => Block::GILDED_BLACKSTONE,
                        2..=4 => Block::CRACKED_POLISHED_BLACKSTONE_BRICKS,
                        _ => BRICKS,
                    };
                    self.fill(x, y, z, (x + 1).min(w - 2), (y + 2).min(h - 1), if z == 0 { 1 } else { z }, b);
                }
            }
        }
        for (x, z) in [(2, 2), (w - 3, 2), (2, d - 3), (w - 3, d - 3)] {
            self.fill(x, 3, z, x, h - 1, z, Block::POLISHED_BASALT);
        }
    }
    fn floors(&mut self, w: i32, h: i32, d: i32) {
        for y in (8..h - 3).step_by(8) {
            self.fill(2, y, 2, w - 3, y, d - 3, BRICKS);
            // Permanent stairwell: each flight rises eight blocks with a landing.
            let z0 = if y % 16 == 8 { 2 } else { d - 11 };
            self.fill(3, y, z0, 5, y, z0 + 8, AIR);
            for step in 0..8 {
                self.fill(
                    3,
                    y - 7 + step,
                    z0 + step,
                    5,
                    y - 7 + step,
                    z0 + step,
                    self.stairs(Block::POLISHED_BLACKSTONE_BRICKS, Facing::South),
                );
                self.fill(3, y - 6 + step, z0 + step, 5, y + 3, z0 + step, AIR);
            }
        }
        // Entrances on every side join the neighbouring pieces at deck height.
        for y in (3..h - 3).step_by(8) {
            self.fill(w / 2 - 2, y, 0, w / 2 + 2, y + 4, 1, AIR);
            self.fill(w / 2 - 2, y, d - 2, w / 2 + 2, y + 4, d - 1, AIR);
            self.fill(0, y, d / 2 - 2, 1, y + 4, d / 2 + 2, AIR);
            self.fill(w - 2, y, d / 2 - 2, w - 1, y + 4, d / 2 + 2, AIR);
        }
        self.fill(w / 2 - 2, 3, 0, w / 2 + 2, 7, 1, AIR);
        self.fill(w / 2 - 2, 3, d - 2, w / 2 + 2, 7, d - 1, AIR);
        self.fill(0, 3, d / 2 - 2, 1, 7, d / 2 + 2, AIR);
        self.fill(w - 2, 3, d / 2 - 2, w - 1, 7, d / 2 + 2, AIR);
        self.fill(w / 2 - 2, 2, d / 2 - 2, w / 2 + 2, 2, d / 2 + 2, Block::GOLD_BLOCK);
    }
    fn treasure(&mut self, w: i32, d: i32) {
        self.fill(3, 2, 3, w - 4, 2, d - 4, Block::MAGMA);
        self.fill(3, 3, 3, w - 4, 3, d - 4, Block::LAVA);
        self.fill(12, 3, 12, 20, 4, 20, BRICKS);
        self.fill(14, 4, 16, 18, 5, 18, Block::GOLD_BLOCK);
        self.fill(15, 6, 17, 17, 6, 17, Block::GOLD_BLOCK);
        for y in [8, 16, 24, 32] {
            self.fill(2, y, 2, w - 3, y, 4, BRICKS);
            self.fill(2, y, d - 5, w - 3, y, d - 3, BRICKS);
            self.fill(2, y, 5, 4, y, d - 6, BRICKS);
            self.fill(w - 5, y, 5, w - 3, y, d - 6, BRICKS);
        }
        // Stairs join the high bridge entrance to the central treasure island.
        for step in 0..15 {
            let z = d - 3 - step;
            self.fill(
                14,
                18 - step,
                z,
                18,
                18 - step,
                z,
                self.stairs(Block::POLISHED_BLACKSTONE_BRICKS, Facing::South),
            );
        }
        self.fill(13, 19, d - 2, 19, 23, d - 1, AIR);
        for x in [12, 20] {
            self.fill(x, 7, 16, x, 32, 16, Block::CHAIN);
        }
    }
}

// Java 1.21.1 loot pools. Unsupported item entries retain their weights
// as empty rolls; supported gear keeps random enchantments and durability.
use super::chest::Chest;
use crate::{
    inventory::Stack,
    item::{ArmorMaterial, ArmorPiece, Tier, ToolKind},
};
pub(super) struct Loot {
    pub(super) item: Option<Item>,
    pub(super) weight: u32,
    pub(super) lo: u32,
    pub(super) hi: u32,
    pub(super) enchanted: bool,
    pub(super) damage: Option<(f32, f32)>,
}
const TREASURE_0: &[Loot] = &[
    Loot { item: Some(Item::NETHERITE_INGOT), weight: 15, lo: 1, hi: 1, enchanted: false, damage: None }, // netherite_ingot
    Loot {
        item: Some(Item::from_block(Block::ANCIENT_DEBRIS)),
        weight: 10,
        lo: 1,
        hi: 1,
        enchanted: false,
        damage: None,
    }, // ancient_debris
    Loot { item: Some(Item::NETHERITE_SCRAP), weight: 8, lo: 1, hi: 1, enchanted: false, damage: None }, // netherite_scrap
    Loot {
        item: Some(Item::from_block(Block::ANCIENT_DEBRIS)),
        weight: 4,
        lo: 2,
        hi: 2,
        enchanted: false,
        damage: None,
    }, // ancient_debris
    Loot {
        item: Some(Item::tool(ToolKind::Sword, Tier::Diamond)),
        weight: 6,
        lo: 1,
        hi: 1,
        enchanted: true,
        damage: Some((0.80, 1.00)),
    }, // diamond_sword
    Loot {
        item: Some(Item::armor(ArmorPiece::Chestplate, ArmorMaterial::Diamond)),
        weight: 6,
        lo: 1,
        hi: 1,
        enchanted: true,
        damage: Some((0.80, 1.00)),
    }, // diamond_chestplate
    Loot {
        item: Some(Item::armor(ArmorPiece::Helmet, ArmorMaterial::Diamond)),
        weight: 6,
        lo: 1,
        hi: 1,
        enchanted: true,
        damage: Some((0.80, 1.00)),
    }, // diamond_helmet
    Loot {
        item: Some(Item::armor(ArmorPiece::Leggings, ArmorMaterial::Diamond)),
        weight: 6,
        lo: 1,
        hi: 1,
        enchanted: true,
        damage: Some((0.80, 1.00)),
    }, // diamond_leggings
    Loot {
        item: Some(Item::armor(ArmorPiece::Boots, ArmorMaterial::Diamond)),
        weight: 6,
        lo: 1,
        hi: 1,
        enchanted: true,
        damage: Some((0.80, 1.00)),
    }, // diamond_boots
    Loot {
        item: Some(Item::tool(ToolKind::Sword, Tier::Diamond)),
        weight: 6,
        lo: 1,
        hi: 1,
        enchanted: false,
        damage: None,
    }, // diamond_sword
    Loot {
        item: Some(Item::armor(ArmorPiece::Chestplate, ArmorMaterial::Diamond)),
        weight: 5,
        lo: 1,
        hi: 1,
        enchanted: false,
        damage: None,
    }, // diamond_chestplate
    Loot {
        item: Some(Item::armor(ArmorPiece::Helmet, ArmorMaterial::Diamond)),
        weight: 5,
        lo: 1,
        hi: 1,
        enchanted: false,
        damage: None,
    }, // diamond_helmet
    Loot {
        item: Some(Item::armor(ArmorPiece::Boots, ArmorMaterial::Diamond)),
        weight: 5,
        lo: 1,
        hi: 1,
        enchanted: false,
        damage: None,
    }, // diamond_boots
    Loot {
        item: Some(Item::armor(ArmorPiece::Leggings, ArmorMaterial::Diamond)),
        weight: 5,
        lo: 1,
        hi: 1,
        enchanted: false,
        damage: None,
    }, // diamond_leggings
    Loot { item: Some(Item::DIAMOND), weight: 5, lo: 2, hi: 6, enchanted: false, damage: None },         // diamond
    Loot { item: None, weight: 2, lo: 1, hi: 1, enchanted: false, damage: None }, // enchanted_golden_apple
];
const TREASURE_1: &[Loot] = &[
    Loot { item: None, weight: 1, lo: 12, hi: 25, enchanted: false, damage: None }, // spectral_arrow
    Loot { item: Some(Item::from_block(Block::GOLD_BLOCK)), weight: 1, lo: 2, hi: 5, enchanted: false, damage: None }, // gold_block
    Loot { item: Some(Item::from_block(Block::IRON_BLOCK)), weight: 1, lo: 2, hi: 5, enchanted: false, damage: None }, // iron_block
    Loot { item: Some(Item::GOLD_INGOT), weight: 1, lo: 3, hi: 9, enchanted: false, damage: None }, // gold_ingot
    Loot { item: Some(Item::IRON_INGOT), weight: 1, lo: 3, hi: 9, enchanted: false, damage: None }, // iron_ingot
    Loot {
        item: Some(Item::from_block(Block::CRYING_OBSIDIAN)),
        weight: 1,
        lo: 3,
        hi: 5,
        enchanted: false,
        damage: None,
    }, // crying_obsidian
    Loot { item: Some(Item::NETHER_QUARTZ), weight: 1, lo: 8, hi: 23, enchanted: false, damage: None }, // quartz
    Loot {
        item: Some(Item::from_block(Block::GILDED_BLACKSTONE)),
        weight: 1,
        lo: 5,
        hi: 15,
        enchanted: false,
        damage: None,
    }, // gilded_blackstone
    Loot { item: Some(Item::MAGMA_CREAM), weight: 1, lo: 3, hi: 8, enchanted: false, damage: None }, // magma_cream
];
const BRIDGE_0: &[Loot] = &[
    Loot { item: None, weight: 1, lo: 1, hi: 1, enchanted: false, damage: None }, // lodestone
];
const BRIDGE_1: &[Loot] = &[
    Loot { item: Some(Item::CROSSBOW), weight: 1, lo: 1, hi: 1, enchanted: true, damage: Some((0.10, 0.50)) }, // crossbow
    Loot { item: None, weight: 1, lo: 10, hi: 28, enchanted: false, damage: None }, // spectral_arrow
    Loot {
        item: Some(Item::from_block(Block::GILDED_BLACKSTONE)),
        weight: 1,
        lo: 8,
        hi: 12,
        enchanted: false,
        damage: None,
    }, // gilded_blackstone
    Loot {
        item: Some(Item::from_block(Block::CRYING_OBSIDIAN)),
        weight: 1,
        lo: 3,
        hi: 8,
        enchanted: false,
        damage: None,
    }, // crying_obsidian
    Loot { item: Some(Item::from_block(Block::GOLD_BLOCK)), weight: 1, lo: 1, hi: 1, enchanted: false, damage: None }, // gold_block
    Loot { item: Some(Item::GOLD_INGOT), weight: 1, lo: 4, hi: 9, enchanted: false, damage: None }, // gold_ingot
    Loot { item: Some(Item::IRON_INGOT), weight: 1, lo: 4, hi: 9, enchanted: false, damage: None }, // iron_ingot
    Loot {
        item: Some(Item::tool(ToolKind::Sword, Tier::Gold)),
        weight: 1,
        lo: 1,
        hi: 1,
        enchanted: false,
        damage: None,
    }, // golden_sword
    Loot {
        item: Some(Item::armor(ArmorPiece::Chestplate, ArmorMaterial::Gold)),
        weight: 1,
        lo: 1,
        hi: 1,
        enchanted: true,
        damage: None,
    }, // golden_chestplate
    Loot {
        item: Some(Item::armor(ArmorPiece::Helmet, ArmorMaterial::Gold)),
        weight: 1,
        lo: 1,
        hi: 1,
        enchanted: true,
        damage: None,
    }, // golden_helmet
    Loot {
        item: Some(Item::armor(ArmorPiece::Leggings, ArmorMaterial::Gold)),
        weight: 1,
        lo: 1,
        hi: 1,
        enchanted: true,
        damage: None,
    }, // golden_leggings
    Loot {
        item: Some(Item::armor(ArmorPiece::Boots, ArmorMaterial::Gold)),
        weight: 1,
        lo: 1,
        hi: 1,
        enchanted: true,
        damage: None,
    }, // golden_boots
    Loot { item: Some(Item::tool(ToolKind::Axe, Tier::Gold)), weight: 1, lo: 1, hi: 1, enchanted: true, damage: None }, // golden_axe
];
const BRIDGE_2: &[Loot] = &[
    Loot { item: Some(Item::STRING), weight: 1, lo: 1, hi: 6, enchanted: false, damage: None }, // string
    Loot { item: Some(Item::LEATHER), weight: 1, lo: 1, hi: 3, enchanted: false, damage: None }, // leather
    Loot { item: Some(Item::ARROW), weight: 1, lo: 5, hi: 17, enchanted: false, damage: None }, // arrow
    Loot { item: Some(Item::IRON_NUGGET), weight: 1, lo: 2, hi: 6, enchanted: false, damage: None }, // iron_nugget
    Loot { item: Some(Item::GOLD_NUGGET), weight: 1, lo: 2, hi: 6, enchanted: false, damage: None }, // gold_nugget
];
const HOGLIN_STABLE_0: &[Loot] = &[
    Loot {
        item: Some(Item::tool(ToolKind::Shovel, Tier::Diamond)),
        weight: 15,
        lo: 1,
        hi: 1,
        enchanted: true,
        damage: Some((0.15, 0.80)),
    }, // diamond_shovel
    Loot {
        item: Some(Item::tool(ToolKind::Pickaxe, Tier::Diamond)),
        weight: 12,
        lo: 1,
        hi: 1,
        enchanted: true,
        damage: Some((0.15, 0.95)),
    }, // diamond_pickaxe
    Loot { item: Some(Item::NETHERITE_SCRAP), weight: 8, lo: 1, hi: 1, enchanted: false, damage: None }, // netherite_scrap
    Loot {
        item: Some(Item::from_block(Block::ANCIENT_DEBRIS)),
        weight: 12,
        lo: 1,
        hi: 1,
        enchanted: false,
        damage: None,
    }, // ancient_debris
    Loot {
        item: Some(Item::from_block(Block::ANCIENT_DEBRIS)),
        weight: 5,
        lo: 2,
        hi: 2,
        enchanted: false,
        damage: None,
    }, // ancient_debris
    Loot { item: None, weight: 12, lo: 1, hi: 1, enchanted: false, damage: None },                       // saddle
    Loot { item: Some(Item::from_block(Block::GOLD_BLOCK)), weight: 16, lo: 2, hi: 4, enchanted: false, damage: None }, // gold_block
    Loot { item: None, weight: 10, lo: 8, hi: 17, enchanted: false, damage: None }, // golden_carrot
    Loot { item: None, weight: 10, lo: 1, hi: 1, enchanted: false, damage: None },  // golden_apple
];
const HOGLIN_STABLE_1: &[Loot] = &[
    Loot { item: Some(Item::tool(ToolKind::Axe, Tier::Gold)), weight: 1, lo: 1, hi: 1, enchanted: true, damage: None }, // golden_axe
    Loot {
        item: Some(Item::from_block(Block::CRYING_OBSIDIAN)),
        weight: 1,
        lo: 1,
        hi: 5,
        enchanted: false,
        damage: None,
    }, // crying_obsidian
    Loot { item: Some(Item::from_block(Block::GLOWSTONE)), weight: 1, lo: 3, hi: 6, enchanted: false, damage: None }, // glowstone
    Loot {
        item: Some(Item::from_block(Block::GILDED_BLACKSTONE)),
        weight: 1,
        lo: 2,
        hi: 5,
        enchanted: false,
        damage: None,
    }, // gilded_blackstone
    Loot { item: Some(Item::from_block(Block::SOUL_SAND)), weight: 1, lo: 2, hi: 7, enchanted: false, damage: None }, // soul_sand
    Loot { item: None, weight: 1, lo: 2, hi: 7, enchanted: false, damage: None }, // crimson_nylium
    Loot { item: Some(Item::GOLD_NUGGET), weight: 1, lo: 2, hi: 8, enchanted: false, damage: None }, // gold_nugget
    Loot { item: Some(Item::LEATHER), weight: 1, lo: 1, hi: 3, enchanted: false, damage: None }, // leather
    Loot { item: Some(Item::ARROW), weight: 1, lo: 5, hi: 17, enchanted: false, damage: None }, // arrow
    Loot { item: Some(Item::STRING), weight: 1, lo: 3, hi: 8, enchanted: false, damage: None }, // string
    Loot { item: Some(Item::RAW_PORKCHOP), weight: 1, lo: 2, hi: 5, enchanted: false, damage: None }, // porkchop
    Loot { item: Some(Item::COOKED_PORKCHOP), weight: 1, lo: 2, hi: 5, enchanted: false, damage: None }, // cooked_porkchop
    Loot { item: None, weight: 1, lo: 2, hi: 7, enchanted: false, damage: None }, // crimson_fungus
    Loot { item: None, weight: 1, lo: 2, hi: 7, enchanted: false, damage: None }, // crimson_roots
];
const OTHER_0: &[Loot] = &[
    Loot {
        item: Some(Item::tool(ToolKind::Pickaxe, Tier::Diamond)),
        weight: 6,
        lo: 1,
        hi: 1,
        enchanted: true,
        damage: None,
    }, // diamond_pickaxe
    Loot {
        item: Some(Item::tool(ToolKind::Shovel, Tier::Diamond)),
        weight: 6,
        lo: 1,
        hi: 1,
        enchanted: false,
        damage: None,
    }, // diamond_shovel
    Loot { item: Some(Item::CROSSBOW), weight: 6, lo: 1, hi: 1, enchanted: true, damage: Some((0.10, 0.90)) }, // crossbow
    Loot {
        item: Some(Item::from_block(Block::ANCIENT_DEBRIS)),
        weight: 12,
        lo: 1,
        hi: 1,
        enchanted: false,
        damage: None,
    }, // ancient_debris
    Loot { item: Some(Item::NETHERITE_SCRAP), weight: 4, lo: 1, hi: 1, enchanted: false, damage: None }, // netherite_scrap
    Loot { item: None, weight: 10, lo: 10, hi: 22, enchanted: false, damage: None }, // spectral_arrow
    Loot { item: None, weight: 9, lo: 1, hi: 1, enchanted: false, damage: None },    // piglin_banner_pattern
    Loot { item: None, weight: 5, lo: 1, hi: 1, enchanted: false, damage: None },    // music_disc_pigstep
    Loot { item: None, weight: 12, lo: 6, hi: 17, enchanted: false, damage: None },  // golden_carrot
    Loot { item: None, weight: 9, lo: 1, hi: 1, enchanted: false, damage: None },    // golden_apple
    Loot { item: Some(Item::BOOK), weight: 10, lo: 1, hi: 1, enchanted: true, damage: None }, // book
];
const OTHER_1: &[Loot] = &[
    Loot {
        item: Some(Item::tool(ToolKind::Sword, Tier::Iron)),
        weight: 2,
        lo: 1,
        hi: 1,
        enchanted: true,
        damage: Some((0.10, 0.90)),
    }, // iron_sword
    Loot { item: Some(Item::from_block(Block::IRON_BLOCK)), weight: 2, lo: 1, hi: 1, enchanted: false, damage: None }, // iron_block
    Loot {
        item: Some(Item::armor(ArmorPiece::Boots, ArmorMaterial::Gold)),
        weight: 1,
        lo: 1,
        hi: 1,
        enchanted: true,
        damage: None,
    }, // golden_boots
    Loot { item: Some(Item::tool(ToolKind::Axe, Tier::Gold)), weight: 1, lo: 1, hi: 1, enchanted: true, damage: None }, // golden_axe
    Loot { item: Some(Item::from_block(Block::GOLD_BLOCK)), weight: 2, lo: 1, hi: 1, enchanted: false, damage: None }, // gold_block
    Loot { item: Some(Item::CROSSBOW), weight: 1, lo: 1, hi: 1, enchanted: false, damage: None }, // crossbow
    Loot { item: Some(Item::GOLD_INGOT), weight: 2, lo: 1, hi: 6, enchanted: false, damage: None }, // gold_ingot
    Loot { item: Some(Item::IRON_INGOT), weight: 2, lo: 1, hi: 6, enchanted: false, damage: None }, // iron_ingot
    Loot {
        item: Some(Item::tool(ToolKind::Sword, Tier::Gold)),
        weight: 1,
        lo: 1,
        hi: 1,
        enchanted: false,
        damage: None,
    }, // golden_sword
    Loot {
        item: Some(Item::armor(ArmorPiece::Chestplate, ArmorMaterial::Gold)),
        weight: 1,
        lo: 1,
        hi: 1,
        enchanted: false,
        damage: None,
    }, // golden_chestplate
    Loot {
        item: Some(Item::armor(ArmorPiece::Helmet, ArmorMaterial::Gold)),
        weight: 1,
        lo: 1,
        hi: 1,
        enchanted: false,
        damage: None,
    }, // golden_helmet
    Loot {
        item: Some(Item::armor(ArmorPiece::Leggings, ArmorMaterial::Gold)),
        weight: 1,
        lo: 1,
        hi: 1,
        enchanted: false,
        damage: None,
    }, // golden_leggings
    Loot {
        item: Some(Item::armor(ArmorPiece::Boots, ArmorMaterial::Gold)),
        weight: 1,
        lo: 1,
        hi: 1,
        enchanted: false,
        damage: None,
    }, // golden_boots
    Loot {
        item: Some(Item::from_block(Block::CRYING_OBSIDIAN)),
        weight: 2,
        lo: 1,
        hi: 5,
        enchanted: false,
        damage: None,
    }, // crying_obsidian
];
const OTHER_2: &[Loot] = &[
    Loot {
        item: Some(Item::from_block(Block::GILDED_BLACKSTONE)),
        weight: 2,
        lo: 1,
        hi: 5,
        enchanted: false,
        damage: None,
    }, // gilded_blackstone
    Loot { item: Some(Item::from_block(Block::CHAIN)), weight: 1, lo: 2, hi: 10, enchanted: false, damage: None }, // chain
    Loot { item: Some(Item::MAGMA_CREAM), weight: 2, lo: 2, hi: 6, enchanted: false, damage: None }, // magma_cream
    Loot { item: None, weight: 1, lo: 3, hi: 6, enchanted: false, damage: None },                    // bone_block
    Loot { item: Some(Item::IRON_NUGGET), weight: 1, lo: 2, hi: 8, enchanted: false, damage: None }, // iron_nugget
    Loot { item: Some(Item::from_block(Block::OBSIDIAN)), weight: 1, lo: 4, hi: 6, enchanted: false, damage: None }, // obsidian
    Loot { item: Some(Item::GOLD_NUGGET), weight: 1, lo: 2, hi: 8, enchanted: false, damage: None }, // gold_nugget
    Loot { item: Some(Item::STRING), weight: 1, lo: 4, hi: 6, enchanted: false, damage: None },      // string
    Loot { item: Some(Item::ARROW), weight: 2, lo: 5, hi: 17, enchanted: false, damage: None },      // arrow
    Loot { item: Some(Item::COOKED_PORKCHOP), weight: 1, lo: 1, hi: 1, enchanted: false, damage: None }, // cooked_porkchop
];

pub(super) fn put(chest: &mut Chest, stack: Stack, rng: &mut Rng) {
    let n = chest.slots.iter().filter(|s| s.is_none()).count();
    if n == 0 {
        return;
    }
    let pick = rng.below(n as u32) as usize;
    let slot = chest.slots.iter_mut().filter(|s| s.is_none()).nth(pick).unwrap();
    *slot = Some(stack);
}
pub(super) fn pool(chest: &mut Chest, rng: &mut Rng, table: &[Loot], rolls: (u32, u32)) {
    let weight: u32 = table.iter().map(|e| e.weight).sum();
    for _ in 0..rng.range(rolls.0, rolls.1) {
        let mut roll = rng.below(weight);
        let e = table
            .iter()
            .find(|e| {
                let hit = roll < e.weight;
                roll = roll.saturating_sub(e.weight);
                hit
            })
            .unwrap();
        let count = rng.range(e.lo, e.hi) as u8;
        let Some(item) = e.item else {
            continue;
        };
        let mut stack = Stack::new(item, count);
        if let Some((lo, hi)) = e.damage
            && let Some(max) = item.durability()
        {
            // Java set_damage measures remaining durability, then floors damage.
            stack.damage = ((1.0 - (lo + rng.unit() as f32 * (hi - lo))) * max as f32).floor() as u16;
        }
        if e.enchanted {
            use crate::enchant::Enchantment;
            let choices: Vec<_> = Enchantment::ALL.into_iter().filter(|e| e.fits(item)).collect();
            if let Some(e) = choices.get(rng.below(choices.len() as u32) as usize) {
                stack.enchants = stack.enchants.with(*e, rng.range(1, e.def().max_level as u32) as u8);
                if item == Item::BOOK {
                    stack.item = Item::ENCHANTED_BOOK;
                }
            }
        }
        put(chest, stack, rng);
    }
}
pub fn loot(seed: u64, kind: ChestKind) -> Chest {
    let mut rng = Rng(seed ^ 0xBA57_1007);
    let mut chest = Chest::default();
    let upgrade = netherite_upgrade(kind);
    if rng.below(upgrade.denominator as u32) < upgrade.numerator as u32 {
        put(&mut chest, Stack::new(upgrade.item, upgrade.count), &mut rng);
    }
    match kind {
        ChestKind::Treasure => {
            pool(&mut chest, &mut rng, TREASURE_0, (3, 3));
            pool(&mut chest, &mut rng, TREASURE_1, (3, 4));
        }
        ChestKind::Bridge => {
            pool(&mut chest, &mut rng, BRIDGE_0, (1, 1));
            pool(&mut chest, &mut rng, BRIDGE_1, (1, 2));
            pool(&mut chest, &mut rng, BRIDGE_2, (2, 4));
        }
        ChestKind::HoglinStable => {
            pool(&mut chest, &mut rng, HOGLIN_STABLE_0, (1, 1));
            pool(&mut chest, &mut rng, HOGLIN_STABLE_1, (3, 4));
        }
        ChestKind::Other => {
            pool(&mut chest, &mut rng, OTHER_0, (1, 1));
            pool(&mut chest, &mut rng, OTHER_1, (2, 2));
            pool(&mut chest, &mut rng, OTHER_2, (3, 4));
        }
    }
    chest
}

#[cfg(test)]
mod generation_tests {
    use super::*;
    use crate::world::{
        World,
        chunk::{chunk_of, index},
        terrain::{Dimension, Generator},
    };
    #[test]
    fn fortresses_and_bastions_never_claim_the_same_region() {
        let f = super::super::fortress::Fortresses::new(7);
        for x in -8..8 {
            for z in -8..8 {
                let r = IVec2::new(x, z);
                assert!(!(f.get(r).is_some() && f.bastions.get(r).is_some()));
            }
        }
    }
    #[test]
    fn bastions_skip_basalt_deltas_like_java() {
        use crate::world::nether_biome::{NetherBiome, NetherBiomeSource};
        let (seed, biomes) = (99, NetherBiomeSource::new(99));
        let bastions = Bastions::new(seed);
        let (mut kept, mut rejected) = (0, 0);
        for x in -40..40 {
            for z in -40..40 {
                let r = IVec2::new(x, z);
                let (start, kind) = nether_complexes::placement(seed, r);
                if kind != Complex::Bastion {
                    continue;
                }
                let deltas = biomes.biome(start.x + 6, start.y + 6) == NetherBiome::BasaltDeltas;
                assert_eq!(bastions.get(r).is_none(), deltas, "region {r}");
                if deltas { rejected += 1 } else { kept += 1 }
            }
        }
        assert!(kept > 100 && rejected > 10, "kept {kept}, rejected {rejected}");
    }
    #[test]
    fn all_four_variants_rotate_and_paint_columns_without_seams() {
        for kind in [Kind::Housing, Kind::Stables, Kind::Treasure, Kind::Bridge] {
            for facing in Facing::ALL {
                let b = Bastion::generate(42, IVec2::new(29, 29), kind, facing);
                assert!(b.pieces.len() >= 3);
                let mut gold = 0;
                let mut chests = 0;
                let mut lava = 0;
                // Compare the same layout painted as 32-cubes and single columns.
                let lo = chunk_of(b.bounds.min);
                let hi = chunk_of(b.bounds.max);
                for cx in lo.x..=hi.x {
                    for cz in lo.z..=hi.z {
                        for cy in lo.y..=hi.y {
                            let base = IVec3::new(cx, cy, cz) * CHUNK_SIZE_I;
                            let top = base + IVec3::splat(CHUNK_SIZE_I - 1);
                            let chunk = Bounds { min: base, max: top };
                            let mut blocks = Box::new([Block::NETHERRACK; CHUNK_VOLUME]);
                            for piece in &b.pieces {
                                if piece.bounds.intersects(&chunk) {
                                    Paint {
                                        blocks: &mut *blocks,
                                        base,
                                        top,
                                        piece,
                                        open: &|_, _, _, _| false,
                                        pillar: BLACK,
                                    }
                                    .bastion_piece();
                                }
                            }
                            gold += blocks.iter().filter(|&&v| v == Block::GOLD_BLOCK).count();
                            chests += blocks.iter().filter(|&&v| v.base() == Block::CHEST).count();
                            lava += blocks.iter().filter(|&&v| v.is_lava()).count();
                            for (x, z) in [(0, 0), (31, 31), (0, 31), (31, 0), (15, 15)] {
                                let column_base = base + IVec3::new(x, 0, z);
                                let column_top = column_base + IVec3::Y * (CHUNK_SIZE_I - 1);
                                let mut column = [Block::NETHERRACK; CHUNK_SIZE];
                                for piece in &b.pieces {
                                    if piece.bounds.intersects(&Bounds { min: column_base, max: column_top }) {
                                        Paint {
                                            blocks: &mut column,
                                            base: column_base,
                                            top: column_top,
                                            piece,
                                            open: &|_, _, _, _| false,
                                            pillar: BLACK,
                                        }
                                        .bastion_piece();
                                    }
                                }
                                for y in 0..CHUNK_SIZE {
                                    assert_eq!(
                                        column[y],
                                        blocks[index(x as usize, y, z as usize)],
                                        "{kind:?} {facing:?} at {column_base:?} y {y}"
                                    );
                                }
                            }
                        }
                    }
                }
                assert!(gold >= 20 && chests >= 2, "{kind:?}: gold {gold}, chests {chests}");
                if matches!(kind, Kind::Treasure | Kind::Stables) {
                    assert!(lava > 0);
                }
                if kind == Kind::Treasure {
                    assert_eq!(
                        b.pieces.iter().flat_map(|p| &p.chests).filter(|(_, k)| *k == ChestKind::Treasure).count(),
                        2
                    );
                }
            }
        }
    }
    #[test]
    fn template_pool_guarantees_treasure_and_is_ten_percent_elsewhere() {
        for kind in [ChestKind::Treasure, ChestKind::Other, ChestKind::Bridge, ChestKind::HoglinStable] {
            let mut templates = 0;
            for seed in 0..10000 {
                let c = loot(seed, kind);
                assert!(c.slots.iter().flatten().all(|s| s.item.is_valid() && s.count > 0));
                templates += c
                    .slots
                    .iter()
                    .flatten()
                    .filter(|s| s.item == Item::NETHERITE_UPGRADE)
                    .map(|s| s.count as usize)
                    .sum::<usize>();
            }
            if kind == ChestKind::Treasure {
                assert_eq!(templates, 10000);
            } else {
                assert!((850..1150).contains(&templates), "{kind:?}: {templates}");
            }
        }
    }
    #[test]
    fn generated_bastion_chests_register_and_saved_loot_wins() {
        let seed = (0..100)
            .find(|&seed| Bastions::new(seed).get(IVec2::ZERO).is_some_and(|b| b.kind == Kind::Treasure))
            .unwrap();
        let bastions = Bastions::new(seed);
        let b = bastions.get(IVec2::ZERO).unwrap();
        let piece = &b.pieces[0];
        let local = piece.chests[0].0;
        let p = piece.world(local.x, local.y, local.z);
        let cpos = chunk_of(p);
        let generator = Arc::new(Generator::for_dimension(seed, Dimension::Nether));
        let data = generator.generate(cpos);
        let mut world = World::new_headless(generator, Default::default(), 2);
        world.register_structure_features(cpos, &data);
        let original = world.chests.get(&p).unwrap();
        assert!(original.slots.iter().flatten().any(|s| s.item == Item::NETHERITE_UPGRADE));
        world.chests.insert(p, Chest::default());
        world.register_structure_features(cpos, &data);
        assert!(world.chests[&p].slots.iter().all(Option::is_none));
    }
}
