//! Observer pulses, hoppers, dispensers and droppers. Pistons live in
//! `world::piston`; this module routes their neighbour updates and saves.
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
    vibration::{EventSource, GameEvent},
};

#[derive(Default)]
pub(super) struct AutomationState {
    powered: FxHashSet<IVec3>,
    /// Each hopper's `HopperBlockEntity.cooldownTime`: ticks of its own
    /// ticker left before it moves items again (absent: 0).
    pub(super) cooldown: FxHashMap<IVec3, u64>,
    /// Redstone tick on which each hopper last ran its transfer tick (Java `tickedGameTime`).
    hopper_ticked: FxHashMap<IVec3, u64>,
    /// v0.17 Track C2: hopper block-entity tickers in the order they were
    /// created (Java's `Level.blockEntityTickers`), run after block events.
    hoppers: Vec<IVec3>,
    hopper_set: FxHashSet<IVec3>,
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
    /// v0.17 Track D1: cells a chest or hopper minecart's box (or a chest
    /// boat's) reaches, from the last entity pass, so a hopper ticking
    /// without entities knows when a container entity is in front of or
    /// above it.
    cart_cells: FxHashSet<IVec3>,
    /// v0.17 Tracks C2 and D1: hoppers whose move `tick_hoppers` (run
    /// without entities) left for this tick's entity pass.
    cart_hoppers: Vec<IVec3>,
    /// Hoppers whose pull (from a cart or items above) is left the same way.
    suck_hoppers: Vec<IVec3>,
    /// The game tick the hopper phase last ran, plus one (0: never).
    hopper_phase: u64,
    /// Whether `item_cells` indexes this hopper phase's items.
    item_index: bool,
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

/// `PistonBaseBlock.TRIGGER_EXTEND`, `TRIGGER_CONTRACT` and `TRIGGER_DROP`,
/// the first block event parameter; the second is the facing's
/// `Direction.get3DDataValue`.
pub const PISTON_EXTEND: i32 = 0;
pub const PISTON_RETRACT: i32 = 1;
pub const PISTON_RETRACT_SHORT: i32 = 2;

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
        // A new hopper block entity joins the tickers (`addBlockEntityTicker`).
        if matches!(r::component(new), Some(Component::Hopper { .. })) {
            self.register_hopper(p);
        }
        // Observers watch their front face through shape updates
        // (`World::redstone_shape_update`).
        self.track_piston(p, old, new);
    }

    /// An observer watches state changes on precisely its front face
    /// (`ObserverBlock.updateShape` from its `FACING` side, while unpowered).
    /// `BlockBehaviour.updateNeighbourShapes` for a change that skips the
    /// rest of the edit path (fire ageing): observers facing `p` see it.
    pub(super) fn notify_observers(&mut self, p: IVec3) {
        for d in super::redstone_order::UPDATE_SHAPE_ORDER {
            self.neighbor_shape_changed(p + d, -d);
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
                // `HopperBlock.checkPoweredState`: `UPDATE_CLIENTS`.
                let powered = self.redstone_power(p) > 0;
                if disabled != powered {
                    self.set_block_flags(p, r::hopper(facing, powered), super::redstone_order::flags::UPDATE_CLIENTS);
                }
                self.register_hopper(p);
            }
            // `PistonBaseBlock.checkIfExtend` (`neighborChanged`, `onPlace`): a
            // block event, run at the end of the tick (`world::piston`).
            Some(Component::Piston { .. }) => self.check_piston(p),
            _ => {}
        }
    }

    pub(super) fn automation_tick(&mut self, p: IVec3, b: Block) {
        match r::component(b) {
            Some(Component::Dispenser { facing, dropper }) => self.dispense(p, facing, dropper),
            // Hoppers tick as block entities (`tick_hoppers`); a scheduled
            // tick from an older save only enrols the hopper.
            Some(Component::Hopper { .. }) => self.register_hopper(p),
            // `ObserverBlock.tick`.
            Some(Component::Observer { facing, on }) => {
                self.set_block_flags(p, r::observer(facing, !on), super::redstone_order::flags::UPDATE_CLIENTS);
                if !on {
                    self.schedule_redstone(p, 2, 0);
                }
                self.update_observer_front(p, facing);
            }
            // Ticks older saves scheduled for pistons only recheck them.
            Some(Component::Piston { .. }) => self.check_piston(p),
            _ => {}
        }
    }

    pub fn automation_to_string(&self) -> String {
        // v2 replaced v1's two-tick `m` motion records with piston block
        // entities and events (`world::piston`); v1 still loads.
        let mut entries = vec!["v2".to_string()];
        self.piston_save_entries(&mut entries);
        for p in &self.automation.powered {
            entries.push(format!("p,{},{},{}", p.x, p.y, p.z));
        }
        for (p, cooldown) in &self.automation.cooldown {
            entries.push(format!("c,{},{},{},{}", p.x, p.y, p.z, cooldown));
        }
        // v0.17 Track C2: the hopper tickers, in order.
        for p in &self.automation.hoppers {
            entries.push(format!("h,{},{},{}", p.x, p.y, p.z));
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
        if !matches!(entries.next(), Some("v1" | "v2")) {
            return;
        }
        self.automation = AutomationState::default();
        self.pistons = Default::default();
        for entry in entries {
            if self.load_piston_entry(entry) {
                continue;
            }
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
            if let Some(rest) = entry.strip_prefix("h,") {
                if let &[x, y, z] = rest.split(',').collect::<Vec<_>>().as_slice()
                    && let (Ok(x), Ok(y), Ok(z)) = (x.parse(), y.parse(), z.parse())
                {
                    self.register_hopper(IVec3::new(x, y, z));
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
                        // The remaining cooldown, as Java's `TransferCooldown`.
                        self.automation.cooldown.insert(p, delay as u64);
                    }
                }
                continue;
            }
        }
    }
}

impl World {
    /// Slots of the container at `p`, 0 when there is none.
    pub(super) fn container_count(&self, p: IVec3) -> usize {
        self.container_kind(p).map_or(0, sides::size)
    }

    /// Returns the remaining stack; merges components exactly like inventories.
    pub(super) fn insert_container(&mut self, p: IVec3, stack: Stack, side: IVec3) -> Option<Stack> {
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
            if !self.can_place_in(p, kind, slot, left.item, old.as_ref(), side) {
                continue;
            }
            // `tryMoveInItem`: an empty slot takes the stack; a slot holding
            // the same item and components grows by what fits.
            let cap = sides::max_stack(kind);
            let room = match old {
                None => left.max().min(cap),
                Some(s) if s.stacks_with(&left) => s.max().min(cap).saturating_sub(s.count),
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
            // `tryMoveInItem`: an emptied hopper not on a custom cooldown
            // (above 8) waits `8 - k`, k = 1 when it already ticked this
            // game tick, so either way it moves the item 7 ticks later.
            if empty_hopper && self.automation.cooldown.get(&p).is_none_or(|&c| c <= 8) {
                let ticked = source_hopper.is_some()
                    && self.automation.hopper_ticked.get(&p).is_some_and(|&t| t >= self.redstone.tick);
                self.automation.cooldown.insert(p, 8 - ticked as u64);
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

    /// Enrols the hopper at `p` as a block-entity ticker (once).
    fn register_hopper(&mut self, p: IVec3) {
        if self.automation.hopper_set.insert(p) {
            self.automation.hoppers.push(p);
        }
    }

    /// v0.17 Track C2: Java's block-entity phase for hoppers
    /// (`ServerLevel.tick` runs `tickBlockEntities` after scheduled ticks,
    /// block events and entities) without entities, for callers that tick
    /// the world alone (`tick_redstone`, the headless harness). Hoppers that
    /// meet only blocks move items here; a hopper whose container in front
    /// or above may be an entity (a chest or hopper minecart, or items to
    /// collect) has its cooldown counted here and its move finished by
    /// `tick_automation_entities` in this same game tick, if that runs. The
    /// game ticks hoppers there instead, with every entity at hand.
    pub fn tick_hoppers(&mut self) {
        self.hopper_phase(None);
    }

    /// Each hopper's `pushItemsTick`, once per game tick, in the order the
    /// hoppers were created. A hopper unlocked or fed by this tick's
    /// redstone moves items in this same tick. Hoppers in chunks that aren't
    /// ticking wait, cooldown and all; a removed hopper leaves the list.
    /// Nothing is allocated once the buffers have grown.
    fn hopper_phase(&mut self, mut entities: Option<&mut crate::entity::Entities>) {
        if self.automation.hopper_phase == self.redstone.tick + 1 {
            return;
        }
        self.automation.hopper_phase = self.redstone.tick + 1;
        self.automation.cart_hoppers.clear();
        self.automation.suck_hoppers.clear();
        self.automation.item_index = false;
        let mut hoppers = std::mem::take(&mut self.automation.hoppers);
        let mut gone = false;
        for &p in &hoppers {
            match self.get_block(p).map(r::component) {
                // Unloaded: the ticker waits for its chunk.
                None => {}
                Some(Some(Component::Hopper { facing, disabled })) => {
                    if self.is_block_ticking(p) {
                        self.hopper_tick(p, facing, !disabled, entities.as_deref_mut());
                        // `setChanged` updates comparators at once
                        // (`updateNeighbourForOutputSignal`), not at the
                        // start of the next tick.
                        self.drain_redstone_changes();
                    }
                }
                Some(_) => {
                    self.automation.hopper_set.remove(&p);
                    gone = true;
                }
            }
        }
        if gone {
            let set = &self.automation.hopper_set;
            hoppers.retain(|p| set.contains(p));
        }
        // Hoppers placed while these ran join at the end.
        hoppers.append(&mut self.automation.hoppers);
        self.automation.hoppers = hoppers;
        if let Some(e) = entities {
            e.items.retain(|i| i.stack.count > 0);
        }
    }

    /// The moves `hopper_phase` left for the entity pass when it ran
    /// without entities this tick: their cooldowns are already counted.
    fn finish_hopper_moves(&mut self, entities: &mut crate::entity::Entities) {
        if self.automation.cart_hoppers.is_empty() && self.automation.suck_hoppers.is_empty() {
            return;
        }
        let hoppers = std::mem::take(&mut self.automation.cart_hoppers);
        let suckers = std::mem::take(&mut self.automation.suck_hoppers);
        for (&p, whole) in hoppers.iter().map(|p| (p, true)).chain(suckers.iter().map(|p| (p, false))) {
            let Some(Component::Hopper { facing, disabled: false }) = self.get_block(p).and_then(r::component) else {
                continue;
            };
            if whole {
                self.try_move_items(p, facing, entities);
            } else if !self.hopper_full(p) && self.suck_in(p, entities) {
                self.automation.cooldown.insert(p, 8);
            }
            self.drain_redstone_changes();
        }
        entities.items.retain(|i| i.stack.count > 0);
        self.automation.cart_hoppers = hoppers;
        self.automation.cart_hoppers.clear();
        self.automation.suck_hoppers = suckers;
        self.automation.suck_hoppers.clear();
    }

    /// `HopperBlockEntity.pushItemsTick`: the cooldown counts down every
    /// tick, locked or not; once it runs out an enabled hopper moves items
    /// (`tryMoveItems`).
    fn hopper_tick(&mut self, p: IVec3, facing: u8, enabled: bool, entities: Option<&mut crate::entity::Entities>) {
        self.automation.hopper_ticked.insert(p, self.redstone.tick);
        if let Some(c) = self.automation.cooldown.get_mut(&p) {
            *c = c.saturating_sub(1);
            if *c > 0 {
                return;
            }
            self.automation.cooldown.remove(&p);
        }
        if !enabled {
            return;
        }
        if let Some(e) = entities {
            self.try_move_items(p, facing, e);
            return;
        }
        // Without entities: a cart in front leaves the whole move for the
        // entity pass; a cart or items above leave only the pull.
        let d = r::hopper_direction(facing);
        let above = p + IVec3::Y;
        if self.container_kind(p + d).is_none() && self.automation.cart_cells.contains(&(p + d)) {
            self.automation.cart_hoppers.push(p);
            return;
        }
        let pushed = self.transfer_one(p, p + d, d, -d);
        let pulled = if self.hopper_full(p) {
            false
        } else if self.container_kind(above).is_some() {
            self.transfer_one(above, p, IVec3::NEG_Y, IVec3::Y)
        } else {
            if self.automation.cart_cells.contains(&above) || !self.blocks_collection(above) {
                self.automation.suck_hoppers.push(p);
            }
            false
        };
        if pushed || pulled {
            self.automation.cooldown.insert(p, 8);
        }
    }

    /// `HopperBlockEntity.tryMoveItems` for the enabled hopper at `p` whose
    /// cooldown ran out: one item out to the container in front
    /// (`getAttachedContainer`: a block container, else a chest or hopper
    /// minecart there, `getEntityContainer`), then, unless the hopper is
    /// full, one in from above (`getSourceContainer`, the same way), or
    /// else the item entities it can reach (`suckInItems`). Either move
    /// starts a cooldown of 8.
    fn try_move_items(&mut self, p: IVec3, facing: u8, entities: &mut crate::entity::Entities) {
        let d = r::hopper_direction(facing);
        let front = p + d;
        let pushed = if self.container_kind(front).is_some() {
            self.transfer_one(p, front, d, -d)
        } else {
            match pick_container_cart(entities, front, || self.roll()) {
                Some(i) => self.eject_into_cart(p, entities, i),
                None => false,
            }
        };
        let pulled = !self.hopper_full(p) && self.suck_in(p, entities);
        if pushed || pulled {
            self.automation.cooldown.insert(p, 8);
        }
    }

    /// `suckInItems`: one item from the block container above, else from a
    /// chest or hopper minecart there (a random one of several), else the
    /// item entities in reach.
    fn suck_in(&mut self, p: IVec3, entities: &mut crate::entity::Entities) -> bool {
        let above = p + IVec3::Y;
        if self.container_kind(above).is_some() {
            self.transfer_one(above, p, IVec3::NEG_Y, IVec3::Y)
        } else if let Some(i) = pick_container_cart(entities, above, || self.roll()) {
            self.suck_from_cart(p, entities, i)
        } else {
            !self.blocks_collection(above) && self.collect_items(p, entities)
        }
    }

    /// `HopperBlockEntity.inventoryFull`: every slot holds a full stack.
    fn hopper_full(&self, p: IVec3) -> bool {
        (0..5).all(|i| self.container_stack(p, i).is_some_and(|s| s.count >= s.max()))
    }

    /// `suckInItems`: a full-cube block above stops item collection unless
    /// it is `#does_not_block_hoppers` (beehives and bee nests).
    fn blocks_collection(&self, above: IVec3) -> bool {
        crate::physics::full_collision(self, above) && self.get_block(above).and_then(super::bees::hive).is_none()
    }

    /// `suckInItems` from item entities: the ones whose boxes meet
    /// `Hopper.SUCK_AABB` (the hopper's top from 11/16 up, and the whole
    /// cell above) go in, ignoring pickup delay, until one goes in whole
    /// (that counts as a move); a partly taken stack stays with the rest and
    /// the next is tried. Items are found through a cell index built once
    /// per hopper phase, not by walking every item for every hopper.
    fn collect_items(&mut self, p: IVec3, entities: &mut crate::entity::Entities) -> bool {
        if entities.items.is_empty() {
            return false;
        }
        if !self.automation.item_index {
            self.automation.item_index = true;
            let cells = &mut self.automation.item_cells;
            cells.clear();
            cells.extend(
                entities.items.iter().enumerate().map(|(i, e)| (e.pos.floor().as_ivec3().to_array(), i as u32)),
            );
            cells.sort_unstable();
        }
        let (min, max) = (p.as_dvec3() + DVec3::new(0.0, SUCK_MIN_Y, 0.0), p.as_dvec3() + DVec3::new(1.0, 2.0, 1.0));
        // An item's feet lie in its own cell; its 0.25-wide, 0.25-tall box
        // reaches the box from the cells around it.
        let mut found = std::mem::take(&mut self.automation.item_found);
        found.clear();
        let cells = &self.automation.item_cells;
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
        let mut took = false;
        for &i in &found {
            let Some(entity) = entities.items.get_mut(i as usize) else { continue };
            let (lo, hi) = entity.aabb();
            if entity.stack.count == 0 || !(lo.cmplt(max).all() && hi.cmpgt(min).all()) {
                continue;
            }
            let left = self.insert_container_from(p, entity.stack, None, None);
            entity.stack.count = left.map_or(0, |s| s.count);
            if left.is_none() {
                took = true;
                break;
            }
        }
        self.automation.item_found = found;
        took
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
            // `DispenserBlock.dispenseFrom` with nothing to dispense (the
            // dropper's override posts nothing).
            if !dropper {
                self.emit_block_event(p, GameEvent::BlockActivate, EventSource::NONE, self.get_block(p));
            }
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
                // `BucketItem.playEmptySound`, then `MobBucketItem.checkExtraContent`.
                self.emit_block_event(front, GameEvent::FluidPlace, EventSource::NONE, None);
                self.emit_block_event(front, GameEvent::EntityPlace, EventSource::NONE, None);
                self.automation.creatures.push((front, one));
                Some(Item::BUCKET)
            }
            // v0.16 Track D: a sulfur cube bucket has no fluid to pour;
            // `MobBucketItem.checkExtraContent` lets the cube out.
            crate::entity::sulfur_cube::BUCKET => {
                self.emit_block_event(front, GameEvent::EntityPlace, EventSource::NONE, None);
                self.automation.creatures.push((front, one));
                Some(Item::BUCKET)
            }
            // `DispensibleContainerItem.emptyContents(null, level, target, null)`:
            // water fills a waterloggable block and evaporates in the Nether.
            Item::POWDER_SNOW_BUCKET => {
                let placed =
                    self.get_block(front) == Some(Block::AIR) && self.set_block(front, super::scaffolding::POWDER_SNOW);
                // `SolidBucketItem.emptyContents`.
                if placed {
                    self.emit_block_event(front, GameEvent::FluidPlace, EventSource::NONE, None);
                }
                placed.then_some(Item::BUCKET)
            }
            Item::WATER_BUCKET | Item::LAVA_BUCKET => {
                let fluid = if stack.item == Item::WATER_BUCKET { Block::WATER } else { Block::LAVA };
                let poured = crate::survival_items::bucket::empty_at(self, front, fluid, None).map(|_| Item::BUCKET);
                if poured.is_some() {
                    self.emit_block_event(front, GameEvent::FluidPlace, EventSource::NONE, None);
                }
                poured
            }
            // `BucketPickup.pickupBlock`, waterlogged blocks included.
            Item::BUCKET => {
                let taken = crate::survival_items::bucket::pick_up(self, front).map(|(item, _)| item);
                if taken.is_some() {
                    self.emit_block_event(front, GameEvent::FluidPickup, EventSource::NONE, None);
                }
                taken
            }
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
            // The TNT behaviour spawns it as an entity (no `prime_fuse`);
            // a sulfur cube that swallows it instead posts nothing.
            if !self.automation.cube_feeds.iter().any(|&(q, f, _)| q == p && f == facing) {
                self.emit_block_event(front, GameEvent::EntityPlace, EventSource::NONE, None);
            }
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

    /// Queues `stack` to leave `p` like a dropped item (`DefaultDispenseItemBehavior.spawnItem`,
    /// speed 6, 0.7 out of the face `facing`); saved with the other pending ejections.
    pub(super) fn queue_dispensed_item(&mut self, p: IVec3, facing: u8, stack: Stack) {
        self.queue_dispensed(p, facing, Output::Item, stack);
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
                if stack.item.block() == Some(Block::TNT) && self.primed_tnt.contains(&(front, false)) {
                    self.emit_block_event(front, GameEvent::EntityPlace, EventSource::NONE, None);
                }
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
        // Java ticks entities, then block entities: the hopper minecarts'
        // own pickup and where the container carts are, then the hoppers,
        // which meet carts where they stand now and items where they lie.
        self.exchange_minecarts(entities);
        if self.automation.hopper_phase == self.redstone.tick + 1 {
            self.finish_hopper_moves(entities);
        } else {
            self.hopper_phase(Some(entities));
        }
    }

    /// `ejectItems` into minecart `i`: the first hopper slot whose one item
    /// fits goes in, at the cart's first empty or matching slot
    /// (`addItem`, `tryMoveInItem`).
    fn eject_into_cart(&mut self, hopper: IVec3, entities: &mut crate::entity::Entities, i: usize) -> bool {
        let count = self.container_count(hopper);
        let cart = &mut entities.minecarts[i];
        let n = cart.slot_count();
        for slot in 0..count {
            let Some(stack) = self.container_stack(hopper, slot) else { continue };
            let target = cart.slots[..n].iter().position(|s| match s {
                None => true,
                Some(have) => have.stacks_with(&stack) && have.count < have.max(),
            });
            let Some(target) = target else { continue };
            match &mut cart.slots[target] {
                Some(have) => have.count += 1,
                empty => *empty = Some(Stack { count: 1, ..stack }),
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

    /// `suckInItems` from minecart `i` above `hopper`: the cart's slots in
    /// order, the first item the hopper takes (`tryTakeInItemFromSlot`).
    fn suck_from_cart(&mut self, hopper: IVec3, entities: &mut crate::entity::Entities, i: usize) -> bool {
        let cart = &mut entities.minecarts[i];
        let n = cart.slot_count();
        for slot in &mut cart.slots[..n] {
            let Some(stack) = *slot else { continue };
            if self.insert_container(hopper, Stack { count: 1, ..stack }, IVec3::Y).is_none() {
                *slot = (stack.count > 1).then_some(Stack { count: stack.count - 1, ..stack });
                return true;
            }
        }
        false
    }

    fn exchange_minecarts(&mut self, entities: &mut crate::entity::Entities) {
        use crate::entity::minecart::{CartKind, insert_slots};
        // Hoppers above, below and beside container carts move items in
        // their own block-entity ticks (`try_move_items`); this
        // pass records where the carts are for them, and runs the hopper
        // minecart's own pickup (`MinecartHopper.suckInItems`).
        self.automation.cart_cells.clear();
        for cart in entities.minecarts.iter().filter(|c| c.slot_count() > 0) {
            let (min, max) = cart.aabb();
            let (lo, hi) = (min.floor().as_ivec3(), (max - DVec3::splat(1e-9)).floor().as_ivec3());
            for y in lo.y..=hi.y {
                for z in lo.z..=hi.z {
                    for x in lo.x..=hi.x {
                        self.automation.cart_cells.insert(IVec3::new(x, y, z));
                    }
                }
            }
        }
        let n = entities.minecarts.len();
        for i in 0..n {
            let cell = entities.minecarts[i].pos.floor().as_ivec3();
            if !self.is_entity_ticking(cell) {
                continue;
            }
            let kind = entities.minecarts[i].kind;
            let slots = entities.minecarts[i].slot_count();
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

/// `HopperBlockEntity.getEntityContainer`: the container entities (chest
/// and hopper minecarts, chest boats) whose boxes meet `cell`, one picked
/// at random (`roll`) when there are several.
fn pick_container_cart(entities: &crate::entity::Entities, cell: IVec3, roll: impl FnOnce() -> u64) -> Option<usize> {
    let (min, max) = (cell.as_dvec3(), cell.as_dvec3() + DVec3::ONE);
    let meets = |c: &crate::entity::minecart::Minecart| {
        let (lo, hi) = c.aabb();
        c.slot_count() > 0 && lo.cmplt(max).all() && hi.cmpgt(min).all()
    };
    let count = entities.minecarts.iter().filter(|c| meets(c)).count();
    if count == 0 {
        return None;
    }
    let k = if count == 1 { 0 } else { (roll() % count as u64) as usize };
    entities.minecarts.iter().enumerate().filter(|(_, c)| meets(c)).nth(k).map(|(i, _)| i)
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
    /// `tryMoveInItem` caps a slot at `getMaxStackSize`: a stack of three
    /// copies (a crafter's clone result) fills three chiseled bookshelf
    /// slots, one book each, and nothing is lost.
    #[test]
    fn a_stack_into_a_chiseled_bookshelf_takes_one_book_a_slot() {
        let mut w = world();
        put(&mut w, P, super::super::wood::bookshelf::state(super::super::block::Facing::North, 0));
        let left = w.insert_container(P, Stack::new(Item::BOOK, 3), IVec3::Y);
        assert_eq!(left, None);
        let books = w.wood_entities().bookshelf(P).unwrap().books;
        assert_eq!(books.iter().flatten().map(|s| s.count as u32).sum::<u32>(), 3);
        assert_eq!(books.iter().flatten().count(), 3, "one book in each of three slots");
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
        // Netherite blocks are `PUSH_PULL` in Java 26.3 (`tests/pistons.rs`).
        for block in
            [Block::OBSIDIAN, Block::BEDROCK, Block::CHEST, Block::FURNACE, Block::BREWING_STAND, Block::END_PORTAL]
        {
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
        // A new hopper transfers on its first tick, as Java's (cooldown -1).
        ticks(&mut w, 1);
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
        assert_eq!(w.automation.cooldown.get(&P), Some(&8));
        let save = w.automation_to_string();
        w.load_automation(&save);
        assert_eq!(w.automation.cooldown.get(&P), Some(&8));
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
        // `DispenserBlock.neighborChanged`: fires four ticks after the edge.
        ticks(&mut w, 3);
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
        // Java ticks entities before block entities: the cart takes a
        // diamond from the chest, then the hopper under it takes that
        // diamond from the cart (`getEntityContainer`) in the same tick.
        assert_eq!(e.cart(id).unwrap().slots[0], None);
        assert_eq!(w.chest(P - IVec3::Y).unwrap().slots[0], Some(Stack::new(Item::DIAMOND, 1)));
        // The hopper waits out its cooldown while the cart takes the next.
        ticks(&mut w, 1);
        w.tick_automation_entities(&mut e);
        assert_eq!(e.cart(id).unwrap().slots[0], Some(Stack::new(Item::DIAMOND, 1)));
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
        // Both hoppers ran their first transfer on tick 1; start from an empty destination.
        w.chest_mut(P + IVec3::X).unwrap().slots.fill(None);
        let now = w.redstone.tick;
        w.automation.cooldown.remove(&(P + IVec3::X));
        w.automation.hopper_ticked.insert(P + IVec3::X, now - 1);
        assert!(w.transfer_one(P, P + IVec3::X, IVec3::X, IVec3::NEG_X));
        assert_eq!(w.automation.cooldown.get(&(P + IVec3::X)), Some(&8));
        w.chest_mut(P + IVec3::X).unwrap().slots[0] = None;
        w.automation.hopper_ticked.insert(P + IVec3::X, now);
        assert!(w.transfer_one(P, P + IVec3::X, IVec3::X, IVec3::NEG_X));
        assert_eq!(w.automation.cooldown.get(&(P + IVec3::X)), Some(&7));
        // A non-hopper source never gets the optimization.
        w.chest_mut(P + IVec3::X).unwrap().slots[0] = None;
        put(&mut w, P - IVec3::X, Block::CHEST);
        w.chest_mut(P - IVec3::X).unwrap().slots[0] = Some(Stack::new(Item::DIAMOND, 1));
        w.automation.hopper_ticked.insert(P + IVec3::X, now);
        assert!(w.insert_container_from(P + IVec3::X, Stack::new(Item::DIAMOND, 1), Some(IVec3::Y), None).is_none());
        assert_eq!(w.automation.cooldown.get(&(P + IVec3::X)), Some(&8));
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
