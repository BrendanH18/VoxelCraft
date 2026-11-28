//! v1.0 gap Track 5 (GEN-04c): portal linking and arrival on the corpus
//! seeds (the M3 corpus's and the golden pins'), checked against Java 26.3's
//! portal rules (`tests/fixtures/java/26.3/portals.json`).
//!
//! For each seed a portal is built on the Overworld's surface near the
//! origin and a player steps into it off-centre. The trip runs the code
//! travel runs (`app::dimension`): the target is the scaled position
//! clamped to the world border; the Nether, generated fresh, has no portal
//! within 16 blocks, so `createPortal` builds one; the player arrives at
//! the same place in it, clear of blocks, standing on its frame. Back in
//! the Overworld the search (128 blocks) finds the portal they left by, and
//! a second trip into the Nether finds the one built (16 blocks): the pair
//! is linked. [`PORTAL_CORPUS`] pins each seed's portals and arrival, so a
//! change to where portals generate or put travellers is a visible change
//! to this table.

use std::sync::Arc;
use std::time::{Duration, Instant};

use glam::{DVec3, IVec3};

use super::{Dimension, Generation, Generator};
use crate::world::World;
use crate::world::block::Block;
use crate::world::portal::{
    BORDER, CREATE_RADIUS, NETHER_PORTAL_RADIUS, OVERWORLD_PORTAL_RADIUS, PortalRect, PortalTrip, exit_position,
    relative_position,
};

const SEEDS: [u64; 2] = [12345, 42];

/// A seed's pinned trip.
type Trip = (u64, [i32; 3], [i32; 3], [i32; 3]);

/// By seed: the Overworld portal's bottom corner, the Nether portal built
/// for it, and where the player arrives there (thousandths of a block).
const PORTAL_CORPUS: &[Trip] =
    &[(12345, [8, 64, 8], [4, 63, 5], [5550, 63000, 5800]), (42, [8, 69, 8], [-2, 74, 4], [-450, 74000, 4800])];

/// A world of `dimension` loaded around `at`.
fn loaded(seed: u64, dimension: Dimension, at: IVec3) -> World {
    let generator = Generator::with_generation(seed, dimension, Generation::LATEST);
    let mut world = World::new(Arc::new(generator), Default::default(), 2);
    let deadline = Instant::now() + Duration::from_secs(60);
    let ready = |w: &World| {
        [(-20, -20), (20, -20), (-20, 20), (20, 20), (0, 0)]
            .iter()
            .all(|&(dx, dz)| w.column_loaded(at.x + dx, at.z + dz))
            && w.pending_jobs() == 0
    };
    while !ready(&world) {
        world.update(at.as_dvec3());
        assert!(Instant::now() < deadline, "{} around {at} never loaded", dimension.name());
        std::thread::sleep(Duration::from_millis(1));
    }
    world
}

/// The top block of column `(x, z)` that blocks motion or holds a fluid.
fn surface(world: &World, x: i32, z: i32) -> i32 {
    (world.min_y()..world.max_y())
        .rev()
        .find(|&y| world.get_block(IVec3::new(x, y, z)).is_some_and(|b| b.is_solid() || b.is_fluid()))
        .expect("a surface")
}

/// A lit 2 x 3 portal along X whose bottom corner is `at`, its frame
/// standing on a platform.
fn build_portal(world: &mut World, at: IVec3) {
    for dx in -2..=3 {
        for dz in -2..=2 {
            world.set_block(at + IVec3::new(dx, -1, dz), Block::OBSIDIAN);
            for dy in 0..5 {
                world.set_block(at + IVec3::new(dx, dy, dz), Block::AIR);
            }
        }
    }
    for i in -1..=2 {
        world.set_block(at + IVec3::new(i, 3, 0), Block::OBSIDIAN);
    }
    for y in 0..3 {
        world.set_block(at + IVec3::new(-1, y, 0), Block::OBSIDIAN);
        world.set_block(at + IVec3::new(2, y, 0), Block::OBSIDIAN);
    }
    assert!(world.light_portal(at), "the frame lights");
}

/// `NetherPortalBlock.getPortalDestination`'s trip for a player at `pos`
/// in the portal around `cell`, as `app::dimension` sets it out.
fn trip(world: &World, cell: IVec3, pos: DVec3, scale: f64) -> PortalTrip {
    let (rect, along_x) = world.portal_rect(cell);
    let (w, h) = (f64::from(0.6f32), f64::from(1.8f32));
    PortalTrip {
        target: BORDER.clamp(DVec3::new(pos.x * scale, pos.y, pos.z * scale)),
        along_x,
        offset: relative_position(rect, along_x, pos, w, h),
        vel: DVec3::ZERO,
        spectator: false,
    }
}

/// Where the player lands in `rect` (along X if `along_x`).
fn arrive(world: &World, rect: PortalRect, along_x: bool, trip: &PortalTrip) -> DVec3 {
    let (w, h) = (f64::from(0.6f32), f64::from(1.8f32));
    world.collision_free_position(exit_position(rect, along_x, trip.offset, w, h), w, h)
}

#[test]
fn portals_link_and_arrive_on_the_corpus_seeds() {
    let mut got = Vec::new();
    let mut failures = Vec::new();
    for seed in SEEDS {
        // The Overworld portal, on the surface by (8, 8).
        let probe = loaded(seed, Dimension::Overworld, IVec3::new(8, 100, 8));
        let at = IVec3::new(8, surface(&probe, 8, 8) + 2, 8);
        drop(probe);
        let mut overworld = loaded(seed, Dimension::Overworld, at);
        build_portal(&mut overworld, at);
        let (entry, entry_x) = overworld.portal_rect(at + IVec3::new(1, 1, 0));
        assert_eq!((entry, entry_x), (PortalRect { min: at, width: 2, height: 3 }, true));
        // Off-centre: near the portal's east side, a little in front.
        let pos = at.as_dvec3() + DVec3::new(1.55, 0.0, 0.8);
        let out = trip(&overworld, at + IVec3::X, pos, 1.0 / 8.0);
        assert_eq!(out.target, (pos * DVec3::new(0.125, 1.0, 0.125)).floor().as_ivec3());

        // The Nether: no portal yet, so one is built within 16 blocks.
        let mut nether = loaded(seed, Dimension::Nether, out.target);
        assert_eq!(nether.find_portal(out.target, NETHER_PORTAL_RADIUS), None, "seed {seed}: a fresh Nether");
        let built = nether.create_portal(out.target, out.along_x).expect("a Nether portal");
        let d = built.min - out.target;
        if d.x.abs() > CREATE_RADIUS + 1 || d.z.abs() > CREATE_RADIUS + 1 {
            failures.push(format!("seed {seed}: portal at {} is beyond 16 of {}", built.min, out.target));
        }
        let (rect, along_x) = nether.portal_rect(built.min);
        assert_eq!(rect, built, "seed {seed}: the built portal measures 2 x 3");
        assert_eq!(along_x, out.along_x, "seed {seed}: it follows the entry portal's axis");
        let landed = arrive(&nether, rect, along_x, &out);
        // Arrival safety: inside the portal, clear of blocks, on its frame.
        let (lo, hi) = (landed - DVec3::new(0.3, 0.0, 0.3), landed + DVec3::new(0.3, 1.8, 0.3));
        if !crate::physics::no_collision(&nether, lo, hi) {
            failures.push(format!("seed {seed}: arrival {landed} collides"));
        }
        let below = (landed - DVec3::Y * 0.5).floor().as_ivec3();
        if nether.get_block(below) != Some(Block::OBSIDIAN) {
            failures.push(format!("seed {seed}: arrival {landed} stands on {:?}", nether.get_block(below)));
        }
        let back = relative_position(rect, along_x, landed, f64::from(0.6f32), f64::from(1.8f32));
        if (back - out.offset).abs().max_element() > 0.31 {
            failures.push(format!("seed {seed}: arrived at {back}, entered at {}", out.offset));
        }

        // Back out: the Overworld search finds the portal left by.
        let home = trip(&nether, rect.min, landed, 8.0);
        let found = overworld.find_portal(home.target, OVERWORLD_PORTAL_RADIUS);
        if found.map(|p| overworld.portal_rect(p).0) != Some(entry) {
            failures.push(format!("seed {seed}: from {} the Overworld finds {found:?}", home.target));
        }
        // And in again: the Nether search finds the portal built.
        let again = nether.find_portal(out.target, NETHER_PORTAL_RADIUS);
        if again.map(|p| nether.portal_rect(p).0) != Some(rect) {
            failures.push(format!("seed {seed}: the second trip finds {again:?}"));
        }
        got.push((seed, at.to_array(), rect.min.to_array(), (landed * 1000.0).round().as_ivec3().to_array()));
    }
    for g in &got {
        println!("PORTAL ({}, {:?}, {:?}, {:?}),", g.0, g.1, g.2, g.3);
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
    for (got, want) in got.iter().zip(PORTAL_CORPUS) {
        assert_eq!(got, want, "seed {}: portals moved", got.0);
    }
}
