use super::*;
use glam::IVec3;
use std::sync::Arc;
use voxelcraft::{
    entity::PlayerId,
    inventory::Stack,
    player::EYE_HEIGHT,
    world::{chunk::ChunkData, copper::Stage, copper_golem_statue as statue, terrain::Generator},
};

#[test]
fn statue_use_prevents_food_and_drink_start_but_sneaking_passes() {
    let mut audio = Audio::new(true, 0.);
    for item in [Item::APPLE, Item::HONEY_BOTTLE] {
        for offhand in [false, true] {
            for sneaking in [false, true] {
                let p = IVec3::new(10, 140, 10);
                let mut world = World::new_remote(Arc::new(Generator::new(263)), 2);
                world.receive_chunk(IVec3::new(0, 4, 0), Arc::new(ChunkData::Uniform(Block::AIR)));
                let block = statue::item_block(Stage::Unaffected, false);
                world.set_block(p, block);
                let eye = DVec3::new(10.5, 141., 13.);
                let direction = (p.as_dvec3() + DVec3::splat(0.5) - eye).normalize();
                let mut session = PlayerSession::new(PlayerId::HOST, eye - DVec3::Y * EYE_HEIGHT);
                session.player.yaw = direction.z.atan2(direction.x) as f32;
                session.player.pitch = direction.y.asin() as f32;
                session.vitals.hunger.food = 8.;
                if offhand {
                    session.inventory.offhand = Some(Stack::new(item, 2));
                } else {
                    session.inventory.slots[0] = Some(Stack::new(item, 2));
                }
                let before = session.inventory.clone();
                let mut body = Body::default();
                let mut hand = HandAnim::default();
                let mut entities = Entities::new(263);
                let rules = GameRules::default();
                let mut actor = Actor {
                    session: &mut session,
                    body: &mut body,
                    hand: &mut hand,
                    world: &mut world,
                    entities: &mut entities,
                    audio: &mut audio,
                    gamerules: &rules,
                    sneaking,
                    sprinting: false,
                    others: &[],
                    requests: Vec::new(),
                };
                actor.use_button(true);
                assert_eq!(actor.world.get_block(p) == Some(block), sneaking);
                for _ in 0..40 {
                    actor.act(true, 0.05, false);
                }
                if sneaking {
                    assert_ne!(actor.session.inventory, before, "sneaking permits consumption");
                } else {
                    assert_eq!(actor.session.inventory, before, "statue use consumes this action");
                    assert!(!actor.session.consuming());
                }
                actor.use_button(false);
                if !sneaking {
                    // A bite started away from the statue continues after the
                    // player looks at it; only starting use is intercepted.
                    actor.session.tick_consumption_either(0, true, actor.entities).unwrap();
                    actor.body.right_held = true;
                    actor.act(true, 0.05, false);
                    assert!(actor.session.consuming());
                    assert!(actor.session.consumption_elapsed() >= 0.1);
                }
            }
        }
    }
}
