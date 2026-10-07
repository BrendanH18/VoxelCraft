//! HumanoidArmorLayer: one procedural texture per material, shared by players,
//! zombies and skeletons. Outer pieces (helmet, chest, boots) inflate by 1
//! model pixel; leggings inflate by 0.5. Chainmail is a monster material;
//! players wear the craftable materials, including Netherite once that item
//! exists.

use crate::inventory::Stack;
use crate::item::{ArmorMaterial, ArmorPiece};

/// Materials with a generated 64×64 armor sheet. Indexed like Java's
/// equipment table: leather, gold, chain, iron, diamond, plus netherite.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum ArmorKind {
    Leather = 0,
    Chain = 1,
    Iron = 2,
    Gold = 3,
    Diamond = 4,
    Netherite = 5,
}

impl ArmorKind {
    pub const ALL: [ArmorKind; 6] = [
        ArmorKind::Leather,
        ArmorKind::Chain,
        ArmorKind::Iron,
        ArmorKind::Gold,
        ArmorKind::Diamond,
        ArmorKind::Netherite,
    ];

    pub fn from_name(name: &str) -> Option<Self> {
        Some(match name {
            "leather" => Self::Leather,
            "chain" | "chainmail" => Self::Chain,
            "iron" => Self::Iron,
            "gold" | "golden" => Self::Gold,
            "diamond" => Self::Diamond,
            "netherite" => Self::Netherite,
            _ => return None,
        })
    }

    pub fn of(material: ArmorMaterial) -> Self {
        match material {
            ArmorMaterial::Leather => Self::Leather,
            ArmorMaterial::Iron => Self::Iron,
            ArmorMaterial::Gold => Self::Gold,
            ArmorMaterial::Diamond => Self::Diamond,
            ArmorMaterial::Netherite => Self::Netherite,
        }
    }
}

/// One worn piece. `glint` is Java's enchantment sheen.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Worn {
    pub kind: ArmorKind,
    pub glint: bool,
}

/// Armor forced onto a spawned zombie or skeleton (`--spawn`).
#[derive(Clone, Copy, Debug)]
pub struct Equipped {
    pub kind: ArmorKind,
    pub glint: bool,
}

/// Java's undyed leather colour (`#A06540`).
pub const LEATHER_DYE: [u8; 3] = [160, 101, 64];

/// Skin occupies array layer 0. Two sheets per material, then leather overlays.
pub const ARRAY_LAYERS: u32 = 15;

pub fn layer(kind: ArmorKind, inner: bool) -> u16 {
    1 + kind as u16 * 2 + u16::from(inner)
}

pub fn leather_overlay(inner: bool) -> u16 {
    13 + u16::from(inner)
}

pub fn tint(worn: &Worn, overlay: bool) -> [u8; 4] {
    let rgb = if worn.kind == ArmorKind::Leather && !overlay { LEATHER_DYE } else { [255, 255, 255] };
    [rgb[0], rgb[1], rgb[2], if worn.glint && !overlay { 255 } else { 0 }]
}

pub fn from_stacks(armor: &[Option<Stack>; 4]) -> [Option<Worn>; 4] {
    std::array::from_fn(|i| {
        let stack = armor[i]?;
        let (piece, material) = stack.item.as_armor()?;
        (piece as usize == i).then_some(Worn { kind: ArmorKind::of(material), glint: !stack.enchants.is_empty() })
    })
}

pub fn worn_pieces(armor: [Option<ArmorKind>; 4], glint: u8) -> [Option<Worn>; 4] {
    std::array::from_fn(|i| armor[i].map(|kind| Worn { kind, glint: glint & (1 << i) != 0 }))
}

/// Java Mob.populateDefaultEquipmentSlots at regional difficulty 1, without
/// a held weapon. Feet are equipped first; each later piece is skipped 25%
/// of the time. The tier starts at leather or gold and climbs toward diamond.
pub fn roll_monster_armor(rng: &mut super::Rng) -> ([Option<ArmorKind>; 4], u8) {
    const TIERS: [ArmorKind; 5] =
        [ArmorKind::Leather, ArmorKind::Gold, ArmorKind::Chain, ArmorKind::Iron, ArmorKind::Diamond];
    let mut armor = [None; 4];
    let mut glint = 0u8;
    if !rng.chance(0.15) {
        return (armor, glint);
    }
    let mut tier = usize::from(!rng.chance(0.5));
    for _ in 0..3 {
        if rng.chance(0.095) {
            tier += 1;
        }
    }
    let kind = TIERS[tier.min(TIERS.len() - 1)];
    let mut first = true;
    for slot in [ArmorPiece::Boots, ArmorPiece::Leggings, ArmorPiece::Chestplate, ArmorPiece::Helmet] {
        if !first && rng.chance(0.25) {
            break;
        }
        first = false;
        let i = slot as usize;
        armor[i] = Some(kind);
        if rng.chance(0.25) {
            glint |= 1 << i;
        }
    }
    (armor, glint)
}

/// Armor sheets packed after the skin layer: 14 × 64 × 64 RGBA.
pub fn layer_pixels() -> Vec<u8> {
    let mut pixels = vec![0u8; 14 * 64 * 64 * 4];
    for kind in ArmorKind::ALL {
        paint(&mut pixels, layer(kind, false), kind, false, false);
        paint(&mut pixels, layer(kind, true), kind, true, false);
    }
    paint(&mut pixels, leather_overlay(false), ArmorKind::Leather, false, true);
    paint(&mut pixels, leather_overlay(true), ArmorKind::Leather, true, true);
    pixels
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Part {
    Head,
    Body,
    Arm,
    Leg,
}

fn paint(pixels: &mut [u8], layer: u16, kind: ArmorKind, inner: bool, overlay: bool) {
    let parts = [
        ([0usize, 0], [8usize, 8, 8], Part::Head),
        ([16, 16], [8, 12, 4], Part::Body),
        ([40, 16], [4, 12, 4], Part::Arm),
        ([32, 48], [4, 12, 4], Part::Arm),
        ([0, 16], [4, 12, 4], Part::Leg),
        ([16, 48], [4, 12, 4], Part::Leg),
    ];
    let base = (layer as usize - 1) * 64 * 64 * 4;
    for (uv, size, part) in parts {
        let [w, h, d] = size;
        // Same net as the player skin: up/down strip, then the four sides.
        let faces = [
            [uv[0] + d, uv[1], w, d],
            [uv[0] + d + w, uv[1], w, d],
            [uv[0], uv[1] + d, d, h],
            [uv[0] + d, uv[1] + d, w, h],
            [uv[0] + d + w, uv[1] + d, d, h],
            [uv[0] + 2 * d + w, uv[1] + d, w, h],
        ];
        for (face, [u, v, width, height]) in faces.into_iter().enumerate() {
            for y in 0..height {
                for x in 0..width {
                    if !covered(part, inner, face, y, h) {
                        continue;
                    }
                    let Some(color) = texel(kind, part, overlay, face, [x, y], [width, height]) else { continue };
                    let i = base + ((v + y) * 64 + u + x) * 4;
                    pixels[i..i + 3].copy_from_slice(&color);
                    pixels[i + 3] = 255;
                }
            }
        }
    }
}

/// Outer sheet keeps the helmet, sleeves, chest and boots. The inner sheet
/// keeps the leggings (hips and upper legs). Caps follow the same split.
fn covered(part: Part, inner: bool, face: usize, y: usize, limb_h: usize) -> bool {
    let side = face >= 2;
    match part {
        Part::Head | Part::Arm => !inner,
        Part::Body => {
            if !side {
                return if inner { face == 1 } else { face == 0 };
            }
            if inner { y + 5 >= limb_h } else { y + 4 < limb_h }
        }
        Part::Leg => {
            if !side {
                return if inner { face == 0 } else { face == 1 };
            }
            let boot = y + 4 >= limb_h;
            if inner { !boot } else { boot }
        }
    }
}

fn texel(kind: ArmorKind, part: Part, overlay: bool, face: usize, at: [usize; 2], size: [usize; 2]) -> Option<[u8; 3]> {
    let [x, y] = at;
    let [width, height] = size;
    if overlay {
        let seam = face >= 2 && (x == 0 || x + 1 == width || x == width / 2);
        return seam.then_some([214, 198, 176]);
    }
    // The helmet's front face is open so the skin shows through, like the
    // visor cutout on Java's layer_1 helmet.
    if part == Part::Head && face == 3 && y >= height / 3 && x > 0 && x + 1 < width {
        return None;
    }
    let edge = x == 0 || x + 1 == width || y == 0 || y + 1 == height;
    Some(match kind {
        ArmorKind::Leather => {
            if edge {
                [176, 176, 176]
            } else {
                [226, 226, 226]
            }
        }
        ArmorKind::Chain => {
            if edge {
                [58, 60, 68]
            } else if (x / 2 + y / 2).is_multiple_of(2) {
                [158, 160, 168]
            } else {
                [92, 94, 102]
            }
        }
        ArmorKind::Iron => plate([206, 206, 210], [148, 148, 156], [236, 236, 240], edge, face),
        ArmorKind::Gold => plate([236, 196, 60], [168, 124, 28], [252, 228, 140], edge, face),
        ArmorKind::Diamond => {
            let facet = (x / 2 + y / 2).is_multiple_of(2);
            plate(if facet { [56, 206, 198] } else { [132, 232, 224] }, [24, 120, 118], [168, 244, 236], edge, face)
        }
        ArmorKind::Netherite => plate([48, 40, 44], [24, 18, 22], [96, 52, 56], edge, face),
    })
}

fn plate(body: [u8; 3], edge_c: [u8; 3], hi: [u8; 3], edge: bool, face: usize) -> [u8; 3] {
    if edge {
        edge_c
    } else if face == 0 {
        hi
    } else {
        body
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::enchant::Enchantment;
    use crate::item::{ArmorPiece, Item};

    fn rgba(layer: u16, x: usize, y: usize) -> [u8; 4] {
        let pixels = layer_pixels();
        let i = ((layer as usize - 1) * 64 * 64 + y * 64 + x) * 4;
        [pixels[i], pixels[i + 1], pixels[i + 2], pixels[i + 3]]
    }

    #[test]
    fn layers_are_unique_and_fill_the_atlas() {
        let mut seen = [false; ARRAY_LAYERS as usize];
        seen[0] = true;
        for kind in ArmorKind::ALL {
            for inner in [false, true] {
                let i = layer(kind, inner) as usize;
                assert!(!seen[i], "{kind:?} {inner}");
                seen[i] = true;
            }
        }
        for inner in [false, true] {
            let i = leather_overlay(inner) as usize;
            assert!(!seen[i]);
            seen[i] = true;
        }
        assert!(seen.iter().all(|s| *s));
        assert_eq!(layer_pixels().len(), 14 * 64 * 64 * 4);
    }

    #[test]
    fn boots_and_leggings_split_the_leg_texture() {
        // Right-leg front face, top and bottom rows (see `paint`).
        let iron_outer = layer(ArmorKind::Iron, false);
        let iron_inner = layer(ArmorKind::Iron, true);
        assert_eq!(rgba(iron_outer, 4, 20)[3], 0, "thigh is not a boot");
        assert_eq!(rgba(iron_outer, 4, 31)[3], 255);
        assert_eq!(rgba(iron_inner, 4, 20)[3], 255);
        assert_eq!(rgba(iron_inner, 4, 31)[3], 0, "leggings stop above the boot");
        assert_ne!(rgba(layer(ArmorKind::Chain, false), 4, 31)[..3], rgba(iron_outer, 4, 31)[..3]);
        assert_eq!(rgba(iron_inner, 8, 8)[3], 0, "helmet lives on the outer sheet");
        assert!(rgba(leather_overlay(false), 44, 22)[3] == 255 || rgba(leather_overlay(false), 40, 22)[3] == 255);
    }

    #[test]
    fn worn_stacks_keep_slot_material_and_glint() {
        let mut helmet = Stack::new(Item::armor(ArmorPiece::Helmet, ArmorMaterial::Diamond), 1);
        helmet.enchants = helmet.enchants.with(Enchantment::Protection, 1);
        let boots = Stack::new(Item::armor(ArmorPiece::Boots, ArmorMaterial::Leather), 1);
        let plate = Stack::new(Item::armor(ArmorPiece::Chestplate, ArmorMaterial::Netherite), 1);
        let worn = from_stacks(&[Some(helmet), Some(plate), None, Some(boots)]);
        assert_eq!(worn[0], Some(Worn { kind: ArmorKind::Diamond, glint: true }));
        assert_eq!(worn[1], Some(Worn { kind: ArmorKind::Netherite, glint: false }));
        assert_eq!(worn[3], Some(Worn { kind: ArmorKind::Leather, glint: false }));
        assert!(worn[2].is_none());
    }

    #[test]
    fn monster_rolls_match_java_slot_order() {
        let mut any = 0;
        let mut counts = [0; 6];
        for seed in 0..2000 {
            let mut rng = super::super::Rng::new(seed);
            let (armor, glint) = roll_monster_armor(&mut rng);
            let equipped: Vec<_> = armor.iter().enumerate().filter_map(|(i, k)| k.map(|k| (i, k))).collect();
            if equipped.is_empty() {
                assert_eq!(glint, 0);
                continue;
            }
            any += 1;
            assert!(armor[ArmorPiece::Boots as usize].is_some(), "feet are first");
            let kind = equipped[0].1;
            assert!(equipped.iter().all(|(_, k)| *k == kind));
            for (i, piece) in armor.iter().enumerate() {
                if piece.is_none() {
                    assert_eq!(glint & (1 << i), 0);
                }
            }
            counts[kind as usize] += 1;
        }
        assert!((200..450).contains(&any), "{any}");
        assert!(counts[ArmorKind::Chain as usize] > 0);
        assert_eq!(counts[ArmorKind::Netherite as usize], 0);
    }
}
