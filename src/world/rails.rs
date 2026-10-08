//! Powered, detector and activator rails (ids 1471..=1506) plus Java's
//! `RailState` shape logic shared by every rail. Plain rails keep ids 500..=509.
//!
//! Special rails are straight or ascending only: id = 1471 + 12 * kind
//! (powered, detector, activator) + 6 * powered + shape (north_south,
//! east_west, ascending east/west/north/south, like [`RailShape`]).
use glam::IVec3;

use super::World;
use super::block::{Block, RailShape, RenderKind};

pub const POWERED_RAIL: Block = Block(1471);
pub const DETECTOR_RAIL: Block = Block(1483);
pub const ACTIVATOR_RAIL: Block = Block(1495);
const FIRST: u16 = 1471;
const LAST: u16 = 1506;
/// First of twelve rail texture layers: kind * 4 + powered * 2 + east_west.
pub const TEXTURE: u16 = 1139;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum RailKind {
    Plain,
    Powered,
    Detector,
    Activator,
}

pub const fn is_special(b: Block) -> bool {
    b.0 >= FIRST && b.0 <= LAST
}

pub const fn special(kind: RailKind, shape: RailShape, powered: bool) -> Block {
    let k = match kind {
        RailKind::Powered => 0,
        RailKind::Detector => 1,
        _ => 2,
    };
    Block(FIRST + k * 12 + powered as u16 * 6 + shape as u16)
}

pub fn special_shape(b: Block) -> Option<RailShape> {
    is_special(b).then(|| RailShape::ALL[((b.0 - FIRST) % 6) as usize])
}

pub fn kind(b: Block) -> Option<RailKind> {
    match b.0 {
        500..=509 => Some(RailKind::Plain),
        FIRST..=1482 => Some(RailKind::Powered),
        1483..=1494 => Some(RailKind::Detector),
        1495..=LAST => Some(RailKind::Activator),
        _ => None,
    }
}

pub fn is_powered(b: Block) -> bool {
    is_special(b) && (b.0 - FIRST) % 12 >= 6
}

/// The same rail with another shape (curves become north_south for straight kinds).
pub fn with_shape(b: Block, shape: RailShape) -> Block {
    match kind(b) {
        Some(RailKind::Plain) => Block::rail(shape),
        Some(k) => special(k, if shape as u16 > 5 { RailShape::NorthSouth } else { shape }, is_powered(b)),
        None => b,
    }
}

pub fn with_power(b: Block, powered: bool) -> Block {
    match special_shape(b).zip(kind(b)) {
        Some((shape, k)) => special(k, shape, powered),
        None => b,
    }
}

/// Item and drop form of a special rail: unpowered, north_south.
pub fn base(b: Block) -> Option<Block> {
    is_special(b).then(|| Block(FIRST + (b.0 - FIRST) / 12 * 12))
}

/// Scheduled-tick family of a detector rail (the engine keys ticks by block family).
pub fn family(b: Block) -> Option<Block> {
    (kind(b) == Some(RailKind::Detector) && is_special(b)).then_some(DETECTOR_RAIL)
}

pub fn palette_ids() -> impl Iterator<Item = u16> {
    [FIRST, 1483, 1495].into_iter()
}

pub const fn registry(id: u16) -> Option<(&'static str, RenderKind, [u16; 6])> {
    if id < FIRST || id > LAST {
        return None;
    }
    let i = id - FIRST;
    let name = match i / 12 {
        0 => "powered rail",
        1 => "detector rail",
        _ => "activator rail",
    };
    let east_west = matches!(i % 6, 1..=3);
    let layer = TEXTURE + (i / 12) * 4 + (i % 12 / 6) * 2 + east_west as u16;
    Some((name, RenderKind::Shaped, [layer; 6]))
}

impl RailShape {
    pub fn is_ascending(self) -> bool {
        matches!(self, Self::AscendingEast | Self::AscendingWest | Self::AscendingNorth | Self::AscendingSouth)
    }

    /// Java `RailState.updateConnections`: neighbouring rail cells relative to this one.
    pub fn connections(self) -> [IVec3; 2] {
        const N: IVec3 = IVec3::NEG_Z;
        const S: IVec3 = IVec3::Z;
        const W: IVec3 = IVec3::NEG_X;
        const E: IVec3 = IVec3::X;
        const UP: IVec3 = IVec3::Y;
        match self {
            Self::NorthSouth => [N, S],
            Self::EastWest => [W, E],
            Self::AscendingEast => [W, E.wrapping_add(UP)],
            Self::AscendingWest => [W.wrapping_add(UP), E],
            Self::AscendingNorth => [N.wrapping_add(UP), S],
            Self::AscendingSouth => [N, S.wrapping_add(UP)],
            Self::SouthEast => [S, E],
            Self::SouthWest => [S, W],
            Self::NorthWest => [N, W],
            Self::NorthEast => [N, E],
        }
    }

    /// Java `AbstractMinecart.EXITS`: the two track ends, lower end carrying -1 in y.
    pub fn exits(self) -> (IVec3, IVec3) {
        let v = IVec3::new;
        match self {
            Self::NorthSouth => (v(0, 0, -1), v(0, 0, 1)),
            Self::EastWest => (v(-1, 0, 0), v(1, 0, 0)),
            Self::AscendingEast => (v(-1, -1, 0), v(1, 0, 0)),
            Self::AscendingWest => (v(-1, 0, 0), v(1, -1, 0)),
            Self::AscendingNorth => (v(0, 0, -1), v(0, -1, 1)),
            Self::AscendingSouth => (v(0, -1, -1), v(0, 0, 1)),
            Self::SouthEast => (v(0, 0, 1), v(1, 0, 0)),
            Self::SouthWest => (v(0, 0, 1), v(-1, 0, 0)),
            Self::NorthWest => (v(0, 0, -1), v(-1, 0, 0)),
            Self::NorthEast => (v(0, 0, -1), v(1, 0, 0)),
        }
    }
}

/// Java `RailState`: a rail and the cells it currently joins.
#[derive(Clone, Copy)]
struct RailState {
    pos: IVec3,
    conns: [IVec3; 3],
    n: usize,
    straight: bool,
}

impl RailState {
    fn new(pos: IVec3, shape: RailShape, straight: bool) -> Self {
        let [a, b] = shape.connections();
        Self { pos, conns: [pos + a, pos + b, IVec3::ZERO], n: 2, straight }
    }

    fn has_connection(&self, p: IVec3) -> bool {
        self.conns[..self.n].iter().any(|c| c.x == p.x && c.z == p.z)
    }

    fn can_connect_to(&self, other: &RailState) -> bool {
        self.has_connection(other.pos) || self.n != 2
    }
}

const NORTH: IVec3 = IVec3::NEG_Z;
const SOUTH: IVec3 = IVec3::Z;
const WEST: IVec3 = IVec3::NEG_X;
const EAST: IVec3 = IVec3::X;

impl World {
    fn rail_block(&self, p: IVec3) -> Option<Block> {
        self.get_block(p).filter(|b| b.is_rail())
    }

    /// Java `RailState.getRail`: the rail at, above or below `p`.
    fn rail_state_near(&self, p: IVec3) -> Option<RailState> {
        [p, p + IVec3::Y, p - IVec3::Y].into_iter().find_map(|q| {
            let b = self.rail_block(q)?;
            Some(RailState::new(q, b.rail_shape()?, kind(b) != Some(RailKind::Plain)))
        })
    }

    /// Java `removeSoftConnections`: drop joins the other rail does not return.
    fn soften(&self, rs: &mut RailState) {
        let mut kept = [IVec3::ZERO; 3];
        let mut m = 0;
        for i in 0..rs.n {
            if let Some(other) = self.rail_state_near(rs.conns[i])
                && other.has_connection(rs.pos)
            {
                kept[m] = other.pos;
                m += 1;
            }
        }
        rs.conns = kept;
        rs.n = m;
    }

    fn has_neighbor_rail(&self, p: IVec3, me: &RailState) -> bool {
        let Some(mut other) = self.rail_state_near(p) else { return false };
        self.soften(&mut other);
        other.can_connect_to(me)
    }

    fn lift(&self, shape: RailShape, pos: IVec3) -> RailShape {
        let up = |d: IVec3| self.rail_block(pos + d + IVec3::Y).is_some();
        match shape {
            RailShape::NorthSouth if up(NORTH) => RailShape::AscendingNorth,
            RailShape::NorthSouth if up(SOUTH) => RailShape::AscendingSouth,
            RailShape::EastWest if up(EAST) => RailShape::AscendingEast,
            RailShape::EastWest if up(WEST) => RailShape::AscendingWest,
            _ => shape,
        }
    }

    /// Java `RailState.connectTo`: a neighbour joins `other`, reshaping it.
    fn rail_connect(&mut self, at: RailState, other: &RailState) {
        let Some(block) = self.rail_block(at.pos) else { return };
        let mut rs = at;
        if rs.n < 3 {
            rs.conns[rs.n] = other.pos;
            rs.n += 1;
        }
        let (n, s, w, e) = (
            rs.has_connection(at.pos + NORTH),
            rs.has_connection(at.pos + SOUTH),
            rs.has_connection(at.pos + WEST),
            rs.has_connection(at.pos + EAST),
        );
        let mut shape = None;
        if n || s {
            shape = Some(RailShape::NorthSouth);
        }
        if w || e {
            shape = Some(RailShape::EastWest);
        }
        if !rs.straight {
            if s && e && !n && !w {
                shape = Some(RailShape::SouthEast);
            }
            if s && w && !n && !e {
                shape = Some(RailShape::SouthWest);
            }
            if n && w && !s && !e {
                shape = Some(RailShape::NorthWest);
            }
            if n && e && !s && !w {
                shape = Some(RailShape::NorthEast);
            }
        }
        let shape = self.lift(shape.unwrap_or(RailShape::NorthSouth), at.pos);
        self.edit(at.pos, with_shape(block, shape), false);
    }

    /// Java `RailState.place`: choose the shape from the neighbours, then
    /// re-join the rails it now touches.
    fn rail_place(&mut self, pos: IVec3, block: Block) {
        let Some(previous) = block.rail_shape() else { return };
        let straight = kind(block) != Some(RailKind::Plain);
        let me = RailState::new(pos, previous, straight);
        let (n, s, w, e) = (
            self.has_neighbor_rail(pos + NORTH, &me),
            self.has_neighbor_rail(pos + SOUTH, &me),
            self.has_neighbor_rail(pos + WEST, &me),
            self.has_neighbor_rail(pos + EAST, &me),
        );
        let (ns, ew) = (n || s, w || e);
        let mut shape = None;
        if ns && !ew {
            shape = Some(RailShape::NorthSouth);
        }
        if ew && !ns {
            shape = Some(RailShape::EastWest);
        }
        let (se, sw, ne, nw) = (s && e, s && w, n && e, n && w);
        if !straight {
            if se && !n && !w {
                shape = Some(RailShape::SouthEast);
            }
            if sw && !n && !e {
                shape = Some(RailShape::SouthWest);
            }
            if nw && !s && !e {
                shape = Some(RailShape::NorthWest);
            }
            if ne && !s && !w {
                shape = Some(RailShape::NorthEast);
            }
        }
        if shape.is_none() {
            if ns && ew {
                shape = Some(previous);
            } else if ns {
                shape = Some(RailShape::NorthSouth);
            } else if ew {
                shape = Some(RailShape::EastWest);
            }
            if !straight {
                // Redstone switches the preferred branch of a plain-rail junction.
                let mut corners = [
                    (nw, RailShape::NorthWest),
                    (ne, RailShape::NorthEast),
                    (sw, RailShape::SouthWest),
                    (se, RailShape::SouthEast),
                ];
                if self.redstone_power(pos) > 0 {
                    corners.reverse();
                }
                for (on, corner) in corners {
                    if on {
                        shape = Some(corner);
                    }
                }
            }
        }
        let shape = self.lift(shape.unwrap_or(previous), pos);
        let placed = with_shape(block, shape);
        let me = RailState::new(pos, shape, straight);
        if placed != block {
            self.edit(pos, placed, false);
            for &c in &me.conns[..me.n] {
                let Some(mut other) = self.rail_state_near(c) else { continue };
                self.soften(&mut other);
                if other.can_connect_to(&me) {
                    self.rail_connect(other, &me);
                }
            }
        }
    }

    fn rail_unsupported(&self, p: IVec3, shape: RailShape) -> bool {
        let solid = |q: IVec3| self.get_block(q).is_none_or(|b| b.is_opaque());
        if !solid(p - IVec3::Y) {
            return true;
        }
        match shape {
            RailShape::AscendingEast => !solid(p + EAST),
            RailShape::AscendingWest => !solid(p + WEST),
            RailShape::AscendingNorth => !solid(p + NORTH),
            RailShape::AscendingSouth => !solid(p + SOUTH),
            _ => false,
        }
    }

    /// Java `PoweredRailBlock.findPoweredRailSignal`, at most eight rails along the line.
    fn rail_chain_powered(&self, p: IVec3, shape: RailShape, forward: bool, depth: u32) -> bool {
        if depth >= 8 {
            return false;
        }
        let mut q = p;
        let mut check_below = true;
        let line = match shape {
            RailShape::NorthSouth => {
                q.z += if forward { 1 } else { -1 };
                RailShape::NorthSouth
            }
            RailShape::EastWest => {
                q.x += if forward { -1 } else { 1 };
                RailShape::EastWest
            }
            RailShape::AscendingEast => {
                if forward {
                    q.x -= 1;
                } else {
                    q.x += 1;
                    q.y += 1;
                    check_below = false;
                }
                RailShape::EastWest
            }
            RailShape::AscendingWest => {
                if forward {
                    q.x -= 1;
                    q.y += 1;
                    check_below = false;
                } else {
                    q.x += 1;
                }
                RailShape::EastWest
            }
            RailShape::AscendingNorth => {
                if forward {
                    q.z += 1;
                } else {
                    q.z -= 1;
                    q.y += 1;
                    check_below = false;
                }
                RailShape::NorthSouth
            }
            RailShape::AscendingSouth => {
                if forward {
                    q.z += 1;
                    q.y += 1;
                    check_below = false;
                } else {
                    q.z -= 1;
                }
                RailShape::NorthSouth
            }
            _ => return false,
        };
        let kind = kind(self.get_block(p).unwrap_or(Block::AIR));
        self.same_rail_with_power(q, kind, forward, depth, line)
            || (check_below && self.same_rail_with_power(q - IVec3::Y, kind, forward, depth, line))
    }

    fn same_rail_with_power(
        &self,
        p: IVec3,
        kind_of: Option<RailKind>,
        forward: bool,
        depth: u32,
        line: RailShape,
    ) -> bool {
        let Some(b) = self.get_block(p) else { return false };
        if kind(b) != kind_of || !is_powered(b) {
            return false;
        }
        let Some(shape) = b.rail_shape() else { return false };
        let across = match line {
            RailShape::EastWest => {
                matches!(shape, RailShape::NorthSouth | RailShape::AscendingNorth | RailShape::AscendingSouth)
            }
            _ => matches!(shape, RailShape::EastWest | RailShape::AscendingEast | RailShape::AscendingWest),
        };
        !across && (self.redstone_power(p) > 0 || self.rail_chain_powered(p, shape, forward, depth + 1))
    }

    /// Re-evaluates a rail after a neighbour changed: support, shape, power.
    pub(super) fn rail_update(&mut self, p: IVec3, b: Block) {
        let Some(shape) = b.rail_shape() else { return };
        if self.rail_unsupported(p, shape) {
            self.spill_block(p, b);
            self.edit(p, Block::AIR, false);
            return;
        }
        self.rail_place(p, b);
        let Some(b) = self.get_block(p).filter(|b| b.is_rail()) else { return };
        if !matches!(kind(b), Some(RailKind::Powered | RailKind::Activator)) {
            return;
        }
        let shape = b.rail_shape().unwrap_or(RailShape::NorthSouth);
        let powered = self.redstone_power(p) > 0
            || self.rail_chain_powered(p, shape, true, 0)
            || self.rail_chain_powered(p, shape, false, 0);
        if powered != is_powered(b) {
            self.edit(p, with_power(b, powered), false);
        }
    }

    /// A detector rail's scheduled check: stay on while a cart sits on it.
    pub(super) fn detector_tick(&mut self, p: IVec3, b: Block) {
        let occupied = self.redstone.contacts.get(&p).is_some_and(|c| c.carts > 0);
        if occupied {
            if !is_powered(b) {
                self.edit(p, with_power(b, true), false);
            }
            self.schedule_redstone(p, 20, 0);
        } else if is_powered(b) {
            self.edit(p, with_power(b, false), false);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::world::chunk::ChunkData;
    use crate::world::terrain::Generator;
    use std::sync::Arc;

    fn world() -> World {
        let mut w = World::new_headless(Arc::new(Generator::new(3)), Default::default(), 2);
        w.insert_chunk(IVec3::new(0, 4, 0), Arc::new(ChunkData::Uniform(Block::AIR)), false);
        w
    }
    fn rail(w: &mut World, p: IVec3, b: Block) {
        w.set_block(p - IVec3::Y, Block::STONE);
        w.set_block(p, b);
    }
    #[test]
    fn normal_rails_connect_curves_slopes_and_drop_without_support() {
        let mut w = world();
        let p = IVec3::new(8, 140, 8);
        rail(&mut w, p, Block::RAIL);
        rail(&mut w, p + EAST, Block::RAIL);
        rail(&mut w, p + SOUTH, Block::RAIL);
        w.tick_redstone();
        assert_eq!(w.get_block(p).unwrap().rail_shape(), Some(RailShape::SouthEast));
        w.set_block(p + SOUTH, Block::AIR);
        rail(&mut w, p + WEST, Block::RAIL);
        rail(&mut w, p + EAST + IVec3::Y, Block::RAIL);
        w.tick_redstone();
        assert_eq!(w.get_block(p).unwrap().rail_shape(), Some(RailShape::AscendingEast));
        w.set_block(p - IVec3::Y, Block::AIR);
        w.tick_redstone();
        assert_eq!(w.get_block(p), Some(Block::AIR));
        assert!(w.drops.iter().any(|(_, s)| s.item == crate::item::Item::from_block(Block::RAIL)));
    }
    #[test]
    fn powered_and_activator_chains_reach_eight_rails_and_turn_off() {
        for kind in [RailKind::Powered, RailKind::Activator] {
            let mut w = world();
            for x in 2..15 {
                rail(&mut w, IVec3::new(x, 140, 8), special(kind, RailShape::EastWest, false));
            }
            let source = IVec3::new(2, 139, 8);
            w.set_block(source, super::super::redstone_blocks::REDSTONE_BLOCK);
            w.tick_redstone();
            for x in 2..15 {
                assert_eq!(is_powered(w.get_block(IVec3::new(x, 140, 8)).unwrap()), x <= 10, "kind {kind:?}, x {x}");
            }
            w.set_block(source, Block::STONE);
            w.tick_redstone();
            for x in 2..15 {
                assert!(!is_powered(w.get_block(IVec3::new(x, 140, 8)).unwrap()));
            }
        }
    }
    #[test]
    fn special_rails_do_not_curve_or_cross_power_between_kinds() {
        let mut w = world();
        let p = IVec3::new(8, 140, 8);
        rail(&mut w, p, POWERED_RAIL);
        rail(&mut w, p + EAST, ACTIVATOR_RAIL);
        rail(&mut w, p + SOUTH, Block::RAIL);
        w.set_block(p - IVec3::Y, super::super::redstone_blocks::REDSTONE_BLOCK);
        w.tick_redstone();
        assert!((w.get_block(p).unwrap().rail_shape().unwrap() as u8) < 6);
        assert!(!is_powered(w.get_block(p + EAST).unwrap()));
    }
}
