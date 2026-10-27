use super::*;
use crate::crafting::{Grid, recipes};
use crate::inventory::Stack;

fn craft(size: usize, cells: &[(usize, usize, Item)]) -> Option<Stack> {
    let mut g = Grid::new(size);
    for &(x, y, item) in cells {
        g.cells[y * size + x] = Some(Stack::new(item, 1));
    }
    g.result()
}

fn fill(rows: &[&str], item: Item) -> Vec<(usize, usize, Item)> {
    rows.iter()
        .enumerate()
        .flat_map(|(y, row)| row.chars().enumerate().filter(|&(_, c)| c == '#').map(move |(x, _)| (x, y, item)))
        .collect()
}

#[test]
fn shapes_craft_from_their_blocks() {
    for f in 0..s::SLAB_FAMILIES {
        let base = b(s::SHAPES[f as usize].base);
        assert_eq!(craft(3, &fill(SLAB, base)), Some(Stack::new(b(s::slab_at(f, 0)), 6)), "family {f} slab");
        if f < s::STAIR_FAMILIES {
            assert_eq!(craft(3, &fill(STAIRS, base)), Some(Stack::new(b(s::stairs_at(f, 0)), 4)), "family {f} stairs");
        }
    }
    for (w, &base) in s::WALLS.iter().enumerate() {
        assert_eq!(craft(3, &fill(WALL, b(base))), Some(Stack::new(b(s::wall(w as u16)), 6)), "wall {w}");
    }
}

#[test]
fn quartz_and_sandstone_shapes_take_javas_alternatives() {
    for kind in [s::CHISELED_QUARTZ_BLOCK, s::QUARTZ_PILLAR] {
        assert_eq!(craft(3, &fill(SLAB, b(kind))), Some(Stack::new(b(s::slab_at(4, 0)), 6)));
        assert_eq!(craft(3, &fill(STAIRS, b(kind))), Some(Stack::new(b(s::stairs_at(4, 0)), 4)));
    }
    let sandstone_slab = b(Block::slab_of(Block::SANDSTONE).unwrap());
    let sandstone_stairs = b(Block::stairs_of(Block::SANDSTONE).unwrap());
    assert_eq!(craft(3, &fill(SLAB, b(s::CHISELED_SANDSTONE))), Some(Stack::new(sandstone_slab, 6)));
    assert_eq!(craft(3, &fill(SLAB, b(Block::SANDSTONE))), Some(Stack::new(sandstone_slab, 6)));
    // Cut sandstone makes its own slab, but stairs of plain sandstone.
    assert_eq!(craft(3, &fill(SLAB, b(s::CUT_SANDSTONE))), Some(Stack::new(b(s::slab_at(12, 0)), 6)));
    assert_eq!(craft(3, &fill(STAIRS, b(s::CUT_SANDSTONE))), Some(Stack::new(sandstone_stairs, 4)));
    assert_eq!(craft(3, &fill(STAIRS, b(s::CUT_RED_SANDSTONE))), Some(Stack::new(b(s::stairs_at(6, 0)), 4)));
    assert_eq!(craft(3, &fill(SLAB, b(s::CHISELED_RED_SANDSTONE))), Some(Stack::new(b(s::slab_at(6, 0)), 6)));
    // One recipe per result: the replaced sandstone recipes are gone.
    for item in [sandstone_slab, sandstone_stairs] {
        assert_eq!(recipes().iter().filter(|r| r.result.item == item).count(), 1);
    }
}

#[test]
fn full_blocks_craft_like_java() {
    let q = Item::NETHER_QUARTZ;
    assert_eq!(craft(2, &[(0, 0, q), (1, 0, q), (0, 1, q), (1, 1, q)]), Some(Stack::new(b(s::QUARTZ_BLOCK), 1)));
    let column = |item: Item| craft(3, &[(1, 0, item), (1, 1, item)]);
    assert_eq!(column(b(s::QUARTZ_BLOCK)), Some(Stack::new(b(s::QUARTZ_PILLAR), 2)));
    assert_eq!(column(b(s::slab_at(4, 0))), Some(Stack::new(b(s::CHISELED_QUARTZ_BLOCK), 1)));
    assert_eq!(column(b(forms::stone_id(7, 4))), Some(Stack::new(b(s::CHISELED_TUFF), 1)));
    assert_eq!(column(b(s::slab_at(1, 0))), Some(Stack::new(b(s::CHISELED_TUFF_BRICKS), 1)));
    assert_eq!(column(b(forms::stone_id(11, 4))), Some(Stack::new(b(s::CHISELED_DEEPSLATE), 1)));
    assert_eq!(column(b(Block::STONE_BRICK_SLAB)), Some(Stack::new(b(s::CHISELED_STONE_BRICKS), 1)));
    assert_eq!(
        column(b(Block::slab_of(Block::NETHER_BRICKS).unwrap())),
        Some(Stack::new(b(s::CHISELED_NETHER_BRICKS), 1))
    );
    assert_eq!(column(b(Block::slab_of(Block::SANDSTONE).unwrap())), Some(Stack::new(b(s::CHISELED_SANDSTONE), 1)));
    assert_eq!(column(b(s::slab_at(6, 0))), Some(Stack::new(b(s::CHISELED_RED_SANDSTONE), 1)));
    let square = |item: Item| craft(2, &[(0, 0, item), (1, 0, item), (0, 1, item), (1, 1, item)]);
    assert_eq!(square(b(Block::TUFF)), Some(Stack::new(b(s::POLISHED_TUFF), 4)));
    assert_eq!(square(b(s::POLISHED_TUFF)), Some(Stack::new(b(s::TUFF_BRICKS), 4)));
    assert_eq!(square(b(Block::POLISHED_DEEPSLATE)), Some(Stack::new(b(s::DEEPSLATE_BRICKS), 4)));
    assert_eq!(square(b(s::DEEPSLATE_BRICKS)), Some(Stack::new(b(s::DEEPSLATE_TILES), 4)));
    assert_eq!(square(b(s::QUARTZ_BLOCK)), Some(Stack::new(b(s::QUARTZ_BRICKS), 4)));
    assert_eq!(square(b(Block::RED_SAND)), Some(Stack::new(b(s::RED_SANDSTONE), 1)));
    assert_eq!(square(b(s::RED_SANDSTONE)), Some(Stack::new(b(s::CUT_RED_SANDSTONE), 4)));
    assert_eq!(square(b(Block::SANDSTONE)), Some(Stack::new(b(s::CUT_SANDSTONE), 4)));
    assert_eq!(square(b(Block::END_STONE)), Some(Stack::new(b(s::END_STONE_BRICKS), 4)));
    let (n, w) = (Item::NETHER_BRICK, Item::NETHER_WART);
    assert_eq!(craft(2, &[(0, 0, n), (1, 0, w), (0, 1, w), (1, 1, n)]), Some(Stack::new(b(s::RED_NETHER_BRICKS), 1)));
    let (g, a) = (b(Block::GLASS), Item::AMETHYST_SHARD);
    assert_eq!(
        craft(3, &[(1, 0, a), (0, 1, a), (1, 1, g), (2, 1, a), (1, 2, a)]),
        Some(Stack::new(b(s::TINTED_GLASS), 2))
    );
    for (item, block) in
        [(Item::COAL, s::COAL_BLOCK), (Item::DIAMOND, s::DIAMOND_BLOCK), (Item::SLIME_BALL, s::SLIME_BLOCK)]
    {
        assert_eq!(craft(3, &fill(NINE, item)), Some(Stack::new(b(block), 1)));
        assert_eq!(craft(2, &[(0, 0, b(block))]), Some(Stack::new(item, 9)));
    }
}

#[test]
fn stonecutting_data_names_javas_blocks() {
    let reference: serde_json::Value =
        serde_json::from_str(include_str!("../../../tests/fixtures/java/26.3/registry_names.json")).unwrap();
    let blocks = reference["blocks"].as_object().unwrap();
    let name = |id: &str| blocks[id].as_str().unwrap().to_lowercase();
    assert_eq!(STONECUTTING.len(), 99);
    for &(input_id, input, output_id, output, count) in STONECUTTING {
        assert_eq!(input.name(), name(input_id), "{input_id}");
        assert_eq!(output.name(), name(output_id), "{output_id}");
        assert_eq!(output.base(), output, "{output_id} is its item state");
        let slab = output.is_slab();
        assert_eq!(count, if slab { 2 } else { 1 }, "{input_id} -> {output_id}");
    }
    let from_tuff: Vec<_> = stonecutting(b(Block::TUFF)).collect();
    assert!(from_tuff.contains(&(b(s::slab_at(1, 0)), 2)));
    assert!(from_tuff.contains(&(b(s::CHISELED_TUFF), 1)));
}
