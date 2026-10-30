//! Java's `EntityFluidInteraction` for the player: how deep it stands in
//! water and lava, whether its eyes are under, and the current pushing it
//! (`FlowingFluid.getFlow`, `CurrentAccumulator.applyTo`).

use glam::{DVec3, IVec3};

use crate::physics::BlockSource;
use crate::world::block::{Block, Fluid};

/// A cell's fluid as Java's `FluidState`: kind, amount (1..=8) and whether
/// it is a source or falling.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct FluidCell {
    pub fluid: Fluid,
    pub amount: u8,
    pub source: bool,
    pub falling: bool,
}

/// Java's fluid state of a block. Flowing water level `n` holds `8 - n`;
/// VoxelCraft's three flowing lava levels are Java's Overworld lava levels
/// 2, 4 and 6 (`LavaFluid.getDropOff` is 2). Waterlogged blocks hold a source.
pub fn fluid_at(b: Block) -> Option<FluidCell> {
    let cell = |fluid, amount, source, falling| Some(FluidCell { fluid, amount, source, falling });
    match b {
        Block::WATER => cell(Fluid::Water, 8, true, false),
        Block::FALLING_WATER => cell(Fluid::Water, 8, false, true),
        Block::LAVA => cell(Fluid::Lava, 8, true, false),
        Block::FALLING_LAVA => cell(Fluid::Lava, 8, false, true),
        _ if b.is_waterlogged() || crate::world::bubble::is_column(b) => cell(Fluid::Water, 8, true, false),
        _ => match (b.fluid(), b.fluid_level()) {
            (Some(Fluid::Water), Some(n)) => cell(Fluid::Water, 8 - n, false, false),
            (Some(Fluid::Lava), Some(n)) => cell(Fluid::Lava, 8 - 2 * n, false, false),
            _ => None,
        },
    }
}

fn fluid_of<W: BlockSource + ?Sized>(world: &W, p: IVec3) -> Option<FluidCell> {
    world.block(p).and_then(fluid_at)
}

/// `FluidState.getOwnHeight`: the amount over nine, as a float.
fn own_height(cell: Option<FluidCell>) -> f32 {
    cell.map_or(0.0, |c| c.amount as f32 / 9.0)
}

/// `FlowingFluid.getHeight`: full when the same fluid is above.
pub fn height<W: BlockSource + ?Sized>(world: &W, p: IVec3, cell: FluidCell) -> f32 {
    if fluid_of(world, p + IVec3::Y).is_some_and(|a| a.fluid == cell.fluid) { 1.0 } else { own_height(Some(cell)) }
}

/// `FluidState.getHeightForCamera`: a source under a sturdy bottom face fills its cell.
pub(super) fn camera_height<W: BlockSource + ?Sized>(world: &W, p: IVec3, cell: FluidCell) -> f32 {
    let above = p + IVec3::Y;
    if cell.source && world.block(above).is_some_and(|b| crate::world::mount_states::face_sturdy(b, IVec3::NEG_Y)) {
        return 1.0;
    }
    height(world, p, cell)
}

/// `FlowingFluid.isSolidFace` for falling fluid beside `p`.
fn solid_face<W: BlockSource + ?Sized>(world: &W, p: IVec3, side: IVec3, fluid: Fluid) -> bool {
    if fluid_of(world, p).is_some_and(|c| c.fluid == fluid) {
        return false;
    }
    // `IceBlock` (ice and frosted ice) never counts. Java asks for the face
    // on the `side` the neighbour was found, its far face.
    world.block(p).is_some_and(|b| b != Block::ICE && crate::world::mount_states::face_sturdy(b, side))
}

/// `FlowingFluid.getFlow`: the normalised direction the fluid at `p` pushes.
pub fn flow<W: BlockSource + ?Sized>(world: &W, p: IVec3, cell: FluidCell) -> DVec3 {
    const SIDES: [IVec3; 4] = [IVec3::NEG_Z, IVec3::X, IVec3::Z, IVec3::NEG_X];
    let (mut fx, mut fz) = (0.0f64, 0.0f64);
    let own = own_height(Some(cell));
    let affects = |c: Option<FluidCell>| c.is_none_or(|c| c.fluid == cell.fluid);
    for side in SIDES {
        let n = p + side;
        let neighbour = fluid_of(world, n);
        if !affects(neighbour) {
            continue;
        }
        let mut neighbour_height = own_height(neighbour);
        let mut distance = 0.0f32;
        if neighbour_height == 0.0 {
            // `BLOCKS_FLUID_FLOW` is the colliding (`blocks_motion`) blocks.
            if !world.block(n).is_some_and(Block::is_solid) {
                let below = fluid_of(world, n - IVec3::Y);
                if affects(below) {
                    neighbour_height = own_height(below);
                    if neighbour_height > 0.0 {
                        distance = own - (neighbour_height - 0.8888889);
                    }
                }
            }
        } else if neighbour_height > 0.0 {
            distance = own - neighbour_height;
        }
        if distance != 0.0 {
            fx += side.x as f64 * distance as f64;
            fz += side.z as f64 * distance as f64;
        }
    }
    let mut flow = DVec3::new(fx, 0.0, fz);
    if cell.falling
        && SIDES.iter().any(|&side| {
            solid_face(world, p + side, side, cell.fluid) || solid_face(world, p + side + IVec3::Y, side, cell.fluid)
        })
    {
        flow = normalize(flow) + DVec3::new(0.0, -6.0, 0.0);
    }
    normalize(flow)
}

/// `Vec3.normalize`: zero below `1.0E-5`.
fn normalize(v: DVec3) -> DVec3 {
    let len = v.length();
    if len < 1.0e-5 { DVec3::ZERO } else { v / len }
}

/// What one `EntityFluidInteraction.update` found.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct FluidContact {
    /// How far the deepest fluid of each kind rises above the feet.
    pub water_height: f64,
    pub lava_height: f64,
    pub eye_in_water: bool,
    pub eye_in_lava: bool,
    water_current: Current,
    lava_current: Current,
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
struct Current {
    height: f64,
    sum: DVec3,
    count: u32,
}

impl Current {
    /// `CurrentAccumulator.applyTo` for a player: the average flow.
    fn impulse(&self, movement: DVec3, scale: f64) -> DVec3 {
        if self.count == 0 || self.sum.length_squared() < 1.0e-5_f32 as f64 {
            return DVec3::ZERO;
        }
        let mut impulse = self.sum * (1.0 / self.count as f64) * scale;
        if movement.x.abs() < 0.003 && movement.z.abs() < 0.003 && impulse.length() < 0.0045000000000000005 {
            impulse = normalize(impulse) * 0.0045000000000000005;
        }
        impulse
    }
}

impl FluidContact {
    pub fn in_water(&self) -> bool {
        self.water_height > 0.0
    }

    pub fn in_lava(&self) -> bool {
        self.lava_height > 0.0
    }

    pub fn height(&self, fluid: Fluid) -> f64 {
        match fluid {
            Fluid::Water => self.water_height,
            Fluid::Lava => self.lava_height,
        }
    }

    /// The push water (0.014) and lava give this tick, given the movement before it.
    pub fn current_impulse(&self, movement: DVec3, fast_lava: bool) -> DVec3 {
        let mut push = DVec3::ZERO;
        if self.in_water() {
            push += self.water_current.impulse(movement, 0.014);
        }
        if self.in_lava() {
            let scale = if fast_lava { 0.007 } else { 0.0023333333333333335 };
            push += self.lava_current.impulse(movement + push, scale);
        }
        push
    }
}

/// `EntityFluidInteraction.update` for a box `min..max` (the bounding box)
/// with eyes at `eye`. Currents are gathered only when `pushed`.
pub fn contact<W: BlockSource + ?Sized>(world: &W, min: DVec3, max: DVec3, eye: DVec3, pushed: bool) -> FluidContact {
    let margin = DVec3::splat(0.001);
    let (bmin, bmax) = (min + margin, max - margin);
    let lo = bmin.floor().as_ivec3();
    let hi = bmax.ceil().as_ivec3() - IVec3::ONE;
    let eye_cell = IVec3::new(eye.x.floor() as i32, 0, eye.z.floor() as i32);
    let mut out = FluidContact::default();
    for x in lo.x..=hi.x {
        for y in lo.y..=hi.y {
            for z in lo.z..=hi.z {
                let p = IVec3::new(x, y, z);
                let Some(cell) = fluid_of(world, p) else { continue };
                let bottom = y as f64;
                let top = bottom + height(world, p, cell) as f64;
                if top < bmin.y {
                    continue;
                }
                if x == eye_cell.x
                    && z == eye_cell.z
                    && eye.y >= bottom
                    && eye.y <= bottom + camera_height(world, p, cell) as f64
                {
                    match cell.fluid {
                        Fluid::Water => out.eye_in_water = true,
                        Fluid::Lava => out.eye_in_lava = true,
                    }
                }
                let (tracked, current) = match cell.fluid {
                    Fluid::Water => (&mut out.water_height, &mut out.water_current),
                    Fluid::Lava => (&mut out.lava_height, &mut out.lava_current),
                };
                *tracked = tracked.max(top - min.y);
                if pushed {
                    let mut push = flow(world, p, cell);
                    current.height = current.height.max(*tracked);
                    if current.height < 0.4 {
                        push *= current.height;
                    }
                    current.sum += push;
                    current.count += 1;
                }
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::physics::test_util::Grid;

    #[test]
    fn java_fluid_amounts_and_heights() {
        let water = fluid_at(Block::WATER).unwrap();
        assert_eq!((water.amount, water.source), (8, true));
        assert_eq!(fluid_at(Block(24)).unwrap().amount, 7, "water level 1");
        assert_eq!(fluid_at(Block(30)).unwrap().amount, 1, "water level 7");
        assert_eq!(fluid_at(Block(38)).unwrap().amount, 6, "lava level 2 in Java");
        assert_eq!(fluid_at(Block(40)).unwrap().amount, 2, "lava level 6 in Java");
        let mut world = Grid::flat(0);
        let p = IVec3::new(0, 0, 0);
        world.set(p, Block::WATER);
        assert_eq!(height(&world, p, water), 8.0 / 9.0);
        world.set(p + IVec3::Y, Block::WATER);
        assert_eq!(height(&world, p, water), 1.0);
    }

    #[test]
    fn flow_runs_downhill_from_a_source() {
        let mut world = Grid::flat(0);
        let p = IVec3::new(0, 0, 0);
        world.set(p, Block::WATER);
        world.set(p + IVec3::X, Block(24));
        let f = flow(&world, p + IVec3::X, fluid_at(Block(24)).unwrap());
        // Air on three sides with stone below is level ground: only the
        // source pushes, along +X.
        assert!((f - DVec3::X).length() < 1e-9, "{f:?}");
    }

    #[test]
    fn standing_in_a_source_reports_its_depth_and_eyes() {
        let mut world = Grid::flat(0);
        world.set(IVec3::new(0, 0, 0), Block::WATER);
        world.set(IVec3::new(0, 1, 0), Block::WATER);
        let feet = DVec3::new(0.5, 0.0, 0.5);
        let c = contact(
            &world,
            feet - DVec3::new(0.3, 0.0, 0.3),
            feet + DVec3::new(0.3, 1.8, 0.3),
            feet + DVec3::Y * 1.62,
            true,
        );
        assert!((c.water_height - (1.0 + 8.0 / 9.0f32 as f64)).abs() < 1e-6);
        assert!(c.eye_in_water);
        assert!(!c.in_lava());
    }
}
