//! Expected values come from Java 26.3 `Blocks` (strength, tools, tiers),
//! the blockstate files (`axis`, stairs/slab/wall properties) and the loot
//! tables in client-26.3.jar; see the verification record.
use super::*;
use crate::item::{Item, Tier, ToolKind};
use crate::mining;
use crate::world::{
    block::{Facing, STATE_CAPACITY},
    slab_stair_states::{self as ss, SlabType, StairShape, Stairs},
    wall_states,
};

/// Java's English names, which the registry audit matches.
#[test]
fn every_state_is_registered_once_with_its_java_name() {
    let reference: serde_json::Value =
        serde_json::from_str(include_str!("../../../tests/fixtures/java/26.3/registry_names.json")).unwrap();
    let blocks = reference["blocks"].as_object().unwrap();
    let ids = [
        "brick_wall",
        "chiseled_deepslate",
        "chiseled_nether_bricks",
        "chiseled_quartz_block",
        "chiseled_red_sandstone",
        "chiseled_sandstone",
        "chiseled_stone_bricks",
        "chiseled_tuff",
        "chiseled_tuff_bricks",
        "coal_block",
        "cracked_deepslate_bricks",
        "cracked_deepslate_tiles",
        "cracked_nether_bricks",
        "cut_red_sandstone",
        "cut_red_sandstone_slab",
        "cut_sandstone",
        "cut_sandstone_slab",
        "deepslate_brick_slab",
        "deepslate_brick_stairs",
        "deepslate_brick_wall",
        "deepslate_bricks",
        "deepslate_tile_slab",
        "deepslate_tile_stairs",
        "deepslate_tile_wall",
        "deepslate_tiles",
        "diamond_block",
        "end_stone_brick_slab",
        "end_stone_brick_stairs",
        "end_stone_brick_wall",
        "end_stone_bricks",
        "mossy_stone_brick_slab",
        "mossy_stone_brick_stairs",
        "mossy_stone_brick_wall",
        "nether_brick_wall",
        "polished_tuff",
        "polished_tuff_slab",
        "polished_tuff_stairs",
        "polished_tuff_wall",
        "quartz_block",
        "quartz_bricks",
        "quartz_pillar",
        "quartz_slab",
        "quartz_stairs",
        "red_nether_brick_slab",
        "red_nether_brick_stairs",
        "red_nether_brick_wall",
        "red_nether_bricks",
        "red_sandstone",
        "red_sandstone_slab",
        "red_sandstone_stairs",
        "red_sandstone_wall",
        "sandstone_wall",
        "slime_block",
        "smooth_quartz",
        "smooth_quartz_slab",
        "smooth_quartz_stairs",
        "smooth_red_sandstone",
        "smooth_red_sandstone_slab",
        "smooth_red_sandstone_stairs",
        "smooth_sandstone",
        "smooth_sandstone_slab",
        "smooth_sandstone_stairs",
        "tinted_glass",
        "tuff_brick_slab",
        "tuff_brick_stairs",
        "tuff_brick_wall",
        "tuff_bricks",
    ];
    assert_eq!(ids.len(), 67, "the audit's v0.12 B list");
    for id in ids {
        let name = blocks[id].as_str().unwrap().to_lowercase();
        let block = Block::from_name(&name).unwrap_or_else(|| panic!("{id} ({name}) is not registered"));
        assert!((ORIGIN..=RESERVED_LAST).contains(&block.0), "{id}");
        // The first state of each name is its item and creative entry.
        assert_eq!(block.base(), block, "{id}");
        assert!(Block::creative_palette().any(|b| b == block), "{id} has no creative entry");
        assert_eq!(Item::from_name(&name), Some(Item::from_block(block)), "{id}");
    }
    // Every registered id in the range is one of those blocks' states; the
    // rest stays invisible.
    let registered = (ORIGIN..=RESERVED_LAST).filter(|&id| Block(id).kind() != RenderKind::Invisible).count();
    assert_eq!(registered, 29 + 3 + 12 * 80 + 14 * 5 + 11 * 4);
}

#[test]
fn full_blocks_mine_like_java() {
    let pick = |tier| Some(Item::tool(ToolKind::Pickaxe, tier));
    let cases: [(Block, f32); 13] = [
        (POLISHED_TUFF, 1.5),
        (CHISELED_TUFF_BRICKS, 1.5),
        (DEEPSLATE_TILES, 3.5),
        (CHISELED_DEEPSLATE, 3.5),
        (QUARTZ_BLOCK, 0.8),
        (Block(QUARTZ_PILLAR.0 + 2), 0.8),
        (SMOOTH_QUARTZ, 2.0),
        (CUT_RED_SANDSTONE, 0.8),
        (SMOOTH_SANDSTONE, 2.0),
        (CRACKED_NETHER_BRICKS, 2.0),
        (END_STONE_BRICKS, 3.0),
        (CHISELED_STONE_BRICKS, 1.5),
        (COAL_BLOCK, 5.0),
    ];
    for (b, hardness) in cases {
        assert_eq!(b.hardness(), hardness, "{}", b.name());
        assert_eq!(b.best_tool(), Some(ToolKind::Pickaxe), "{}", b.name());
        assert!(!mining::can_harvest(b, None), "{} needs a pickaxe", b.name());
        assert!(mining::can_harvest(b, pick(Tier::Wood)), "{}", b.name());
        assert_eq!(b.drop(), Some(Item::from_block(b.base())), "{}", b.name());
    }
    assert_eq!(DIAMOND_BLOCK.hardness(), 5.0);
    assert!(!mining::can_harvest(DIAMOND_BLOCK, pick(Tier::Stone)));
    assert!(mining::can_harvest(DIAMOND_BLOCK, pick(Tier::Iron)));
    // Glass-like and slime: by hand, dropping themselves (loot tables).
    assert_eq!((TINTED_GLASS.hardness(), TINTED_GLASS.best_tool()), (0.3, None));
    assert_eq!(TINTED_GLASS.drop(), Some(Item::from_block(TINTED_GLASS)));
    assert!(mining::can_harvest(TINTED_GLASS, None));
    assert_eq!((SLIME_BLOCK.hardness(), SLIME_BLOCK.best_tool()), (0.0, None));
    assert_eq!(SLIME_BLOCK.drop(), Some(Item::from_block(SLIME_BLOCK)));
}

#[test]
fn tinted_glass_blocks_all_light_and_slime_dims_like_a_leaf() {
    // `TintedGlassBlock.getLightDampening` 15; slime: `noOcclusion` full block, 1.
    assert_eq!(TINTED_GLASS.light_opacity(), 15);
    assert!(TINTED_GLASS.borrows_light());
    assert!(!TINTED_GLASS.is_opaque() && TINTED_GLASS.is_solid() && TINTED_GLASS.info().self_cull);
    assert_eq!(SLIME_BLOCK.light_opacity(), 1);
    assert!(!SLIME_BLOCK.is_opaque() && SLIME_BLOCK.is_solid() && SLIME_BLOCK.info().self_cull);
    assert_eq!(COAL_BLOCK.light_opacity(), 15);
    assert!(COAL_BLOCK.is_opaque());
}

#[test]
fn quartz_pillar_takes_the_clicked_axis_and_keeps_it() {
    use glam::IVec3;
    for (normal, offset) in [(IVec3::Y, 0), (IVec3::NEG_Y, 0), (IVec3::X, 1), (IVec3::NEG_X, 1), (IVec3::Z, 2)] {
        let placed = crate::world::nether_blocks::placed(QUARTZ_PILLAR, normal);
        assert_eq!(placed, Block(QUARTZ_PILLAR.0 + offset));
        for facing in Facing::ALL {
            assert_eq!(placed.with_facing(facing), placed, "facing never turns a pillar");
        }
        assert_eq!(placed.base(), QUARTZ_PILLAR);
        assert_eq!(placed.drop(), Some(Item::from_block(QUARTZ_PILLAR)));
        assert_eq!(placed.name(), "quartz pillar");
    }
    // The ends face along the axis.
    assert_eq!(QUARTZ_PILLAR.info().tex[2], t::QUARTZ_PILLAR_TOP);
    assert_eq!(Block(QUARTZ_PILLAR.0 + 1).info().tex[0], t::QUARTZ_PILLAR_TOP);
    assert_eq!(Block(QUARTZ_PILLAR.0 + 2).info().tex[4], t::QUARTZ_PILLAR_TOP);
}

#[test]
fn stairs_and_slabs_have_every_java_state_and_their_bases_rules() {
    for (f, fam) in SHAPES.iter().enumerate() {
        let family = (ss::LEGACY_COUNT + f) as u8;
        assert_eq!(ss::FAMILIES[family as usize], *fam);
        let mut stairs_ids = std::collections::HashSet::new();
        if (f as u16) < STAIR_FAMILIES {
            for facing in Facing::ALL {
                for top in [false, true] {
                    for shape in StairShape::ALL {
                        for wet in [false, true] {
                            let s = Stairs { family, facing, top, shape, wet };
                            let id = ss::stairs_id(s);
                            assert!(stairs_ids.insert(id));
                            assert_eq!(ss::stairs(id), Some(s));
                            assert_eq!(id.name(), fam.stairs.name());
                            assert_eq!(id.base(), fam.stairs);
                            assert_eq!(id.stairs_base(), Some(fam.base));
                            assert_eq!(id.is_waterlogged(), wet);
                            assert_eq!(id.with_water(!wet), ss::stairs_id(Stairs { wet: !wet, ..s }));
                            assert_eq!(id.drop(), Some(Item::from_block(fam.stairs)));
                            assert_eq!(id.hardness(), fam.base.hardness());
                            assert_eq!(id.harvest_level(), fam.base.harvest_level());
                            assert!(id.uses_shape_for_light_occlusion());
                            assert_eq!(id.light_opacity(), wet as u8);
                            assert_eq!(id.info().tex, fam.base.info().tex);
                        }
                    }
                }
            }
            assert_eq!(stairs_ids.len(), 80);
        }
        for (kind, wet) in [
            (SlabType::Bottom, false),
            (SlabType::Bottom, true),
            (SlabType::Top, false),
            (SlabType::Top, true),
            (SlabType::Double, false),
        ] {
            let s = ss::Slab { family, kind, wet };
            let id = ss::slab_id(s);
            assert_eq!(ss::slab(id), Some(s));
            assert_eq!(id.name(), fam.slab.name());
            assert_eq!(id.slab_base(), Some(fam.base));
            assert_eq!(id.drop(), Some(Item::from_block(fam.slab)));
            assert_eq!(id.is_waterlogged(), wet);
            assert_eq!(id.is_opaque(), kind == SlabType::Double);
            assert_eq!(id.top_drop(), if kind == SlabType::Bottom { 8 } else { 0 });
            assert_eq!(id.waterloggable(), kind != SlabType::Double);
        }
    }
}

#[test]
fn slab_destroy_times_follow_register_slab() {
    // `registerSlab(id, base, 2.0F)` for sandstone, cut sandstone, quartz,
    // red sandstone, cut red sandstone, stone, stone bricks, blackstone and
    // polished blackstone bricks; other slabs copy their base.
    let slab = |f: u16| SHAPES[f as usize].slab;
    for f in [4, 6, 12, 13] {
        assert_eq!(slab(f).hardness(), 2.0, "{}", slab(f).name());
        assert_eq!(SHAPES[f as usize].base.hardness(), 0.8);
    }
    assert_eq!(Block::slab_of(Block::SANDSTONE).unwrap().hardness(), 2.0);
    assert_eq!(Block::stairs_of(Block::SANDSTONE).unwrap().hardness(), 0.8);
    assert_eq!(stairs_at(4, 0).hardness(), 0.8, "quartz stairs copy the block");
    assert_eq!(slab(5).hardness(), 2.0, "smooth quartz slab copies smooth quartz");
    assert_eq!(slab(2).hardness(), 3.5);
    assert_eq!(Block::slab_of(Block::STONE).unwrap().hardness(), 2.0);
    assert_eq!(Block::slab_of(Block::COBBLESTONE).unwrap().hardness(), Block::COBBLESTONE.hardness());
}

#[test]
fn walls_have_post_and_water_states() {
    for (w, &base) in WALLS.iter().enumerate() {
        let w = w as u16;
        let first = wall(w);
        assert_eq!(first.shaped(), Some(Shaped::Wall));
        for up in [true, false] {
            for wet in [false, true] {
                let b = wall_states::with_up(first.with_water(wet), up);
                assert_eq!(b.0, first.0 + (!up as u16) * 2 + wet as u16);
                assert_eq!(wall_states::up(b), Some(up));
                assert_eq!(b.is_waterlogged(), wet);
                assert_eq!(b.with_water(!wet), wall_states::with_up(first.with_water(!wet), up));
                assert_eq!(b.shaped(), Some(Shaped::Wall));
                assert_eq!(b.wall_base(), Some(base));
                assert_eq!(b.name(), first.name());
                assert_eq!(b.base(), first);
                assert_eq!(b.drop(), Some(Item::from_block(first)));
                assert_eq!(b.hardness(), base.hardness());
                assert_eq!(b.best_tool(), Some(ToolKind::Pickaxe));
                assert_eq!(b.harvest_level(), base.harvest_level());
                assert_eq!(b.light_opacity(), wet as u8);
                assert_eq!(b.info().tex, base.info().tex);
            }
        }
    }
    assert_eq!(wall(10).wall_base(), Some(Block::BRICKS));
    assert_eq!(wall(10).name(), "brick wall");
}

#[test]
fn walls_connect_and_raise_posts_like_existing_walls() {
    use crate::world::shape;
    let w = wall(4);
    let cobble = Block::COBBLESTONE_WALL;
    let side = |f: Facing| if f.along_x() { cobble } else { Block::AIR };
    let (arms, post) = shape::wall_state(&side, shape::Above::NONE);
    assert!(!post, "a straight run between walls has no post");
    assert_eq!(arms.iter().filter(|a| a.is_some()).count(), 2);
    let reshaped = wall_states::reshaped(w, |d| side(Facing::from_offset(d).unwrap_or(Facing::South)));
    assert_eq!(wall_states::up(reshaped), Some(false));
    let alone = wall_states::reshaped(w, |_| Block::AIR);
    assert_eq!(wall_states::up(alone), Some(true));
}

#[test]
fn retired_forms_stay_placed_but_leave_recipes_and_the_palette() {
    let palette: Vec<Block> = Block::creative_palette().collect();
    let names = [
        "calcite stairs",
        "calcite slab",
        "calcite wall",
        "deepslate stairs",
        "deepslate slab",
        "deepslate wall",
        "smooth stone stairs",
        "smooth stone wall",
        "polished granite wall",
        "polished diorite wall",
        "polished andesite wall",
    ];
    for (&b, name) in RETIRED.iter().zip(names) {
        assert_eq!(b.name(), name);
        assert!(b.kind() != RenderKind::Invisible, "{name} still renders");
        assert_eq!(b.drop(), Some(Item::from_block(b)), "{name} drops itself");
        assert!(!palette.contains(&b), "{name} left the palette");
        assert!(crate::crafting::recipes().iter().all(|r| r.result.item != Item::from_block(b)), "{name} has a recipe");
    }
    // Their Java siblings remain: smooth stone slab and the polished stairs and slabs.
    for kept in [
        crate::world::forms::stone_id(9, 4),
        crate::world::forms::stone_id(2, 0),
        crate::world::forms::stone_id(2, 4),
        crate::world::forms::stone_id(1, 5),
    ] {
        assert!(palette.contains(&kept), "{}", kept.name());
    }
    // Every other id, including the retired ones' other states, is unchanged.
    for &b in &RETIRED {
        assert!(b.0 < 400);
    }
}

#[test]
fn smelting_makes_cracked_and_smooth_forms() {
    use crate::world::furnace::smelt;
    let b = Item::from_block;
    assert_eq!(smelt(b(DEEPSLATE_BRICKS)), Some(b(CRACKED_DEEPSLATE_BRICKS)));
    assert_eq!(smelt(b(DEEPSLATE_TILES)), Some(b(CRACKED_DEEPSLATE_TILES)));
    assert_eq!(smelt(b(Block::NETHER_BRICKS)), Some(b(CRACKED_NETHER_BRICKS)));
    assert_eq!(smelt(b(Block::SANDSTONE)), Some(b(SMOOTH_SANDSTONE)));
    assert_eq!(smelt(b(RED_SANDSTONE)), Some(b(SMOOTH_RED_SANDSTONE)));
    assert_eq!(smelt(b(QUARTZ_BLOCK)), Some(b(SMOOTH_QUARTZ)));
    assert_eq!(smelt(b(Block::STONE_BRICKS)), Some(b(Block::CRACKED_STONE_BRICKS)));
    assert_eq!(crate::world::furnace::smelt_xp(b(SMOOTH_QUARTZ)), 0.1);
    assert_eq!(smelt(b(CHISELED_DEEPSLATE)), None);
}

#[test]
fn states_survive_a_save_and_reload() {
    use crate::world::{
        chunk::ChunkData,
        storage::{LevelInfo, Storage},
    };
    use std::sync::Arc;
    let ids: Vec<u16> = (ORIGIN..=RESERVED_LAST).filter(|&id| Block(id).kind() != RenderKind::Invisible).collect();
    assert!(ids.len() < 32 * 32 * 32 && ids.iter().all(|&id| (id as usize) < STATE_CAPACITY));
    let at = |i: usize| (i % 32, i / 1024, (i / 32) % 32);
    let mut chunk = ChunkData::Uniform(Block::AIR);
    for (i, &id) in ids.iter().enumerate() {
        let (x, y, z) = at(i);
        chunk.set(x, y, z, Block(id));
    }
    let dir = std::env::temp_dir().join(format!("voxelcraft-stone-{}", std::process::id()));
    let storage = Storage::new(&dir);
    let pos = glam::IVec3::new(2, 4, -3);
    let level = LevelInfo { seed: 7, player: None, props: Default::default() };
    storage.save(&level, &[(pos, Arc::new(chunk))]).unwrap();
    let loaded = storage.load_chunks().unwrap();
    for (i, &id) in ids.iter().enumerate() {
        let (x, y, z) = at(i);
        assert_eq!(loaded[&pos].get(x, y, z), Block(id));
    }
    std::fs::remove_dir_all(dir).unwrap();
}
