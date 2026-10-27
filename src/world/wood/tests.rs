//! Expected values come from Java 26.3: `Blocks` properties, the blockstate
//! and recipe files, `block_transformer/axe.json` and `FireBlock.bootStrap`
//! (see the verification record).
use super::*;
use crate::crafting::Grid;
use crate::inventory::Stack;
use crate::item::Tier;
use crate::mining;
use crate::world::redstone_blocks::{self as r, Component};
use crate::world::{nether_biome_blocks as nb, waterlog};

fn axis_of_normal(n: IVec3) -> u16 {
    if n.x != 0 {
        1
    } else if n.z != 0 {
        2
    } else {
        0
    }
}

const NORMALS: [IVec3; 6] = [IVec3::Y, IVec3::NEG_Y, IVec3::X, IVec3::NEG_X, IVec3::Z, IVec3::NEG_Z];

#[test]
fn every_registered_state_is_inside_the_reservation_and_named() {
    let mut registered = 0;
    for id in FIRST..=LAST {
        let b = Block(id);
        if registry(id).is_none() {
            assert_eq!(b.kind(), RenderKind::Invisible, "{id} is unregistered");
            continue;
        }
        registered += 1;
        assert_ne!(b.name(), "unknown", "{id}");
        assert!(b.base().kind() != RenderKind::Invisible, "{id} has a registered base");
    }
    assert!(Block(LAST + 1).kind() == RenderKind::Invisible || LAST + 1 > 20479);
    assert!(registered > 0);
    // Palette entries are default states that names resolve to.
    for id in palette_ids() {
        let b = Block(id);
        assert_eq!(b.base(), b, "palette holds default states: {}", b.name());
        assert_eq!(Block::from_name(b.name()), Some(b), "{} names its default state", b.name());
        assert!(Block::creative_palette().any(|p| p == b), "{} is in the palette", b.name());
        assert_eq!(Item::from_block(b).block(), Some(b));
    }
}

#[test]
fn existing_log_ids_mean_the_y_axis_and_placement_follows_the_clicked_face() {
    for (i, &log) in LOGS.iter().enumerate() {
        for n in NORMALS {
            let placed = axis_placed(log, n).unwrap();
            let a = axis_of_normal(n);
            assert_eq!(axis(placed), Some(a), "{} on {n}", log.name());
            if a == 0 {
                assert_eq!(placed, log, "the existing id is the Y-axis state");
            } else {
                assert!(owns(placed.0), "new X and Z states");
            }
            assert_eq!(placed.name(), log.name());
            assert_eq!(placed.base(), log);
            assert_eq!(placed.drop(), Some(Item::from_block(log)));
            assert_eq!(placed.hardness(), log.hardness());
            assert_eq!(placed.best_tool(), Some(ToolKind::Axe));
            assert_eq!(placed.is_log(), log.is_log(), "{}", log.name());
            assert_eq!(placed.fire_odds(), log.fire_odds());
            assert!(placed.is_opaque());
            // Placing again from an X or Z state keeps working (pick block, rotate).
            assert_eq!(axis_placed(placed, IVec3::Y), Some(log));
            // The end texture sits on the faces the axis points out of.
            let end = log.info().tex[2];
            let ends: Vec<usize> = (0..6).filter(|&f| placed.info().tex[f] == end).collect();
            assert_eq!(ends, [[2, 3], [0, 1], [4, 5]][a as usize], "{} axis {a}", log.name());
            if a != 0 {
                assert!(placed.info().tex.contains(&(tex::LOG_SIDE_TURNED + i as u16)));
            }
        }
    }
    assert_eq!(Block::from_name("oak_log"), Some(Block::LOG));
    assert_eq!(Item::from_name("bamboo_block"), Some(Item::from_block(crate::world::overworld_blocks::BAMBOO_BLOCK)));
}

#[test]
fn wood_and_stripped_blocks_have_three_axes_and_java_properties() {
    let mut blocks: Vec<Block> = (0..9u16)
        .flat_map(|w| [Pillar::Wood, Pillar::StrippedLog, Pillar::StrippedWood].map(|k| pillar(w, k, 0)))
        .collect();
    blocks.push(Block(STRIPPED_BAMBOO));
    for b in blocks {
        for n in NORMALS {
            let placed = axis_placed(b, n).unwrap();
            assert_eq!(axis(placed), Some(axis_of_normal(n)));
            assert_eq!(placed.base(), b);
            assert_eq!((placed.name(), placed.drop()), (b.name(), Some(Item::from_block(b))));
            assert_eq!(placed.hardness(), 2.0, "{}", b.name());
            assert_eq!(placed.best_tool(), Some(ToolKind::Axe));
            assert!(mining::can_harvest(placed, None), "{} needs no tool", b.name());
            assert_eq!(placed.fire_odds(), (5, 5), "FireBlock: {}", b.name());
            assert!(placed.ignited_by_lava());
            assert!(placed.is_opaque() && placed.is_solid());
        }
        let bamboo = b.0 == STRIPPED_BAMBOO;
        assert_eq!(b.is_log(), !bamboo, "#logs holds wood and stripped wood, not bamboo: {}", b.name());
    }
    let names = ["oak wood", "stripped spruce log", "stripped cherry wood", "block of stripped bamboo"];
    for name in names {
        assert!(Block::from_name(name).is_some(), "{name}");
    }
    assert_eq!(Block::from_name("stripped_pale_oak_wood"), Some(pillar(8, Pillar::StrippedWood, 0)));
}

#[test]
fn axes_strip_every_log_wood_and_bamboo_block_keeping_the_axis() {
    for (i, &log) in LOGS.iter().enumerate() {
        for n in NORMALS {
            let placed = axis_placed(log, n).unwrap();
            let s = stripped(placed).unwrap();
            assert_eq!(axis(s), axis(placed), "{}", log.name());
            let expected = if i == 9 { Block(STRIPPED_BAMBOO) } else { pillar(i as u16, Pillar::StrippedLog, 0) };
            assert_eq!(s.base(), expected);
            assert_eq!(stripped(s), None, "stripped blocks stay stripped");
            if i < 9 {
                let wood = axis_placed(pillar(i as u16, Pillar::Wood, 0), n).unwrap();
                let sw = stripped(wood).unwrap();
                assert_eq!((sw.base(), axis(sw)), (pillar(i as u16, Pillar::StrippedWood, 0), axis(wood)));
            }
        }
    }
    assert_eq!(stripped(Block::PLANKS), None);
    assert_eq!(stripped(nb::CRIMSON_STEM), None, "Nether stems strip in their own module");
}

#[test]
fn jungle_logs_tag_holds_cocoa() {
    use crate::world::overworld_blocks as ob;
    let cocoa = ob::cocoa(0, Facing::East);
    for k in [Pillar::Wood, Pillar::StrippedLog, Pillar::StrippedWood] {
        for axis in 0..3 {
            let b = Block(pillar_on(pillar(3, k, 0).0, axis));
            assert!(is_jungle_log(b));
            assert_eq!(ob::wall_ok(cocoa, b), Some(true), "{}", b.name());
        }
    }
    assert_eq!(ob::wall_ok(cocoa, Block(pillar_on(Block::JUNGLE_LOG.0, 1))), Some(true));
    assert_eq!(ob::wall_ok(cocoa, pillar(0, Pillar::Wood, 0)), Some(false));
}

fn craft(size: usize, cells: &[(usize, usize, Item)]) -> Option<Stack> {
    let mut g = Grid::new(size);
    for &(x, y, item) in cells {
        g.cells[y * size + x] = Some(Stack::new(item, 1));
    }
    g.result()
}

fn square(item: Item) -> Vec<(usize, usize, Item)> {
    vec![(0, 0, item), (1, 0, item), (0, 1, item), (1, 1, item)]
}

#[test]
fn bark_and_planks_recipes_take_the_log_tags() {
    for w in 0..9u16 {
        let wood = pillar(w, Pillar::Wood, 0);
        let s_log = pillar(w, Pillar::StrippedLog, 0);
        let s_wood = pillar(w, Pillar::StrippedWood, 0);
        assert_eq!(craft(2, &square(Item::from_block(LOGS[w as usize]))), Some(Stack::new(Item::from_block(wood), 3)));
        assert_eq!(craft(2, &square(Item::from_block(s_log))), Some(Stack::new(Item::from_block(s_wood), 3)));
        let planks = Wood::ALL[w as usize].planks();
        for b in [LOGS[w as usize], wood, s_log, s_wood] {
            assert_eq!(
                craft(2, &[(0, 0, Item::from_block(b))]),
                Some(Stack::new(Item::from_block(planks), 4)),
                "{}",
                b.name()
            );
        }
    }
    let bamboo_planks = crate::world::overworld_blocks::BAMBOO_PLANKS;
    assert_eq!(
        craft(2, &[(1, 1, Item::from_block(Block(STRIPPED_BAMBOO)))]),
        Some(Stack::new(Item::from_block(bamboo_planks), 2))
    );
}

#[test]
fn logs_wood_and_stripped_blocks_smelt_and_burn() {
    use crate::content::fuel;
    for item in ALL_LOGS {
        assert_eq!(crate::world::furnace::smelt(item), Some(Item::CHARCOAL), "{}", item.name());
        assert_eq!(fuel::ticks(item), Some(300));
    }
    assert_eq!(fuel::ticks(Item::from_block(Block(STRIPPED_BAMBOO))), Some(300));
    assert_eq!(crate::world::furnace::smelt(Item::from_block(Block(STRIPPED_BAMBOO))), None);
}

// ---- Switches ------------------------------------------------------------

#[test]
fn switch_families_mirror_oak_or_stone_and_round_trip() {
    for f in 0..switches::FAMILIES {
        let wood = f + switches::WOOD_OFFSET;
        let stone = f == 7;
        let originals: Vec<u16> = if stone {
            (1128..=1139).chain(1213..=1214).chain(7696..=7707).collect()
        } else {
            (1437..=1452).chain(1140..=1151).chain(1215..=1216).chain(7708..=7719).collect()
        };
        for o in originals {
            let twin = switches::reskin(Block(o), wood);
            assert_ne!(twin, Block(o));
            assert!(owns(twin.0));
            assert_eq!(nb::switch_oak(twin.0), Some((Block(o), wood)));
            assert_eq!(nb::switch_reskin(Block(o), wood), twin);
            assert_eq!(r::component(twin), r::component(Block(o)), "{}", twin.name());
            assert_eq!(nb::keep_wood(twin, Block(o)), twin);
            assert_eq!(twin.base(), nb::switch_reskin(Block(o).base(), wood));
            assert_eq!(twin.drop(), Some(Item::from_block(twin.base())), "{}", twin.name());
            assert_eq!(twin.hardness(), Block(o).hardness());
            assert_eq!(twin.kind(), RenderKind::Shaped);
            assert!(twin.name().starts_with(switches::family_name(f)), "{}", twin.name());
        }
        // Trapdoors have waterlogged twins; buttons and plates don't.
        if !stone {
            for local in 0..16 {
                let dry = Block(switches::trapdoor(f).0 + local);
                let wet = dry.with_water(true);
                assert_ne!(wet, dry);
                assert!(wet.is_waterlogged() && !dry.is_waterlogged());
                assert_eq!(wet.canonical(), dry);
                assert_eq!(r::component(wet), r::component(dry));
                assert_eq!(wet.base(), switches::trapdoor(f));
                assert_eq!(waterlog::pair(wet.0), Some((dry.0, true)));
            }
            assert!(!switches::button(f).waterloggable());
            assert_eq!(switches::trapdoor(f).hardness(), 3.0);
            assert_eq!(switches::trapdoor(f).best_tool(), Some(ToolKind::Axe));
        }
        assert_eq!(switches::button(f).hardness(), 0.5);
        assert_eq!(switches::plate(f).hardness(), 0.5);
        assert!(matches!(r::component(switches::button(f)), Some(Component::Button { wood, .. }) if wood == !stone));
    }
    let pb = switches::button(7);
    assert_eq!(pb.name(), "polished blackstone button");
    assert_eq!(switches::plate(7).best_tool(), Some(ToolKind::Pickaxe));
    assert!(mining::can_harvest(switches::plate(7), Some(Item::tool(ToolKind::Pickaxe, Tier::Wood))));
    // The trapdoor ids of polished blackstone stay free.
    assert_eq!(Block(switches::FIRST + 7 * switches::STRIDE).kind(), RenderKind::Invisible);
}

#[test]
fn switches_are_crafted_and_burn_like_java() {
    use crate::content::fuel;
    for f in 0..switches::FAMILIES {
        let mat = Item::from_block(switches::material(f));
        if f < 7 {
            let six: Vec<_> = (0..3).flat_map(|x| [(x, 0, mat), (x, 1, mat)]).collect();
            assert_eq!(craft(3, &six), Some(Stack::new(Item::from_block(switches::trapdoor(f)), 2)));
        }
        assert_eq!(craft(2, &[(0, 0, mat)]).map(|s| s.item), Some(Item::from_block(switches::button(f))));
        assert_eq!(craft(2, &[(0, 1, mat), (1, 1, mat)]), Some(Stack::new(Item::from_block(switches::plate(f)), 1)));
        let burns = f < 7;
        assert_eq!(fuel::ticks(Item::from_block(switches::button(f))), burns.then_some(100));
        assert_eq!(fuel::ticks(Item::from_block(switches::plate(f))), burns.then_some(300));
        if burns {
            assert_eq!(fuel::ticks(Item::from_block(switches::trapdoor(f))), Some(300));
        }
    }
}

#[test]
fn switches_place_on_every_mount_and_keep_their_family() {
    for f in 0..switches::FAMILIES {
        let wood = f + switches::WOOD_OFFSET;
        let button = switches::button(f);
        for (n, mount) in [(IVec3::Y, 0), (IVec3::NEG_Y, 5), (IVec3::Z, 1), (IVec3::NEG_Z, 2), (IVec3::X, 3)] {
            let placed = r::placed(button, n, Facing::South);
            assert_eq!(nb::switch_oak(placed.0).map(|(_, w)| w), Some(wood));
            assert!(matches!(r::component(placed), Some(Component::Button { mount: m, on: false, .. }) if m == mount));
        }
        for mount in 6..12 {
            let oak = r::button(mount, false, f != 7);
            let twin = switches::reskin(oak, wood);
            assert_eq!(nb::keep_wood(button, oak), twin);
            assert!(matches!(r::component(twin), Some(Component::Button { mount: m, .. }) if m == mount));
        }
        if f < 7 {
            let t = r::placed(switches::trapdoor(f), IVec3::NEG_Y, Facing::North);
            assert!(matches!(r::component(t), Some(Component::Trapdoor { top: true, iron: false, .. })));
            assert_eq!(nb::switch_oak(t.0).map(|(_, w)| w), Some(wood));
        }
    }
}

// ---- Signs, shelves and the chiseled bookshelf ------------------------------

/// `pattern` rows over a 3×3 grid, each character keyed in `key`.
fn grid(pattern: [&str; 3], key: &[(char, Item)]) -> Vec<(usize, usize, Item)> {
    let mut out = Vec::new();
    for (y, row) in pattern.iter().enumerate() {
        for (x, c) in row.chars().enumerate() {
            if let Some(&(_, item)) = key.iter().find(|(k, _)| *k == c) {
                out.push((x, y, item));
            }
        }
    }
    out
}

#[test]
fn signs_shelves_and_the_chiseled_bookshelf_are_crafted_like_java() {
    use signs::{Mount, Sign};
    for kind in Kind::ALL {
        let planks = Item::from_block(kind.planks());
        let stripped = Item::from_block(kind.stripped_log());
        let sign = Sign { kind, mount: Mount::Standing { rotation: 0 }, wet: false }.block();
        let hanging = Sign { kind, mount: Mount::Hanging { rotation: 0, attached: false }, wet: false }.block();
        let shelf = shelves::Shelf {
            kind,
            facing: Facing::North,
            powered: false,
            chain: shelves::Chain::Unconnected,
            wet: false,
        }
        .block();
        // `oak_sign.json`, `oak_hanging_sign.json`, `oak_shelf.json`.
        let cells = grid(["###", "###", " X "], &[('#', planks), ('X', Item::STICK)]);
        assert_eq!(craft(3, &cells), Some(Stack::new(Item::from_block(sign), 3)), "{kind:?} sign");
        let cells = grid(["X X", "###", "###"], &[('#', stripped), ('X', Item::from_block(Block::CHAIN))]);
        assert_eq!(craft(3, &cells), Some(Stack::new(Item::from_block(hanging), 6)), "{kind:?} hanging sign");
        let cells = grid(["###", "   ", "###"], &[('#', stripped)]);
        assert_eq!(craft(3, &cells), Some(Stack::new(Item::from_block(shelf), 6)), "{kind:?} shelf");
        // Planks make no shelf, and logs with bark no hanging sign.
        assert_eq!(craft(3, &grid(["###", "   ", "###"], &[('#', planks)])).map(|s| s.item), None);
    }
    // `chiseled_bookshelf.json`: `#planks` around a row of `#wooden_slabs`, mixed woods allowed.
    let spruce = Item::from_block(Wood::ALL[1].planks());
    let cells = grid(
        ["###", "XXX", "#O#"],
        &[('#', Item::from_block(Block::PLANKS)), ('O', spruce), ('X', Item::from_block(Block(108)))],
    );
    assert_eq!(craft(3, &cells), Some(Stack::new(Item::from_block(bookshelf::CHISELED_BOOKSHELF), 1)));
}
