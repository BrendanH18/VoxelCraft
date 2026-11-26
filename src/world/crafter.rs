//! v0.17 Track C1: the crafter (Java 26.3 `CrafterBlock`,
//! `CrafterBlockEntity`, `CrafterMenu` and `CrafterSlot`). Sources, the
//! fixture and every difference from Java are in
//! `docs/verification/2026-11-23-v0.17-crafter.md`.
//!
//! States 13088–13135 (of Track C1's 13088–13343): `FIRST + orientation * 4
//! + triggered * 2 + crafting`, with `orientation` in Java's `FrontAndTop`
//! order. The item is the default state, `north_up`, idle. Texture layers
//! 1200–1210 (of 1200–1239). The nine slots are an ordinary chest-table
//! entry, like a dispenser's, so saves, hoppers, droppers, LAN container
//! sync and the CLI's `chest` commands reach them; the disabled slots and
//! the crafting countdown are this module's block entity, saved in the
//! `crafters` dimension property.
//!
//! - A rising redstone edge (`hasNeighborSignal`, no quasi-connectivity)
//!   sets `triggered` and schedules a craft [`CRAFTING_TICK_DELAY`] ticks
//!   later; a falling edge clears `triggered` and `crafting`.
//! - The craft matches the 3×3 grid with the game's recipe matcher
//!   ([`crate::crafting::Grid`]). A result sets `crafting` for
//!   [`MAX_CRAFTING_TICKS`] block-entity ticks, ejects the result and then
//!   each remainder out of the front (into a container there, one item at a
//!   time into another crafter), and takes one item from every filled slot.
//! - Hoppers and droppers fill enabled slots smallest stack first
//!   (`canPlaceItem`); comparators read the number of slots that are filled
//!   or disabled (`getRedstoneSignal`).
use std::collections::BTreeMap;

use glam::{DVec3, IVec3, Vec3};
use serde_json::{Value, json};

use super::World;
use super::block::{Block, Facing, RenderKind};
use crate::inventory::Stack;

pub const FIRST: u16 = 13088;
/// Twelve orientations, `triggered` and `crafting`.
pub const STATES: u16 = 48;
pub const LAST: u16 = FIRST + STATES - 1;
/// Java's `FrontAndTop` values, in its ordinal order.
pub const ORIENTATIONS: [&str; 12] = [
    "down_east",
    "down_north",
    "down_south",
    "down_west",
    "up_east",
    "up_north",
    "up_south",
    "up_west",
    "west_up",
    "east_up",
    "north_up",
    "south_up",
];
/// `FrontAndTop.NORTH_UP`, the default state's orientation.
pub const NORTH_UP: u8 = 10;
/// The default state: the crafter item places this before orientation.
pub const CRAFTER: Block = encode(State { orientation: NORTH_UP, triggered: false, crafting: false });

pub const TEX: u16 = 1200;
const FRONT: u16 = TEX;
const FRONT_CRAFTING: u16 = TEX + 1;
const BACK: u16 = TEX + 2;
const BACK_TRIGGERED: u16 = TEX + 3;
const SIDE: u16 = TEX + 4;
const SIDE_TRIGGERED: u16 = TEX + 5;
const SIDE_CRAFTING: u16 = TEX + 6;
const TOP: u16 = TEX + 7;
const TOP_TRIGGERED: u16 = TEX + 8;
const TOP_CRAFTING: u16 = TEX + 9;
const BOTTOM: u16 = TEX + 10;
pub const TEX_END: u16 = TEX + 11;
// Inside Track C1's reservations: states 13088–13343, layers 1200–1239.
const _: () = assert!(FIRST >= 13088 && LAST <= 13343 && TEX >= 1200 && TEX_END <= 1240);

/// `CrafterBlock.CRAFTING_TICK_DELAY`: game ticks from the rising edge to the craft.
pub const CRAFTING_TICK_DELAY: u64 = 4;
/// `CrafterBlock.MAX_CRAFTING_TICKS`: block-entity ticks the `crafting` state lasts.
pub const MAX_CRAFTING_TICKS: u8 = 6;
/// `CrafterBlockEntity.CONTAINER_SIZE`.
pub const SLOTS: usize = 9;
/// `Blocks.CRAFTER`: `strength(1.5F, 3.5F)`.
pub const HARDNESS: f32 = 1.5;
pub const BLAST_RESISTANCE: f32 = 3.5;
/// `DefaultDispenseItemBehavior.spawnItem`'s speed argument in `dispenseItem`.
pub const EJECT_SPEED: u8 = 6;
/// `Vec3.atCenterOf(pos).relative(front, 0.7)`.
pub const EJECT_OFFSET: f64 = 0.7;

/// A crafter's block state.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct State {
    /// Index into [`ORIENTATIONS`].
    pub orientation: u8,
    pub triggered: bool,
    pub crafting: bool,
}

pub const fn owns(id: u16) -> bool {
    id >= FIRST && id <= LAST
}

pub const fn decode(b: Block) -> Option<State> {
    if !owns(b.0) {
        return None;
    }
    let local = b.0 - FIRST;
    Some(State { orientation: (local / 4) as u8, triggered: local & 2 != 0, crafting: local & 1 != 0 })
}

pub const fn encode(s: State) -> Block {
    Block(FIRST + s.orientation as u16 * 4 + (s.triggered as u16) * 2 + s.crafting as u16)
}

/// `FrontAndTop.front()` and `top()` of orientation `o`.
pub const fn front_and_top(o: u8) -> (IVec3, IVec3) {
    const D: IVec3 = IVec3::NEG_Y;
    const U: IVec3 = IVec3::Y;
    const N: IVec3 = IVec3::NEG_Z;
    const S: IVec3 = IVec3::Z;
    const W: IVec3 = IVec3::NEG_X;
    const E: IVec3 = IVec3::X;
    match o {
        0 => (D, E),
        1 => (D, N),
        2 => (D, S),
        3 => (D, W),
        4 => (U, E),
        5 => (U, N),
        6 => (U, S),
        7 => (U, W),
        8 => (W, U),
        9 => (E, U),
        10 => (N, U),
        _ => (S, U),
    }
}

/// `FrontAndTop.fromFrontAndTop`.
pub fn orientation_of(front: IVec3, top: IVec3) -> Option<u8> {
    (0..12).find(|&o| front_and_top(o) == (front, top))
}

/// The direction the crafter's front (its output) faces.
pub fn front(b: Block) -> Option<IVec3> {
    decode(b).map(|s| front_and_top(s.orientation).0)
}

/// The dispenser-style facing code (`redstone_blocks::direction`) of a
/// unit direction, for the shared item-ejection queue.
pub(super) fn facing_code(d: IVec3) -> u8 {
    match (d.x, d.y, d.z) {
        (_, 1, _) => 4,
        (_, -1, _) => 5,
        _ => Facing::from_offset(d).map_or(0, |f| f as u8),
    }
}

/// `BlockPlaceContext.getNearestLookingDirection`
/// (`Direction.orderedByNearest`): the vertical axis wins only when it is
/// strictly larger than the leading horizontal one, and an x/z tie goes to z.
pub fn nearest_looking(look: Vec3) -> IVec3 {
    let (x, y, z) = (look.x.abs(), look.y.abs(), look.z.abs());
    let vertical = if look.y > 0.0 { IVec3::Y } else { IVec3::NEG_Y };
    let east_west = if look.x > 0.0 { IVec3::X } else { IVec3::NEG_X };
    let north_south = if look.z > 0.0 { IVec3::Z } else { IVec3::NEG_Z };
    if x > z {
        if y > x { vertical } else { east_west }
    } else if y > z {
        vertical
    } else {
        north_south
    }
}

/// `CrafterBlock.getStateForPlacement`: the front faces the placer (the
/// nearest looking direction's opposite); a crafter facing down has its top
/// toward the player's horizontal facing's opposite, one facing up toward
/// it, any other `up`. `triggered` is `hasNeighborSignal` at the cell.
/// `None` when `raw` isn't a crafter.
pub fn placed(raw: Block, look: Vec3, horizontal: Facing, powered: bool) -> Option<Block> {
    decode(raw)?;
    let front = -nearest_looking(look);
    let top = match front.y {
        -1 => horizontal.opposite().offset(),
        1 => horizontal.offset(),
        _ => IVec3::Y,
    };
    let orientation = orientation_of(front, top).unwrap_or(NORTH_UP);
    Some(encode(State { orientation, triggered: powered, crafting: false }))
}

/// Name, kind and layers (+X, -X, +Y, -Y, +Z, -Z): the front, back, top,
/// bottom and two sides follow the orientation; `crafting` lights the front,
/// sides and top, `triggered` the back, sides and top. The layers are not
/// rotated within a face.
pub const fn registry(id: u16) -> (&'static str, RenderKind, [u16; 6]) {
    const fn same(a: IVec3, b: IVec3) -> bool {
        a.x == b.x && a.y == b.y && a.z == b.z
    }
    const fn neg(a: IVec3) -> IVec3 {
        IVec3::new(-a.x, -a.y, -a.z)
    }
    let Some(s) = decode(Block(id)) else { return ("unknown", RenderKind::Invisible, [0; 6]) };
    let (front, top) = front_and_top(s.orientation);
    let dirs = [IVec3::X, IVec3::NEG_X, IVec3::Y, IVec3::NEG_Y, IVec3::Z, IVec3::NEG_Z];
    let mut faces = [0; 6];
    let mut i = 0;
    while i < 6 {
        let d = dirs[i];
        faces[i] = if same(d, front) {
            if s.crafting { FRONT_CRAFTING } else { FRONT }
        } else if same(d, neg(front)) {
            if s.triggered || s.crafting { BACK_TRIGGERED } else { BACK }
        } else if same(d, top) {
            if s.crafting {
                TOP_CRAFTING
            } else if s.triggered {
                TOP_TRIGGERED
            } else {
                TOP
            }
        } else if same(d, neg(top)) {
            BOTTOM
        } else if s.crafting {
            SIDE_CRAFTING
        } else if s.triggered {
            SIDE_TRIGGERED
        } else {
            SIDE
        };
        i += 1;
    }
    ("crafter", RenderKind::Opaque, faces)
}

/// Original art: a dark iron casing with riveted corners; the front holds a
/// three-by-three window grid that glows amber while crafting, the back a
/// redstone port, the sides a redstone channel and the top an output
/// mouth, all lit while triggered.
pub fn pixel(layer: u16, x: usize, y: usize) -> [u8; 4] {
    let (xi, yi) = (x as i32, y as i32);
    let grain = ((x * 7 + y * 13 + layer as usize * 3) % 9) as u8;
    let casing = [58 + grain, 60 + grain, 66 + grain];
    let rim = x == 0 || y == 0 || x == 15 || y == 15;
    let rivet = matches!((x, y), (1, 1) | (14, 1) | (1, 14) | (14, 14));
    let c: [u8; 3] = if rivet {
        [150, 150, 158]
    } else if rim {
        [104, 106, 114]
    } else {
        match layer {
            FRONT | FRONT_CRAFTING => {
                // Three windows a row, each 3×3 with a one-pixel frame.
                let cell = |v: i32| (2..14).contains(&v) && (v - 2) % 4 != 3;
                if cell(xi) && cell(yi) {
                    if layer == FRONT_CRAFTING {
                        [236 - grain * 4, 168 + grain * 3, 64]
                    } else {
                        [30 + grain, 30 + grain, 36 + grain]
                    }
                } else if (1..15).contains(&xi) && (1..15).contains(&yi) {
                    [86 + grain, 88 + grain, 96 + grain]
                } else {
                    casing
                }
            }
            BACK | BACK_TRIGGERED => {
                let (dx, dy) = (xi - 8, yi - 8);
                let r2 = dx * dx + dy * dy;
                if r2 <= 9 {
                    if layer == BACK_TRIGGERED { [230, 40 + grain * 3, 30] } else { [104, 22, 18] }
                } else if r2 <= 20 {
                    [92, 94, 102]
                } else {
                    casing
                }
            }
            SIDE | SIDE_TRIGGERED | SIDE_CRAFTING => {
                if (7..=8).contains(&yi) && (2..14).contains(&xi) {
                    match layer {
                        SIDE_CRAFTING => [250, 190, 80],
                        SIDE_TRIGGERED => [226, 44 + grain * 2, 28],
                        _ => [110, 26, 20],
                    }
                } else if yi == 6 || yi == 9 {
                    [92, 94, 102]
                } else {
                    casing
                }
            }
            TOP | TOP_TRIGGERED | TOP_CRAFTING => {
                let mouth = (5..11).contains(&xi) && (5..11).contains(&yi);
                let lip = (4..12).contains(&xi) && (4..12).contains(&yi);
                if mouth {
                    match layer {
                        TOP_CRAFTING => [248, 196, 96],
                        TOP_TRIGGERED => [200, 40, 30],
                        _ => [24, 24, 28],
                    }
                } else if lip {
                    [112, 114, 122]
                } else {
                    casing
                }
            }
            _ => casing,
        }
    };
    [c[0], c[1], c[2], 255]
}

/// `CrafterBlockEntity.slotCanBeDisabled`'s bound and the sided-transfer
/// seam for v0.17 Track C2: the crafter isn't a `WorldlyContainer`, so
/// every face reaches all nine slots.
pub fn slots_for_face(_face: IVec3) -> std::ops::Range<usize> {
    0..SLOTS
}

/// The block entity, minus the items (they're the chest-table entry).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Crafter {
    /// Bit `i` set: slot `i` is disabled (`SLOT_DISABLED`).
    pub disabled: u16,
    /// `craftingTicksRemaining`.
    pub crafting_ticks: u8,
}

impl Crafter {
    pub fn disabled(&self, slot: usize) -> bool {
        slot < SLOTS && self.disabled & (1 << slot) != 0
    }

    fn save(&self) -> Value {
        let slots: Vec<usize> = (0..SLOTS).filter(|&i| self.disabled(i)).collect();
        json!({"disabled_slots": slots, "crafting_ticks_remaining": self.crafting_ticks})
    }

    fn load(v: &Value) -> Self {
        let mut disabled = 0;
        for i in v["disabled_slots"].as_array().into_iter().flatten().filter_map(Value::as_u64) {
            if (i as usize) < SLOTS {
                disabled |= 1 << i;
            }
        }
        let ticks = v["crafting_ticks_remaining"].as_u64().unwrap_or(0).min(u64::from(MAX_CRAFTING_TICKS)) as u8;
        Crafter { disabled, crafting_ticks: ticks }
    }
}

/// The crafter's level events (`CRAFTER_CRAFT` 1049 with its
/// `ANIMATION_CRAFTER_CRAFT` 2010 particles, `CRAFTER_FAIL` 1050).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Cue {
    Craft,
    Fail,
}

impl Cue {
    pub const ALL: [Cue; 2] = [Cue::Craft, Cue::Fail];
}

/// What crafters did, for the game to play.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Event {
    pub pos: IVec3,
    pub cue: Cue,
}

/// Crafters by position, kept while their chunk is unloaded.
#[derive(Default, Debug)]
pub struct Crafters {
    pub(in crate::world) map: BTreeMap<[i32; 3], Crafter>,
    /// Crafters whose countdown runs (`craftingTicksRemaining > 0`): the
    /// only ones the per-tick pass visits, so idle crafters cost nothing.
    pub(in crate::world) active: Vec<IVec3>,
    events: Vec<Event>,
}

impl World {
    pub(super) fn track_crafter(&mut self, p: IVec3, old: Block, new: Block) {
        let (was, is) = (owns(old.0), owns(new.0));
        if was && !is {
            self.crafters.map.remove(&p.to_array());
            self.crafters.active.retain(|&q| q != p);
        } else if is && !was {
            self.crafters.map.entry(p.to_array()).or_default();
            // `setPlacedBy`: a crafter placed powered crafts after the delay.
            if decode(new).is_some_and(|s| s.triggered) && !self.remote {
                self.schedule_redstone(p, CRAFTING_TICK_DELAY, 0);
            }
        }
    }

    /// The block entity at `p` (default for a crafter without a record,
    /// such as one a LAN replica hasn't synced yet).
    pub fn crafter(&self, p: IVec3) -> Option<Crafter> {
        self.get_block(p).filter(|b| owns(b.0))?;
        Some(self.crafters.map.get(&p.to_array()).copied().unwrap_or_default())
    }

    pub fn crafter_slot_disabled(&self, p: IVec3, slot: usize) -> bool {
        self.crafter(p).is_some_and(|c| c.disabled(slot))
    }

    /// A LAN replica copies the host's disabled slots.
    pub fn set_crafter_disabled_mask(&mut self, p: IVec3, mask: u16) {
        if self.get_block(p).is_some_and(|b| owns(b.0)) {
            self.crafters.map.entry(p.to_array()).or_default().disabled = mask & 0x1ff;
        }
    }

    /// `CrafterBlockEntity.setSlotState`: only an empty slot changes.
    /// Returns whether it did; comparators see the change.
    pub fn set_crafter_slot_enabled(&mut self, p: IVec3, slot: usize, enabled: bool) -> bool {
        if slot >= SLOTS || self.crafter(p).is_none() || self.chest_slot(p, slot).is_some() {
            return false;
        }
        let c = self.crafters.map.entry(p.to_array()).or_default();
        let before = c.disabled;
        if enabled {
            c.disabled &= !(1 << slot);
        } else {
            c.disabled |= 1 << slot;
        }
        let changed = c.disabled != before;
        if changed {
            self.redstone_changed(p);
        }
        changed
    }

    /// `CrafterBlockEntity.canPlaceItem`: an enabled slot that isn't full,
    /// and either empty or holding a stack no smaller than every later
    /// enabled slot's stack of the same item (an empty later slot also
    /// refuses it). Hoppers and droppers fill the first slot that accepts.
    pub fn crafter_can_place(&self, p: IVec3, slot: usize) -> bool {
        let Some(c) = self.crafter(p) else { return false };
        if slot >= SLOTS || c.disabled(slot) {
            return false;
        }
        let Some(existing) = self.chest_slot(p, slot) else { return true };
        if existing.count >= existing.max() {
            return false;
        }
        !(slot + 1..SLOTS).any(|i| {
            !c.disabled(i)
                && match self.chest_slot(p, i) {
                    None => true,
                    Some(other) => other.count < existing.count && other.stacks_with(&existing),
                }
        })
    }

    /// `getRedstoneSignal`: slots that are filled or disabled.
    pub fn crafter_signal(&self, p: IVec3) -> Option<u8> {
        let c = self.crafter(p)?;
        Some((0..SLOTS).filter(|&i| c.disabled(i) || self.chest_slot(p, i).is_some()).count() as u8)
    }

    fn crafter_grid(&self, p: IVec3) -> crate::crafting::Grid {
        let mut grid = crate::crafting::Grid::new(3);
        for (i, cell) in grid.cells.iter_mut().enumerate() {
            *cell = self.chest_slot(p, i);
        }
        grid
    }

    /// `CrafterMenu.refreshRecipeResult`: what the grid would craft now.
    pub fn crafter_result(&self, p: IVec3) -> Option<Stack> {
        self.crafter(p)?;
        self.crafter_grid(p).result_in(&self.maps)
    }

    /// `CrafterBlock.neighborChanged`.
    pub(super) fn crafter_update(&mut self, p: IVec3, b: Block) {
        // A LAN replica shows the host's states; it never crafts.
        let Some(s) = decode(b).filter(|_| !self.remote) else { return };
        let powered = self.redstone_power(p) > 0;
        if powered && !s.triggered {
            self.schedule_redstone(p, CRAFTING_TICK_DELAY, 0);
            self.edit(p, encode(State { triggered: true, ..s }), false);
        } else if !powered && s.triggered {
            self.edit(p, encode(State { triggered: false, crafting: false, ..s }), false);
        }
    }

    /// `CrafterBlock.tick` → `dispenseFrom`.
    pub(super) fn crafter_tick(&mut self, p: IVec3, b: Block) {
        let Some(s) = decode(b).filter(|_| !self.remote) else { return };
        if self.chest(p).is_none() {
            return;
        }
        let grid = self.crafter_grid(p);
        let Some(mut result) = grid.result_in(&self.maps) else {
            self.crafters.events.push(Event { pos: p, cue: Cue::Fail });
            return;
        };
        let remainders = grid.remainders();
        self.crafters.map.entry(p.to_array()).or_default().crafting_ticks = MAX_CRAFTING_TICKS;
        if !self.crafters.active.contains(&p) {
            self.crafters.active.push(p);
        }
        self.edit(p, encode(State { crafting: true, ..s }), false);
        // `ItemStack.onCraftedBySystem`: a zoomed-out map gets its new id.
        self.maps.post_process(&mut result);
        let front = front_and_top(s.orientation).0;
        self.crafter_eject(p, front, result);
        for rest in remainders.into_iter().flatten() {
            self.crafter_eject(p, front, rest);
        }
        let _ = self.with_chest_slots(p, |slots| {
            for slot in slots.iter_mut().take(SLOTS) {
                if let Some(s) = slot {
                    s.count -= 1;
                    if s.count == 0 {
                        *slot = None;
                    }
                }
            }
        });
    }

    /// `CrafterBlock.dispenseItem`: into the container in front (one item
    /// at a time into a crafter, so its slot rules spread them), and what's
    /// left out of the front like a dropper, with the craft's sound and
    /// smoke.
    fn crafter_eject(&mut self, p: IVec3, front: IVec3, stack: Stack) {
        let at = p + front;
        let mut rest = Some(stack);
        if self.container_count(at) > 0 {
            if self.get_block(at).is_some_and(|b| owns(b.0)) {
                while let Some(left) = rest {
                    if self.insert_container(at, Stack { count: 1, ..left }, -front).is_some() {
                        break;
                    }
                    rest = (left.count > 1).then_some(Stack { count: left.count - 1, ..left });
                }
            } else {
                rest = self.insert_container(at, stack, -front);
            }
        }
        let Some(rest) = rest else { return };
        self.queue_dispensed_item(p, facing_code(front), rest);
        self.crafters.events.push(Event { pos: p, cue: Cue::Craft });
        // `shootParticles(..., WHITE_SMOKE)`: ten puffs out of the front.
        let centre = p.as_dvec3() + DVec3::splat(0.5) + front.as_dvec3() * 0.61;
        let across = DVec3::ONE - front.abs().as_dvec3();
        let mut b = crate::particles::Burst::new(crate::particles::Kind::Smoke, centre, 10);
        b.spread = across * 0.25;
        b.velocity = front.as_dvec3() * 0.11;
        b.velocity_spread = DVec3::splat(0.01) + front.abs().as_dvec3() * 0.05;
        b.color = Some([0.92, 0.92, 0.92, 1.0]);
        self.particles.push(crate::particles::Request::Burst(b));
    }

    /// `CrafterBlockEntity.serverTick` for the crafters whose countdown
    /// runs: at zero `crafting` turns off. Runs after the scheduled ticks,
    /// as Java's block entities tick after them.
    pub(super) fn tick_crafters(&mut self) {
        if self.crafters.active.is_empty() || self.remote {
            return;
        }
        let mut i = 0;
        while i < self.crafters.active.len() {
            let p = self.crafters.active[i];
            if !self.is_block_ticking(p) {
                i += 1;
                continue;
            }
            let block = self.get_block(p).filter(|b| owns(b.0));
            let Some(c) = self.crafters.map.get_mut(&p.to_array()).filter(|_| block.is_some()) else {
                self.crafters.active.swap_remove(i);
                continue;
            };
            c.crafting_ticks = c.crafting_ticks.saturating_sub(1);
            if c.crafting_ticks == 0 {
                self.crafters.active.swap_remove(i);
                if let Some(s) = block.and_then(decode) {
                    self.edit(p, encode(State { crafting: false, ..s }), false);
                }
                continue;
            }
            i += 1;
        }
    }

    /// Takes the crafter events since the last call.
    pub fn take_crafter_events(&mut self) -> Vec<Event> {
        std::mem::take(&mut self.crafters.events)
    }

    /// `crafters` dimension property: Java's `disabled_slots` and
    /// `crafting_ticks_remaining` per crafter (the items are chest-table
    /// entries, `triggered` is the block state).
    pub fn crafters_to_string(&self) -> String {
        let list: Vec<Value> = self
            .crafters
            .map
            .iter()
            .filter(|(_, c)| **c != Crafter::default())
            .map(|(p, c)| json!([p, c.save()]))
            .collect();
        json!({"version": 1, "crafters": list}).to_string()
    }

    pub fn load_crafters(&mut self, text: &str) {
        let Ok(v) = serde_json::from_str::<Value>(text) else { return };
        for e in v["crafters"].as_array().into_iter().flatten() {
            let Some(p) = e[0].as_array().filter(|a| a.len() == 3).and_then(|a| {
                let c = |i: usize| a[i].as_i64().and_then(|n| i32::try_from(n).ok());
                Some([c(0)?, c(1)?, c(2)?])
            }) else {
                continue;
            };
            let c = Crafter::load(&e[1]);
            if c.crafting_ticks > 0 && !self.crafters.active.contains(&IVec3::from_array(p)) {
                self.crafters.active.push(IVec3::from_array(p));
            }
            self.crafters.map.insert(p, c);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn states_round_trip_and_the_item_is_north_up_idle() {
        for id in FIRST..=LAST {
            assert_eq!(encode(decode(Block(id)).unwrap()), Block(id));
        }
        assert_eq!(decode(CRAFTER), Some(State { orientation: NORTH_UP, triggered: false, crafting: false }));
        assert_eq!(front_and_top(NORTH_UP), (IVec3::NEG_Z, IVec3::Y));
        for o in 0..12 {
            let (f, t) = front_and_top(o);
            assert_eq!(f.dot(t), 0, "{}", ORIENTATIONS[o as usize]);
            assert_eq!(orientation_of(f, t), Some(o));
        }
    }

    #[test]
    fn placement_faces_the_player_with_java_tops() {
        // Looking north (-z) and level: the front faces south, top up.
        let b = placed(CRAFTER, Vec3::new(0.1, -0.2, -1.0), Facing::North, false).unwrap();
        assert_eq!(front_and_top(decode(b).unwrap().orientation), (IVec3::Z, IVec3::Y));
        // Looking down while facing east: front up, top east.
        let b = placed(CRAFTER, Vec3::new(0.3, -0.9, 0.1), Facing::East, true).unwrap();
        let s = decode(b).unwrap();
        assert_eq!(front_and_top(s.orientation), (IVec3::Y, IVec3::X));
        assert!(s.triggered);
        // Looking up while facing south: front down, top north.
        let b = placed(CRAFTER, Vec3::new(0.0, 0.9, 0.3), Facing::South, false).unwrap();
        assert_eq!(front_and_top(decode(b).unwrap().orientation), (IVec3::NEG_Y, IVec3::NEG_Z));
        assert_eq!(placed(Block::STONE, Vec3::X, Facing::North, false), None);
    }

    #[test]
    fn every_layer_is_painted_opaque() {
        for layer in TEX..TEX_END {
            for (x, y) in [(0, 0), (8, 8), (3, 3), (15, 7)] {
                assert_eq!(pixel(layer, x, y)[3], 255);
            }
        }
    }
}
