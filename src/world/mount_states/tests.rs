use super::*;
use crate::item::Item;
use crate::world::block::Shaped;
use crate::world::shape::{self, Box16};
use glam::DVec3;

const RIGHT_DOORS: std::ops::Range<u16> = DOOR_FIRST..DOOR_FIRST + DOOR_FAMILIES * 8;

fn place_ctx(at: IVec3, facing: Facing, click: DVec3) -> PlaceContext {
    PlaceContext {
        clicked: at - IVec3::Y,
        clicked_block: Block::STONE,
        face: IVec3::Y,
        click,
        at,
        facing,
        look: facing.offset().as_vec3(),
        sneaking: false,
    }
}

#[test]
fn every_new_id_is_registered_and_the_old_ones_keep_their_meaning() {
    for id in LEVER_FIRST..LEVER_FIRST + 12 {
        assert_eq!(Block(id).name(), "lever");
    }
    for id in BUTTON_FIRST..BUTTON_FIRST + 72 {
        let kind = (id - BUTTON_FIRST) / 12;
        let name = ["stone", "oak", "crimson", "warped", "pale oak", "bamboo"][kind as usize];
        assert_eq!(Block(id).name(), format!("{name} button"));
    }
    for id in RIGHT_DOORS {
        assert_eq!(Block(id).name(), door_twin(id).unwrap().name());
        assert_eq!(right_of(door_twin(id).unwrap()), Some(Block(id)));
        assert!(Block(id).is_door() && !Block(id).is_door_upper() && is_right_hinge(Block(id)));
    }
    for id in 7904..=7911 {
        assert_eq!(Block(id).name(), "grindstone");
        assert_eq!(Block(id).base(), Block::GRINDSTONE);
    }
    assert_eq!(LANTERN.name(), "lantern");
    assert_eq!(SOUL_LANTERN.name(), "soul lantern");
    // Ids are append-only: the pre-existing door, lever and button states are unchanged.
    assert_eq!(Block::door(Facing::South, false, false), Block(149));
    assert_eq!(Block::door(Facing::West, true, true), Block(164));
    assert_eq!(r::iron_door(Facing::East, true, false), Block(1255));
    assert_eq!(r::lever(0, false), Block(1116));
    assert_eq!(r::lever(5, true), Block(1127));
    assert_eq!(r::button(1, false, false), Block(1130));
    assert_eq!(r::button(5, true, true), Block(1151));
    assert_eq!(Block::GRINDSTONE, Block(933));
    for id in (149..=164).chain(1249..=1264).chain(1116..=1151).chain(933..=936) {
        assert!(!is_right_hinge(Block(id)), "{id}");
    }
}

#[test]
fn right_hinge_doors_act_like_their_left_twins() {
    for id in RIGHT_DOORS {
        let (right, left) = (Block(id), door_twin(id).unwrap());
        assert_eq!(right.shaped(), left.shaped());
        assert_eq!(right.base(), left.base());
        assert_eq!(right.oriented(), left.oriented());
        assert_eq!(right.drop(), left.drop());
        assert_eq!(right.hardness(), left.hardness());
        assert_eq!(right.best_tool(), left.best_tool());
        assert_eq!(right.harvest_level(), left.harvest_level());
        assert_eq!(right.info().tex, left.info().tex);
        assert_eq!(right.kind(), left.kind());
        assert_eq!(right.emission(), 0);
        let Some(Shaped::Door { facing, open, .. }) = right.shaped() else { panic!("{id}") };
        // Hand and redstone toggling keep the hinge.
        let toggled = right.toggled(facing);
        assert!(is_right_hinge(toggled), "{id}");
        assert_eq!(toggled.shaped(), Some(Shaped::Door { facing, open: !open, upper: false }));
        assert_eq!(toggled.toggled(facing), right);
        assert_eq!(
            keep_hinge(right, r::iron_door(facing, true, false)),
            right_of(r::iron_door(facing, true, false)).unwrap()
        );
        assert_eq!(keep_hinge(left, left.toggled(facing)), left.toggled(facing));
    }
}

fn panel_side(boxes: &shape::Boxes) -> Facing {
    let bx = boxes.bounds().unwrap();
    assert!(bx.max[1] == 16 && bx.min[1] == 0);
    let flat = |axis: usize| bx.max[axis] - bx.min[axis] == 3;
    match (flat(0), flat(2), bx) {
        (false, true, Box16 { min, .. }) => {
            if min[2] == 13 {
                Facing::South
            } else {
                Facing::North
            }
        }
        (true, false, Box16 { min, .. }) => {
            if min[0] == 13 {
                Facing::East
            } else {
                Facing::West
            }
        }
        _ => panic!("not a door panel: {bx:?}"),
    }
}

#[test]
fn open_right_hinge_doors_swing_to_the_opposite_side_like_java() {
    // Java `DoorBlock.getShape`: closed, the panel is on the side the door
    // closes on (our facing); open, a left hinge swings it to the clockwise
    // neighbour of that side and a right hinge to the counter-clockwise one.
    let none = |_: Facing| Block::AIR;
    for facing in Facing::ALL {
        for (lower, upper) in [
            (Block::door(facing, false, false), Block::door(facing, false, true)),
            (r::iron_door(facing, false, false), r::iron_door(facing, false, true)),
            (forms_wood_door(facing, false), crate::world::forms::wood_id(0, 22)),
        ] {
            let right = right_of(lower).unwrap();
            for door in [lower, right] {
                assert_eq!(panel_side(&shape::shape(door, none, Block::AIR)), facing);
            }
            let open_left = lower.toggled(facing);
            let open_right = right.toggled(facing);
            assert_eq!(panel_side(&shape::shape(open_left, none, Block::AIR)), facing.clockwise());
            assert_eq!(
                panel_side(&shape::shape(open_right, none, Block::AIR)),
                facing.clockwise().clockwise().clockwise()
            );
            // The upper half copies the lower half, hinge included.
            assert_eq!(
                shape::shape(upper.toggled(facing), none, open_right).as_slice(),
                shape::shape(open_right, none, Block::AIR).as_slice()
            );
            assert_eq!(
                shape::collision(open_right, none, Block::AIR).as_slice(),
                shape::shape(open_right, none, Block::AIR).as_slice()
            );
        }
    }
}

fn forms_wood_door(facing: Facing, open: bool) -> Block {
    crate::world::forms::wood_id(0, 14 + open as u16 * 4 + facing as u16)
}

#[test]
fn double_doors_swing_their_panels_apart_around_the_hinges() {
    // Two doors side by side facing the same way: the left one is placed
    // first (left hinge), the second sees a door on its left and takes the
    // right hinge, so opened they leave the doorway clear.
    let at = IVec3::new(4, 70, 4);
    for look in Facing::ALL {
        let facing = look.opposite();
        let left_dir = look.clockwise().clockwise().clockwise().offset();
        let first = Block::door(facing, false, false);
        let blocks = |p: IVec3| (p == at + left_dir).then_some(first);
        let ctx = place_ctx(at, look, DVec3::splat(0.5));
        assert!(door_hinge_right(&ctx, blocks), "{look:?}: a door on the left puts the hinge on the right");
        let right_blocks = |p: IVec3| (p == at + look.clockwise().offset()).then_some(first);
        assert!(!door_hinge_right(&ctx, right_blocks), "{look:?}: a door on the right puts the hinge on the left");
        // Opened, the two panels sit on opposite outer sides.
        let second = right_of(first).unwrap();
        let none = |_: Facing| Block::AIR;
        let a = panel_side(&shape::shape(first.toggled(facing), none, Block::AIR));
        let b = panel_side(&shape::shape(second.toggled(facing), none, Block::AIR));
        assert_eq!(a, b.opposite(), "{look:?}");
    }
}

#[test]
fn hinge_selection_follows_java_get_hinge() {
    let at = IVec3::new(4, 70, 4);
    let stone = Block::STONE;
    let door = Block::door(Facing::South, false, false);
    let upper = Block::door(Facing::South, false, true);
    for look in Facing::ALL {
        let left = look.clockwise().clockwise().clockwise().offset();
        let right = look.clockwise().offset();
        let ctx = place_ctx(at, look, DVec3::splat(0.5));
        let hinge = |cells: &[(IVec3, Block)]| {
            door_hinge_right(&ctx, |p| Some(cells.iter().find(|c| c.0 == p).map_or(Block::AIR, |c| c.1)))
        };
        // Solid blocks beside the cell: the hinge goes against the larger side.
        assert!(hinge(&[(at + right, stone)]), "{look:?} wall right");
        assert!(!hinge(&[(at + left, stone)]), "{look:?} wall left");
        assert!(hinge(&[(at + right + IVec3::Y, stone)]), "{look:?} wall above right");
        assert!(!hinge(&[(at + left + IVec3::Y, stone)]), "{look:?} wall above left");
        assert!(hinge(&[(at + right, stone), (at + right + IVec3::Y, stone), (at + left, stone)]));
        // Equal walls on both sides fall through to the click.
        let both = [(at + left, stone), (at + right, stone)];
        let hinge_for = |click: DVec3| {
            door_hinge_right(&place_ctx(at, look, click), |p| {
                Some(both.iter().find(|c| c.0 == p).map_or(Block::AIR, |c| c.1))
            })
        };
        // Doors beside the cell: the hinge moves away from them; upper halves don't count.
        assert!(hinge(&[(at + left, door)]), "{look:?} door left");
        assert!(!hinge(&[(at + right, door)]), "{look:?} door right");
        assert!(!hinge(&[(at + left, upper)]), "{look:?} upper half left is not a door");
        // A door on both sides cancels out and the solid blocks decide.
        assert!(hinge(&[(at + left, door), (at + right, door), (at + right + IVec3::Y, stone)]));
        // The click: the half of the cell on the placer's right takes the right hinge.
        let click_at = |side: IVec3| DVec3::splat(0.5) + side.as_dvec3() * 0.4;
        assert!(hinge_for(click_at(right)), "{look:?} click right");
        assert!(!hinge_for(click_at(left)), "{look:?} click left");
    }
}

#[test]
fn hinge_click_is_measured_in_the_placed_cell() {
    // Placing against the east face of a block at (3,70,4): the door cell is
    // (4,70,4); the click point 0.9 along z inside the *clicked* cell is the
    // same coordinate inside the placed one.
    let ctx = PlaceContext {
        clicked: IVec3::new(3, 70, 4),
        clicked_block: Block::STONE,
        face: IVec3::X,
        click: DVec3::new(1.0, 0.5, 0.9),
        at: IVec3::new(4, 70, 4),
        facing: Facing::East,
        look: Vec3::X,
        sneaking: false,
    };
    // Looking east: right is south (+z), so a click on the south half hinges right.
    assert!(door_hinge_right(&ctx, |_| Some(Block::AIR)));
    let north = PlaceContext { click: DVec3::new(1.0, 0.5, 0.1), ..ctx };
    assert!(!door_hinge_right(&north, |_| Some(Block::AIR)));
}

#[test]
fn mounts_roundtrip_and_support_the_right_side() {
    for mount in 0..12u8 {
        let (face, facing) = mount_attach(mount);
        assert_eq!(attach_mount(face, facing), mount, "{mount}");
        assert_eq!(attach_direction(face, facing), r::support(mount), "{mount}");
    }
    // Floor/ceiling mounts 0 and 5 stay "facing south"; wall mounts 1..=4 face away from the wall.
    assert_eq!(mount_attach(0), (AttachFace::Floor, Facing::South));
    assert_eq!(mount_attach(5), (AttachFace::Ceiling, Facing::South));
    assert_eq!(mount_attach(2), (AttachFace::Wall, Facing::North));
    for mount in 6..12 {
        for on in [false, true] {
            assert_eq!(r::component(r::lever(mount, on)), Some(r::Component::Lever { mount, on }));
            for wood in [false, true] {
                assert_eq!(r::component(r::button(mount, on, wood)), Some(r::Component::Button { mount, on, wood }));
            }
        }
    }
    // Every button wood keeps its wood through the oak twin mapping.
    for wood in 0..4 {
        for mount in 6..12 {
            let oak = r::button(mount, true, true);
            let twin = nb::switch_reskin(oak, wood);
            assert_eq!(nb::switch_oak(twin.0), Some((oak, wood)));
            assert_eq!(
                nb::keep_wood(twin, r::button(mount, false, true)),
                nb::switch_reskin(r::button(mount, false, true), wood)
            );
            assert_eq!(twin.base(), nb::switch_reskin(r::WOOD_BUTTON, wood));
        }
    }
}

#[test]
fn nearest_looking_matches_direction_ordered_by_nearest() {
    let cases = [
        // Pitch stops at 89 degrees, so a look is never exactly vertical;
        // near it the horizontal order still follows the larger component.
        (Vec3::new(0.01, -1.0, 0.02), [IVec3::NEG_Y, IVec3::Z, IVec3::X, IVec3::NEG_X, IVec3::NEG_Z, IVec3::Y]),
        (Vec3::new(-0.02, 1.0, 0.01), [IVec3::Y, IVec3::NEG_X, IVec3::Z, IVec3::NEG_Z, IVec3::X, IVec3::NEG_Y]),
        (Vec3::new(1.0, 0.2, 0.3), [IVec3::X, IVec3::Z, IVec3::Y, IVec3::NEG_Y, IVec3::NEG_Z, IVec3::NEG_X]),
        (Vec3::new(0.2, -0.9, -0.3), [IVec3::NEG_Y, IVec3::NEG_Z, IVec3::X, IVec3::NEG_X, IVec3::Z, IVec3::Y]),
        (Vec3::new(-0.3, 0.1, -1.0), [IVec3::NEG_Z, IVec3::NEG_X, IVec3::Y, IVec3::NEG_Y, IVec3::X, IVec3::Z]),
    ];
    for (look, order) in cases {
        assert_eq!(nearest_looking(look), order, "{look}");
    }
}

#[test]
fn sturdy_and_centre_support_follow_the_block_shape() {
    use IVec3 as D;
    let slab = Block::slab_of(Block::STONE).unwrap();
    for face in [D::X, D::NEG_X, D::Y, D::NEG_Y, D::Z, D::NEG_Z] {
        assert!(face_sturdy(Block::STONE, face) && supports_center(Block::STONE, face));
        assert!(face_sturdy(Block::GLASS, face));
        assert!(!face_sturdy(Block::AIR, face) && !face_sturdy(Block::LEAVES, face));
        assert!(!supports_center(Block::WATER, face));
    }
    // A bottom slab is solid underneath only.
    assert!(face_sturdy(slab, D::NEG_Y) && !face_sturdy(slab, D::Y) && !face_sturdy(slab, D::X));
    // A fence post carries a torch or lantern in its centre, but is not a sturdy face.
    assert!(supports_center(Block::OAK_FENCE, D::Y) && supports_center(Block::OAK_FENCE, D::NEG_Y));
    assert!(!face_sturdy(Block::OAK_FENCE, D::Y));
    // Gates are `#unstable_bottom_center`: nothing hangs from them.
    let gate = Block::gate(Facing::South, false);
    assert!(!supports_center(gate, D::NEG_Y));
    // Stairs: a full back face, a full bottom, but a partial top.
    let stairs = Block::STONE_STAIRS.with_facing(Facing::East);
    assert!(face_sturdy(stairs, D::NEG_Y) && !face_sturdy(stairs, D::Y));
    assert!(face_sturdy(stairs, D::NEG_X), "east-facing stairs have their tall half on the west");
    assert!(!face_sturdy(stairs, D::X));
}

#[test]
fn lanterns_have_java_states_light_and_drops() {
    for soul in [false, true] {
        for hanging in [false, true] {
            for wet in [false, true] {
                let b = lantern_block(soul, hanging, wet);
                assert_eq!(lantern_of(b), Some(LanternState { soul, hanging, wet }));
                assert_eq!(b.name(), if soul { "soul lantern" } else { "lantern" });
                assert_eq!(b.emission(), if soul { 10 } else { 15 });
                assert_eq!(b.drop(), Some(Item::from_block(if soul { SOUL_LANTERN } else { LANTERN })));
                assert_eq!(b.hardness(), 3.5);
                assert_eq!(b.is_waterlogged(), wet);
                assert_eq!(b.with_water(!wet).is_waterlogged(), !wet);
                assert_eq!(b.with_water(!wet), lantern_block(soul, hanging, !wet));
                // Holding water, it no longer lets skylight through
                // (`propagatesSkylightDown` needs an empty fluid state).
                assert_eq!(b.light_opacity(), wet as u8);
                assert!(b.is_solid() && !b.is_opaque());
                // Hanging, the whole lantern sits one pixel higher.
                let bounds = shape::shape(b, |_| Block::AIR, Block::AIR).bounds().unwrap();
                assert_eq!((bounds.min[1], bounds.max[1]), if hanging { (1, 10) } else { (0, 9) });
            }
        }
    }
    assert!(LANTERN.can_stay_on(Block::STONE) && !LANTERN.can_stay_on(Block::AIR));
    assert!(LANTERN.can_stay_on(Block::OAK_FENCE) && !LANTERN.can_stay_on(Block::LEAVES));
    assert!(lantern_block(false, true, false).can_stay_on(Block::AIR), "hanging ones check the block above");
}

#[test]
fn a_hanging_lantern_breaks_when_its_ceiling_goes() {
    use crate::world::{World, chunk::ChunkData, terrain::Generator};
    use std::sync::Arc;
    let mut world = World::new_remote(Arc::new(Generator::new(7)), 2);
    world.receive_chunk(IVec3::new(0, 4, 0), Arc::new(ChunkData::Uniform(Block::AIR)));
    world.set_tile_drops(true);
    let ceiling = IVec3::new(4, 140, 4);
    for (x, lantern) in [(4, lantern_block(false, true, false)), (6, lantern_block(true, true, true))] {
        let ceiling = IVec3::new(x, ceiling.y, ceiling.z);
        world.set_block(ceiling, Block::STONE);
        world.set_block(ceiling - IVec3::Y, lantern);
        // A fence post above still holds it (`canSupportCenter`).
        world.set_block(ceiling, Block::OAK_FENCE);
        assert_eq!(world.get_block(ceiling - IVec3::Y), Some(lantern));
        world.drops.clear();
        world.set_block(ceiling, Block::AIR);
        let left = if lantern_of(lantern).unwrap().wet { Block::WATER } else { Block::AIR };
        assert_eq!(world.get_block(ceiling - IVec3::Y), Some(left));
        assert_eq!(
            world.drops,
            vec![(ceiling - IVec3::Y, crate::inventory::Stack::new(Item::from_block(lantern.base()), 1))]
        );
    }
    // A standing lantern on the floor isn't touched by changes above it.
    let floor = IVec3::new(8, 140, 4);
    world.set_block(floor, Block::STONE);
    world.set_block(floor + IVec3::Y, LANTERN);
    world.set_block(floor + IVec3::Y * 2, Block::STONE);
    world.set_block(floor + IVec3::Y * 2, Block::AIR);
    assert_eq!(world.get_block(floor + IVec3::Y), Some(LANTERN));
}

#[test]
fn the_clicked_face_comes_first_like_get_nearest_looking_directions() {
    // Clicking a stone's east side while looking almost straight down: Java
    // tries the clicked block first, so the grindstone goes on the wall.
    let stone = IVec3::new(0, 64, 0);
    let ctx = PlaceContext {
        clicked: stone,
        clicked_block: Block::STONE,
        face: IVec3::X,
        click: DVec3::new(1.0, 0.5, 0.5),
        at: stone + IVec3::X,
        facing: Facing::West,
        look: glam::Vec3::new(-0.2, -1.0, 0.0).normalize(),
        sneaking: false,
    };
    assert_eq!(context_looking(&ctx)[..2], [IVec3::NEG_X, IVec3::NEG_Y]);
    let get = |p: IVec3| Some(if p == stone || p == stone + IVec3::X - IVec3::Y { Block::STONE } else { Block::AIR });
    let grindstone = placed(Block::GRINDSTONE, &ctx, get).unwrap().unwrap();
    assert_eq!(grindstone_state(grindstone.0), Some((AttachFace::Wall, Facing::East)));
    // A button clicked onto the floor's top still lies on the floor.
    let ctx = PlaceContext { clicked: stone, face: IVec3::Y, at: stone + IVec3::Y, ..ctx };
    let get = |p: IVec3| Some(if p == stone { Block::STONE } else { Block::AIR });
    let button = placed(r::STONE_BUTTON, &ctx, get).unwrap().unwrap();
    assert!(
        matches!(r::component(button), Some(r::Component::Button { mount, .. }) if mount_attach(mount).0 == AttachFace::Floor)
    );
}
