//! Java 26.3 frogspawn and the three axis-oriented froglights.
//! Track 3 owns these saved state IDs and procedural texture layers.
use super::{
    World,
    block::{Block, RenderKind},
    shape::{Box16, Boxes},
};
use glam::IVec3;

pub const FROGSPAWN: Block = Block(11008);
pub const OCHRE: Block = Block(11009);
pub const VERDANT: Block = Block(11012);
pub const PEARLESCENT: Block = Block(11015);
pub const LAST: u16 = 11017;
pub const HATCH_MIN: u64 = 3600;
pub const HATCH_MAX: u64 = 12000;
pub const HEIGHT: f64 = 1.5 / 16.0;
pub mod tex {
    pub const BASE: u16 = 3136;
    pub const SPAWN: u16 = BASE;
    pub const END: u16 = BASE + 10;
}
pub const fn owns(id: u16) -> bool {
    id >= FROGSPAWN.0 && id <= LAST
}
/// Axis order is Y, X, Z; every Java state has a distinct saved ID.
pub const fn light_state(b: Block) -> Option<(u16, u16)> {
    if b.0 >= OCHRE.0 && b.0 <= LAST { Some(((b.0 - OCHRE.0) / 3, (b.0 - OCHRE.0) % 3)) } else { None }
}
pub const fn base(b: Block) -> Option<Block> {
    match light_state(b) {
        Some((kind, _)) => Some(Block(OCHRE.0 + kind * 3)),
        None => None,
    }
}
pub fn placed(b: Block, normal: IVec3) -> Block {
    base(b).map_or(b, |base| {
        Block(
            base.0
                + if normal.x != 0 {
                    1
                } else if normal.z != 0 {
                    2
                } else {
                    0
                },
        )
    })
}
pub fn palette_ids() -> impl Iterator<Item = u16> {
    [FROGSPAWN.0, OCHRE.0, VERDANT.0, PEARLESCENT.0].into_iter()
}
pub const fn registry(id: u16) -> Option<(&'static str, RenderKind, [u16; 6])> {
    if id == FROGSPAWN.0 {
        return Some(("frogspawn", RenderKind::Shaped, [tex::SPAWN; 6]));
    }
    match light_state(Block(id)) {
        Some((kind, axis)) => {
            let side = tex::BASE + 1 + kind * 3;
            let end = side + 1;
            let rot = side + 2;
            let faces = match axis {
                1 => [end, end, rot, rot, side, side],
                2 => [rot, rot, rot, rot, end, end],
                _ => [side, side, end, end, side, side],
            };
            Some((
                match kind {
                    0 => "ochre froglight",
                    1 => "verdant froglight",
                    _ => "pearlescent froglight",
                },
                RenderKind::Opaque,
                faces,
            ))
        }
        None => None,
    }
}
/// Integer-grid proxy; picking/rendering scale its Y to Java's 1.5/16.
pub fn spawn_boxes() -> Boxes {
    Boxes::from_box(Box16 { min: [0; 3], max: [16, 2, 16] })
}
pub const fn emission(id: u16) -> u8 {
    if id >= OCHRE.0 && id <= LAST { 15 } else { 0 }
}
pub fn supports(b: Block) -> bool {
    b.holds_water()
}
pub fn pixel(layer: u16, x: usize, y: usize) -> [u8; 4] {
    if layer == tex::SPAWN {
        let dx = (x % 5) as i32 - 2;
        let dy = (y % 5) as i32 - 2;
        return if dx * dx + dy * dy <= 1 {
            [39, 47, 35, 255]
        } else if dx * dx + dy * dy <= 4 {
            [204, 219, 177, 255]
        } else {
            [102, 134, 99, 0]
        };
    }
    let local = layer - tex::BASE - 1;
    let rgb = match local / 3 {
        0 => [228u16, 204, 137],
        1 => [173, 224, 154],
        _ => [221, 186, 226],
    };
    let (x, y) = if local % 3 == 2 { (y, x) } else { (x, y) };
    let seam = if local % 3 == 1 { (x % 7 == 0) || (y % 7 == 0) } else { (x % 6 == 0) || ((y + x / 6 * 3) % 11 == 0) };
    let noise = ((x * 13 + y * 7 + layer as usize) % 11) as i16 - 5;
    let c = rgb.map(|v| (v as i16 + noise - if seam { 20 } else { 0 }).clamp(0, 255) as u8);
    [c[0], c[1], c[2], 255]
}
impl World {
    pub(super) fn track_frog_blocks(&mut self, p: IVec3, old: Block, new: Block) {
        if old != new && new == FROGSPAWN {
            let delay = HATCH_MIN + self.roll() % (HATCH_MAX - HATCH_MIN);
            self.schedule_redstone(p, delay, 0);
        }
        if old != new {
            for at in [p, p + IVec3::Y] {
                if self.get_block(at) == Some(FROGSPAWN) && self.get_block(at - IVec3::Y).is_some_and(|b| !supports(b))
                {
                    self.edit(at, Block::AIR, false);
                }
            }
        }
    }
    pub(super) fn hatch_frogspawn(&mut self, p: IVec3) {
        if self.get_block(p) != Some(FROGSPAWN) {
            return;
        }
        let valid = self.get_block(p - IVec3::Y).is_some_and(supports);
        self.edit(p, Block::AIR, false);
        if valid {
            let count = 2 + (self.roll() % 4) as u8;
            self.hatched_tadpoles.push((p, count));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        entity::{Entities, MobKind},
        item::{Item, ToolKind},
        world::terrain::Generator,
    };
    use glam::{DVec3, IVec2};
    const AT: IVec3 = IVec3::new(4, 65, 4);
    fn world() -> World {
        let mut w = World::new_headless(std::sync::Arc::new(Generator::new(42)), Default::default(), 2);
        w.load_test_column(IVec2::ZERO, 64);
        w
    }
    fn eggs(w: &mut World) {
        w.set_block(AT - IVec3::Y, Block::WATER);
        w.set_block(AT, FROGSPAWN);
    }
    #[test]
    fn frogspawn_uses_one_saved_tick_with_java_hatch_range_and_survives_restart() {
        let mut w = world();
        eggs(&mut w);
        let pending = w.redstone_to_string();
        let entry = pending.split('|').find(|s| s.starts_with("t,4,65,4,")).unwrap();
        let delay = entry.split(',').nth(4).unwrap().parse::<u64>().unwrap();
        assert!((HATCH_MIN..HATCH_MAX).contains(&delay));
        w.load_redstone("v1,0,1|t,4,65,4,2,0,0,11008");
        w.tick_redstone();
        assert_eq!(w.get_block(AT), Some(FROGSPAWN));
        let remaining = w.redstone_to_string();
        let mut loaded = world();
        eggs(&mut loaded);
        loaded.load_redstone(&remaining);
        loaded.tick_redstone();
        assert_eq!(loaded.get_block(AT), Some(Block::AIR));
        assert_eq!(loaded.hatched_tadpoles.len(), 1);
        let count = loaded.hatched_tadpoles[0].1;
        assert!((2..=5).contains(&count));
        let mut entities = Entities::new(9);
        crate::entity::frog::finish_hatches(&mut loaded, &mut entities);
        assert_eq!(entities.mobs.len(), usize::from(count));
        assert!(entities.mobs.iter().all(|m| m.kind == MobKind::Tadpole && m.persistent && !m.baby && m.pos.y == 64.5));
        assert!(
            entities
                .mobs
                .iter()
                .all(|m| (4.2 - 1e-7..=4.8 + 1e-7).contains(&m.pos.x) && (4.2 - 1e-7..=4.8 + 1e-7).contains(&m.pos.z))
        );
        assert!(loaded.drops.is_empty());
    }
    #[test]
    fn frogspawn_hatch_delay_does_not_advance_in_an_unloaded_chunk() {
        let mut w = world();
        eggs(&mut w);
        w.load_redstone("v1,0,1|t,4,65,4,3,0,0,11008");
        w.tick_redstone();
        let c = super::super::chunk::chunk_of(AT);
        let data = w.chunks[&c].data.clone();
        w.remove_chunk(c);
        for _ in 0..100 {
            w.tick_redstone();
        }
        assert!(w.hatched_tadpoles.is_empty());
        let saved = w.redstone_to_string();
        assert!(saved.contains("t,4,65,4,2,0,"), "{saved}");
        w.insert_chunk(c, data, true);
        w.tick_redstone();
        assert_eq!(w.get_block(AT), Some(FROGSPAWN));
        w.tick_redstone();
        assert_eq!(w.get_block(AT), Some(Block::AIR));
        assert_eq!(w.hatched_tadpoles.len(), 1);
    }

    #[test]
    fn support_loss_mining_and_falling_blocks_destroy_frogspawn_without_loot() {
        let mut w = world();
        eggs(&mut w);
        assert!(!FROGSPAWN.is_solid());
        assert_eq!(FROGSPAWN.drop(), None);
        w.spill_with_item(
            AT,
            FROGSPAWN,
            Some(crate::inventory::Stack {
                enchants: crate::enchant::Enchants::NONE.with(crate::enchant::Enchantment::SilkTouch, 1),
                ..crate::inventory::Stack::new(Item::tool(ToolKind::Pickaxe, crate::item::Tier::Diamond), 1)
            }),
        );
        assert!(w.drops.is_empty());
        w.set_block(AT - IVec3::Y, Block::AIR);
        assert_eq!(w.get_block(AT), Some(Block::AIR));
        assert!(w.hatched_tadpoles.is_empty());
        eggs(&mut w);
        w.set_block(AT + IVec3::Y, Block::SAND);
        for _ in 0..20 {
            w.tick_falling(0.05);
        }
        assert_ne!(w.get_block(AT), Some(FROGSPAWN));
        assert!(w.hatched_tadpoles.is_empty());
    }
    #[test]
    fn outline_picking_and_empty_collision_use_java_half_sixteenth_height() {
        let mut w = world();
        eggs(&mut w);
        assert_eq!(w.outline(AT), ([0.0; 3], [1.0, 0.09375, 1.0]));
        let hit = crate::physics::ray_shape(&w, AT, AT.as_dvec3() + DVec3::new(0.5, 1.0, 0.5), DVec3::NEG_Y).unwrap();
        assert!((hit.0 - (1.0 - HEIGHT)).abs() < 1e-12);
        assert!(super::super::shape::collision(FROGSPAWN, |_| Block::AIR, Block::WATER).is_empty());
    }
    #[test]
    fn all_nine_froglight_axis_states_emit_drop_and_reload_as_distinct_states() {
        let mut w = world();
        for (i, base) in [OCHRE, VERDANT, PEARLESCENT].into_iter().enumerate() {
            for (axis, normal) in [IVec3::Y, IVec3::X, IVec3::Z].into_iter().enumerate() {
                let block = placed(base, normal);
                assert_eq!(block.0, base.0 + axis as u16);
                assert_eq!(block.base(), base);
                assert_eq!(block.drop(), Some(Item::from(base)));
                assert_eq!(block.hardness(), 0.3);
                assert_eq!(block.best_tool(), Some(ToolKind::Hoe));
                assert_eq!(block.emission(), 15);
                let p = AT + IVec3::new((i * 3 + axis) as i32, 0, 0);
                w.set_block(p, block);
                assert_eq!(w.get_block(p), Some(block));
            }
        }
        // Exercise the real chunk codec, rather than only holding the IDs in RAM.
        let root = std::env::temp_dir().join(format!(
            "voxelcraft-froglights-{}-{}",
            std::process::id(),
            std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos()
        ));
        let storage = crate::world::storage::Storage::new(&root);
        storage
            .save(
                &crate::world::storage::LevelInfo { seed: 42, player: None, props: Default::default() },
                &w.modified_chunks(),
            )
            .unwrap();
        let mut loaded = world();
        for (pos, data) in storage.load_chunks().unwrap() {
            loaded.insert_chunk(pos, data, true);
        }
        for (i, base) in [OCHRE, VERDANT, PEARLESCENT].into_iter().enumerate() {
            for axis in 0..3 {
                assert_eq!(
                    loaded.get_block(AT + IVec3::new((i * 3 + axis) as i32, 0, 0)),
                    Some(Block(base.0 + axis as u16))
                );
            }
        }
        std::fs::remove_dir_all(root).unwrap();
        w.update_block_light();
        assert_eq!(w.block_light(AT), 15);
    }
}
