//! v0.7 release regressions (from the 2026-10-10 audit): interactions run through the
//! shared dispatch every input path uses, and lifecycle/LAN state is preserved.
use glam::{DVec3, IVec3};
use std::sync::Arc;
use voxelcraft::{
    entity::{Ctx, Entities, MobKind, MobWorld, PlayerId, Target},
    inventory::{Inventory, Stack, StackName},
    item::Item,
    physics::BlockSource,
    player::Player,
    world::{
        World,
        block::Block,
        chunk::ChunkData,
        terrain::{Dimension, Generator},
    },
};

struct Flat;
impl BlockSource for Flat {
    fn block(&self, p: IVec3) -> Option<Block> {
        Some(if p.y < 1 { Block::STONE } else { Block::AIR })
    }
}
impl MobWorld for Flat {
    fn loaded(&self, _: IVec3) -> bool {
        true
    }
    fn surface(&self, _: i32, _: i32) -> Option<i32> {
        Some(0)
    }
    fn exposed(&self, _: IVec3) -> bool {
        true
    }
}
fn ctx() -> Ctx {
    Ctx {
        players: vec![Target::new(PlayerId::HOST, DVec3::new(0., 1., 0.), false)],
        daylight: 1.,
        spawning: false,
        raining: false,
        dimension: Dimension::Overworld,
    }
}
/// An empty loaded chunk with a stone floor at y=139: the player stands at y=140 looking +Z.
fn world() -> World {
    let mut w = World::new_remote(Arc::new(Generator::new(1)), 2);
    w.receive_chunk(IVec3::new(0, 4, 0), Arc::new(ChunkData::Uniform(Block::AIR)));
    for x in 0..16 {
        for z in 0..16 {
            w.set_block(IVec3::new(x, 139, z), Block::STONE);
        }
    }
    w
}
fn player() -> Player {
    let mut p = Player::new(DVec3::new(5.5, 140., 2.));
    p.yaw = std::f32::consts::FRAC_PI_2;
    p.pitch = -0.35;
    p
}
fn tamed(kind: MobKind) -> Entities {
    let mut e = Entities::new(1);
    e.spawn(kind, DVec3::new(5.5, 140., 5.));
    e.ensure_mob_ids();
    e.mobs[0].mount.as_mut().unwrap().tame = true;
    e
}
/// The click order desktop, controller and LAN players use: animal interactions, then mounting.
fn use_on(e: &mut Entities, inv: &mut Inventory) -> bool {
    let (w, p) = (world(), player());
    voxelcraft::survival_items::use_animal_target(&p, inv, 0, false, &w, e, PlayerId::HOST)
        || e.use_mount(p.eye(), p.forward().as_dvec3(), 5., PlayerId::HOST, inv, 0, false)
}

#[test]
fn all_new_mounts_accept_leads() {
    for kind in [MobKind::Horse, MobKind::Donkey, MobKind::Mule, MobKind::Llama, MobKind::Camel] {
        let mut e = tamed(kind);
        let mut inv = Inventory::default();
        inv.slots[0] = Some(Stack::new(Item::LEAD, 1));
        assert!(use_on(&mut e, &mut inv), "{kind:?}");
        assert!(e.mobs[0].leash.is_some(), "{kind:?} should be leashed");
        assert_eq!(e.mount_for_player(PlayerId::HOST), None, "{kind:?} must not be mounted");
    }
}
#[test]
fn a_name_tag_names_a_tamed_horse() {
    let mut e = tamed(MobKind::Horse);
    let mut inv = Inventory::default();
    inv.slots[0] = Some(Stack { name: StackName::new("ReleaseHorse").unwrap(), ..Stack::new(Item::NAME_TAG, 1) });
    assert!(use_on(&mut e, &mut inv));
    assert_eq!(e.mobs[0].name.as_str(), Some("ReleaseHorse"));
    assert_eq!(e.mount_for_player(PlayerId::HOST), None);
    assert!(inv.get(0).is_none(), "the tag is used up");
}
#[test]
fn a_carrot_breeds_a_saddled_pig_and_a_saddle_still_mounts() {
    let mut e = tamed(MobKind::Pig);
    e.mobs[0].mount.as_mut().unwrap().slots[0] = Some(Stack::new(Item::SADDLE, 1));
    let mut inv = Inventory::default();
    inv.slots[0] = Some(Stack::new(Item::CARROT, 1));
    assert!(use_on(&mut e, &mut inv));
    assert!(e.mobs[0].animal.as_ref().unwrap().love > 0., "feeding enters love mode");
    assert_eq!(e.mount_for_player(PlayerId::HOST), None);
    // An empty hand still rides it.
    assert!(use_on(&mut e, &mut Inventory::default()));
    assert!(e.mount_for_player(PlayerId::HOST).is_some());
}
#[test]
fn warped_fungus_breeds_a_saddled_strider() {
    let mut e = tamed(MobKind::Strider);
    e.mobs[0].mount.as_mut().unwrap().slots[0] = Some(Stack::new(Item::SADDLE, 1));
    let mut inv = Inventory::default();
    inv.slots[0] = Some(Stack::new(Item::from_block(voxelcraft::world::nether_biome_blocks::WARPED_FUNGUS), 1));
    assert!(use_on(&mut e, &mut inv));
    assert!(e.mobs[0].animal.as_ref().unwrap().love > 0.);
}
#[test]
fn fox_drops_its_carried_diamond_when_it_dies() {
    let mut e = Entities::new(1);
    e.spawn(MobKind::Fox, DVec3::new(3., 1., 0.));
    let mut gem = Stack::new(Item::DIAMOND, 1);
    gem.name = StackName::new("Heirloom").unwrap();
    e.mobs[0].animal.as_mut().unwrap().mouth = Some(gem);
    assert_eq!(e.melee(0, DVec3::X, None, 100., false, None), Some(MobKind::Fox));
    for _ in 0..22 {
        e.update(0.05, &Flat, &ctx());
    }
    let drops: Vec<_> = e.items.iter().filter(|i| i.stack.item == Item::DIAMOND).collect();
    assert_eq!(drops.len(), 1, "the carried stack drops exactly once");
    assert_eq!(drops[0].stack.name.as_str(), Some("Heirloom"));
}
#[test]
fn parrots_do_not_double_book_a_shoulder() {
    let mut e = Entities::new(1);
    for _ in 0..3 {
        e.spawn(MobKind::Parrot, DVec3::new(0., 1., 0.));
        e.mobs.last_mut().unwrap().animal.as_mut().unwrap().owner = Some(PlayerId::HOST);
    }
    e.mobs[1].animal.as_mut().unwrap().perched = Some(0);
    e.mobs[2].animal.as_mut().unwrap().perched = Some(1);
    e.update(0.05, &Flat, &ctx());
    let perched = e.mobs.iter().filter(|m| m.animal.as_ref().unwrap().perched.is_some()).count();
    assert_eq!(perched, 2, "only two shoulder slots exist");
}
#[test]
fn lan_mount_look_preserves_health_and_jump_hud_values() {
    let mut e = tamed(MobKind::Horse);
    let s = e.mobs[0].mount.as_mut().unwrap();
    s.max_health = 17.;
    s.jump_charge = 0.9;
    let mut replica = voxelcraft::entity::Mob::new(MobKind::Horse, DVec3::ZERO, 0.);
    replica.apply_look(&e.mobs[0].look());
    let s = replica.mount.as_ref().unwrap();
    assert_eq!((s.max_health, s.jump_charge), (17., 0.9));
}
#[test]
fn wolf_attacks_its_owners_arrow_target() {
    let mut e = Entities::new(1);
    e.spawn(MobKind::Wolf, DVec3::new(3., 1., 0.));
    e.mobs[0].animal.as_mut().unwrap().owner = Some(PlayerId::HOST);
    e.spawn(MobKind::Cow, DVec3::new(0., 1., 4.));
    e.ensure_mob_ids();
    e.shoot_arrow(DVec3::new(0., 1.6, 0.), DVec3::Z, 0.4, true);
    for _ in 0..20 {
        e.update(0.05, &Flat, &ctx());
        if e.mobs[1].health < MobKind::Cow.max_health() {
            break;
        }
    }
    assert!(e.mobs[1].health < MobKind::Cow.max_health(), "fixture arrow must hit the cow");
    assert!(e.mobs[0].animal.as_ref().unwrap().foe.is_some(), "wolf should retaliate against its owner's target");
}
#[test]
fn wolf_defends_its_owner_from_a_zombie() {
    let mut e = Entities::new(1);
    e.spawn(MobKind::Wolf, DVec3::new(3., 1., 0.));
    e.mobs[0].animal.as_mut().unwrap().owner = Some(PlayerId::HOST);
    e.spawn(MobKind::Zombie, DVec3::new(0.8, 1., 0.));
    e.ensure_mob_ids();
    let zombie = e.mobs[1].uid;
    let mut c = ctx();
    c.players[0] = Target::new(PlayerId::HOST, DVec3::new(0., 1., 0.), true);
    c.daylight = 0.;
    for _ in 0..80 {
        e.update(0.05, &Flat, &c);
        if e.mobs[0].animal.as_ref().unwrap().foe.is_some() {
            break;
        }
    }
    assert_eq!(e.mobs[0].animal.as_ref().unwrap().foe.map(|f| f.0), Some(zombie));
}
