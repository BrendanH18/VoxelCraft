//! Piston block events, saved moving blocks, and observer pulses.
use crate::inventory::Stack;
use crate::item::Item;
use glam::{DVec3, IVec3};
use rustc_hash::{FxHashMap, FxHashSet};

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
    powered: FxHashSet<IVec3>,
    cooldown: FxHashMap<IVec3, u64>,
    pickups: Vec<IVec3>,
    output: Vec<(IVec3, u8, bool, Stack)>,
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
        if r::base(old) != r::base(new) {
            self.automation.powered.remove(&p);
            self.automation.cooldown.remove(&p);
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
        if let Some(Component::PistonHead { facing, .. }) = r::component(old)
            && !matches!(r::component(new), Some(Component::PistonHead { .. }))
        {
            let base = p - r::direction(facing);
            if let Some(block) = self
                .get_block(base)
                .filter(|b| matches!(r::component(*b), Some(Component::Piston { extended: true, .. })))
            {
                self.spill_block(base, block);
                self.edit(base, Block::AIR, false);
            }
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
            Some(Component::Dispenser { .. }) => {
                let on = self.redstone_power(p) > 0 || self.redstone_power(p + IVec3::Y) > 0;
                let was = self.automation.powered.contains(&p);
                if on && !was {
                    self.automation.powered.insert(p);
                    self.schedule_redstone(p, 4, 0);
                } else if !on {
                    self.automation.powered.remove(&p);
                }
            }
            Some(Component::Hopper { facing, disabled }) => {
                let powered = self.redstone_power(p) > 0;
                if disabled != powered {
                    self.edit(p, r::hopper(facing, powered), false);
                }
                if !powered {
                    self.schedule_redstone(p, 1, 0);
                }
            }
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
            Some(Component::Dispenser { facing, dropper }) => self.dispense(p, facing, dropper),
            Some(Component::Hopper { facing, disabled: false }) => self.hopper_tick(p, facing),
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
                    | Block::ENCHANTING_TABLE
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
                Some(
                    Component::Piston { extended: true, .. }
                        | Component::PistonHead { .. }
                        | Component::Moving
                        | Component::Daylight { .. }
                )
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
        self.edit(p, r::piston(facing, sticky, false), false);
        self.edit(p + d, Block::AIR, false);
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
        for p in &self.automation.powered {
            entries.push(format!("p,{},{},{}", p.x, p.y, p.z));
        }
        for (p, due) in &self.automation.cooldown {
            entries.push(format!("c,{},{},{},{}", p.x, p.y, p.z, due.saturating_sub(self.redstone.tick)));
        }
        for &(p, facing, arrow, stack) in &self.automation.output {
            entries.push(format!(
                "e,{},{},{},{},{},{}",
                p.x,
                p.y,
                p.z,
                facing,
                arrow as u8,
                crate::inventory::stack_to_string(Some(stack))
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
            if let Some(rest) = entry.strip_prefix("e,") {
                let fields: Vec<&str> = rest.split(',').collect();
                if let &[x, y, z, facing, arrow, stack] = fields.as_slice() {
                    let parsed = (|| {
                        Some((
                            IVec3::new(x.parse().ok()?, y.parse().ok()?, z.parse().ok()?),
                            facing.parse::<u8>().ok()?,
                            arrow.parse::<u8>().ok()?,
                            crate::inventory::stack_from_str(stack)??,
                        ))
                    })();
                    if let Some((p, facing, arrow, stack)) = parsed
                        && facing < 6
                        && arrow < 2
                    {
                        self.automation.output.push((p, facing, arrow == 1, stack));
                    }
                }
                continue;
            }
            if let Some(rest) = entry.strip_prefix("p,").or_else(|| entry.strip_prefix("c,")) {
                let Ok(fields) = rest.split(',').map(str::parse::<i64>).collect::<Result<Vec<_>, _>>() else {
                    continue;
                };
                if fields.len() >= 3 && fields[..3].iter().all(|&x| i32::try_from(x).is_ok()) {
                    let p = IVec3::new(fields[0] as i32, fields[1] as i32, fields[2] as i32);
                    if entry.starts_with("p,") && fields.len() == 3 {
                        self.automation.powered.insert(p);
                    } else if let &[_, _, _, delay] = fields.as_slice()
                        && delay >= 0
                    {
                        self.automation.cooldown.insert(p, self.redstone.tick.saturating_add(delay as u64));
                    }
                }
                continue;
            }
            let Some(rest) = entry.strip_prefix("m,") else {
                continue;
            };
            let Ok(fields) = rest.split(',').map(str::parse::<i64>).collect::<Result<Vec<_>, _>>() else {
                continue;
            };
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

impl World {
    fn container_count(&self, p: IVec3) -> usize {
        if self.get_block(p).is_none() {
            return 0;
        }
        if self.get_block(p).and_then(super::composter::level).is_some() {
            return 1;
        }
        if self.chest(p).is_some() {
            self.container_slots(p)
        } else if self.furnace(p).is_some() {
            3
        } else if self.brewing_stand(p).is_some() {
            5
        } else {
            0
        }
    }

    fn container_stack(&self, p: IVec3, slot: usize) -> Option<Stack> {
        if self.get_block(p).and_then(super::composter::level) == Some(8) {
            return Some(Stack::new(Item::BONE_MEAL, 1));
        }
        if let Some(c) = self.chest(p) {
            return c.slots[slot];
        }
        if let Some(f) = self.furnace(p) {
            return [f.input, f.fuel, f.output][slot];
        }
        let b = self.brewing_stand(p)?;
        [b.bottles[0], b.bottles[1], b.bottles[2], b.ingredient, b.fuel][slot]
    }

    fn set_container_stack(&mut self, p: IVec3, slot: usize, stack: Option<Stack>) {
        if stack.is_none() && self.take_compost(p).is_some() {
            return;
        }
        if let Some(c) = self.chest_mut(p) {
            c.slots[slot] = stack;
            return;
        }
        if let Some(f) = self.furnace_mut(p) {
            match slot {
                0 => {
                    f.input = stack;
                    f.cook = 0.0;
                }
                1 => f.fuel = stack,
                _ => f.output = stack,
            }
            return;
        }
        if let Some(b) = self.brewing_stand_mut(p) {
            match slot {
                0..=2 => b.bottles[slot] = stack,
                3 => b.ingredient = stack,
                _ => b.fuel = stack,
            }
        }
    }

    fn can_insert(&self, p: IVec3, slot: usize, item: Item, side: IVec3) -> bool {
        if self.chest(p).is_some() {
            return true;
        }
        if self.furnace(p).is_some() {
            return if side == IVec3::Y {
                slot == 0
            } else if side == IVec3::NEG_Y {
                false
            } else {
                slot == 1 && (super::furnace::burn_time(item).is_some() || item == Item::BUCKET)
            };
        }
        if self.brewing_stand(p).is_some() {
            return if side == IVec3::Y {
                slot == 3 && super::brewing::is_ingredient(item)
            } else if slot == 4 {
                item == Item::BLAZE_POWDER
            } else {
                slot < 3 && super::brewing::fits_bottle_slot(item)
            };
        }
        false
    }

    fn can_extract(&self, p: IVec3, slot: usize, side: IVec3) -> bool {
        if self.get_block(p).and_then(super::composter::level).is_some() {
            return side == IVec3::NEG_Y;
        }
        if self.chest(p).is_some() {
            return true;
        }
        if self.furnace(p).is_some() {
            return side == IVec3::NEG_Y
                && (slot == 2
                    || slot == 1
                        && self
                            .container_stack(p, slot)
                            .is_some_and(|s| matches!(s.item, Item::BUCKET | Item::WATER_BUCKET)));
        }
        if self.brewing_stand(p).is_some() {
            return slot < 3
                || slot == 3 && self.container_stack(p, slot).is_some_and(|s| s.item == Item::GLASS_BOTTLE);
        }
        false
    }

    /// Returns the remaining stack; merges components exactly like inventories.
    fn insert_container(&mut self, p: IVec3, stack: Stack, side: IVec3) -> Option<Stack> {
        if self.get_block(p).and_then(super::composter::level).is_some() {
            if side == IVec3::Y && self.compost(p, stack.item) {
                return (stack.count > 1).then_some(Stack { count: stack.count.saturating_sub(1), ..stack });
            }
            return Some(stack);
        }
        let count = self.container_count(p);
        let mut left = stack;
        for slot in 0..count {
            if !self.can_insert(p, slot, left.item, side) {
                continue;
            }
            let old = self.container_stack(p, slot);
            let room = match old {
                None => left.max(),
                Some(s) if s.stacks_with(&left) => s.max().saturating_sub(s.count),
                _ => 0,
            };
            let n = room.min(left.count);
            if n == 0 {
                continue;
            }
            let empty_hopper = matches!(self.get_block(p).and_then(r::component), Some(Component::Hopper { .. }))
                && (0..count).all(|i| self.container_stack(p, i).is_none());
            self.set_container_stack(p, slot, Some(Stack { count: old.map_or(0, |s| s.count) + n, ..left }));
            if empty_hopper {
                self.automation.cooldown.insert(p, self.redstone.tick + 8);
            }
            left.count -= n;
            if left.count == 0 {
                return None;
            }
        }
        Some(left)
    }

    fn transfer_one(&mut self, from: IVec3, to: IVec3, out_side: IVec3, in_side: IVec3) -> bool {
        if self.container_count(to) == 0 {
            return false;
        }
        for slot in 0..self.container_count(from) {
            let Some(stack) = self.container_stack(from, slot) else {
                continue;
            };
            if !self.can_extract(from, slot, out_side) {
                continue;
            }
            if self.insert_container(to, Stack { count: 1, ..stack }, in_side).is_none() {
                self.set_container_stack(
                    from,
                    slot,
                    (stack.count > 1).then_some(Stack { count: stack.count.saturating_sub(1), ..stack }),
                );
                return true;
            }
        }
        false
    }

    fn hopper_tick(&mut self, p: IVec3, facing: u8) {
        if let Some(&due) = self.automation.cooldown.get(&p)
            && due > self.redstone.tick
        {
            self.schedule_redstone(p, due - self.redstone.tick, 0);
            return;
        }
        let d = r::hopper_direction(facing);
        let pushed = self.transfer_one(p, p + d, d, -d);
        let above = p + IVec3::Y;
        let pulled = if self.container_count(above) > 0 {
            self.transfer_one(above, p, IVec3::NEG_Y, IVec3::Y)
        } else {
            self.automation.pickups.push(p);
            false
        };
        let delay = if pushed || pulled {
            self.automation.cooldown.insert(p, self.redstone.tick + 8);
            8
        } else {
            1
        };
        self.schedule_redstone(p, delay, 0);
    }

    fn dispense(&mut self, p: IVec3, facing: u8, dropper: bool) {
        let Some(c) = self.chest(p) else {
            return;
        };
        let mut slots = [0; 9];
        let mut count = 0;
        for (i, s) in c.slots[..9].iter().enumerate() {
            if s.is_some() {
                slots[count] = i;
                count += 1;
            }
        }
        if count == 0 {
            return;
        }
        // Select a nonempty SLOT uniformly; stack sizes do not bias the choice.
        let slot = slots[(self.roll() % count as u64) as usize];
        let stack = self.container_stack(p, slot).unwrap();
        let one = Stack { count: 1, ..stack };
        let d = r::direction(facing);
        let front = p + d;
        if self.get_block(front).is_none() {
            return;
        }
        if dropper && self.container_count(front) > 0 {
            if self.insert_container(front, one, -d).is_some() {
                return;
            }
            self.set_container_stack(
                p,
                slot,
                (stack.count > 1).then_some(Stack { count: stack.count.saturating_sub(1), ..stack }),
            );
            return;
        }
        self.set_container_stack(
            p,
            slot,
            (stack.count > 1).then_some(Stack { count: stack.count.saturating_sub(1), ..stack }),
        );
        if !dropper {
            let block = self.get_block(front).unwrap();
            let replacement = match stack.item {
                Item::WATER_BUCKET | Item::LAVA_BUCKET if block.is_replaceable() => {
                    self.set_block(front, if stack.item == Item::WATER_BUCKET { Block::WATER } else { Block::LAVA });
                    Some(Item::BUCKET)
                }
                Item::BUCKET if block == Block::WATER || block == Block::LAVA => {
                    self.set_block(front, Block::AIR);
                    Some(if block == Block::WATER { Item::WATER_BUCKET } else { Item::LAVA_BUCKET })
                }
                _ => None,
            };
            if let Some(item) = replacement {
                let left = self.insert_container(p, Stack::new(item, 1), IVec3::Y);
                if let Some(left) = left {
                    self.automation.output.push((p, facing, false, left));
                }
                return;
            }
            if stack.item.block() == Some(Block::TNT) {
                self.primed_tnt.push((front, false));
                return;
            }
            if stack.item == Item::FIRE_CHARGE {
                // Practical fire-charge support until small fireballs exist.
                self.ignite(front);
                return;
            }
        }
        self.automation.output.push((p, facing, !dropper && stack.item == Item::ARROW, one));
    }

    /// Consume entity work after shared world rules and before entity physics.
    /// Headless callers use this same method with their Entities collection.
    pub fn tick_automation_entities(&mut self, entities: &mut crate::entity::Entities) {
        let mut rings = std::mem::take(&mut self.bell_rings);
        for &pos in &rings {
            entities.ring_bell(pos);
        }
        rings.clear();
        self.bell_rings = rings;
        let mut output = std::mem::take(&mut self.automation.output);
        for &(p, facing, arrow, stack) in &output {
            let d = r::direction(facing).as_dvec3();
            let origin = p.as_dvec3() + DVec3::splat(0.5) + d * 0.7;
            if arrow {
                entities.arrows.push(crate::entity::Arrow::dispensed(origin, (d + DVec3::Y * 0.1).normalize()));
            } else {
                entities.throw(stack, origin + DVec3::Y * 0.3, d);
            }
        }
        output.clear();
        self.automation.output = output;
        let mut pickups = std::mem::take(&mut self.automation.pickups);
        for &p in &pickups {
            if !matches!(self.get_block(p).and_then(r::component), Some(Component::Hopper { disabled: false, .. })) {
                continue;
            }
            for entity in &mut entities.items {
                // Item entities are 1/4-block wide and tall. Include the hopper's
                // upper cavity and the cell above, ignoring player pickup delay.
                let min = p.as_dvec3() + DVec3::new(0.0, 0.625, 0.0);
                let max = p.as_dvec3() + DVec3::new(1.0, 2.0, 1.0);
                if entity.stack.count == 0
                    || entity.pos.x + 0.125 < min.x
                    || entity.pos.x - 0.125 > max.x
                    || entity.pos.z + 0.125 < min.z
                    || entity.pos.z - 0.125 > max.z
                    || entity.pos.y + 0.25 < min.y
                    || entity.pos.y > max.y
                {
                    continue;
                }
                let before = entity.stack.count;
                let left = self.insert_container(p, entity.stack, IVec3::Y);
                entity.stack.count = left.map_or(0, |s| s.count);
                if entity.stack.count < before {
                    self.automation.cooldown.insert(p, self.redstone.tick + 8);
                    break;
                }
            }
        }
        entities.items.retain(|e| e.stack.count > 0);
        pickups.clear();
        self.automation.pickups = pickups;
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
    #[test]
    fn hoppers_pull_then_transfer_on_eight_tick_cooldown_and_lock_with_power() {
        let mut w = world();
        put(&mut w, P, r::hopper(4, false));
        put(&mut w, P + IVec3::Y, Block::CHEST);
        put(&mut w, P - IVec3::Y, Block::CHEST);
        w.chest_mut(P + IVec3::Y).unwrap().slots[0] = Some(Stack::new(Item::DIAMOND, 3));
        ticks(&mut w, 2);
        assert_eq!(w.chest(P).unwrap().slots[0].unwrap().count, 1);
        assert!(w.chest(P - IVec3::Y).unwrap().slots[0].is_none());
        ticks(&mut w, 7);
        assert!(w.chest(P - IVec3::Y).unwrap().slots[0].is_none());
        ticks(&mut w, 1);
        assert_eq!(w.chest(P - IVec3::Y).unwrap().slots[0].unwrap().count, 1);
        assert_eq!(w.chest(P).unwrap().slots[0].unwrap().count, 1);
        put(&mut w, P - IVec3::X, r::REDSTONE_BLOCK);
        ticks(&mut w, 10);
        assert_eq!(w.chest(P - IVec3::Y).unwrap().slots[0].unwrap().count, 1);
        assert_eq!(w.get_block(P), Some(r::hopper(4, true)));
        put(&mut w, P - IVec3::X, Block::AIR);
        ticks(&mut w, 2);
        assert_eq!(w.chest(P - IVec3::Y).unwrap().slots[0].unwrap().count, 2);
    }
    #[test]
    fn full_containers_do_not_consume_items_and_sided_furnace_slots_are_obeyed() {
        let mut w = world();
        put(&mut w, P, r::hopper(2, false));
        put(&mut w, P + IVec3::X, Block::CHEST);
        w.chest_mut(P).unwrap().slots[0] = Some(Stack::new(Item::DIAMOND, 2));
        w.chest_mut(P + IVec3::X).unwrap().slots.fill(Some(Stack::new(Item::WATER_BUCKET, 1)));
        ticks(&mut w, 10);
        assert_eq!(w.chest(P).unwrap().slots[0].unwrap().count, 2);
        put(&mut w, P + IVec3::X, Block::FURNACE);
        assert!(w.insert_container(P + IVec3::X, Stack::new(Item::COAL, 1), IVec3::NEG_X).is_none());
        assert_eq!(w.furnace(P + IVec3::X).unwrap().fuel, Some(Stack::new(Item::COAL, 1)));
        assert!(w.insert_container(P + IVec3::X, Stack::new(Item::DIAMOND, 1), IVec3::NEG_X).is_some());
        assert!(w.insert_container(P + IVec3::X, Stack::new(Item::RAW_IRON, 1), IVec3::Y).is_none());
        assert!(!w.transfer_one(P + IVec3::X, P, IVec3::NEG_Y, IVec3::Y));
        w.furnace_mut(P + IVec3::X).unwrap().output = Some(Stack::new(Item::IRON_INGOT, 1));
        assert!(w.transfer_one(P + IVec3::X, P, IVec3::NEG_Y, IVec3::Y));
    }
    #[test]
    fn hopper_entity_pickup_preserves_components_and_ignores_player_pickup_delay() {
        let mut w = world();
        put(&mut w, P, r::hopper(4, false));
        let stack = Stack::new(Item::DIAMOND, 64).with_name("Collected diamonds").unwrap();
        let mut e = crate::entity::Entities::new(7);
        let mut rng = crate::entity::Rng::new(7);
        e.items.push(crate::entity::ItemEntity::new(
            stack,
            P.as_dvec3() + DVec3::new(0.5, 1.1, 0.5),
            DVec3::ZERO,
            999.0,
            &mut rng,
        ));
        ticks(&mut w, 2);
        w.tick_automation_entities(&mut e);
        assert!(e.items.is_empty());
        assert_eq!(w.chest(P).unwrap().slots[0], Some(stack));
        assert_eq!(w.automation.cooldown.get(&P), Some(&(w.redstone.tick + 8)));
        let save = w.automation_to_string();
        w.load_automation(&save);
        assert_eq!(w.automation.cooldown.get(&P), Some(&(w.redstone.tick + 8)));
    }
    #[test]
    fn automation_container_capacity_and_fullness_survive_save_and_family_replacement() {
        for (block, n) in [(r::hopper(4, false), 5), (r::DISPENSER, 9), (r::DROPPER, 9)] {
            let mut w = world();
            put(&mut w, P, block);
            assert_eq!(w.container_slots(P), n);
            w.chest_mut(P).unwrap().slots[..n].fill(Some(Stack::new(Item::DIAMOND, 64)));
            assert_eq!(w.container_signal(P), Some(15));
            let saved = w.chests_to_string();
            w.load_chests(&saved);
            assert_eq!(w.container_signal(P), Some(15));
            put(&mut w, P, Block::CHEST);
            assert!(w.chest(P).unwrap().slots.iter().all(Option::is_none));
            assert_eq!(w.drops.len(), n);
        }
    }
    #[test]
    fn droppers_wait_four_ticks_trigger_once_and_retain_items_when_target_is_full() {
        let mut w = world();
        put(&mut w, P, r::dispenser(2, true));
        put(&mut w, P + IVec3::X, Block::CHEST);
        w.chest_mut(P).unwrap().slots[0] = Some(Stack::new(Item::DIAMOND, 3));
        put(&mut w, P - IVec3::X, r::REDSTONE_BLOCK);
        ticks(&mut w, 4);
        assert!(w.chest(P + IVec3::X).unwrap().slots[0].is_none());
        ticks(&mut w, 1);
        assert_eq!(w.chest(P + IVec3::X).unwrap().slots[0], Some(Stack::new(Item::DIAMOND, 1)));
        ticks(&mut w, 20);
        assert_eq!(w.chest(P).unwrap().slots[0].unwrap().count, 2);
        w.chest_mut(P + IVec3::X).unwrap().slots.fill(Some(Stack::new(Item::DIAMOND, 64)));
        put(&mut w, P - IVec3::X, Block::AIR);
        ticks(&mut w, 1);
        put(&mut w, P - IVec3::X, r::REDSTONE_BLOCK);
        ticks(&mut w, 5);
        assert_eq!(w.chest(P).unwrap().slots[0].unwrap().count, 2);
    }
    #[test]
    fn dispensers_place_and_collect_source_buckets_shoot_arrows_and_light_fire() {
        let mut w = world();
        put(&mut w, P, r::dispenser(2, false));
        put(&mut w, P - IVec3::X, r::REDSTONE_BLOCK);
        w.chest_mut(P).unwrap().slots[0] = Some(Stack::new(Item::WATER_BUCKET, 1));
        ticks(&mut w, 5);
        assert_eq!(w.get_block(P + IVec3::X), Some(Block::WATER));
        assert_eq!(w.chest(P).unwrap().slots[0], Some(Stack::new(Item::BUCKET, 1)));
        put(&mut w, P - IVec3::X, Block::AIR);
        ticks(&mut w, 1);
        put(&mut w, P - IVec3::X, r::REDSTONE_BLOCK);
        ticks(&mut w, 5);
        assert_eq!(w.get_block(P + IVec3::X), Some(Block::AIR));
        assert_eq!(w.chest(P).unwrap().slots[0], Some(Stack::new(Item::WATER_BUCKET, 1)));
        w.chest_mut(P).unwrap().slots[0] = Some(Stack::new(Item::ARROW, 2));
        w.dispense(P, 2, false);
        let save = w.automation_to_string();
        w.load_automation(&save);
        let mut e = crate::entity::Entities::new(7);
        w.tick_automation_entities(&mut e);
        assert_eq!(e.arrows.len(), 1);
        assert!(e.items.is_empty());
        assert_eq!(w.chest(P).unwrap().slots[0].unwrap().count, 1);
        put(&mut w, P + IVec3::X - IVec3::Y, Block::STONE);
        w.chest_mut(P).unwrap().slots[0] = Some(Stack::new(Item::FIRE_CHARGE, 1));
        w.dispense(P, 2, false);
        assert!(w.get_block(P + IVec3::X).unwrap().is_fire());
        assert!(w.chest(P).unwrap().slots[0].is_none());
        w.chest_mut(P).unwrap().slots[0] = Some(Stack::new(Item::from_block(Block::TNT), 1));
        w.dispense(P, 2, false);
        assert_eq!(w.primed_tnt.len(), 1);
    }
    #[test]
    fn breaking_piston_heads_removes_the_base_without_duplicate_drops() {
        let mut w = world();
        put(&mut w, P, r::piston(2, false, true));
        put(&mut w, P + IVec3::X, r::piston_head(2, false));
        put(&mut w, P + IVec3::X, Block::AIR);
        assert_eq!(w.get_block(P), Some(Block::AIR));
        assert_eq!(w.drops.len(), 1);
        assert_eq!(w.drops[0].1.item, Item::from_block(r::PISTON));
    }
}
