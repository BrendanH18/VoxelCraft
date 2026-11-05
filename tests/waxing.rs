//! Honeycomb/axe transactions and every registered mirrored copper state.
use glam::{DVec3, IVec3};
use std::sync::Arc;
use voxelcraft::{
    crafting::Grid,
    entity::{Entities, PlayerId},
    inventory::Stack,
    item::{Item, Tier, ToolKind},
    player::EYE_HEIGHT,
    session::{
        PlayerSession,
        interact::{UseContext, Used},
    },
    world::{
        World,
        block::Block,
        chunk::ChunkData,
        copper::{self, Copper, Family, Stage},
        terrain::Generator,
        wood::{self, signs},
    },
};
const P: IVec3 = IVec3::new(10, 140, 10);
fn scene(b: Block) -> (World, PlayerSession, Entities) {
    let mut w = World::new_remote(Arc::new(Generator::new(42)), 2);
    w.receive_chunk(IVec3::new(0, 4, 0), Arc::new(ChunkData::Uniform(Block::AIR)));
    let mut s = PlayerSession::new(PlayerId::HOST, DVec3::ZERO);
    w.set_block(P - IVec3::Y, Block::STONE);
    w.set_block(P, b);
    let eye = DVec3::new(10.5, 141., 13.);
    let d = (P.as_dvec3() + DVec3::new(0.5, 0.6, 0.5) - eye).normalize();
    s.player.pos = eye - DVec3::Y * EYE_HEIGHT;
    s.player.yaw = d.z.atan2(d.x) as f32;
    s.player.pitch = d.y.asin() as f32;
    (w, s, Entities::new(4))
}
fn use_held(w: &mut World, s: &mut PlayerSession, e: &mut Entities, sneaking: bool) -> Option<Used> {
    s.use_held(0, w, e, &UseContext { sneaking, others: &[] }).unwrap().map(|x| x.used)
}
#[test]
fn all_56_families_and_840_registered_states_mirror_without_losing_properties() {
    let mut states = 0;
    for id in copper::WAXED_FIRST..=copper::WAXED_RESERVED_LAST {
        let b = Block(id);
        let Some(c) = copper::decode(id) else {
            assert_eq!(b.name(), "unknown");
            continue;
        };
        states += 1;
        assert!(c.waxed);
        assert_eq!(copper::state(b), Some(c));
        assert_eq!(copper::encode(c), b);
        assert!(!copper::ages(b));
        let original = copper::encode(Copper { waxed: false, ..c });
        assert_eq!(b.info().tex, original.info().tex);
        assert_eq!(b.hardness(), original.hardness());
        assert_eq!(b.emission(), original.emission());
        assert_eq!(b.is_waterlogged(), original.is_waterlogged());
        assert_eq!(b.top_drop(), original.top_drop());
        assert_eq!(b.base(), copper::encode(Copper { local: 0, ..c }));
        if matches!(c.family, Family::Stairs | Family::Slab) {
            use voxelcraft::world::slab_stair_states as ss;
            assert!(ss::stairs(b).is_some() || ss::slab(b).is_some());
            if let Some(s) = ss::stairs(b) {
                assert_eq!(ss::stairs_id(s), b);
            }
            if let Some(s) = ss::slab(b) {
                assert_eq!(ss::slab_id(s), b);
            }
        }
    }
    // 792 plus the 48 powered lightning-rod locals.
    assert_eq!(states, 840);
    let palette: Vec<_> = Block::creative_palette().collect();
    for family in Family::ALL.into_iter().filter(|f| f.weathers()) {
        for stage in Stage::ALL {
            let b = copper::encode(Copper { family, stage, local: 0, waxed: true });
            assert_eq!(palette.iter().filter(|&&x| x == b).count(), 1);
            assert_eq!(Block::from_name(b.name()), Some(b));
            let mut g = Grid::new(2);
            g.cells[0] = Some(Stack::new(copper::block(family, stage, 0), 1));
            g.cells[3] = Some(Stack::new(Item::HONEYCOMB, 1));
            assert_eq!(g.result(), Some(Stack::new(b, 1)), "{}", b.name());
        }
    }
}
#[test]
fn honeycomb_then_axe_keeps_the_stage_before_the_next_scrape() {
    let original = copper::block(Family::Grate, Stage::Weathered, 1);
    let (mut w, mut s, mut e) = scene(original);
    s.inventory.slots[0] = Some(Stack::new(Item::HONEYCOMB, 2));
    assert_eq!(use_held(&mut w, &mut s, &mut e, false), Some(Used::Waxed { cell: P }));
    assert_eq!(s.inventory.slots[0].unwrap().count, 1);
    let wax = copper::state(w.get_block(P).unwrap()).unwrap();
    assert!(wax.waxed);
    assert_eq!((wax.stage, wax.local), (Stage::Weathered, 1));
    s.inventory.slots[0] = Some(Stack::new(Item::tool(ToolKind::Axe, Tier::Iron), 1));
    assert_eq!(use_held(&mut w, &mut s, &mut e, false), Some(Used::Scraped { cell: P }));
    assert_eq!(w.get_block(P), Some(original));
    assert_eq!(s.inventory.slots[0].unwrap().damage, 1);
    assert_eq!(use_held(&mut w, &mut s, &mut e, false), Some(Used::Scraped { cell: P }));
    assert_eq!(copper::state(w.get_block(P).unwrap()).unwrap().stage, Stage::Exposed);
}
#[test]
fn waxed_signs_refuse_both_text_and_ink_after_save_reload() {
    let sign = signs::Sign { kind: wood::Kind::Oak, mount: signs::Mount::Standing { rotation: 0 }, wet: false }.block();
    let (mut w, mut s, mut e) = scene(sign);
    w.set_sign_text(P, true, ["Keep me".into(), String::new(), String::new(), String::new()]).unwrap();
    s.inventory.slots[0] = Some(Stack::new(Item::HONEYCOMB, 2));
    assert_eq!(use_held(&mut w, &mut s, &mut e, false), Some(Used::Waxed { cell: P }));
    assert_eq!(s.inventory.slots[0].unwrap().count, 1);
    assert!(w.sign_at(P).unwrap().1.waxed);
    assert!(s.edit_sign(&mut w, P, true, Default::default()).is_err());
    assert!(!w.update_sign(P, true, |t| t.glowing = true));
    let text = w.wood_to_string();
    let (mut loaded, _, _) = scene(sign);
    loaded.load_wood(&text);
    assert!(loaded.sign_at(P).unwrap().1.waxed);
    assert_eq!(loaded.sign_at(P).unwrap().1.front.lines[0], "Keep me");
    assert!(loaded.set_sign_text(P, true, Default::default()).is_err());
}
