//! Dyed leather (ITEM-13-DYED-LEATHER) against `dyed_items.json`: the
//! `#cauldron_can_remove_dye` and `#dyes` tags, the six `crafting_dye`
//! recipes, `DyedItemColor.applyDyes` on every reference vector for every
//! dyeable item, the undyed colours of the item models and the leather
//! equipment asset, washing in water cauldrons at every level through the
//! shared use paths, and dyed stacks through saves.
use glam::{DVec3, IVec3};
use serde_json::Value;
use std::{collections::BTreeSet, sync::Arc};
use voxelcraft::{
    agent::{Agent, Command},
    color::DyeColor,
    crafting::{self, Grid, Layout},
    enchant::Enchantment,
    entity::{
        Entities, MobKind, PlayerId,
        armor::{self, ArmorKind, Worn},
        decor, mounts,
    },
    inventory::{Inventory, Stack, components as c, stack_from_str, stack_to_string},
    item::{ArmorMaterial, ArmorPiece, Item},
    rules::GameMode,
    session::{
        PlayerSession,
        interact::{UseContext, Used},
    },
    world::{
        World,
        block::Block,
        chunk::ChunkData,
        terrain::Generator,
        utilities::{Contents, cauldron, cauldron_of, interact::Effect},
    },
};

fn fixture() -> Value {
    serde_json::from_str(include_str!("fixtures/java/26.3/dyed_items.json")).unwrap()
}

fn registry() -> Value {
    serde_json::from_str(include_str!("fixtures/java/26.3/registry_names.json")).unwrap()
}

/// A Java item id as the game's item.
fn item(id: &str) -> Item {
    let english = registry()["items"][id].as_str().unwrap_or_else(|| panic!("{id} is not a Java item")).to_owned();
    voxelcraft::java_registry::item(id, &english).unwrap_or_else(|| panic!("{id} is not registered"))
}

fn rgb(hex: &str) -> [u8; 3] {
    let v = u32::from_str_radix(hex.trim_start_matches('#'), 16).unwrap();
    [(v >> 16) as u8, (v >> 8) as u8, v as u8]
}

fn dye(name: &str) -> DyeColor {
    let index = fixture()["dyes"].as_array().unwrap().iter().position(|d| d == &format!("{name}_dye")).unwrap();
    DyeColor::ALL[index]
}

fn dyeable() -> Vec<Item> {
    fixture()["dyeable"].as_array().unwrap().iter().map(|v| item(v.as_str().unwrap())).collect()
}

fn leather() -> Vec<Item> {
    dyeable().into_iter().filter(|&i| i != Item::WOLF_ARMOR).collect()
}

/// A worn, named, enchanted piece, so the recipe and the wash can be seen
/// keeping every other component.
fn worn_piece(item: Item) -> Stack {
    let mut s = Stack::new(item, 1).with_name("Dyed").unwrap();
    s.damage = 3;
    s.repair_cost = 1;
    s.enchants = s.enchants.with(Enchantment::Unbreaking, 2);
    s
}

/// The tags and recipes: everything Java has, the game has, and nothing more.
#[test]
fn dyeable_items_dyes_and_dye_recipes_match_java() {
    let f = fixture();
    let ours: BTreeSet<u16> = Item::all_items().filter(|&i| c::dyeable(i)).map(|i| i.0).collect();
    let java: BTreeSet<u16> = dyeable().iter().map(|i| i.0).collect();
    assert_eq!(ours, java, "#cauldron_can_remove_dye");
    // `#dyes`, in `DyeColor` order, with each dye's `textureDiffuseColor`.
    let dyes = f["dyes"].as_array().unwrap();
    assert_eq!(dyes.len(), DyeColor::ALL.len());
    for (name, colour) in dyes.iter().zip(DyeColor::ALL) {
        assert_eq!(item(name.as_str().unwrap()).dye_color(), Some(colour), "{name}");
    }
    assert_eq!(Item::all_items().filter(|i| i.dye_color().is_some()).count(), dyes.len());
    for (name, hex) in f["dye_colors"].as_object().unwrap() {
        assert_eq!(dye(name).rgb(), rgb(hex.as_str().unwrap()), "{name}");
    }
    // `crafting_dye`: each target dyes into itself with any `#dyes`.
    let mut java_targets = BTreeSet::new();
    for (id, r) in f["recipes"].as_object().unwrap() {
        assert_eq!((r["dye"].as_str(), r["target"].as_str()), (Some("#dyes"), r["result"].as_str()), "{id}");
        let target = item(r["target"].as_str().unwrap());
        assert!(c::dyeable(target), "{id}");
        java_targets.insert(target.0);
    }
    assert_eq!(java_targets, java, "a dye recipe for every dyeable item");
    let dye_items: BTreeSet<u16> = DyeColor::ALL.iter().map(|d| d.dye().0).collect();
    let mut our_targets = BTreeSet::new();
    for r in crafting::recipes() {
        if let Layout::Shapeless(list) = r.layout()
            && list.len() == 2
            && list[1].iter().map(|i| i.0).collect::<BTreeSet<_>>() == dye_items
        {
            assert_eq!(list[0], &[r.result.item][..]);
            assert!(our_targets.insert(r.result.item.0), "{} listed twice", r.result.item.name());
            // The recipe book shows `DyedSlotDemo`'s first case: white dye.
            assert_eq!(c::dyed_color(&r.result), Some(DyeColor::White.rgb()));
        }
    }
    assert_eq!(our_targets, java_targets);
}

/// `DyedItemColor.applyDyes` on every reference vector, crafted in a grid
/// for all six dyeable items: the colour, one item, every other component
/// kept (`TransmuteRecipe.createWithOriginalComponents`).
#[test]
fn every_reference_mix_crafts_on_every_dyeable_item() {
    let f = fixture();
    let vectors = f["vectors"].as_array().unwrap();
    assert!(vectors.len() > 1000);
    let mut checked = 0;
    for v in vectors {
        let current = v[0].as_str().map(rgb);
        let dyes: Vec<DyeColor> = v[1].as_array().unwrap().iter().map(|d| dye(d.as_str().unwrap())).collect();
        let want = rgb(v[2].as_str().unwrap());
        assert_eq!(c::mix_dyes(current, &dyes), Some(want), "{v}");
        for (k, &target) in dyeable().iter().enumerate() {
            let start = c::with_dyed_color(worn_piece(target), current);
            // The inventory grid when it fits, else the crafting table; the
            // target in a different cell each time.
            let size = if dyes.len() < 4 && k % 2 == 0 { 2 } else { 3 };
            let mut g = Grid::new(size);
            let at = (checked + k) % (dyes.len() + 1);
            let mut cells = dyes.iter().map(|d| Stack::new(d.dye(), 1));
            for (i, cell) in g.cells.iter_mut().take(dyes.len() + 1).enumerate() {
                *cell = Some(if i == at { start } else { cells.next().unwrap() });
            }
            let got = g.result().unwrap_or_else(|| panic!("{} with {v}", target.name()));
            assert_eq!(got, c::with_dyed_color(start, Some(want)), "{} with {v}", target.name());
            checked += 1;
        }
    }
    assert_eq!(checked, vectors.len() * 6);
}

/// `DyeRecipe.matches`: exactly one target, at least one dye, nothing else;
/// crafting takes one of each ingredient.
#[test]
fn dye_grids_need_one_target_and_only_dyes_and_consume_one_each() {
    let helmet = Item::armor(ArmorPiece::Helmet, ArmorMaterial::Leather);
    let boots = Item::armor(ArmorPiece::Boots, ArmorMaterial::Leather);
    let grid = |stacks: &[Stack]| {
        let mut g = Grid::new(3);
        for (cell, &s) in g.cells.iter_mut().zip(stacks) {
            *cell = Some(s);
        }
        g
    };
    let red = Stack::new(DyeColor::Red.dye(), 4);
    assert_eq!(grid(&[Stack::new(helmet, 1)]).result(), None, "no dye");
    assert_eq!(grid(&[red]).result(), None, "no target");
    assert_eq!(grid(&[Stack::new(helmet, 1), Stack::new(boots, 1), red]).result(), None, "two targets");
    assert_eq!(grid(&[Stack::new(helmet, 1), red, Stack::new(Item::STICK, 1)]).result(), None, "a non-dye");
    assert_eq!(grid(&[Stack::new(helmet, 1), red, Stack::new(Block::WOOL, 1)]).result(), None, "wool is not in #dyes");
    let mut g = grid(&[Stack::new(helmet, 1), red, Stack::new(DyeColor::Yellow.dye(), 1)]);
    let made = g.result().unwrap();
    assert_eq!(c::dyed_color(&made), c::mix_dyes(None, &[DyeColor::Red, DyeColor::Yellow]));
    g.consume();
    assert_eq!(g.cells[0], None);
    assert_eq!(g.cells[1].map(|s| s.count), Some(3));
    assert_eq!(g.cells[2], None);
}

/// The CLI crafts with the shared grid: a leather cap and red dye make a
/// red cap, re-dyeing mixes with the colour it has.
#[test]
fn the_cli_dyes_and_re_dyes_leather_through_the_shared_grid() {
    let mut world = World::new_remote(Arc::new(Generator::new(7)), 2);
    world.receive_chunk(IVec3::new(0, 4, 0), Arc::new(ChunkData::Uniform(Block::AIR)));
    let mut entities = Entities::new(7);
    let mut a = Agent::new(DVec3::new(4.5, 145., 4.5));
    let helmet = Item::armor(ArmorPiece::Helmet, ArmorMaterial::Leather);
    a.session.inventory.slots[0] = Some(worn_piece(helmet));
    a.session.inventory.slots[1] = Some(Stack::new(DyeColor::Red.dye(), 2));
    a.execute(Command::Craft(helmet), &mut world, &mut entities, &[]).unwrap();
    let red = a.session.inventory.find(helmet).and_then(|i| a.session.inventory.get(i)).unwrap();
    assert_eq!(red, c::with_dyed_color(worn_piece(helmet), Some(DyeColor::Red.rgb())));
    assert_eq!(
        a.session.inventory.find(DyeColor::Red.dye()).map(|i| a.session.inventory.get(i).unwrap().count),
        Some(1)
    );
    a.execute(Command::Craft(helmet), &mut world, &mut entities, &[]).unwrap();
    let twice = a.session.inventory.find(helmet).and_then(|i| a.session.inventory.get(i)).unwrap();
    assert_eq!(c::dyed_color(&twice), Some(DyeColor::Red.rgb()), "red on red stays red");
    assert!(a.session.inventory.find(DyeColor::Red.dye()).is_none());
    assert!(a.execute(Command::Craft(helmet), &mut world, &mut entities, &[]).is_err(), "no dye and no leather");
}

/// The `minecraft:dye` tint defaults of the item models and the dyeable
/// layers of `equipment/leather.json`, as the icon, the worn layers and an
/// armor stand draw them.
#[test]
fn undyed_and_dyed_colours_follow_the_models_and_the_leather_equipment_asset() {
    let f = fixture();
    let leather_color = f["leather_color"].as_i64().unwrap();
    assert_eq!(leather_color, -6265536);
    let default = rgb(f["leather_color_rgb"].as_str().unwrap());
    assert_eq!(c::LEATHER_COLOR, default);
    assert_eq!(armor::LEATHER_DYE, default);
    for (id, model) in f["item_models"].as_object().unwrap() {
        let i = item(id);
        let tints: Vec<i64> = model["dye_defaults"].as_array().unwrap().iter().map(|v| v.as_i64().unwrap()).collect();
        if model["undyed_untinted"].as_bool().unwrap() {
            assert_eq!(c::undyed_tint(i), None, "{id}");
            assert_eq!(c::dye_tint_bytes(Stack::new(i, 1)), [255; 4], "{id}");
        } else {
            assert_eq!(tints, [leather_color], "{id}");
            assert_eq!(c::undyed_tint(i), Some(default), "{id}");
            assert_eq!(c::dye_tint_bytes(Stack::new(i, 1)), [default[0], default[1], default[2], 255], "{id}");
        }
        let lime = c::apply_dyes(Stack::new(i, 1), &[DyeColor::Lime]).unwrap();
        let [r, g, b] = DyeColor::Lime.rgb();
        assert_eq!(c::dye_tint_bytes(lime), [r, g, b, 255], "{id}: a dyed icon is tinted with its colour");
    }
    assert_eq!(c::dye_tint_bytes(Stack::new(Item::DIAMOND, 1)), [255; 4]);
    // The humanoid layers: the first is dyeable (undyed #A06540), the
    // overlay is not.
    for kind in ["humanoid", "humanoid_leggings", "horse_body"] {
        let layers = f["leather_equipment_layers"][kind].as_array().unwrap();
        assert_eq!(layers.len(), 2);
        assert_eq!(layers[0]["color_when_undyed"].as_i64(), Some(leather_color), "{kind}");
        assert_eq!(layers[1]["dyeable"].as_bool(), Some(false), "{kind}");
    }
    for &piece in &ArmorPiece::ALL {
        let plain = Stack::new(Item::armor(piece, ArmorMaterial::Leather), 1);
        let dyed = c::apply_dyes(plain, &[DyeColor::Cyan, DyeColor::Purple]).unwrap();
        let want = c::mix_dyes(None, &[DyeColor::Cyan, DyeColor::Purple]).unwrap();
        let mut worn = [None; 4];
        worn[piece as usize] = Some(dyed);
        let w = armor::from_stacks(&worn)[piece as usize].unwrap();
        assert_eq!(w, Worn { kind: ArmorKind::Leather, glint: false, dye: Some(want) });
        assert_eq!(armor::tint(&w, false)[..3], want);
        assert_eq!(armor::tint(&w, true)[..3], [255; 3], "the overlay is not dyed");
        worn[piece as usize] = Some(plain);
        let w = armor::from_stacks(&worn)[piece as usize].unwrap();
        assert_eq!(armor::tint(&w, false)[..3], default);
        // An armor stand's shell for the piece.
        assert_eq!(decor::stand_shell_colour(&dyed), want);
        assert_eq!(decor::stand_shell_colour(&plain), default);
    }
    // Iron is never tinted, whatever its stack holds.
    let iron = Stack::new(Item::armor(ArmorPiece::Helmet, ArmorMaterial::Iron), 1);
    let w = armor::from_stacks(&[Some(iron), None, None, None])[0].unwrap();
    assert_eq!((w.dye, armor::tint(&w, false)[..3].to_vec()), (None, vec![255; 3]));
}

/// The block the scene's player looks at, level with the eye, along +x.
const AT: IVec3 = IVec3::new(7, 146, 4);

fn scene(contents: Contents, held: Stack) -> (World, PlayerSession, Entities) {
    let mut world = World::new_remote(Arc::new(Generator::new(7)), 2);
    world.receive_chunk(IVec3::new(0, 4, 0), Arc::new(ChunkData::Uniform(Block::AIR)));
    world.set_block(IVec3::new(4, 144, 4), Block::STONE);
    world.set_block(AT, cauldron(contents));
    let mut session = PlayerSession::new(PlayerId(6), DVec3::new(4.5, 145., 4.5));
    session.player.on_ground = true;
    session.inventory.slots[0] = Some(held);
    (world, session, Entities::new(7))
}

/// What the shared use chain did; `None` also when it refused (an item
/// that is neither washed nor placeable reports "cannot be placed").
fn use_held(s: &mut PlayerSession, w: &mut World, e: &mut Entities, sneaking: bool) -> Option<Used> {
    s.use_held(0, w, e, &UseContext { sneaking, others: &[] }).ok().flatten().map(|u| u.used)
}

/// `CauldronInteractions.dyedItemIteration` for every dyeable item at every
/// water level, in Survival and Creative: the colour comes off, the rest of
/// the stack stays, the water drops a level (level 1 leaves an empty
/// cauldron), and no sound plays (`Effect::Washed`).
#[test]
fn water_cauldrons_wash_every_dyeable_item_at_every_level() {
    let f = fixture();
    assert_eq!(f["cauldron_wash"]["levels_lowered"], 1);
    assert!(f["cauldron_wash"]["sound"].is_null());
    for target in dyeable() {
        let clean = worn_piece(target);
        let dyed = c::apply_dyes(clean, &[DyeColor::Blue]).unwrap();
        for level in 1..=3u8 {
            for mode in [GameMode::Survival, GameMode::Creative] {
                let (mut w, mut s, mut e) = scene(Contents::Water(level), dyed);
                s.set_mode(mode);
                assert_eq!(s.block_target(&w).map(|t| t.0), Some(AT));
                assert!(s.block_takes_use(0, &w, false), "{} at level {level}", target.name());
                assert_eq!(
                    use_held(&mut s, &mut w, &mut e, false),
                    Some(Used::Utility { cell: AT, effect: Effect::Washed }),
                    "{} at level {level}",
                    target.name()
                );
                let after = if level == 1 { Contents::Empty } else { Contents::Water(level - 1) };
                assert_eq!(w.get_block(AT).and_then(cauldron_of), Some(after));
                assert_eq!(s.inventory.get(0), Some(clean), "{}", target.name());
                // Washed, it passes over the cauldron.
                assert!(!s.block_takes_use(0, &w, false));
                if let Some(Used::Utility { effect, .. }) = use_held(&mut s, &mut w, &mut e, false) {
                    assert_ne!(effect, Effect::Washed);
                }
                assert_eq!(w.get_block(AT).and_then(cauldron_of), Some(after));
            }
        }
        // Lava, powder snow and an empty cauldron do not wash; neither
        // does sneaking (block use is skipped with an item in hand).
        for contents in [Contents::Empty, Contents::Lava, Contents::PowderSnow(3)] {
            let (mut w, mut s, mut e) = scene(contents, dyed);
            assert!(!s.block_takes_use(0, &w, false));
            let _ = use_held(&mut s, &mut w, &mut e, false);
            assert_eq!(c::dyed_color(&s.inventory.get(0).unwrap()), Some(DyeColor::Blue.rgb()));
            assert_eq!(w.get_block(AT).and_then(cauldron_of), Some(contents));
        }
        let (mut w, mut s, mut e) = scene(Contents::Water(3), dyed);
        assert!(!s.block_takes_use(0, &w, true));
        let _ = use_held(&mut s, &mut w, &mut e, true);
        assert_eq!(w.get_block(AT).and_then(cauldron_of), Some(Contents::Water(3)));
        assert!(c::dyed_color(&s.inventory.get(0).unwrap()).is_some());
    }
}

/// Java returns `TRY_WITH_EMPTY_HAND` for an undyed item, so the item's own
/// use follows: undyed leather is put on at a water cauldron, dyed leather
/// is washed instead, for the shared equip path and the CLI.
#[test]
fn undyed_leather_is_put_on_at_a_water_cauldron_and_dyed_leather_is_washed() {
    for &piece in &ArmorPiece::ALL {
        let plain = Stack::new(Item::armor(piece, ArmorMaterial::Leather), 1);
        let dyed = c::apply_dyes(plain, &[DyeColor::Orange]).unwrap();
        // The equip path every adapter shares.
        let (w, mut s, _) = scene(Contents::Water(2), dyed);
        assert!(!voxelcraft::survival_items::equip_armor(&s.player, &mut s.inventory, 0, GameMode::Survival, &w));
        assert_eq!(s.inventory.armor[piece as usize], None);
        let (w, mut s, _) = scene(Contents::Water(2), plain);
        assert!(voxelcraft::survival_items::equip_armor(&s.player, &mut s.inventory, 0, GameMode::Survival, &w));
        assert_eq!(s.inventory.armor[piece as usize], Some(plain));
        // The CLI's place command.
        for (held, washed) in [(dyed, true), (plain, false)] {
            let (mut w, s, mut e) = scene(Contents::Water(2), held);
            let mut cli = Agent::new(s.player.pos);
            cli.session.player.yaw = s.player.yaw;
            cli.session.player.pitch = s.player.pitch;
            cli.session.player.on_ground = true;
            cli.session.inventory.slots[cli.selected] = Some(held);
            cli.execute(Command::Place, &mut w, &mut e, &[]).unwrap();
            if washed {
                assert_eq!(cli.session.inventory.get(cli.selected), Some(plain));
                assert_eq!(cli.session.inventory.armor[piece as usize], None);
                assert_eq!(w.get_block(AT).and_then(cauldron_of), Some(Contents::Water(1)));
            } else {
                assert_eq!(cli.session.inventory.armor[piece as usize], Some(plain));
                assert_eq!(w.get_block(AT).and_then(cauldron_of), Some(Contents::Water(2)));
            }
        }
    }
}

/// Putting dyed leather horse armor on a horse keeps the stack (it used to
/// equip a fresh copy of the item), and the horse's slots save the colour.
#[test]
fn a_horse_wears_dyed_leather_horse_armor_through_save_and_reload() {
    let mut e = Entities::new(1);
    e.spawn(MobKind::Horse, DVec3::new(5., 140., 5.));
    let m = e.mobs.last_mut().unwrap();
    (m.baby, m.age) = (false, 0);
    m.mount.as_mut().unwrap().tame = true;
    let armor = c::apply_dyes(worn_piece(Item::LEATHER_HORSE_ARMOR), &[DyeColor::Pink]).unwrap();
    let mut inv = Inventory::default();
    inv.slots[0] = Some(armor);
    let eye = DVec3::new(5., 140.8, 2.);
    assert!(e.use_mount(eye, DVec3::Z, 5., PlayerId::HOST, &mut inv, 0, false));
    assert_eq!(inv.get(0), None);
    let state = e.mobs.last().unwrap().mount.as_ref().unwrap();
    assert_eq!(state.slots[1], Some(armor));
    let reloaded = mounts::State::load(MobKind::Horse, &state.save()).unwrap();
    assert_eq!(reloaded.slots[1], Some(armor));
}

/// Dyed leather in the inventory, worn, on an armor stand and as a stack
/// string, saved and loaded; older strings (no dye) load undyed.
#[test]
fn dyed_leather_survives_save_and_reload_and_old_saves_load_undyed() {
    let pieces: Vec<Stack> = leather()
        .into_iter()
        .zip([DyeColor::Red, DyeColor::Green, DyeColor::Blue, DyeColor::Black, DyeColor::White])
        .map(|(i, d)| c::apply_dyes(worn_piece(i), &[d, DyeColor::Yellow]).unwrap())
        .collect();
    for &p in &pieces {
        assert_eq!(stack_from_str(&stack_to_string(Some(p))), Some(Some(p)));
    }
    let mut inv = Inventory::default();
    for (i, &p) in pieces.iter().enumerate() {
        inv.slots[i] = Some(p);
    }
    for (slot, &p) in inv.armor.iter_mut().zip(&pieces) {
        *slot = Some(p);
    }
    let back = Inventory::deserialize(&inv.serialize()).unwrap();
    assert_eq!(back, inv);
    assert_eq!(c::dyed_color(&back.armor[0].unwrap()), c::dyed_color(&pieces[0]));
    // An armor stand wearing the four pieces.
    let s = |i: usize| stack_to_string(Some(pieces[i]));
    let mut e = Entities::new(1);
    e.load_decor(&format!("S,8.500,145.000,8.500,90,{},{},{},{}", s(0), s(1), s(2), s(3)));
    let mut e2 = Entities::new(1);
    e2.load_decor(&e.decor_to_string());
    assert_eq!(e2.decor[0].armor, [Some(pieces[0]), Some(pieces[1]), Some(pieces[2]), Some(pieces[3])]);
    // A pre-v0.17 leather piece: the plain stack string, undyed.
    let old = Stack::new(Item::armor(ArmorPiece::Chestplate, ArmorMaterial::Leather), 1);
    let text = stack_to_string(Some(old));
    let loaded = stack_from_str(&text).unwrap().unwrap();
    assert_eq!(c::dyed_color(&loaded), None);
    assert_eq!(c::dye_tint(&loaded), Some(c::LEATHER_COLOR));
}
