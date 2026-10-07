//! Deterministic diagnostic locations for screenshotting the four layouts.
use glam::{IVec2, IVec3};
use voxelcraft::world::{
    bastion::{Bastions, Kind},
    block::Facing,
    structure::Oriented,
};
fn main() {
    let bastions = Bastions::new(12345);
    for kind in [Kind::Housing, Kind::Stables, Kind::Treasure, Kind::Bridge] {
        let b = (-4..=4)
            .flat_map(|z| (-4..=4).map(move |x| IVec2::new(x, z)))
            .filter_map(|r| bastions.get(r))
            .find(|b| b.kind == kind)
            .unwrap();
        let piece = &b.pieces[0];
        let eye = if kind == Kind::Bridge {
            piece.world(16, 5, 45)
        } else if kind == Kind::Stables {
            piece.world(10, 5, 18)
        } else if kind == Kind::Treasure {
            piece.world(16, 21, 28)
        } else {
            piece.world((piece.bounds.max - piece.bounds.min).min_element().max(12) / 2, 18, 0)
                - piece.turn(Facing::South).offset() * 24
        };
        let forward = piece
            .turn(if matches!(kind, Kind::Treasure | Kind::Bridge) { Facing::North } else { Facing::South })
            .offset();
        let yaw = (forward.z as f32).atan2(forward.x as f32).to_degrees();
        let pitch = if kind == Kind::Treasure {
            -52
        } else if kind == Kind::Bridge {
            27
        } else {
            -12
        };
        let center = (b.bounds.min + b.bounds.max) / 2;
        println!(
            "{kind:?}: --pose {},{},{},{yaw},{pitch} (center {})",
            eye.x,
            eye.y,
            eye.z,
            IVec3::new(center.x, center.y, center.z)
        );
    }
}
