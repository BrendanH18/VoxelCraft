//! Per-view Java camera modes and eight-probe third-person clipping.
use glam::{DVec3, IVec3, Mat4, Vec3};

use crate::physics::{BlockSource, ray_aabb, ray_shape};

#[derive(Default, Clone, Copy, Debug, PartialEq, Eq)]
pub enum CameraMode {
    #[default]
    First,
    Third,
    Front,
}

impl CameraMode {
    pub fn cycle(&mut self) {
        *self = match self {
            Self::First => Self::Third,
            Self::Third => Self::Front,
            Self::Front => Self::First,
        };
    }

    pub fn first_person(self) -> bool {
        self == Self::First
    }

    /// Front view reverses both yaw and pitch, exactly opposite the player's
    /// look direction. Picking/movement still use the player's original look.
    pub fn view(self, world: &impl BlockSource, eye: DVec3, look: Vec3) -> (DVec3, Vec3) {
        let forward = if self == Self::Front { -look } else { look };
        if self.first_person() {
            return (eye, forward);
        }
        let backward = -forward.as_dvec3();
        (eye + backward * max_zoom(world, eye, backward, 4.0), forward)
    }
}

/// Java Camera.getMaxZoom probes all corners of a +/-0.1-block eye cube,
/// clipping visual block shapes and ignoring fluids. Use the ray distance
/// as an additional conservative bound: Java's Euclidean hit-to-eye distance
/// can put a corner marginally through a surface at shallow angles.
pub fn max_zoom(world: &impl BlockSource, eye: DVec3, backward: DVec3, desired: f64) -> f64 {
    let mut distance = desired;
    for i in 0..8 {
        let offset = DVec3::new(
            if i & 1 == 0 { -0.1 } else { 0.1 },
            if i & 2 == 0 { -0.1 } else { 0.1 },
            if i & 4 == 0 { -0.1 } else { 0.1 },
        );
        if let Some(t) = clip(world, eye + offset, backward, distance) {
            let hit = eye + offset + backward * t;
            distance = distance.min(hit.distance(eye)).min((t - 1e-4).max(0.0));
        }
    }
    distance
}

/// DDA traversal: at most a handful of block lookups for the four-block ray,
/// with the cell below also checked for shapes extending above their cell.
fn clip(world: &impl BlockSource, origin: DVec3, dir: DVec3, max: f64) -> Option<f64> {
    let mut cell = origin.floor().as_ivec3();
    let step = dir.signum().as_ivec3();
    let inv = dir.abs().map(|v| if v > 1e-12 { v.recip() } else { f64::INFINITY });
    let frac = origin - origin.floor();
    let mut next = DVec3::ZERO;
    for axis in 0..3 {
        next[axis] = if inv[axis].is_infinite() {
            f64::INFINITY
        } else {
            (if dir[axis] > 0.0 { 1.0 - frac[axis] } else { frac[axis] }) * inv[axis]
        };
    }
    let mut traveled = 0.0;
    while traveled <= max {
        let mut hit: Option<f64> = None;
        for at in [cell, cell - IVec3::Y] {
            let t = match world.block(at) {
                Some(b) if !b.is_solid() => None,
                Some(b) if b.kind() == crate::world::block::RenderKind::Shaped => {
                    ray_shape(world, at, origin, dir).map(|(t, _)| t)
                }
                Some(b) => ray_aabb(origin, dir, at.as_dvec3(), at.as_dvec3() + DVec3::new(1.0, b.height(), 1.0)),
                // The camera must also stop at the streaming boundary.
                None => ray_aabb(origin, dir, at.as_dvec3(), at.as_dvec3() + DVec3::ONE),
            };
            if let Some(t) = t.filter(|&t| t <= max) {
                hit = Some(hit.map_or(t, |old| old.min(t)));
            }
        }
        if hit.is_some() {
            return hit;
        }
        let axis = if next.x < next.y && next.x < next.z {
            0
        } else if next.y < next.z {
            1
        } else {
            2
        };
        traveled = next[axis];
        next[axis] += inv[axis];
        cell[axis] += step[axis];
    }
    None
}

/// GameRenderer.bobView: applied before camera rotation, in view space.
/// Java intentionally extrapolates walkDist by the current tick delta.
pub fn bob_view(walk: &crate::entity::player_model::WalkAnimation, alpha: f32) -> Mat4 {
    let phase = -(walk.walk_dist + (walk.walk_dist - walk.previous_walk_dist) * alpha) * std::f32::consts::PI;
    let bob = walk.previous_bob + (walk.bob - walk.previous_bob) * alpha;
    Mat4::from_translation(Vec3::new(phase.sin() * bob * 0.5, -(phase.cos() * bob).abs(), 0.0))
        * Mat4::from_rotation_z((phase.sin() * bob * 3.0).to_radians())
        * Mat4::from_rotation_x(((phase - 0.2).cos() * bob).abs() * 5f32.to_radians())
}

/// GameRenderer.bobHurt: a ten-tick hurt interval, quartic envelope and
/// fourteen-degree directional tilt, plus the twenty-tick death roll.
pub fn bob_hurt(since_damage: f32, death: Option<f32>, direction: f32) -> Mat4 {
    let roll = death.map_or(0.0, |t| (40.0 - 8000.0 / ((t * 20.0).min(20.0) + 200.0)).to_radians());
    let remaining = (1.0 - since_damage / 0.5).clamp(0.0, 1.0);
    let tilt = -(remaining.powi(4) * std::f32::consts::PI).sin() * 14f32.to_radians();
    Mat4::from_rotation_z(roll)
        * Mat4::from_rotation_y(-direction)
        * Mat4::from_rotation_z(tilt)
        * Mat4::from_rotation_y(direction)
}

pub fn view_effect(
    player: &crate::player::Player,
    vitals: &crate::simulation::survival::Vitals,
    alpha: f32,
    bobbing: bool,
) -> Mat4 {
    let elapsed = vitals.since_damage() + alpha * 0.05;
    let hurt = bob_hurt(elapsed, vitals.is_dead().then_some(elapsed), player.animation.hurt_direction);
    if bobbing && !vitals.is_dead() { hurt * bob_view(&player.animation, alpha) } else { hurt }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::world::block::Block;
    struct Grid(Vec<(IVec3, Block)>);
    impl BlockSource for Grid {
        fn block(&self, p: IVec3) -> Option<Block> {
            Some(self.0.iter().find(|(at, _)| *at == p).map_or(Block::AIR, |(_, b)| *b))
        }
    }
    #[test]
    fn cycle_and_front_pitch_are_per_view() {
        let world = Grid(vec![]);
        let mut mode = CameraMode::default();
        mode.cycle();
        assert_eq!(mode, CameraMode::Third);
        let look = Vec3::new(1., 0.5, 0.).normalize();
        let (back, f) = mode.view(&world, DVec3::ZERO, look);
        assert_eq!(f, look);
        assert!((back.length() - 4.).abs() < 1e-6);
        mode.cycle();
        let (front, f) = mode.view(&world, DVec3::ZERO, look);
        assert_eq!(f, -look);
        assert!((front + back).length() < 1e-6);
        mode.cycle();
        assert_eq!(mode, CameraMode::First);
    }
    #[test]
    fn eight_probes_catch_a_corner_missed_by_the_centre_ray() {
        let world = Grid(vec![(IVec3::new(2, 1, 0), Block::STONE)]);
        let eye = DVec3::new(0., 0.95, 0.5);
        assert!(clip(&world, eye, DVec3::X, 4.).is_none());
        let d = max_zoom(&world, eye, DVec3::X, 4.);
        assert!((d - 1.8999).abs() < 1e-4, "{d}");
    }
    #[test]
    fn slabs_clip_their_shape_and_fluids_do_not_clip() {
        let world = Grid(vec![(IVec3::new(2, 0, 0), Block::STONE_SLAB)]);
        assert_eq!(max_zoom(&world, DVec3::new(0., 0.8, 0.5), DVec3::X, 4.), 4.);
        assert!(max_zoom(&world, DVec3::new(0., 0.3, 0.5), DVec3::X, 4.) < 2.);
        let world = Grid(vec![(IVec3::new(2, 0, 0), Block::WATER)]);
        assert_eq!(max_zoom(&world, DVec3::new(0., 0.3, 0.5), DVec3::X, 4.), 4.);
    }
    #[test]
    fn wall_ceiling_and_start_inside_never_put_the_camera_in_a_block() {
        let world = Grid(vec![(IVec3::new(0, 2, 0), Block::STONE), (IVec3::new(-2, 1, 0), Block::STONE)]);
        let eye = DVec3::new(0.5, 1.5, 0.5);
        assert!(max_zoom(&world, eye, DVec3::Y, 4.) < 0.5);
        assert!(max_zoom(&world, eye, DVec3::NEG_X, 4.) < 1.5);
        assert_eq!(max_zoom(&world, DVec3::new(0.5, 2.5, 0.5), DVec3::X, 4.), 0.);
    }
    #[test]
    fn java_bob_translation_and_rotation_amplitudes() {
        let mut walk = crate::entity::player_model::WalkAnimation::default();
        walk.walk_dist = 0.5;
        walk.previous_walk_dist = 0.5;
        walk.bob = 0.1;
        walk.previous_bob = 0.1;
        let bob = bob_view(&walk, 1.0);
        assert!((bob.w_axis.x + 0.05).abs() < 1e-6);
        assert!(bob.w_axis.y.abs() < 1e-6);
        let expected = Mat4::from_translation(Vec3::new(-0.05, 0., 0.))
            * Mat4::from_rotation_z(-0.3f32.to_radians())
            * Mat4::from_rotation_x(((-std::f32::consts::FRAC_PI_2 - 0.2).cos() * 0.1).abs() * 5f32.to_radians());
        assert!(bob.abs_diff_eq(expected, 1e-6));
    }
    #[test]
    fn hurt_uses_quartic_envelope_and_damage_direction() {
        assert!(bob_hurt(0.5, None, 0.).abs_diff_eq(Mat4::IDENTITY, 1e-6));
        let angle = -(0.5f32.powi(4) * std::f32::consts::PI).sin() * 14f32.to_radians();
        assert!(bob_hurt(0.25, None, 0.).abs_diff_eq(Mat4::from_rotation_z(angle), 1e-6));
        assert!(bob_hurt(0.25, None, std::f32::consts::FRAC_PI_2).abs_diff_eq(Mat4::from_rotation_x(-angle), 1e-6));
        let death = bob_hurt(2., Some(1.), 0.);
        assert!(death.abs_diff_eq(Mat4::from_rotation_z((40f32 - 8000. / 220.).to_radians()), 1e-6));
    }
}
