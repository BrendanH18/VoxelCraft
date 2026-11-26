//! v0.16 Track A's items (BLD-01b, MOB-04b): chorus fruit, popped chorus
//! fruit and the shulker shell, items 1660–1662 of Track A's reservation
//! 1660–1674 (1663–1674 stay unregistered).
//!
//! Java 26.3 references: `Items.CHORUS_FRUIT` (`Foods.CHORUS_FRUIT`:
//! nutrition 4, saturation modifier 0.3, `alwaysEdible`;
//! `Consumables.CHORUS_FRUIT`: the default food consumable with
//! `TeleportRandomlyConsumeEffect`; `useCooldown(1.0F)`),
//! `TeleportRandomlyConsumeEffect.apply` (16 tries over a 16-block cube)
//! and `LivingEntity.randomTeleport`; `recipe/popped_chorus_fruit.json`
//! (smelting, 200 ticks, 0.1 experience); `Items.SHULKER_SHELL`.
use glam::DVec3;

use crate::{
    item::{Item, ItemInfo, ItemKind, Sprite},
    physics::Shape,
    world::{World, block::Block},
};

pub const CHORUS_FRUIT: Item = Item(1660);
pub const POPPED_CHORUS_FRUIT: Item = Item(1661);
pub const SHULKER_SHELL: Item = Item(1662);
pub const FIRST: u16 = 1660;
pub const END: u16 = 1663;
const _: () = assert!(FIRST >= 1660 && END <= 1675);

/// `Foods.CHORUS_FRUIT`: 4 hunger, saturation 4 × 0.3 × 2.
pub const NUTRITION: u8 = 4;
pub const SATURATION: f32 = crate::item::saturation_by_modifier(NUTRITION, 0.3);
/// `useCooldown(1.0F)`: seconds before another can be eaten.
pub const COOLDOWN: f32 = 1.0;
/// `TeleportRandomlyConsumeEffect.DEFAULT_DIAMETER` and its tries.
pub const DIAMETER: f64 = 16.0;
pub const TRIES: u32 = 16;

pub fn items() -> impl Iterator<Item = Item> {
    (FIRST..END).map(Item)
}

pub fn is_item(item: Item) -> bool {
    (FIRST..END).contains(&item.0)
}

pub(crate) fn item_info(item: Item) -> Option<ItemInfo> {
    Some(match item {
        CHORUS_FRUIT => ItemInfo {
            name: "chorus fruit",
            kind: ItemKind::Food { hunger: NUTRITION, saturation: SATURATION },
            max_stack: 64,
            sprite: Sprite::Lump([150, 102, 150]),
        },
        POPPED_CHORUS_FRUIT => ItemInfo {
            name: "popped chorus fruit",
            kind: ItemKind::Material,
            max_stack: 64,
            sprite: Sprite::Lump([196, 160, 196]),
        },
        SHULKER_SHELL => ItemInfo {
            name: "shulker shell",
            kind: ItemKind::Material,
            max_stack: 64,
            sprite: Sprite::Lump([150, 106, 150]),
        },
        _ => return None,
    })
}

/// The item's inventory icon layer.
pub fn icon_layer(item: Item) -> Option<u16> {
    is_item(item).then(|| crate::world::end_blocks::ICON_TEX + item.0 - FIRST)
}

/// Original icons: a knobbly violet fruit, the same puffed pale, and a
/// purple shell half with a lighter rim.
pub fn icon_pixel(i: u16, x: usize, y: usize) -> [u8; 4] {
    let (dx, dy) = (x as f32 - 7.5, y as f32 - 8.0);
    match i {
        0 | 1 => {
            // Three lobes around a stalk.
            let lobe = |cx: f32, cy: f32, r: f32| (dx - cx).powi(2) + (dy - cy).powi(2) <= r * r;
            let body = lobe(0.0, 1.5, 4.5) || lobe(-3.0, -1.5, 3.0) || lobe(3.0, -1.5, 3.0);
            let stalk = (7..=8).contains(&x) && (1..=3).contains(&y);
            if stalk {
                return [96, 64, 70, 255];
            }
            if !body {
                return [0, 0, 0, 0];
            }
            let speck = (x * 7 + y * 3).is_multiple_of(5);
            let lit = dx + dy < -2.0;
            let base: [i32; 3] = if i == 0 { [136, 90, 140] } else { [214, 178, 214] };
            let d = if speck {
                -26
            } else if lit {
                22
            } else {
                0
            };
            [
                (base[0] + d).clamp(0, 255) as u8,
                (base[1] + d).clamp(0, 255) as u8,
                (base[2] + d).clamp(0, 255) as u8,
                255,
            ]
        }
        _ => {
            // A dome seen from the side.
            let r = (dx * dx / 49.0 + (dy - 3.0).powi(2) / 36.0).sqrt();
            if r > 1.0 || dy > 4.0 {
                return [0, 0, 0, 0];
            }
            let rim = r > 0.82 || dy > 2.5;
            let ridge = (x + y).is_multiple_of(4);
            let c: [u8; 3] = if rim {
                [196, 156, 196]
            } else if ridge {
                [122, 82, 128]
            } else {
                [150, 106, 154]
            };
            [c[0], c[1], c[2], 255]
        }
    }
}

/// `LivingEntity.randomTeleport` aimed at `(x, y, z)`: down from there to
/// the first block that blocks motion (`#entities_can_teleport_to`), then
/// the body must fit there, out of any liquid (Java's
/// `#consumable_does_not_teleport_to` is empty). Returns the new feet.
pub fn random_teleport(world: &World, target: DVec3, shape: Shape) -> Option<DVec3> {
    let mut pos = target.floor().as_ivec3();
    let mut y = target.y;
    world.get_block(pos)?;
    while pos.y > world.min_y() {
        pos.y -= 1;
        let state = world.get_block(pos).unwrap_or(Block::AIR);
        if state.is_solid() {
            let to = DVec3::new(target.x, y, target.z);
            let fits = !crate::physics::overlaps_solid(world, to, shape)
                && !crate::physics::touches_block(world, to, shape, |b| b.is_fluid() || b.holds_water());
            return fits.then_some(to);
        }
        y -= 1.0;
    }
    None
}

/// `TeleportRandomlyConsumeEffect.apply`: up to 16 tries at a random spot
/// within 8 blocks on each axis (the height clamped to the dimension's
/// logical height), the first that [`random_teleport`] accepts. `next` is
/// `random.nextDouble()`.
pub fn teleport_target(world: &World, from: DVec3, shape: Shape, next: &mut impl FnMut() -> f64) -> Option<DVec3> {
    let (lo, hi) = (world.min_y() as f64, (world.max_y() - 1) as f64);
    for _ in 0..TRIES {
        let x = from.x + (next() - 0.5) * DIAMETER;
        let y = (from.y + (next() - 0.5) * DIAMETER).clamp(lo, hi);
        let z = from.z + (next() - 0.5) * DIAMETER;
        if let Some(to) = random_teleport(world, DVec3::new(x, y, z), shape) {
            return Some(to);
        }
    }
    None
}
