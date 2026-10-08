//! Piston block events, saved moving blocks, and observer pulses.
use glam::{DVec3, IVec3};
use rustc_hash::FxHashMap;

use super::{
    World,
    block::Block,
    redstone_blocks::{self as r, Component},
};

const SIDES: [IVec3; 6] = [IVec3::Z, IVec3::NEG_Z, IVec3::X, IVec3::NEG_X, IVec3::Y, IVec3::NEG_Y];

#[derive(Clone, Copy)]
struct MovingBlock {
    block: Block,
    from: IVec3,
    base: IVec3,
    started: u64,
    install: bool,
}

#[derive(Default)]
pub(super) struct AutomationState {
    moving: FxHashMap<IVec3, MovingBlock>,
    finished: Vec<IVec3>,
}

#[derive(PartialEq)]
enum Reaction {
    Move,
    Destroy,
    Block,
    Empty,
}

impl World {
    pub(super) fn track_automation(&mut self, p: IVec3, old: Block, new: Block) {
        if old == new {
            return;
        }
        // An observer watches state changes on precisely its front face.
        for d in SIDES {
            let q = p - d;
            if matches!(self.get_block(q).and_then(r::component), Some(Component::Observer { facing, on: false }) if r::direction(facing)==d)
            {
                self.schedule_redstone(q, 2, 0);
            }
        }
        if new != r::MOVING {
            self.automation.moving.remove(&p);
        }
        // Remove abandoned heads when their base is broken. Retracting a piston
        // keeps its family, so this does not interfere with the moving head.
        if let Some(Component::Piston { facing, extended: true, .. }) = r::component(old)
            && r::base(old) != r::base(new)
        {
            let front = p + r::direction(facing);
            if matches!(self.get_block(front).and_then(r::component), Some(Component::PistonHead { .. })) {
                self.edit(front, Block::AIR, false);
            }
        }
    }

    pub(super) fn automation_update(&mut self, p: IVec3, b: Block) {
        match r::component(b) {
            Some(Component::Piston { facing, extended, .. }) if self.piston_powered(p, facing) != extended => {
                self.schedule_redstone(p, 0, 0)
            }
            // A loaded powered observer needs its falling edge even if an old
            // save did not carry the scheduled tick.
            Some(Component::Observer { on: true, .. }) => self.schedule_redstone(p, 2, 0),
            _ => {}
        }
    }

    pub(super) fn automation_tick(&mut self, p: IVec3, b: Block) {
        match r::component(b) {
            Some(Component::Observer { facing, on }) => {
                self.edit(p, r::observer(facing, !on), false);
                if !on {
                    self.schedule_redstone(p, 2, 0);
                }
            }
            Some(Component::Piston { facing, sticky, extended }) => {
                if self.automation.moving.values().any(|m| m.base == p) {
                    self.schedule_redstone(p, 2, 0);
                    return;
                }
                let powered = self.piston_powered(p, facing);
                if powered && !extended {
                    self.extend_piston(p, facing, sticky);
                } else if !powered && extended {
                    self.retract_piston(p, facing, sticky);
                }
            }
            _ => {}
        }
    }

    fn piston_powered(&self, p: IVec3, facing: u8) -> bool {
        let front = r::direction(facing);
        SIDES.into_iter().any(|d|d!=front&&self.signal_from(p+d,-d,true)>0)
            // Java quasi-connectivity: a piston also checks the block above,
            // except the downward face. A block update still triggers its event.
            || SIDES.into_iter().any(|d| d!=IVec3::NEG_Y&&self.signal_from(p+IVec3::Y+d,-d,true)>0)
    }

    fn piston_reaction(&self, p: IVec3, b: Block) -> Reaction {
        if p.y < 0 || p.y >= super::WORLD_HEIGHT {
            return Reaction::Block;
        }
        if b == Block::AIR || b.is_fluid() {
            return Reaction::Empty;
        }
        if !b.hardness().is_finite()
            || matches!(
                b,
                Block::OBSIDIAN
                    | Block::NETHERITE_BLOCK
                    | Block::SPAWNER
                    | Block::END_PORTAL_FRAME
                    | Block::NETHER_PORTAL
                    | Block::END_PORTAL
                    | Block::END_GATEWAY
            )
            || self.chest(p).is_some()
            || self.furnace(p).is_some()
            || self.brewing_stand(p).is_some()
            || matches!(
                r::component(b),
                Some(Component::Piston { extended: true, .. } | Component::PistonHead { .. } | Component::Moving)
            )
        {
            return Reaction::Block;
        }
        if b.kind() == super::block::RenderKind::Cross
            || b.is_door()
            || b.is_fire()
            || matches!(
                r::component(b),
                Some(
                    Component::Wire(_)
                        | Component::Lever { .. }
                        | Component::Button { .. }
                        | Component::Torch { .. }
                        | Component::Repeater { .. }
                        | Component::Comparator { .. }
                        | Component::Plate { .. }
                        | Component::IronDoor { .. }
                )
            )
        {
            return Reaction::Destroy;
        }
        Reaction::Move
    }

    fn start_piston_move(&mut self, from: IVec3, to: IVec3, block: Block, base: IVec3, install: bool) {
        if install {
            self.edit(to, r::MOVING, false);
        }
        self.automation.moving.insert(to, MovingBlock { block, from, base, started: self.redstone.tick, install });
    }

    fn extend_piston(&mut self, p: IVec3, facing: u8, sticky: bool) {
        let d = r::direction(facing);
        let mut push = [(IVec3::ZERO, Block::AIR); 12];
        let mut count = 0;
        let mut end = p + d;
        let destroy = loop {
            let Some(b) = self.get_block(end) else {
                return;
            };
            match self.piston_reaction(end, b) {
                Reaction::Empty => break None,
                Reaction::Destroy => break Some(b),
                Reaction::Block => return,
                Reaction::Move => {
                    if count == 12 {
                        return;
                    }
                    push[count] = (end, b);
                    count += 1;
                    end += d;
                }
            }
        };
        if let Some(b) = destroy {
            self.spill_block(end, b);
            self.edit(end, Block::AIR, false);
        }
        for &(from, block) in push[..count].iter().rev() {
            self.edit(from, Block::AIR, false);
            self.start_piston_move(from, from + d, block, p, true);
        }
        self.edit(p, r::piston(facing, sticky, true), false);
        self.start_piston_move(p, p + d, r::piston_head(facing, sticky), p, true);
    }

    fn retract_piston(&mut self, p: IVec3, facing: u8, sticky: bool) {
        let d = r::direction(facing);
        self.edit(p + d, Block::AIR, false);
        self.edit(p, r::piston(facing, sticky, false), false);
        self.start_piston_move(p + d, p, r::piston_head(facing, sticky), p, false);
        if sticky {
            let from = p + d * 2;
            if let Some(block) = self.get_block(from)
                && self.piston_reaction(from, block) == Reaction::Move
            {
                self.edit(from, Block::AIR, false);
                self.start_piston_move(from, p + d, block, p, true);
            }
        }
    }

    pub(super) fn finish_piston_moves(&mut self) {
        if self.automation.moving.is_empty() {
            return;
        }
        let mut finished = std::mem::take(&mut self.automation.finished);
        finished.extend(self.automation.moving.iter().filter_map(|(&p, m)| {
            (self.redstone.tick.saturating_sub(m.started) >= 2 && self.get_block(p).is_some()).then_some(p)
        }));
        for &p in &finished {
            let Some(m) = self.automation.moving.remove(&p) else {
                continue;
            };
            if m.install && self.get_block(p) == Some(r::MOVING) {
                // Broken bases must not leave floating piston heads behind.
                let head = matches!(r::component(m.block), Some(Component::PistonHead { .. }));
                let valid = matches!(
                    self.get_block(m.base).and_then(r::component),
                    Some(Component::Piston { extended: true, .. })
                );
                self.edit(p, if head && !valid { Block::AIR } else { m.block }, false);
            }
            self.redstone_changed(m.base);
        }
        finished.clear();
        self.automation.finished = finished;
    }

    /// Interpolates the two-tick motion without a separate per-frame collection.
    pub fn moving_piston_blocks(&self, alpha: f64) -> impl Iterator<Item = (Block, DVec3)> + '_ {
        self.automation.moving.iter().map(move |(&to, m)| {
            let elapsed = self.redstone.tick.saturating_sub(m.started) as f64;
            let progress = ((elapsed + alpha) / 2.0).clamp(0.0, 1.0);
            (m.block, m.from.as_dvec3().lerp(to.as_dvec3(), progress))
        })
    }

    pub fn automation_to_string(&self) -> String {
        let mut entries = vec!["v1".to_string()];
        for (to, m) in &self.automation.moving {
            entries.push(format!(
                "m,{},{},{},{},{},{},{},{},{},{},{},{}",
                to.x,
                to.y,
                to.z,
                m.block.0,
                m.from.x,
                m.from.y,
                m.from.z,
                m.base.x,
                m.base.y,
                m.base.z,
                self.redstone.tick.saturating_sub(m.started),
                m.install as u8
            ));
        }
        entries.join("|")
    }

    pub fn load_automation(&mut self, text: &str) {
        let mut entries = text.split('|');
        if entries.next() != Some("v1") {
            return;
        }
        self.automation = AutomationState::default();
        for entry in entries {
            let Some(rest) = entry.strip_prefix("m,") else {
                continue;
            };
            let fields: Vec<i64> = rest.split(',').filter_map(|x| x.parse().ok()).collect();
            let &[x, y, z, block, fx, fy, fz, bx, by, bz, age, install] = fields.as_slice() else {
                continue;
            };
            if !(0..4096).contains(&block) || !(0..=2).contains(&age) || !(0..=1).contains(&install) {
                continue;
            }
            let xyz = [x, y, z, fx, fy, fz, bx, by, bz];
            if xyz.into_iter().any(|n| i32::try_from(n).is_err()) {
                continue;
            }
            self.automation.moving.insert(
                IVec3::new(x as i32, y as i32, z as i32),
                MovingBlock {
                    block: Block(block as u16),
                    from: IVec3::new(fx as i32, fy as i32, fz as i32),
                    base: IVec3::new(bx as i32, by as i32, bz as i32),
                    started: self.redstone.tick.saturating_sub(age as u64),
                    install: install == 1,
                },
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::world::{chunk::ChunkData, terrain::Generator};
    use std::sync::Arc;
    const P: IVec3 = IVec3::new(4, 145, 8);
    fn world() -> World {
        let mut w = World::new_headless(Arc::new(Generator::new(7)), Default::default(), 2);
        w.insert_chunk(IVec3::new(0, 4, 0), Arc::new(ChunkData::Uniform(Block::AIR)), false);
        w
    }
    fn put(w: &mut World, p: IVec3, b: Block) {
        assert!(w.edit(p, b, false));
    }
    fn ticks(w: &mut World, n: usize) {
        for _ in 0..n {
            w.tick_redstone();
        }
    }
    #[test]
    fn piston_pushes_twelve_atomically_but_rejects_thirteen_and_containers() {
        for (count, blocked) in [(12, false), (13, true)] {
            let mut w = world();
            // Adjacent chunks may be loaded without changing the push limit.
            w.insert_chunk(IVec3::new(1, 4, 0), Arc::new(ChunkData::Uniform(Block::AIR)), false);
            put(&mut w, P, r::piston(2, false, false));
            for i in 1..=count {
                put(&mut w, P + IVec3::X * i, Block::STONE);
            }
            put(&mut w, P - IVec3::X, r::REDSTONE_BLOCK);
            ticks(&mut w, 3);
            assert_eq!(w.get_block(P), Some(r::piston(2, false, !blocked)));
            assert_eq!(w.get_block(P + IVec3::X), Some(if blocked { Block::STONE } else { r::piston_head(2, false) }));
            assert_eq!(w.get_block(P + IVec3::X * (count + 1)), Some(if blocked { Block::AIR } else { Block::STONE }));
        }
        for block in [
            Block::OBSIDIAN,
            Block::BEDROCK,
            Block::NETHERITE_BLOCK,
            Block::CHEST,
            Block::FURNACE,
            Block::BREWING_STAND,
            Block::END_PORTAL,
        ] {
            let mut w = world();
            put(&mut w, P, r::PISTON);
            put(&mut w, P + IVec3::Z, block);
            put(&mut w, P - IVec3::X, r::REDSTONE_BLOCK);
            ticks(&mut w, 3);
            assert_eq!(w.get_block(P), Some(r::PISTON), "{}", block.name());
            assert_eq!(w.get_block(P + IVec3::Z), Some(block));
        }
    }
    #[test]
    fn sticky_pulls_one_while_normal_piston_leaves_it_and_front_power_is_ignored() {
        for sticky in [false, true] {
            let mut w = world();
            put(&mut w, P, r::piston(2, sticky, false));
            put(&mut w, P + IVec3::X, Block::STONE);
            put(&mut w, P - IVec3::X, r::REDSTONE_BLOCK);
            ticks(&mut w, 3);
            put(&mut w, P - IVec3::X, Block::AIR);
            ticks(&mut w, 3);
            assert_eq!(w.get_block(P), Some(r::piston(2, sticky, false)));
            assert_eq!(w.get_block(P + IVec3::X), Some(if sticky { Block::STONE } else { Block::AIR }));
            assert_eq!(w.get_block(P + IVec3::X * 2), Some(if sticky { Block::AIR } else { Block::STONE }));
        }
        let mut w = world();
        put(&mut w, P, r::PISTON);
        put(&mut w, P + IVec3::Z, r::REDSTONE_BLOCK);
        ticks(&mut w, 3);
        assert_eq!(w.get_block(P), Some(r::PISTON));
    }
    #[test]
    fn unloaded_destination_stops_push_and_saved_motion_finishes_without_duplication() {
        let mut w = world();
        let p = IVec3::new(super::super::chunk::CHUNK_SIZE_I - 1, 145, 8);
        put(&mut w, p, r::piston(2, false, false));
        put(&mut w, p - IVec3::X, r::REDSTONE_BLOCK);
        ticks(&mut w, 3);
        assert_eq!(w.get_block(p), Some(r::piston(2, false, false)));
        put(&mut w, P, r::piston(2, true, false));
        put(&mut w, P + IVec3::X, Block::GOLD_BLOCK);
        put(&mut w, P - IVec3::X, r::REDSTONE_BLOCK);
        ticks(&mut w, 1);
        assert_eq!(w.get_block(P + IVec3::X * 2), Some(r::MOVING));
        assert_eq!(w.moving_piston_blocks(0.5).count(), 2);
        let redstone = w.redstone_to_string();
        let moving = w.automation_to_string();
        w.load_redstone(&redstone);
        w.load_automation(&moving);
        ticks(&mut w, 2);
        assert_eq!(w.get_block(P + IVec3::X * 2), Some(Block::GOLD_BLOCK));
        assert_eq!(w.moving_piston_blocks(0.5).count(), 0);
    }
    #[test]
    fn observer_watches_only_front_and_emits_delayed_two_tick_directional_pulse() {
        let mut w = world();
        put(&mut w, P, r::observer(2, false));
        ticks(&mut w, 1);
        put(&mut w, P + IVec3::Z, Block::STONE);
        ticks(&mut w, 3);
        assert_eq!(w.get_block(P), Some(r::observer(2, false)));
        put(&mut w, P + IVec3::X, Block::STONE);
        ticks(&mut w, 1);
        assert_eq!(w.get_block(P), Some(r::observer(2, false)));
        ticks(&mut w, 1);
        assert_eq!(w.get_block(P), Some(r::observer(2, true)));
        assert_eq!(w.redstone_power(P - IVec3::X), 15);
        assert_eq!(w.redstone_power(P + IVec3::X), 0);
        ticks(&mut w, 2);
        assert_eq!(w.get_block(P), Some(r::observer(2, false)));
    }
}
