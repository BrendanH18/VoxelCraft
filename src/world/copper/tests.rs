//! Fixtures from the Java 26.3 classes and data listed in
//! docs/verification/2026-10-25-v0.12-copper.md; expected values are explicit.
use super::*;
use crate::crafting::Grid;
use crate::inventory::Stack;
use crate::world::block::STATE_CAPACITY;
use crate::world::chunk::ChunkData;
use crate::world::redstone_blocks::{self as r, Component};
use crate::world::slab_stair_states::{Slab, SlabType, StairShape, Stairs};
use crate::world::storage::{LevelInfo, Storage};
use crate::world::terrain::Generator;
use glam::DVec3;
use std::sync::Arc;

const P: IVec3 = IVec3::new(8, 140, 8);

fn world() -> World {
    let mut w = World::new_headless(Arc::new(Generator::new(7)), Default::default(), 2);
    for y in 4..7 {
        w.insert_chunk(IVec3::new(0, y, 0), Arc::new(ChunkData::Uniform(Block::AIR)), false);
    }
    w
}

fn registered() -> impl Iterator<Item = (Block, Copper)> {
    (FIRST..END).chain([231]).filter_map(|id| decode(id).map(|c| (Block(id), c)))
}

#[test]
fn layout_fits_the_reservation_and_round_trips() {
    assert_eq!(FIRST, 22528);
    assert_eq!(END, 23373);
    let mut count = 0;
    for id in FIRST..=RESERVED_LAST {
        let b = Block(id);
        match decode(id) {
            Some(c) => {
                count += 1;
                assert_eq!(encode(c), b, "{c:?}");
                assert_eq!(state(b), Some(c));
                assert_eq!(b.name(), name(c), "{id}");
            }
            None => assert_eq!(b.name(), "unknown", "{id} stays unregistered"),
        }
    }
    // Three new blocks of copper, four cut and chiseled, 179 states per
    // stage for the others (2 + 4 + 80 + 5 + 24 + 32 + 2 + 6 + 4 + 8 + 12)
    // and five torch states.
    assert_eq!(count, 3 + 8 + 4 * 179 + 5);
    assert_eq!(state(Block::COPPER_BLOCK), Some(Copper { family: Family::Block, stage: Stage::Unaffected, local: 0 }));
    assert_eq!(block(Family::Block, Stage::Unaffected, 0), Block::COPPER_BLOCK);
    for id in (0..STATE_CAPACITY as u16).filter(|&id| !owns(id) && id != 231) {
        assert!(state(Block(id)).is_none(), "{id}");
    }
    // Spot checks against the table in docs/architecture.md.
    assert_eq!(Family::Grate.origin(), 22540);
    assert_eq!(Family::Stairs.origin(), 22564);
    assert_eq!(Family::Door.origin(), 22904);
    assert_eq!(Family::Trapdoor.origin(), 23000);
    assert_eq!(Family::Chest.origin(), 23176);
    assert_eq!(Family::Rod.origin(), 23272);
    assert_eq!(Family::Torch.origin(), 23368);
    // Reserved chest and rod states (double chests, powered rods) stay free.
    assert_eq!(Block(Family::Chest.origin() + 8).name(), "unknown");
    assert_eq!(Block(Family::Rod.origin() + 12).name(), "unknown");
    // Texture layers stay inside the track's 2128–2383.
    assert_eq!((t::BASE, t::END), (2128, 2201));
}

#[test]
fn every_java_name_resolves_and_every_item_is_in_the_palette_once() {
    let java = |f: Family, s: Stage| -> String {
        let prefix = ["", "exposed_", "weathered_", "oxidized_"][s as usize];
        let id = match f {
            Family::Block => {
                return ["copper_block", "exposed_copper", "weathered_copper", "oxidized_copper"][s as usize].into();
            }
            Family::Cut => "cut_copper",
            Family::Chiseled => "chiseled_copper",
            Family::Grate => "copper_grate",
            Family::Bulb => "copper_bulb",
            Family::Stairs => "cut_copper_stairs",
            Family::Slab => "cut_copper_slab",
            Family::Door => "copper_door",
            Family::Trapdoor => "copper_trapdoor",
            Family::Bars => "copper_bars",
            Family::Chain => "copper_chain",
            Family::Lantern => "copper_lantern",
            Family::Chest => "copper_chest",
            Family::Rod => "lightning_rod",
            Family::Torch => "copper_torch",
        };
        format!("{prefix}{id}")
    };
    let palette: Vec<Block> = Block::creative_palette().collect();
    for f in Family::ALL {
        for &s in &Stage::ALL[..f.stages() as usize] {
            let b = block(f, s, 0);
            let id = java(f, s);
            // The block of copper's Java id is `copper_block`; its name is Java's "Block of Copper".
            if f != Family::Block || s != Stage::Unaffected {
                assert_eq!(Block::from_name(&id), Some(b), "{id}");
            }
            assert_eq!(Block::from_name(b.name()), Some(b), "{id}");
            assert_eq!(crate::item::Item::from_name(&id).or(Item::from_name(b.name())), Some(Item::from_block(b)));
            assert_eq!(palette.iter().filter(|&&p| p == b).count(), 1, "{id} in the palette once");
        }
    }
    // The wall torch has Java's block id name; it is picked as the torch.
    let wall = block(Family::Torch, Stage::Unaffected, 1);
    assert_eq!(Block::from_name("copper_wall_torch"), Some(wall));
    assert_eq!(
        crate::session::interact::picked_item(wall),
        Item::from_block(block(Family::Torch, Stage::Unaffected, 0))
    );
}

#[test]
fn every_state_mines_drops_and_glows_like_java() {
    for (b, c) in registered() {
        let item = Item::from_block(block(c.family, c.stage, 0));
        let (hardness, tool, level) = match c.family {
            Family::Door => (3.0, Some(ToolKind::Pickaxe), None),
            Family::Bars | Family::Chain => (5.0, Some(ToolKind::Pickaxe), Some(0)),
            Family::Lantern => (3.5, Some(ToolKind::Pickaxe), None),
            Family::Torch => (0.0, None, None),
            _ => (3.0, Some(ToolKind::Pickaxe), Some(1)),
        };
        assert_eq!((b.hardness(), b.best_tool(), b.harvest_level()), (hardness, tool, level), "{b:?} {c:?}");
        let upper_door = c.family == Family::Door && c.local >= 16;
        assert_eq!(b.drop(), (!upper_door).then_some(item), "{c:?}");
        assert_eq!(b.base(), block(c.family, c.stage, 0), "{c:?}");
        assert_eq!(crate::session::interact::picked_item(b), item);
        assert_eq!(b.fire_odds(), (0, 0));
        assert!(!b.ignited_by_lava());
        let glow = match c.family {
            Family::Bulb if c.local & 1 == 1 => [15, 12, 8, 4][c.stage as usize],
            Family::Lantern => 15,
            Family::Torch => 14,
            _ => 0,
        };
        assert_eq!(b.emission(), glow, "{c:?}");
        // Full cubes block light; a dry grate is see-through; shapes don't.
        let opacity = match c.family {
            Family::Block | Family::Cut | Family::Chiseled | Family::Bulb | Family::Chest => 15,
            Family::Slab if c.local == 4 => 15,
            Family::Grate if c.local == 0 => 0,
            _ if b.is_waterlogged() => 1,
            _ => 0,
        };
        assert_eq!(b.light_opacity(), opacity, "{c:?}");
        assert_eq!(b.is_solid(), !matches!(c.family, Family::Torch) || c.local > 0, "{c:?}");
    }
}

#[test]
fn waterloggable_families_pair_dry_and_wet_states() {
    for (b, c) in registered() {
        let waterloggable = match c.family {
            Family::Grate
            | Family::Stairs
            | Family::Trapdoor
            | Family::Bars
            | Family::Chain
            | Family::Lantern
            | Family::Chest
            | Family::Rod => true,
            Family::Slab => c.local != 4,
            _ => false,
        };
        assert_eq!(b.waterloggable(), waterloggable, "{c:?}");
        if waterloggable {
            let (dry, wet) = (b.with_water(false), b.with_water(true));
            assert!(!dry.is_waterlogged() && wet.is_waterlogged());
            assert_eq!(dry.base(), wet.base());
            assert_eq!(state(dry).map(|c| (c.family, c.stage)), Some((c.family, c.stage)));
            assert_eq!(dry.info().tex, wet.info().tex);
        }
    }
}

#[test]
fn stairs_and_slabs_are_slab_stair_families() {
    for s in Stage::ALL {
        let family = (STAIR_FAMILY + s as usize) as u8;
        let fam = ss::FAMILIES[family as usize];
        assert_eq!(fam.base, block(Family::Cut, s, 0));
        for facing in Facing::ALL {
            for top in [false, true] {
                for shape in StairShape::ALL {
                    for wet in [false, true] {
                        let st = Stairs { family, facing, top, shape, wet };
                        let id = ss::stairs_id(st);
                        assert_eq!(of(id, Family::Stairs).map(|c| c.stage), Some(s));
                        assert_eq!(ss::stairs(id), Some(st));
                        assert_eq!(id.stairs_base(), Some(fam.base));
                        assert_eq!(id.shaped(), Some(Shaped::Stairs(facing)));
                        assert_eq!(id.oriented(), Some((fam.stairs, facing)));
                        assert_eq!(
                            id.with_facing(facing.opposite()),
                            ss::stairs_id(Stairs { facing: facing.opposite(), ..st })
                        );
                        assert!(id.uses_shape_for_light_occlusion());
                    }
                }
            }
        }
        for (kind, wet) in [
            (SlabType::Bottom, false),
            (SlabType::Bottom, true),
            (SlabType::Top, false),
            (SlabType::Top, true),
            (SlabType::Double, false),
        ] {
            let id = ss::slab_id(Slab { family, kind, wet });
            assert_eq!(ss::slab(id), Some(Slab { family, kind, wet }));
            assert_eq!(id.slab_base(), Some(fam.base));
            assert_eq!(id.is_opaque(), kind == SlabType::Double);
            assert_eq!(id.shaped().is_some(), kind == SlabType::Top);
        }
        // A second slab merges into a double one.
        let double = ss::placed(fam.slab, fam.slab, IVec3::Y, 0.5, |_| None);
        assert_eq!(ss::slab_type(double), Some(SlabType::Double));
        assert_eq!(of(double, Family::Slab).map(|c| c.stage), Some(s));
    }
}

#[test]
fn a_double_slab_drops_two_slabs() {
    let mut w = world();
    let double = block(Family::Slab, Stage::Weathered, 4);
    w.set_block(P, double);
    w.drops.clear();
    w.spill_block(P, double);
    let slab = Item::from_block(block(Family::Slab, Stage::Weathered, 0));
    assert_eq!(w.drops.iter().map(|(_, s)| (s.item, s.count)).collect::<Vec<_>>(), [(slab, 2)]);
}

/// Ages `b` alone at `P` with neighbours `around` (offset, block), `n` times
/// from scratch, and counts how often it aged.
fn aged(b: Block, around: &[(IVec3, Block)], n: usize) -> usize {
    let mut w = world();
    for &(d, n) in around {
        w.set_block(P + d, n);
    }
    let mut count = 0;
    for _ in 0..n {
        w.set_block(P, b);
        w.tick_copper(P, b);
        if w.get_block(P) != Some(b) {
            count += 1;
        }
    }
    count
}

#[test]
fn aging_follows_change_over_time_odds() {
    let copper = Block::COPPER_BLOCK;
    let exposed = block(Family::Block, Stage::Exposed, 0);
    let n = 40_000;
    // Alone: 0.0569 × 1² × 0.75 for unaffected copper, 0.0569 later.
    let expect = |p: f64| (p * n as f64) as usize;
    let near = |got: usize, want: usize| (got as f64 - want as f64).abs() < 5.0 * (want as f64).sqrt() + 5.0;
    let alone = aged(copper, &[], n);
    assert!(near(alone, expect(0.056_888_89 * 0.75)), "{alone}");
    let alone = aged(exposed, &[], n);
    assert!(near(alone, expect(0.056_888_89)), "{alone}");
    // One neighbour of the same age: ((0 + 1) / (0 + 1 + 1))² = 1/4.
    let same = aged(exposed, &[(IVec3::X * 4, exposed)], n);
    assert!(near(same, expect(0.056_888_89 * 0.25)), "{same}");
    // An older neighbour within Manhattan distance 4 doesn't slow it.
    let older = aged(exposed, &[(IVec3::new(1, 2, 1), block(Family::Door, Stage::Oxidized, 0))], n);
    assert!(near(older, expect(0.056_888_89)), "{older}");
    // A younger one anywhere within distance 4 stops it; one at 5 doesn't.
    assert_eq!(aged(exposed, &[(IVec3::new(0, -2, 2), copper)], n), 0);
    let far = aged(exposed, &[(IVec3::new(3, 2, 0), copper)], n);
    assert!(near(far, expect(0.056_888_89)), "{far}");
    // Waxed-less torches don't count; oxidized copper and upper door halves never tick.
    let torch = aged(exposed, &[(IVec3::X, block(Family::Torch, Stage::Unaffected, 0))], n);
    assert!(near(torch, expect(0.056_888_89)), "{torch}");
    assert!(!ages(block(Family::Block, Stage::Oxidized, 0)));
    assert!(!ages(block(Family::Door, Stage::Exposed, 16)));
    assert!(!ages(block(Family::Torch, Stage::Unaffected, 0)));
    assert!(ages(block(Family::Door, Stage::Exposed, 8)));
}

#[test]
fn aging_and_scraping_keep_every_property() {
    for (b, c) in registered().filter(|(_, c)| c.family.weathers()) {
        if c.family == Family::Door {
            continue;
        }
        let mut w = world();
        w.set_block(P - IVec3::Y, Block::STONE);
        w.set_block(P + IVec3::Y, Block::STONE);
        w.edit(P, b, true);
        match c.stage.next() {
            Some(next) => {
                w.restage_copper(P, c, next, false);
                assert_eq!(w.get_block(P), Some(encode(Copper { stage: next, ..c })), "{c:?}");
                assert!(w.scrape_copper(P));
                assert_eq!(w.get_block(P), Some(b), "{c:?} scraped back");
            }
            None => {
                assert!(w.scrape_copper(P));
                assert_eq!(w.get_block(P), Some(encode(Copper { stage: Stage::Weathered, ..c })));
            }
        }
    }
    let mut w = world();
    w.set_block(P, Block::COPPER_BLOCK);
    assert!(!w.scrape_copper(P), "unaffected copper has nothing to scrape");
    w.set_block(P, block(Family::Torch, Stage::Unaffected, 0));
    assert!(!w.scrape_copper(P));
}

#[test]
fn both_door_halves_age_and_scrape_together() {
    let mut w = world();
    w.set_block(P - IVec3::Y, Block::STONE);
    let [lower, upper] = door_halves(block(Family::Door, Stage::Unaffected, 0), Facing::East).unwrap();
    let lower = right_of(lower).unwrap().toggled(Facing::East);
    let upper = upper.toggled(Facing::East);
    w.set_block(P, lower);
    w.set_block(P + IVec3::Y, upper);
    let c = state(lower).unwrap();
    w.restage_copper(P, c, Stage::Exposed, false);
    let (l, u) = (w.get_block(P).unwrap(), w.get_block(P + IVec3::Y).unwrap());
    assert_eq!(state(l).map(|c| (c.stage, c.local)), Some((Stage::Exposed, state(lower).unwrap().local)));
    assert_eq!(state(u).map(|c| (c.stage, c.local)), Some((Stage::Exposed, state(upper).unwrap().local)));
    assert!(ms::is_right_hinge(l));
    assert_eq!(l.shaped(), Some(Shaped::Door { facing: Facing::East, open: true, upper: false }));
    // Scraping the upper half scrapes the lower one too.
    assert!(w.scrape_copper(P + IVec3::Y));
    assert_eq!(w.get_block(P), Some(lower));
    assert_eq!(w.get_block(P + IVec3::Y), Some(upper));
    // Breaking the lower half drops one door; the upper half drops nothing.
    assert_eq!(upper.drop(), None);
    assert_eq!(lower.drop(), Some(Item::from_block(block(Family::Door, Stage::Unaffected, 0))));
}

#[test]
fn doors_open_by_hand_and_redstone_and_keep_their_hinge() {
    let mut w = world();
    w.set_block(P - IVec3::Y, Block::STONE);
    let [lower, upper] = door_halves(block(Family::Door, Stage::Weathered, 0), Facing::South).unwrap();
    let lower = right_of(lower).unwrap();
    w.set_block(P, lower);
    w.set_block(P + IVec3::Y, upper);
    let open = |w: &World| matches!(w.get_block(P).unwrap().shaped(), Some(Shaped::Door { open: true, .. }));
    assert_eq!(crate::session::interact::toggle_door(&mut w, P + IVec3::Y, glam::Vec3::Z), Some(true));
    assert!(open(&w) && ms::is_right_hinge(w.get_block(P).unwrap()));
    assert!(matches!(w.get_block(P + IVec3::Y).unwrap().shaped(), Some(Shaped::Door { open: true, upper: true, .. })));
    assert_eq!(crate::session::interact::toggle_door(&mut w, P, glam::Vec3::Z), Some(false));
    w.set_block(P + IVec3::X, r::REDSTONE_BLOCK);
    for _ in 0..4 {
        w.tick_redstone();
    }
    assert!(open(&w), "a signal opens it");
    assert_eq!(state(w.get_block(P).unwrap()).map(|c| c.stage), Some(Stage::Weathered));
    w.set_block(P + IVec3::X, Block::AIR);
    for _ in 0..4 {
        w.tick_redstone();
    }
    assert!(!open(&w));
    assert!(ms::is_right_hinge(w.get_block(P).unwrap()));
}

#[test]
fn aging_keeps_a_powered_door_that_was_closed_by_hand_closed() {
    let mut w = world();
    w.set_block(P - IVec3::Y, Block::STONE);
    let [lower, upper] = door_halves(block(Family::Door, Stage::Unaffected, 0), Facing::South).unwrap();
    w.set_block(P, lower);
    w.set_block(P + IVec3::Y, upper);
    let open = |w: &World| matches!(w.get_block(P).unwrap().shaped(), Some(Shaped::Door { open: true, .. }));
    w.set_block(P + IVec3::X, r::REDSTONE_BLOCK);
    for _ in 0..4 {
        w.tick_redstone();
    }
    assert!(open(&w));
    assert_eq!(crate::session::interact::toggle_door(&mut w, P, glam::Vec3::Z), Some(false));
    // Java copies POWERED across `changeOverTime`, so no new rising edge.
    w.restage_copper(P, state(w.get_block(P).unwrap()).unwrap(), Stage::Exposed, false);
    assert!(w.scrape_copper(P));
    w.set_block(P + IVec3::NEG_X, Block::STONE);
    for _ in 0..4 {
        w.tick_redstone();
    }
    assert!(!open(&w), "aging and scraping are not a rising edge");
}

#[test]
fn uniform_copper_chunks_still_age() {
    let mut w = World::new_headless(Arc::new(Generator::new(7)), Default::default(), 2);
    let copper = block(Family::Block, Stage::Unaffected, 0);
    w.insert_chunk(IVec3::new(0, 4, 0), Arc::new(ChunkData::Uniform(copper)), false);
    let at = DVec3::new(16.0, 140.0, 16.0);
    for _ in 0..400 {
        w.tick_random_rules(1.0, at, 3, false);
    }
    let aged = (0..32 * 32 * 32)
        .map(|i| IVec3::new(i % 32, 128 + i / 32 % 32, i / 1024))
        .filter(|&q| w.get_block(q) != Some(copper))
        .count();
    assert!(aged > 0, "random ticks reach a uniform copper chunk");
}

#[test]
fn trapdoors_act_as_oak_trapdoors_on_their_own_copper() {
    let mut w = world();
    for s in Stage::ALL {
        let trapdoor = block(Family::Trapdoor, s, 0);
        assert!(matches!(r::component(trapdoor), Some(Component::Trapdoor { iron: false, open: false, .. })));
        assert_eq!(r::base(trapdoor), Some(trapdoor));
        let wet = trapdoor.with_water(true);
        assert!(matches!(r::component(wet), Some(Component::Trapdoor { iron: false, .. })));
        w.set_block(P, wet);
        assert!(w.redstone_usable(P));
        assert!(w.use_redstone(P));
        let opened = w.get_block(P).unwrap();
        assert!(matches!(r::component(opened), Some(Component::Trapdoor { open: true, .. })));
        assert!(opened.is_waterlogged(), "toggling keeps its water");
        assert_eq!(state(opened).map(|c| c.stage), Some(s));
        assert_eq!(opened.drop(), Some(Item::from_block(trapdoor)));
    }
    // Redstone opens a closed one.
    let trapdoor = block(Family::Trapdoor, Stage::Oxidized, 0);
    w.set_block(P, trapdoor);
    w.set_block(P + IVec3::X, r::REDSTONE_BLOCK);
    for _ in 0..4 {
        w.tick_redstone();
    }
    let b = w.get_block(P).unwrap();
    assert!(matches!(r::component(b), Some(Component::Trapdoor { open: true, .. })));
    assert_eq!(state(b).map(|c| c.stage), Some(Stage::Oxidized));
    // Placement takes the clicked face like the oak trapdoor.
    let placed = r::placed(trapdoor, IVec3::NEG_Y, Facing::North);
    assert!(matches!(r::component(placed), Some(Component::Trapdoor { top: true, .. })));
    assert_eq!(state(placed).map(|c| c.stage), Some(Stage::Oxidized));
}

#[test]
fn bulbs_toggle_on_each_rising_edge_and_feed_comparators() {
    let mut w = world();
    let bulb0 = bulb_block(Stage::Exposed, false, false);
    w.set_block(P, bulb0);
    let tick = |w: &mut World| {
        for _ in 0..4 {
            w.tick_redstone();
        }
    };
    tick(&mut w);
    assert_eq!(w.get_block(P), Some(bulb0), "no signal, no change");
    assert_eq!(w.container_signal(P), Some(0));
    w.set_block(P + IVec3::Z, r::REDSTONE_BLOCK);
    tick(&mut w);
    assert_eq!(w.get_block(P), Some(bulb_block(Stage::Exposed, true, true)));
    assert_eq!(w.container_signal(P), Some(15));
    assert_eq!(w.get_block(P).unwrap().emission(), 12);
    // Losing the signal keeps it lit.
    w.set_block(P + IVec3::Z, Block::AIR);
    tick(&mut w);
    assert_eq!(w.get_block(P), Some(bulb_block(Stage::Exposed, true, false)));
    // The next rising edge turns it off.
    w.set_block(P - IVec3::Z, r::REDSTONE_BLOCK);
    tick(&mut w);
    assert_eq!(w.get_block(P), Some(bulb_block(Stage::Exposed, false, true)));
    assert_eq!(w.get_block(P).unwrap().emission(), 0);
    // Aging keeps lit and powered.
    let c = state(w.get_block(P).unwrap()).unwrap();
    w.restage_copper(P, c, Stage::Weathered, false);
    tick(&mut w);
    assert_eq!(w.get_block(P), Some(bulb_block(Stage::Weathered, false, true)));
}

#[test]
fn lanterns_and_torches_break_when_their_support_goes() {
    let mut w = world();
    // A hanging lantern under a ceiling.
    let ceiling = P + IVec3::Y;
    w.set_block(ceiling, Block::STONE);
    let hanging = block(Family::Lantern, Stage::Weathered, 3);
    w.set_block(P, Block::WATER);
    w.edit(P, hanging, true);
    assert!(hanging.is_waterlogged());
    w.drops.clear();
    w.set_block(ceiling, Block::AIR);
    assert_eq!(w.get_block(P), Some(Block::WATER), "it leaves its water");
    assert_eq!(w.drops.len(), 1);
    // A standing torch and lantern need their floor's centre.
    let torch = block(Family::Torch, Stage::Unaffected, 0);
    assert!(torch.can_stay_on(Block::STONE) && !torch.can_stay_on(Block::AIR));
    let lantern = block(Family::Lantern, Stage::Unaffected, 0);
    assert!(lantern.can_stay_on(Block::COBBLESTONE_WALL) && !lantern.can_stay_on(Block::AIR));
    w.set_block(P - IVec3::Y, Block::STONE);
    w.set_block(P, torch);
    w.set_block(P - IVec3::Y, Block::AIR);
    assert_eq!(w.get_block(P), Some(Block::AIR));
    // A wall torch facing east hangs on the block to its west.
    let wall = P - IVec3::X;
    w.set_block(wall, Block::STONE);
    let wall_torch = block(Family::Torch, Stage::Unaffected, 1 + Facing::East as u16);
    w.set_block(P, wall_torch);
    w.set_block(wall + IVec3::Y, Block::AIR);
    assert_eq!(w.get_block(P), Some(wall_torch), "another neighbour changing leaves it");
    w.drops.clear();
    w.set_block(wall, Block::AIR);
    assert_eq!(w.get_block(P), Some(Block::AIR));
    assert_eq!(w.drops.iter().map(|(_, s)| s.item).collect::<Vec<_>>(), [Item::from_block(torch)]);
}

#[test]
fn shapes_follow_java_models() {
    use crate::world::shape::{self, Box16};
    let none = |_| Block::AIR;
    let rod = |dir: u16| block(Family::Rod, Stage::Exposed, dir);
    // Up: a 4×16×4 column to collide with and select; the drawn rod and tip.
    assert_eq!(shape::collision(rod(0), none, Block::AIR).as_slice(), &[b16([6, 0, 6], [10, 16, 10])]);
    assert_eq!(shape::outline(rod(0), none, Block::AIR).as_slice(), &[b16([6, 0, 6], [10, 16, 10])]);
    assert_eq!(
        shape::shape(rod(0), none, Block::AIR).as_slice(),
        &[b16([7, 0, 7], [9, 12, 9]), b16([6, 12, 6], [10, 16, 10])]
    );
    // Down puts the tip at the bottom; east lays the column along x with the tip at +x.
    assert_eq!(shape::shape(rod(1), none, Block::AIR).as_slice()[1], b16([6, 0, 6], [10, 4, 10]));
    assert_eq!(
        shape::collision(rod(2 + Facing::East as u16), none, Block::AIR).as_slice(),
        &[b16([0, 6, 6], [16, 10, 10])]
    );
    assert_eq!(
        shape::shape(rod(2 + Facing::East as u16), none, Block::AIR).as_slice()[1],
        b16([12, 6, 6], [16, 10, 10])
    );
    assert_eq!(
        shape::shape(rod(2 + Facing::North as u16), none, Block::AIR).as_slice()[1],
        b16([6, 6, 0], [10, 10, 4])
    );
    assert_eq!(
        shape::shape(rod(2 + Facing::South as u16), none, Block::AIR).as_slice()[1],
        b16([6, 6, 12], [10, 10, 16])
    );
    assert_eq!(shape::shape(rod(2 + Facing::West as u16), none, Block::AIR).as_slice()[1], b16([0, 6, 6], [4, 10, 10]));
    // Chains along each axis; bars join iron bars.
    for (axis, bx) in
        [(0, b16([6, 0, 6], [9, 16, 9])), (1, b16([0, 6, 6], [16, 9, 9])), (2, b16([6, 6, 0], [9, 9, 16]))]
    {
        assert_eq!(shape::shape(block(Family::Chain, Stage::Oxidized, axis), none, Block::AIR).as_slice(), &[bx]);
    }
    let bars = block(Family::Bars, Stage::Unaffected, 0);
    let joined = shape::shape(bars, |f| if f == Facing::East { Block::IRON_BARS } else { Block::AIR }, Block::AIR);
    assert_eq!(joined.as_slice().len(), 2);
    assert!(shape::pane_connects(bars), "iron bars join copper bars");
    // A hanging lantern sits a pixel higher; wall torches don't collide.
    let lantern = shape::shape(block(Family::Lantern, Stage::Unaffected, 2), none, Block::AIR);
    assert_eq!(lantern.as_slice()[0], b16([5, 1, 5], [11, 8, 11]));
    let wall = block(Family::Torch, Stage::Unaffected, 1);
    assert!(shape::collision(wall, none, Block::AIR).is_empty());
    assert_eq!(shape::outline(wall, none, Block::AIR).as_slice(), &[b16([5, 3, 0], [11, 13, 5])]);
    // A copper chest collides like a chest.
    let chest = block(Family::Chest, Stage::Unaffected, 0);
    let cube = shape::collision_or_cube(chest, none, Block::AIR, shape::Above::NONE);
    assert_eq!(cube.as_slice(), &[Box16 { min: [1, 0, 1], max: [15, 14, 15] }]);
}

#[test]
fn chests_keep_their_contents_as_they_age() {
    let mut w = world();
    let chest = block(Family::Chest, Stage::Unaffected, Facing::West as u16);
    w.set_block(P, chest);
    assert!(crate::world::chest::is_chest(chest));
    w.chest_mut(P).unwrap().slots[3] = Some(Stack::new(Item::DIAMOND, 5));
    w.restage_copper(P, state(chest).unwrap(), Stage::Exposed, false);
    assert_eq!(w.get_block(P), Some(block(Family::Chest, Stage::Exposed, Facing::West as u16)));
    assert_eq!(w.chest(P).unwrap().slots[3], Some(Stack::new(Item::DIAMOND, 5)));
    assert!(w.scrape_copper(P));
    assert_eq!(w.chest(P).unwrap().slots[3], Some(Stack::new(Item::DIAMOND, 5)));
    w.drops.clear();
    w.set_block(P, Block::AIR);
    assert!(w.drops.iter().any(|(_, s)| *s == Stack::new(Item::DIAMOND, 5)), "breaking it spills the contents");
}

#[test]
fn every_state_saves_and_reloads() {
    let mut w = world();
    let states: Vec<Block> = registered().map(|(b, _)| b).collect();
    // Apart, so no stairs reshape against another on load.
    let at = |i: usize| IVec3::new((i % 15) as i32 * 2, 140 + (i / 225) as i32 * 2, (i / 15 % 15) as i32 * 2);
    for (i, &b) in states.iter().enumerate() {
        w.edit_exact(at(i), b, false);
    }
    let dir = std::env::temp_dir().join(format!("voxelcraft-copper-{}", std::process::id()));
    let storage = Storage::new(&dir);
    storage.save(&LevelInfo { seed: 7, player: None, props: Default::default() }, &w.modified_chunks()).unwrap();
    let mut loaded = World::new_headless(Arc::new(Generator::new(7)), Default::default(), 2);
    for (pos, data) in storage.load_chunks().unwrap() {
        loaded.insert_chunk(pos, data, true);
    }
    for (i, &b) in states.iter().enumerate() {
        assert_eq!(loaded.get_block(at(i)), Some(b), "{:?}", state(b));
    }
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn recipes_match_the_java_data() {
    let craft = |size: usize, cells: &[(usize, Item)]| {
        let mut g = Grid::new(size);
        for &(i, item) in cells {
            g.cells[i] = Some(Stack::new(item, 1));
        }
        g.result()
    };
    let item = |f: Family, s: Stage| Item::from_block(block(f, s, 0));
    for s in Stage::ALL {
        let copper = item(Family::Block, s);
        let cut = item(Family::Cut, s);
        assert_eq!(craft(2, &[(0, copper), (1, copper), (2, copper), (3, copper)]), Some(Stack::new(cut, 4)));
        assert_eq!(craft(3, &[(6, cut), (7, cut), (8, cut)]), Some(Stack::new(item(Family::Slab, s), 6)));
        assert_eq!(
            craft(3, &[(0, cut), (3, cut), (4, cut), (6, cut), (7, cut), (8, cut)]),
            Some(Stack::new(item(Family::Stairs, s), 4))
        );
        let slab = item(Family::Slab, s);
        assert_eq!(craft(2, &[(0, slab), (2, slab)]), Some(Stack::new(item(Family::Chiseled, s), 1)));
        assert_eq!(
            craft(3, &[(1, copper), (3, copper), (5, copper), (7, copper)]),
            Some(Stack::new(item(Family::Grate, s), 4))
        );
        assert_eq!(
            craft(3, &[(1, copper), (3, copper), (4, Item::BLAZE_ROD), (5, copper), (7, Item::REDSTONE)]),
            Some(Stack::new(item(Family::Bulb, s), 4))
        );
    }
    let u = Stage::Unaffected;
    let ingot = Item::COPPER_INGOT;
    let nugget = Item::COPPER_NUGGET;
    assert_eq!(craft(3, &(0..6).map(|i| (i, ingot)).collect::<Vec<_>>()), Some(Stack::new(item(Family::Bars, u), 16)));
    assert_eq!(
        craft(3, &[(0, ingot), (1, ingot), (3, ingot), (4, ingot), (6, ingot), (7, ingot)]),
        Some(Stack::new(item(Family::Door, u), 3))
    );
    assert_eq!(
        craft(2, &[(0, ingot), (1, ingot), (2, ingot), (3, ingot)]),
        Some(Stack::new(item(Family::Trapdoor, u), 1))
    );
    assert_eq!(craft(3, &[(1, nugget), (4, ingot), (7, nugget)]), Some(Stack::new(item(Family::Chain, u), 1)));
    assert_eq!(craft(3, &[(1, ingot), (4, ingot), (7, ingot)]), Some(Stack::new(item(Family::Rod, u), 1)));
    for fuel in [Item::COAL, Item::CHARCOAL] {
        assert_eq!(craft(3, &[(1, nugget), (4, fuel), (7, Item::STICK)]), Some(Stack::new(item(Family::Torch, u), 4)));
    }
    let torch = item(Family::Torch, u);
    let around = |centre: Item, ring: Item| (0..9).map(|i| (i, if i == 4 { centre } else { ring })).collect::<Vec<_>>();
    assert_eq!(craft(3, &around(torch, nugget)), Some(Stack::new(item(Family::Lantern, u), 1)));
    assert_eq!(craft(3, &around(Item::from_block(Block::CHEST), ingot)), Some(Stack::new(item(Family::Chest, u), 1)));
    // Only unaffected doors and bars are crafted; later stages come from aging.
    let exposed = item(Family::Block, Stage::Exposed);
    assert_eq!(craft(3, &(0..9).map(|i| (i, exposed)).collect::<Vec<_>>()), None);
    // Stonecutting data: eight recipes per stage, as in Java's files.
    let cutting = crate::crafting::copper_stonecutting();
    assert_eq!(cutting.len(), 32);
    assert!(cutting.contains(&(item(Family::Block, Stage::Oxidized), item(Family::Slab, Stage::Oxidized), 8)));
    assert!(cutting.contains(&(item(Family::Cut, Stage::Exposed), item(Family::Stairs, Stage::Exposed), 1)));
}
