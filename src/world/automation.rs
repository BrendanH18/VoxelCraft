//! Piston block events, saved moving blocks, and observer pulses.
use crate::inventory::Stack;
use crate::item::Item;
use glam::{DVec3, IVec3};
use rustc_hash::{FxHashMap, FxHashSet};

use super::{
    World,
    block::Block,
    container_sides as sides,
    dispensing::{self, Behaviour},
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
    pub(super) cooldown: FxHashMap<IVec3, u64>,
    /// Redstone tick on which each hopper last ran its transfer tick (Java `tickedGameTime`).
    hopper_ticked: FxHashMap<IVec3, u64>,
    pickups: Vec<IVec3>,
    pub(super) output: Vec<(IVec3, u8, Output, Stack)>,
    creatures: Vec<(IVec3, Stack)>,
    /// v0.16 Track H: blocks a dispenser offers a sulfur cube in front.
    cube_feeds: Vec<(IVec3, u8, Stack)>,
    /// v0.17 Track C2: dispenses waiting for the entity pass: the
    /// dispenser, its facing, the slot and the stack it held.
    pub(super) deferred: Vec<(IVec3, u8, u8, Stack)>,
    /// v0.17 Track C2: reused buffers for hopper item collection (item
    /// cells sorted, and the candidates of one hopper).
    item_cells: Vec<([i32; 3], u32)>,
    item_found: Vec<u32>,
}

/// `Hopper.SUCK_AABB`'s bottom: `Block.column(16, 11, 32)`.
const SUCK_MIN_Y: f64 = 11.0 / 16.0;

/// What the entity pass makes of a stack a dispenser let go of.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(super) enum Output {
    /// `DefaultDispenseItemBehavior.spawnItem`.
    Item,
    /// `ProjectileDispenseBehavior`.
    Shot,
    Boat,
    Cart,
    Stand,
}

impl Output {
    const ALL: [Output; 5] = [Output::Item, Output::Shot, Output::Boat, Output::Cart, Output::Stand];
    /// The saved code: 0 and 1 are the item and shot flags of older saves.
    fn code(self) -> u8 {
        Output::ALL.iter().position(|&o| o == self).unwrap() as u8
    }
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
            self.automation.hopper_ticked.remove(&p);
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
        if !self.contains_y(p.y) {
            return Reaction::Block;
        }
        if b == Block::AIR || b.is_fluid() {
            return Reaction::Empty;
        }
        // v0.16 Track B: shulker boxes pop (`PushReaction.POPPED`), dropping
        // themselves with their contents.
        if super::shulker_box::owns(b.0) {
            return Reaction::Destroy;
        }
        if !b.hardness().is_finite()
            || matches!(
                b,
                Block::OBSIDIAN
                    | Block::CRYING_OBSIDIAN
                    | Block::NETHERITE_BLOCK
                    | Block::SPAWNER
                    | Block::ENCHANTING_TABLE
                    | Block::END_PORTAL_FRAME
                    | Block::NETHER_PORTAL
                    | Block::END_PORTAL
                    | Block::END_GATEWAY
            )
            || self.chest(p).is_some()
            // Banners are block entities, which pistons don't move.
            || super::colour_blocks::banner::decode(b.0).is_some()
            || self.furnace(p).is_some()
            || self.brewing_stand(p).is_some()
            // Signs, shelves and chiseled bookshelves have block entities
            // (`PistonBaseBlock.isPushable`: `!hasBlockEntity()`).
            || super::wood::has_entity(b)
            // So do ender chests and campfires.
            || super::utilities::ender_chest_of(b).is_some()
            // Jukeboxes (a block entity) and reinforced deepslate
            // (`PushReaction.IMMOVEABLE`).
            || super::jukebox::piston_immovable(b)
            // v0.16 Track C: the beacon's block entity.
            || b == super::beacon::BEACON
            // v0.16 Track F: and the conduit's.
            || super::conduit::owns(b.0)
            || super::utilities::campfire_of(b).is_some()
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
            // `PushReaction.POPPED`.
            || super::mount_states::is_lantern(b.0)
            || super::copper::pops_when_pushed(b)
            || super::copper_golem_statue::owns(b.0)
            || b==super::frog_blocks::FROGSPAWN
            // `PushReaction.DESTROY` for flower pots and decorated pots.
            || b == super::utilities::FLOWER_POT
            || super::utilities::potted_plant(b).is_some()
            || super::utilities::decorated_pot_of(b).is_some()
            || super::scaffolding::is_scaffolding(b)
            || super::colour_blocks::pops_on_push(b)
            // v0.16 Track C: heads are `PushReaction.POPPED`.
            || super::skulls::owns(b.0)
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
        // `PistonMovingBlockEntity.tick` lands a waterlogged block dry.
        let block = block.with_water(false);
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
            // `PistonBaseBlock.moveBlocks` leaves air, not the moved block's water.
            self.edit_exact(from, Block::AIR, false);
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
                self.edit_exact(from, Block::AIR, false);
                self.start_piston_move(from, p + d, block, p, true);
            }
        }
    }

    pub(super) fn finish_piston_moves(&mut self) {
        if self.automation.moving.is_empty() {
            return;
        }
        for (&p, m) in &mut self.automation.moving {
            if !self.activity.contains(p, true) || !self.chunks.contains_key(&super::chunk::chunk_of(p)) {
                m.started = m.started.saturating_add(1);
            }
        }
        let mut finished = std::mem::take(&mut self.automation.finished);
        finished.extend(self.automation.moving.iter().filter_map(|(&p, m)| {
            (self.redstone.tick.saturating_sub(m.started) >= 2 && self.is_block_ticking(p)).then_some(p)
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
                let landed = if head && !valid { Block::AIR } else { self.shaped_by_neighbours(p, m.block) };
                self.edit(p, landed, false);
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
        for &(p, facing, kind, stack) in &self.automation.output {
            entries.push(format!(
                "e,{},{},{},{},{},{}",
                p.x,
                p.y,
                p.z,
                facing,
                kind.code(),
                crate::inventory::stack_to_string(Some(stack))
            ));
        }
        // v0.17 Track C2: dispenses waiting for the entity pass.
        for &(p, facing, slot, stack) in &self.automation.deferred {
            entries.push(format!(
                "d,{},{},{},{},{},{}",
                p.x,
                p.y,
                p.z,
                facing,
                slot,
                crate::inventory::stack_to_string(Some(stack))
            ));
        }
        for &(p, stack) in &self.automation.creatures {
            entries.push(format!("w,{},{},{},{}", p.x, p.y, p.z, crate::inventory::stack_to_string(Some(stack))));
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
            if let Some((tag, rest)) = entry.split_once(',').filter(|(tag, _)| matches!(*tag, "e" | "d")) {
                let fields: Vec<&str> = rest.split(',').collect();
                if let &[x, y, z, facing, code, stack] = fields.as_slice() {
                    let parsed = (|| {
                        Some((
                            IVec3::new(x.parse().ok()?, y.parse().ok()?, z.parse().ok()?),
                            facing.parse::<u8>().ok()?,
                            code.parse::<u8>().ok()?,
                            crate::inventory::stack_from_str(stack)??,
                        ))
                    })();
                    if let Some((p, facing, code, stack)) = parsed
                        && facing < 6
                    {
                        if tag == "d" {
                            if code < 9 {
                                self.automation.deferred.push((p, facing, code, stack));
                            }
                        } else if let Some(&kind) = Output::ALL.get(code as usize) {
                            self.automation.output.push((p, facing, kind, stack));
                        }
                    }
                }
                continue;
            }
            if let Some(rest) = entry.strip_prefix("w,") {
                if let &[x, y, z, stack] = rest.split(',').collect::<Vec<_>>().as_slice()
                    && let (Ok(x), Ok(y), Ok(z), Some(Some(stack))) =
                        (x.parse(), y.parse(), z.parse(), crate::inventory::stack_from_str(stack))
                    && (crate::entity::aquatic::bucket_kind(stack.item).is_some()
                        || stack.item == crate::entity::sulfur_cube::BUCKET)
                {
                    self.automation.creatures.push((IVec3::new(x, y, z), stack));
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
            if !(0..crate::world::block::STATE_CAPACITY as i64).contains(&block)
                || !(0..=2).contains(&age)
                || !(0..=1).contains(&install)
            {
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
    /// Slots of the container at `p`, 0 when there is none.
    fn container_count(&self, p: IVec3) -> usize {
        self.container_kind(p).map_or(0, sides::size)
    }

    /// Returns the remaining stack; merges components exactly like inventories.
    fn insert_container(&mut self, p: IVec3, stack: Stack, side: IVec3) -> Option<Stack> {
        self.insert_container_from(p, stack, Some(side), None)
    }

    /// Java's `HopperBlockEntity.addItem` into the container at `p` through
    /// `side` (`None`: an item entity a hopper collects, which tries every
    /// slot): the face's slots in order, each taking what
    /// `canPlaceItemInContainer` allows (`tryMoveInItem`).
    /// `source_hopper` is the hopper pushing into `p`: Java gives an emptied destination hopper
    /// cooldown `8 - k`, where `k` is 1 when the destination already ticked this game tick.
    fn insert_container_from(
        &mut self,
        p: IVec3,
        stack: Stack,
        side: Option<IVec3>,
        source_hopper: Option<IVec3>,
    ) -> Option<Stack> {
        let Some(kind) = self.container_kind(p) else { return Some(stack) };
        // `ComposterBlock.InputContainer.setChanged` composts the one item
        // the top face takes.
        if kind == sides::Kind::ComposterInput {
            if side == Some(IVec3::Y)
                && sides::can_place(kind, 0, stack.item, None, side)
                && self.compost(p, stack.item)
            {
                return (stack.count > 1).then_some(Stack { count: stack.count.saturating_sub(1), ..stack });
            }
            return Some(stack);
        }
        let slots = match side {
            Some(side) => sides::slots_for_face(kind, side),
            None => sides::all_slots(kind),
        };
        let mut left = stack;
        for &slot in slots {
            let slot = slot as usize;
            let old = self.container_stack(p, slot);
            if !sides::can_place(kind, slot, left.item, old.as_ref(), side) {
                continue;
            }
            // `tryMoveInItem`: an empty slot takes the stack; a slot holding
            // the same item and components grows by what fits.
            let room = match old {
                None => left.max(),
                Some(s) if s.stacks_with(&left) => s.max().saturating_sub(s.count),
                _ => 0,
            };
            let n = room.min(left.count);
            if n == 0 {
                continue;
            }
            let count = self.container_count(p);
            let empty_hopper = matches!(self.get_block(p).and_then(r::component), Some(Component::Hopper { .. }))
                && (0..count).all(|i| self.container_stack(p, i).is_none());
            self.set_container_stack(p, slot, Some(Stack { count: old.map_or(0, |s| s.count) + n, ..left }));
            if empty_hopper {
                let ticked = source_hopper.is_some()
                    && self.automation.hopper_ticked.get(&p).is_some_and(|&t| t >= self.redstone.tick);
                self.automation.cooldown.insert(p, self.redstone.tick + 8 - ticked as u64);
            }
            left.count -= n;
            if left.count == 0 {
                return None;
            }
        }
        Some(left)
    }

    /// One item from `from` (through its face `out_side`) into `to`
    /// (through `in_side`): Java's `ejectItems` and `suckInItems`, which try
    /// the source face's slots in order.
    fn transfer_one(&mut self, from: IVec3, to: IVec3, out_side: IVec3, in_side: IVec3) -> bool {
        let (Some(source), Some(_)) = (self.container_kind(from), self.container_kind(to)) else {
            return false;
        };
        for &slot in sides::slots_for_face(source, out_side) {
            let slot = slot as usize;
            let Some(stack) = self.container_stack(from, slot) else {
                continue;
            };
            if !sides::can_take(source, slot, stack.item, out_side) {
                continue;
            }
            let source_hopper =
                matches!(self.get_block(from).and_then(r::component), Some(Component::Hopper { .. })).then_some(from);
            if self.insert_container_from(to, Stack { count: 1, ..stack }, Some(in_side), source_hopper).is_none() {
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
        self.automation.hopper_ticked.insert(p, self.redstone.tick);
        if let Some(&due) = self.automation.cooldown.get(&p)
            && due > self.redstone.tick
        {
            self.schedule_redstone(p, due - self.redstone.tick, 0);
            return;
        }
        let d = r::hopper_direction(facing);
        let pushed = self.transfer_one(p, p + d, d, -d);
        let above = p + IVec3::Y;
        let pulled = if self.container_kind(above).is_some() {
            self.transfer_one(above, p, IVec3::NEG_Y, IVec3::Y)
        } else {
            // `suckInItems`: a full-cube block above stops collection unless
            // it is `#does_not_block_hoppers` (beehives and bee nests).
            let blocked = crate::physics::full_collision(self, above)
                && self.get_block(above).and_then(super::bees::hive).is_none();
            if !blocked {
                self.automation.pickups.push(p);
            }
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
        let rest = (stack.count > 1).then_some(Stack { count: stack.count.saturating_sub(1), ..stack });
        let d = r::direction(facing);
        let front = p + d;
        if self.get_block(front).is_none() {
            return;
        }
        if dropper {
            // `DropperBlock.dispenseFrom`: into the container in front
            // through its facing side (keeping the item if it won't fit),
            // else out as a plain drop.
            if self.container_kind(front).is_some() {
                if self.insert_container(front, one, -d).is_none() {
                    self.set_container_stack(p, slot, rest);
                }
                return;
            }
            self.set_container_stack(p, slot, rest);
            self.queue_dispensed(p, facing, Output::Item, one);
            return;
        }
        let behaviour = dispensing::behaviour(stack.item);
        // v0.17 Track C2: behaviours that look for entities finish in
        // `tick_automation_entities`, which takes the item then.
        if behaviour.needs_entities() {
            self.automation.deferred.push((p, facing, slot as u8, stack));
            return;
        }
        match behaviour {
            // Java's TNT dispense behaviour fails, keeping the item, without `tnt_explodes`.
            Behaviour::Tnt if !self.rules.tnt_explodes => return,
            // v0.16 Track B: `ShulkerBoxDispenseBehavior` places the box; a box
            // that can't be placed stays in the dispenser.
            Behaviour::ShulkerBox => {
                if self.dispense_shulker_box(front, d, &one) {
                    self.set_container_stack(p, slot, None);
                }
                return;
            }
            // v0.16 Track H: `DispenseItemBehavior`'s wither skeleton skull. It
            // goes into an empty cell that is part of a Wither's base
            // (`WitherSkullBlock.canSpawnMob`), turned the dispenser's way, and
            // the placement checks the full pattern; otherwise it is offered to
            // an entity in front (`EquipmentDispenseItemBehavior`).
            Behaviour::WitherSkull => {
                let peaceful = self.rules.difficulty == 0;
                let get = |q: IVec3| self.get_block(q);
                if self.get_block(front) == Some(Block::AIR)
                    && crate::entity::wither::can_spawn_mob(get, front, self.min_y(), peaceful)
                {
                    // `RotationSegment.convertToSegment(direction)`: south 0, west
                    // 4, north 8, east 12, vertical 0.
                    let rotation = match (d.x, d.z) {
                        (-1, _) => 4,
                        (_, -1) => 8,
                        (1, _) => 12,
                        _ => 0,
                    };
                    self.set_block(front, super::skulls::floor(super::skulls::Kind::WitherSkeleton, rotation));
                    self.emit_game_event(
                        front.as_dvec3() + DVec3::splat(0.5),
                        super::vibration::GameEvent::BlockPlace,
                        super::vibration::EventSource::NONE,
                        None,
                    );
                    self.set_container_stack(p, slot, rest);
                } else {
                    self.automation.deferred.push((p, facing, slot as u8, stack));
                }
                return;
            }
            // v0.17 Track C2: a carved pumpkin completes a golem in front, or
            // is offered to an entity's head.
            Behaviour::CarvedPumpkin => {
                if self.place_dispensed_pumpkin(front) {
                    self.set_container_stack(p, slot, rest);
                } else {
                    self.automation.deferred.push((p, facing, slot as u8, stack));
                }
                return;
            }
            Behaviour::BoneMeal => {
                // Java `BoneMealDispenseItemBehavior`: it fertilises the block in front and is only
                // used up when that grew something; otherwise the stack stays in the slot.
                if self.apply_bone_meal(front) {
                    self.set_container_stack(p, slot, rest);
                }
                return;
            }
            // v0.17 Track C2: glowstone charges a respawn anchor (a full one
            // keeps it); anything else in front gets a plain drop.
            Behaviour::Glowstone => match self.dispense_glowstone(front) {
                Some(true) => {
                    self.set_container_stack(p, slot, rest);
                    return;
                }
                Some(false) => return,
                None => {}
            },
            // `HoneycombItem.getWaxed`: copper in front is waxed.
            Behaviour::Honeycomb if self.wax_copper(front) => {
                self.set_container_stack(p, slot, rest);
                return;
            }
            // A water bottle turns dirt into mud and leaves a glass bottle.
            Behaviour::WaterPotion
                if stack.item.as_potion() == Some(crate::potion::Potion::WATER)
                    && self.dispense_water_onto_dirt(front) =>
            {
                self.consume_with_remainder(p, facing, slot, Item::GLASS_BOTTLE);
                return;
            }
            Behaviour::Projectile => {
                self.set_container_stack(p, slot, rest);
                self.queue_dispensed(p, facing, Output::Shot, one);
                return;
            }
            Behaviour::Boat if self.dispensed_boat_pos(p, d).is_some() => {
                self.set_container_stack(p, slot, rest);
                self.queue_dispensed(p, facing, Output::Boat, one);
                return;
            }
            Behaviour::Minecart if self.dispensed_cart_pos(p, d).is_some() => {
                self.set_container_stack(p, slot, rest);
                self.queue_dispensed(p, facing, Output::Cart, one);
                return;
            }
            Behaviour::ArmorStand => {
                self.set_container_stack(p, slot, rest);
                self.queue_dispensed(p, facing, Output::Stand, one);
                return;
            }
            _ => {}
        }
        // v0.16 Track H: `SulfurCubeBlockDispenseItemBehavior`: a swallowable
        // block (without a behaviour of its own) or TNT goes into an adult
        // sulfur cube in front instead, which the entities decide; until
        // then it is dispensed (or primed) as usual.
        if matches!(behaviour, Behaviour::Tnt | Behaviour::SulfurCubeBlock) {
            self.automation.cube_feeds.push((p, facing, one));
        }
        self.set_container_stack(p, slot, rest);
        let block = self.get_block(front).unwrap();
        let replacement = match stack.item {
            item if crate::entity::aquatic::bucket_kind(item).is_some()
                && (block.is_replaceable() || block.holds_water()) =>
            {
                // Waterlogged plants already hold the water.
                if self.generator.dimension != super::terrain::Dimension::Nether && !block.holds_water() {
                    self.set_block(front, Block::WATER);
                }
                self.automation.creatures.push((front, one));
                Some(Item::BUCKET)
            }
            // v0.16 Track D: a sulfur cube bucket has no fluid to pour;
            // `MobBucketItem.checkExtraContent` lets the cube out.
            crate::entity::sulfur_cube::BUCKET => {
                self.automation.creatures.push((front, one));
                Some(Item::BUCKET)
            }
            // `DispensibleContainerItem.emptyContents(null, level, target, null)`:
            // water fills a waterloggable block and evaporates in the Nether.
            Item::POWDER_SNOW_BUCKET => (self.get_block(front) == Some(Block::AIR)
                && self.set_block(front, super::scaffolding::POWDER_SNOW))
            .then_some(Item::BUCKET),
            Item::WATER_BUCKET | Item::LAVA_BUCKET => {
                let fluid = if stack.item == Item::WATER_BUCKET { Block::WATER } else { Block::LAVA };
                crate::survival_items::bucket::empty_at(self, front, fluid, None).map(|_| Item::BUCKET)
            }
            // `BucketPickup.pickupBlock`, waterlogged blocks included.
            Item::BUCKET => crate::survival_items::bucket::pick_up(self, front).map(|(item, _)| item),
            _ => None,
        };
        if let Some(item) = replacement {
            // `consumeWithRemainder`: the slot just emptied takes the
            // remainder, or it joins another slot, or it pops out.
            if rest.is_none() {
                self.set_container_stack(p, slot, Some(Stack::new(item, 1)));
            } else if let Some(left) = self.insert_dispenser(p, Stack::new(item, 1)) {
                self.queue_dispensed(p, facing, Output::Item, left);
            }
            return;
        }
        if behaviour == Behaviour::Tnt {
            self.primed_tnt.push((front, false));
            return;
        }
        self.queue_dispensed(p, facing, Output::Item, one);
    }

    /// `DispenserBlockEntity.insertItem`: the first empty or matching slot.
    pub(super) fn insert_dispenser(&mut self, p: IVec3, stack: Stack) -> Option<Stack> {
        self.insert_container(p, stack, IVec3::Y)
    }

    /// Hands `stack` from the dispenser at `p` to the entity pass.
    pub(super) fn queue_dispensed(&mut self, p: IVec3, facing: u8, kind: Output, stack: Stack) {
        self.automation.output.push((p, facing, kind, stack));
    }

    /// Consume entity work after shared world rules and before entity physics.
    /// Headless callers use this same method with their Entities collection.
    pub fn tick_automation_entities(&mut self, entities: &mut crate::entity::Entities) {
        self.tick_automation_entities_with(entities, &mut []);
    }

    /// Whether a dispense waits for the entity pass (the app then lends
    /// [`World::tick_automation_entities_with`] its players).
    pub fn has_deferred_dispenses(&self) -> bool {
        !self.automation.deferred.is_empty()
    }

    /// [`World::tick_automation_entities`] with the players a dispenser can
    /// dress (v0.17 Track C2: `EquipmentDispenseItemBehavior` reaches
    /// players too).
    pub fn tick_automation_entities_with(
        &mut self,
        entities: &mut crate::entity::Entities,
        players: &mut [crate::entity::dispensed::DispenseTarget<'_>],
    ) {
        // v0.17 Track C2: dispenses that look for entities.
        if !self.automation.deferred.is_empty() {
            self.run_deferred_dispenses(entities, players);
        }
        // v0.16 Track H: `SulfurCubeBlockDispenseItemBehavior.dispenseBlock`:
        // the first adult cube in the cell in front that takes the block;
        // otherwise TNT is primed and other blocks are dispensed as items.
        for (p, facing, stack) in std::mem::take(&mut self.automation.cube_feeds) {
            let front = p + r::direction(facing);
            if !entities.feed_sulfur_cube(front, stack) {
                continue;
            }
            // The cube took it: no primed TNT, no dispensed item.
            if stack.item.block() == Some(Block::TNT) {
                if let Some(i) = self.primed_tnt.iter().position(|&t| t == (front, false)) {
                    self.primed_tnt.remove(i);
                }
            } else if let Some(i) = self
                .automation
                .output
                .iter()
                .position(|&(q, f, kind, s)| q == p && f == facing && kind == Output::Item && s == stack)
            {
                self.automation.output.remove(i);
            }
        }
        for (p, stack) in std::mem::take(&mut self.automation.creatures) {
            let at = p.as_dvec3() + DVec3::new(0.5, 0.0, 0.5);
            if !entities.release_sulfur_cube(stack, at) {
                entities.release_water_creature(stack, at);
            }
        }
        let mut rings = std::mem::take(&mut self.bell_rings);
        for &pos in &rings {
            entities.ring_bell(pos);
        }
        rings.clear();
        self.bell_rings = rings;
        let mut output = std::mem::take(&mut self.automation.output);
        for &(p, facing, kind, stack) in &output {
            let step = r::direction(facing);
            match kind {
                Output::Shot if stack.item == Item::FIRE_CHARGE => entities.dispense_fire_charge(p, step),
                Output::Shot if stack.item == crate::content::fireworks::FIREWORK_ROCKET => {
                    entities.dispense_firework(stack, p, step)
                }
                Output::Shot if matches!(stack.item, Item::ARROW | Item::SPECTRAL_ARROW | Item::TIPPED_ARROW) => {
                    entities.dispense_arrow_stack(stack, p, step)
                }
                // Eggs, snowballs, wind charges, experience bottles, splash
                // and lingering potions.
                Output::Shot if entities.dispense_projectile(stack, p, step) => {}
                // `BoatDispenseItemBehavior` and `MinecartDispenseItemBehavior`
                // through the cart API Track D1 owns; if the water or rail
                // went away since, the item pops out instead.
                Output::Boat | Output::Cart => {
                    let pos = if kind == Output::Boat {
                        self.dispensed_boat_pos(p, step)
                    } else {
                        self.dispensed_cart_pos(p, step)
                    };
                    match (pos, crate::entity::minecart::CartKind::from_item(stack.item)) {
                        (Some(pos), Some(cart)) => {
                            entities.spawn_cart(cart, pos);
                        }
                        _ => entities.dispense_item(stack, p, step),
                    }
                }
                Output::Stand => entities.dispense_armor_stand(p + step, step),
                _ => entities.dispense_item(stack, p, step),
            }
        }
        output.clear();
        self.automation.output = output;
        self.collect_hopper_items(entities);
        self.exchange_minecarts(entities);
    }

    /// v0.17 Track C2: Java's `HopperBlockEntity.suckInItems` for each
    /// hopper with nothing above it to pull from: the item entities whose
    /// boxes meet `Hopper.SUCK_AABB` (the hopper's top from 11/16 up, and
    /// the whole cell above) go in, ignoring pickup delay, until one goes in
    /// whole; a partly taken stack stays with the rest and the next is
    /// tried. Items are found through a sorted cell index built once per
    /// tick, not by walking every item for every hopper.
    fn collect_hopper_items(&mut self, entities: &mut crate::entity::Entities) {
        let mut pickups = std::mem::take(&mut self.automation.pickups);
        if pickups.is_empty() || entities.items.is_empty() {
            pickups.clear();
            self.automation.pickups = pickups;
            return;
        }
        let mut cells = std::mem::take(&mut self.automation.item_cells);
        cells.clear();
        cells.extend(entities.items.iter().enumerate().map(|(i, e)| (e.pos.floor().as_ivec3().to_array(), i as u32)));
        cells.sort_unstable();
        let mut found = std::mem::take(&mut self.automation.item_found);
        for &p in &pickups {
            if !self.is_block_ticking(p)
                || !matches!(self.get_block(p).and_then(r::component), Some(Component::Hopper { disabled: false, .. }))
            {
                continue;
            }
            let (min, max) =
                (p.as_dvec3() + DVec3::new(0.0, SUCK_MIN_Y, 0.0), p.as_dvec3() + DVec3::new(1.0, 2.0, 1.0));
            // An item's feet lie in its own cell; its 0.25-wide, 0.25-tall
            // box reaches the box from the cells around it.
            found.clear();
            for x in p.x - 1..=p.x + 1 {
                for z in p.z - 1..=p.z + 1 {
                    for y in p.y..=p.y + 1 {
                        let key = [x, y, z];
                        let at = cells.partition_point(|c| c.0 < key);
                        found.extend(cells[at..].iter().take_while(|c| c.0 == key).map(|c| c.1));
                    }
                }
            }
            found.sort_unstable();
            for &i in &found {
                let entity = &mut entities.items[i as usize];
                let (lo, hi) = entity.aabb();
                if entity.stack.count == 0 || !(lo.cmplt(max).all() && hi.cmpgt(min).all()) {
                    continue;
                }
                let left = self.insert_container_from(p, entity.stack, None, None);
                entity.stack.count = left.map_or(0, |s| s.count);
                if left.is_none() {
                    self.automation.cooldown.insert(p, self.redstone.tick + 8);
                    break;
                }
            }
        }
        entities.items.retain(|e| e.stack.count > 0);
        found.clear();
        self.automation.item_found = found;
        self.automation.item_cells = cells;
        pickups.clear();
        self.automation.pickups = pickups;
    }

    fn exchange_minecarts(&mut self, entities: &mut crate::entity::Entities) {
        use crate::entity::minecart::{CartKind, insert_slots, take_one};
        let n = entities.minecarts.len();
        for i in 0..n {
            let cell = entities.minecarts[i].pos.floor().as_ivec3();
            if !self.is_entity_ticking(cell) {
                continue;
            }
            let kind = entities.minecarts[i].kind;
            let slots = entities.minecarts[i].slot_count();
            if !matches!(kind, CartKind::Chest | CartKind::Hopper) {
                continue;
            }
            let below = cell - IVec3::Y;
            if matches!(self.get_block(below).and_then(r::component), Some(Component::Hopper { disabled: false, .. }))
                && self.automation.cooldown.get(&below).is_none_or(|&due| due <= self.redstone.tick)
                && let Some(stack) = take_one(&mut entities.minecarts[i].slots[..slots])
            {
                if let Some(left) = self.insert_container(below, stack, IVec3::Y) {
                    let _ = insert_slots(&mut entities.minecarts[i].slots[..slots], left);
                } else {
                    self.automation.cooldown.insert(below, self.redstone.tick + 8);
                }
            }
            for (neighbor, side) in [
                (cell + IVec3::Y, IVec3::NEG_Y),
                (cell + IVec3::X, IVec3::NEG_X),
                (cell - IVec3::X, IVec3::X),
                (cell + IVec3::Z, IVec3::NEG_Z),
                (cell - IVec3::Z, IVec3::Z),
            ] {
                let Some(Component::Hopper { facing, disabled: false }) =
                    self.get_block(neighbor).and_then(r::component)
                else {
                    continue;
                };
                if r::hopper_direction(facing) != side
                    || self.automation.cooldown.get(&neighbor).is_some_and(|&due| due > self.redstone.tick)
                {
                    continue;
                }
                if self.transfer_into_cart(neighbor, &mut entities.minecarts[i].slots[..slots], side) {
                    self.automation.cooldown.insert(neighbor, self.redstone.tick + 8);
                }
            }
            if kind == CartKind::Hopper && !entities.minecarts[i].disabled && entities.minecarts[i].cooldown == 0 {
                let above = cell + IVec3::Y;
                if self.container_kind(above).is_some()
                    && self.pull_into_cart(above, &mut entities.minecarts[i].slots[..slots])
                {
                    entities.minecarts[i].cooldown = 0;
                } else if let Some(item) = item_above_cart(cell, entities) {
                    let stack = entities.items[item].stack;
                    let left = insert_slots(&mut entities.minecarts[i].slots[..slots], stack);
                    entities.items[item].stack.count = left.map_or(0, |s| s.count);
                }
            }
        }
        entities.items.retain(|e| e.stack.count > 0);
    }

    fn transfer_into_cart(&mut self, hopper: IVec3, slots: &mut [Option<Stack>], side: IVec3) -> bool {
        use crate::entity::minecart::insert_slots;
        let Some(kind) = self.container_kind(hopper) else { return false };
        for &slot in sides::slots_for_face(kind, side) {
            let slot = slot as usize;
            let Some(stack) = self.container_stack(hopper, slot) else { continue };
            if !sides::can_take(kind, slot, stack.item, side) {
                continue;
            }
            let one = Stack { count: 1, ..stack };
            if insert_slots(slots, one).is_some() {
                continue;
            }
            self.set_container_stack(
                hopper,
                slot,
                (stack.count > 1).then_some(Stack { count: stack.count - 1, ..stack }),
            );
            return true;
        }
        false
    }

    fn pull_into_cart(&mut self, from: IVec3, slots: &mut [Option<Stack>]) -> bool {
        self.transfer_into_cart(from, slots, IVec3::NEG_Y)
    }
}

fn item_above_cart(cell: IVec3, entities: &crate::entity::Entities) -> Option<usize> {
    let min = cell.as_dvec3();
    let max = min + DVec3::ONE;
    entities.items.iter().position(|entity| {
        entity.stack.count > 0
            && entity.pos.x + 0.125 >= min.x
            && entity.pos.x - 0.125 <= max.x
            && entity.pos.z + 0.125 >= min.z
            && entity.pos.z - 0.125 <= max.z
            && entity.pos.y >= min.y
            && entity.pos.y <= max.y + 0.5
    })
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
    fn pushed_stairs_take_their_corner_shape_where_they_land() {
        use crate::world::block::Facing;
        use crate::world::slab_stair_states::{self as ss, StairShape};
        let mut w = world();
        put(&mut w, P, r::piston(2, false, false));
        // Fronts: south is Java's north (tall side north), west is Java's east.
        put(&mut w, P + IVec3::X, Block::STONE_STAIRS.with_facing(Facing::South));
        let beside = P + IVec3::X * 2 - IVec3::Z;
        put(&mut w, beside, Block::STONE_STAIRS.with_facing(Facing::West));
        assert_eq!(ss::stairs(w.get_block(P + IVec3::X).unwrap()).unwrap().shape, StairShape::Straight);
        put(&mut w, P - IVec3::X, r::REDSTONE_BLOCK);
        ticks(&mut w, 3);
        // `PistonMovingBlockEntity` lands it through `updateFromNeighbourShapes`.
        let landed = ss::stairs(w.get_block(P + IVec3::X * 2).unwrap()).unwrap();
        assert_eq!((landed.facing, landed.shape), (Facing::South, StairShape::OuterRight));
    }
    #[test]
    fn pistons_break_lanterns_instead_of_moving_them() {
        use crate::world::mount_states::LANTERN;
        let mut w = world();
        for x in 1..4 {
            put(&mut w, P + IVec3::new(x, -1, 0), Block::STONE);
        }
        put(&mut w, P, r::piston(2, false, false));
        put(&mut w, P + IVec3::X, LANTERN);
        put(&mut w, P - IVec3::X, r::REDSTONE_BLOCK);
        ticks(&mut w, 3);
        // `PushReaction.POPPED`: the head takes the lantern's cell.
        assert_ne!(w.get_block(P + IVec3::X), Some(LANTERN));
        assert_ne!(w.get_block(P + IVec3::X * 2), Some(LANTERN));
        // So do copper lanterns and copper wall torches.
        use crate::world::copper::{Family, Stage, block};
        for popped in [block(Family::Lantern, Stage::Weathered, 0), block(Family::Torch, Stage::Unaffected, 1)] {
            assert!(crate::world::copper::pops_when_pushed(popped));
            let mut w = world();
            for x in 1..4 {
                put(&mut w, P + IVec3::new(x, -1, 0), Block::STONE);
            }
            put(&mut w, P, r::piston(2, false, false));
            w.set_block(P + IVec3::X, popped);
            put(&mut w, P - IVec3::X, r::REDSTONE_BLOCK);
            ticks(&mut w, 3);
            assert_ne!(w.get_block(P + IVec3::X), Some(popped));
            assert_ne!(w.get_block(P + IVec3::X * 2), Some(popped));
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
    fn dispensers_release_captured_water_creatures_once_with_their_payload() {
        let mut w = world();
        put(&mut w, P, r::dispenser(2, false));
        let mut bucket = Stack::new(Item::AXOLOTL_BUCKET, 1);
        bucket.entity_data = 1 | (3 << 1) | (625 << 9);
        w.chest_mut(P).unwrap().slots[0] = Some(bucket);
        put(&mut w, P - IVec3::X, r::REDSTONE_BLOCK);
        ticks(&mut w, 5);
        // A save between dispensing and the entity tick keeps the creature.
        let save = w.automation_to_string();
        w.load_automation(&save);
        let mut e = crate::entity::Entities::new(1);
        w.tick_automation_entities(&mut e);
        assert_eq!(e.count(crate::entity::MobKind::Axolotl), 1);
        assert_eq!(e.mobs[0].health, 6.25);
        assert_eq!(e.mobs[0].aquatic.as_ref().unwrap().variant, 3);
        assert!(e.mobs[0].persistent);
        assert_eq!(w.chest(P).unwrap().slots[0].unwrap().item, Item::BUCKET);
        w.tick_automation_entities(&mut e);
        assert_eq!(e.mobs.len(), 1);
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
        w.chest_mut(P).unwrap().slots[0] = Some(Stack::new(Item::FIRE_CHARGE, 1));
        w.dispense(P, 2, false);
        assert!(w.chest(P).unwrap().slots[0].is_none());
        w.tick_automation_entities(&mut e);
        assert_eq!(e.fireballs.len(), 1, "a dispensed fire charge is a travelling small fireball");
        assert!(w.get_block(P + IVec3::X).is_some_and(|b| !b.is_fire()), "it does not light the front cell");
        w.chest_mut(P).unwrap().slots[0] = Some(Stack::new(Item::from_block(Block::TNT), 1));
        w.dispense(P, 2, false);
        assert_eq!(w.primed_tnt.len(), 1);
    }
    /// v0.16 Track H: `DispenseItemBehavior`'s wither skeleton skull goes
    /// only where `WitherSkullBlock.canSpawnMob` finds a Wither's base.
    #[test]
    fn dispensers_set_wither_skulls_only_on_a_wither_base() {
        use crate::world::skulls::{self, Kind};
        let mut w = world();
        put(&mut w, P, r::dispenser(2, false));
        let skull = skulls::item(Kind::WitherSkeleton);
        w.chest_mut(P).unwrap().slots[0] = Some(Stack::new(skull, 2));
        let front = P + r::direction(2);
        w.dispense(P, 2, false);
        assert_eq!(w.get_block(front), Some(Block::AIR));
        assert_eq!(w.chest(P).unwrap().slots[0], Some(Stack::new(skull, 2)), "no base: the skull stays");
        assert!(w.automation.output.is_empty());
        for dz in -1..=1 {
            put(&mut w, front + IVec3::new(0, -1, dz), Block::SOUL_SAND);
        }
        put(&mut w, front + IVec3::new(0, -2, 0), Block::SOUL_SAND);
        w.wither_skulls.clear();
        w.dispense(P, 2, false);
        // Facing east, the dispenser's way (`RotationSegment` 12).
        assert_eq!(w.get_block(front), Some(skulls::floor(Kind::WitherSkeleton, 12)));
        assert_eq!(w.chest(P).unwrap().slots[0], Some(Stack::new(skull, 1)));
        assert_eq!(w.wither_skulls, vec![front], "the placement checks for a Wither");
        w.rules.difficulty = 0;
        put(&mut w, front, Block::AIR);
        w.dispense(P, 2, false);
        assert_eq!(w.get_block(front), Some(Block::AIR), "never in Peaceful");
    }

    /// v0.16 Track H: `SulfurCubeBlockDispenseItemBehavior`: a swallowable
    /// block (or TNT) goes into the adult sulfur cube in front; without
    /// one it is dispensed (or primed) as before.
    #[test]
    fn dispensers_feed_blocks_to_sulfur_cubes() {
        use crate::entity::{Entities, MobKind};
        let mut w = world();
        put(&mut w, P, r::dispenser(2, false));
        let front = P + r::direction(2);
        let dirt = Item::from_block(Block::DIRT);
        assert!(crate::entity::sulfur_cube::swallowable(dirt));
        w.chest_mut(P).unwrap().slots[0] = Some(Stack::new(dirt, 2));
        w.chest_mut(P).unwrap().slots[1] = Some(Stack::new(Item::from_block(Block::TNT), 1));
        let mut e = Entities::new(3);
        e.spawn(MobKind::SulfurCube, front.as_dvec3() + DVec3::new(0.5, 0.0, 0.5));
        e.ensure_mob_ids();
        w.chest_mut(P).unwrap().slots[1] = None;
        w.dispense(P, 2, false);
        w.tick_automation_entities(&mut e);
        assert_eq!(e.mobs[0].sulfur_body().map(|s| s.item), Some(dirt));
        assert!(e.items.is_empty(), "nothing was dropped");
        // The same block again: refused, so it is dispensed as an item.
        w.dispense(P, 2, false);
        w.tick_automation_entities(&mut e);
        assert_eq!(e.items.len(), 1);
        // TNT goes in instead of being primed.
        w.chest_mut(P).unwrap().slots[0] = Some(Stack::new(Item::from_block(Block::TNT), 1));
        w.dispense(P, 2, false);
        assert_eq!(w.primed_tnt.len(), 1);
        w.tick_automation_entities(&mut e);
        assert!(w.primed_tnt.is_empty());
        assert_eq!(e.mobs[0].sulfur_body().map(|s| s.item), Some(Item::from_block(Block::TNT)));
    }

    #[test]
    fn dispensers_keep_tnt_without_tnt_explodes() {
        let mut w = world();
        put(&mut w, P, r::dispenser(2, false));
        let tnt = Stack::new(Item::from_block(Block::TNT), 2);
        w.chest_mut(P).unwrap().slots[0] = Some(tnt);
        w.rules.tnt_explodes = false;
        w.dispense(P, 2, false);
        assert!(w.primed_tnt.is_empty());
        assert_eq!(w.chest(P).unwrap().slots[0], Some(tnt), "the dispense fails and keeps the item");
        w.rules.tnt_explodes = true;
        w.dispense(P, 2, false);
        assert_eq!(w.primed_tnt.len(), 1);
        assert_eq!(w.chest(P).unwrap().slots[0], Some(Stack::new(Item::from_block(Block::TNT), 1)));
    }

    #[test]
    fn dispensers_fertilise_the_block_in_front_and_keep_unused_bone_meal() {
        let mut w = world();
        put(&mut w, P, r::dispenser(2, false));
        w.chest_mut(P).unwrap().slots[0] = Some(Stack::new(Item::BONE_MEAL, 2));
        let crop = P + IVec3::X;
        put(&mut w, crop - IVec3::Y, Block::FARMLAND);
        put(&mut w, crop, Block::crop(crate::world::block::Crop::Wheat, 0));
        w.dispense(P, 2, false);
        assert!(w.get_block(crop).unwrap().crop_stage().unwrap() > 0, "the wheat grew");
        assert_eq!(w.chest(P).unwrap().slots[0], Some(Stack::new(Item::BONE_MEAL, 1)));
        // Fully grown wheat takes no bone meal: nothing is dispensed or spent.
        put(&mut w, crop, Block::crop(crate::world::block::Crop::Wheat, 7));
        w.dispense(P, 2, false);
        assert_eq!(w.chest(P).unwrap().slots[0], Some(Stack::new(Item::BONE_MEAL, 1)));
        let mut e = crate::entity::Entities::new(1);
        w.tick_automation_entities(&mut e);
        assert!(e.items.is_empty());
        // A dropper still just drops it.
        put(&mut w, P, r::dispenser(2, true));
        w.dispense(P, 2, true);
        assert!(w.chest(P).unwrap().slots[0].is_none());
    }
    #[test]
    fn dispensed_projectiles_and_items_follow_java_position_speed_and_inaccuracy() {
        let mut e = crate::entity::Entities::new(11);
        // Java 26.3: arrows leave 0.7 out and 0.1 up at power 1.1 with triangle(0.0172275 * 6)
        // noise per axis; the fire charge leaves 1.0 out and ignores power (normalised heading).
        let bound = 0.0172275 * 6.0;
        for _ in 0..200 {
            e.dispense_arrow(P, IVec3::X);
            e.dispense_fire_charge(P, IVec3::X);
        }
        let centre = P.as_dvec3() + DVec3::splat(0.5);
        for (arrow, fireball) in e.arrows.iter().zip(&e.fireballs) {
            assert_eq!(arrow.pos, centre + DVec3::new(0.7, 0.1, 0.0));
            assert_eq!(fireball.pos, centre + DVec3::X);
            let speed = arrow.velocity() / 20.0;
            assert!(
                (speed.x - 1.1).abs() <= 1.1 * bound * 2.0
                    && speed.y.abs() <= 1.1 * bound
                    && speed.z.abs() <= 1.1 * bound
            );
            assert!(fireball.heading().x > 0.97);
        }
        assert!(e.arrows.iter().any(|a| a.velocity().y > 0.5), "noise spreads the shots in both directions");
        assert!(e.arrows.iter().any(|a| a.velocity().y < -0.5));
        assert!(e.arrows.iter().all(|a| !a.is_stuck()));
        // The default item spawn: 0.7 out, 0.15625 below the centre, 0.2-0.3 blocks per tick.
        e.items.clear();
        for _ in 0..200 {
            e.dispense_item(Stack::new(Item::DIAMOND, 1), P, IVec3::X);
        }
        for item in &e.items {
            assert_eq!(item.pos, centre + DVec3::new(0.7, -0.15625, 0.0));
            let v = item.vel / 20.0;
            assert!((0.2 - bound..=0.3 + bound).contains(&v.x), "{v:?}");
            assert!((0.2 - bound..=0.2 + bound).contains(&v.y) && v.z.abs() <= bound, "{v:?}");
        }
        e.items.clear();
        e.dispense_item(Stack::new(Item::DIAMOND, 1), P, IVec3::NEG_Y);
        assert_eq!(e.items[0].pos, centre + DVec3::new(0.0, -0.7 - 0.125, 0.0));
        // Droppers never shoot a fire charge: it is an ordinary item.
        let mut w = world();
        put(&mut w, P, r::dispenser(2, true));
        w.chest_mut(P).unwrap().slots[0] = Some(Stack::new(Item::FIRE_CHARGE, 1));
        w.dispense(P, 2, true);
        let mut e = crate::entity::Entities::new(3);
        w.tick_automation_entities(&mut e);
        assert!(e.fireballs.is_empty());
        assert_eq!(e.items.len(), 1);
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
    #[test]
    fn hopper_cart_collects_stacks_and_container_hoppers_obey_cooldown() {
        use crate::entity::minecart::CartKind;
        let mut w = world();
        put(&mut w, P, crate::world::rails::DETECTOR_RAIL);
        put(&mut w, P - IVec3::Y, r::hopper(4, false));
        put(&mut w, P + IVec3::Y, Block::CHEST);
        w.chest_mut(P + IVec3::Y).unwrap().slots[0] = Some(Stack::new(Item::DIAMOND, 3));
        let mut e = crate::entity::Entities::new(3);
        let id = e.spawn_cart(CartKind::Hopper, P.as_dvec3() + DVec3::new(0.5, 0.0625, 0.5));
        w.tick_automation_entities(&mut e);
        assert_eq!(e.cart(id).unwrap().slots[0], Some(Stack::new(Item::DIAMOND, 1)));
        w.tick_automation_entities(&mut e);
        assert_eq!(w.chest(P - IVec3::Y).unwrap().slots[0], Some(Stack::new(Item::DIAMOND, 1)));
        e.cart_mut(id).unwrap().disabled = true;
        w.tick_automation_entities(&mut e);
        assert_eq!(e.cart(id).unwrap().slots[0], Some(Stack::new(Item::DIAMOND, 1)));
        assert_eq!(w.chest(P + IVec3::Y).unwrap().slots[0], Some(Stack::new(Item::DIAMOND, 1)));
        put(&mut w, P + IVec3::Y, Block::AIR);
        let stack = Stack::new(Item::IRON_INGOT, 64);
        e.items.push(crate::entity::ItemEntity::new(
            stack,
            P.as_dvec3() + DVec3::new(0.5, 0.7, 0.5),
            DVec3::ZERO,
            99.0,
            &mut crate::entity::Rng::new(7),
        ));
        e.cart_mut(id).unwrap().disabled = false;
        w.tick_automation_entities(&mut e);
        assert!(e.items.is_empty());
        assert!(e.cart(id).unwrap().slots.contains(&Some(stack)));
    }
    #[test]
    fn emptied_destination_hopper_cooldown_is_seven_when_it_already_ticked_this_tick() {
        // Java HopperBlockEntity.tryMoveInItem: cooldown 8 - k, k = 1 when the destination
        // hopper's tickedGameTime >= the source hopper's (and only a hopper source counts).
        let mut w = world();
        put(&mut w, P, r::hopper(2, false));
        put(&mut w, P + IVec3::X, r::hopper(4, false));
        w.chest_mut(P).unwrap().slots[0] = Some(Stack::new(Item::DIAMOND, 4));
        ticks(&mut w, 1);
        let now = w.redstone.tick;
        w.automation.cooldown.remove(&(P + IVec3::X));
        w.automation.hopper_ticked.insert(P + IVec3::X, now - 1);
        assert!(w.transfer_one(P, P + IVec3::X, IVec3::X, IVec3::NEG_X));
        assert_eq!(w.automation.cooldown.get(&(P + IVec3::X)), Some(&(now + 8)));
        w.chest_mut(P + IVec3::X).unwrap().slots[0] = None;
        w.automation.hopper_ticked.insert(P + IVec3::X, now);
        assert!(w.transfer_one(P, P + IVec3::X, IVec3::X, IVec3::NEG_X));
        assert_eq!(w.automation.cooldown.get(&(P + IVec3::X)), Some(&(now + 7)));
        // A non-hopper source never gets the optimization.
        w.chest_mut(P + IVec3::X).unwrap().slots[0] = None;
        put(&mut w, P - IVec3::X, Block::CHEST);
        w.chest_mut(P - IVec3::X).unwrap().slots[0] = Some(Stack::new(Item::DIAMOND, 1));
        w.automation.hopper_ticked.insert(P + IVec3::X, now);
        assert!(w.insert_container_from(P + IVec3::X, Stack::new(Item::DIAMOND, 1), Some(IVec3::Y), None).is_none());
        assert_eq!(w.automation.cooldown.get(&(P + IVec3::X)), Some(&(now + 8)));
    }
    /// Java 26.3 `ExperienceBottleItem.createDispenseConfig` through
    /// `ProjectileDispenseBehavior`: 0.7 out and 0.1 up, power 1.375 and
    /// uncertainty 3; a dropper drops the bottle as an item.
    #[test]
    fn dispensers_shoot_experience_bottles_with_java_power_and_uncertainty() {
        let f: serde_json::Value =
            serde_json::from_str(include_str!("../../tests/fixtures/java/26.3/thrown_items.json")).unwrap();
        let d = &f["experience_bottle"]["dispense"];
        let (power, uncertainty) = (d["power"].as_f64().unwrap(), d["uncertainty"].as_f64().unwrap());
        let offset = DVec3::new(
            d["position_offset"][0].as_f64().unwrap(),
            d["position_offset"][1].as_f64().unwrap(),
            d["position_offset"][2].as_f64().unwrap(),
        );
        let scale = d["position_scale"].as_f64().unwrap();
        let bound = f["throwable"]["shoot_noise"].as_f64().unwrap() * uncertainty * power + 1e-9;
        let mut w = world();
        put(&mut w, P, r::dispenser(2, false));
        w.chest_mut(P).unwrap().slots[0] = Some(Stack::new(Item::EXPERIENCE_BOTTLE, 2));
        put(&mut w, P - IVec3::X, r::REDSTONE_BLOCK);
        ticks(&mut w, 5);
        let mut e = crate::entity::Entities::new(9);
        w.tick_automation_entities(&mut e);
        assert_eq!(w.chest(P).unwrap().slots[0], Some(Stack::new(Item::EXPERIENCE_BOTTLE, 1)));
        assert!(e.items.is_empty(), "shot, not dropped");
        assert_eq!(e.thrown.len(), 1);
        let step = r::direction(2);
        let centre = P.as_dvec3() + DVec3::splat(0.5);
        for _ in 0..500 {
            assert!(e.dispense_thrown(Item::EXPERIENCE_BOTTLE, P, step));
        }
        assert!(!e.dispense_thrown(Item::DIAMOND, P, step));
        let mut spread = (0.0f64, 0.0f64);
        for t in &e.thrown {
            assert_eq!(t.kind, crate::entity::thrown::Kind::ExperienceBottle);
            assert!(t.owner.is_none());
            assert!((t.pos - (centre + step.as_dvec3() * scale + offset)).length() < 1e-12, "{}", t.pos);
            let v = t.velocity() / 20.0 - step.as_dvec3() * power;
            assert!(v.abs().max_element() <= bound, "{v}");
            spread = (spread.0.min(v.y), spread.1.max(v.y));
        }
        assert!(spread.0 < -bound * 0.5 && spread.1 > bound * 0.5, "triangle noise both ways: {spread:?}");
        // A dropper drops it.
        let mut w = world();
        put(&mut w, P, r::dispenser(2, true));
        w.chest_mut(P).unwrap().slots[0] = Some(Stack::new(Item::EXPERIENCE_BOTTLE, 1));
        w.dispense(P, 2, true);
        let mut e = crate::entity::Entities::new(3);
        w.tick_automation_entities(&mut e);
        assert!(e.thrown.is_empty());
        assert_eq!(e.items.len(), 1);
    }
}
