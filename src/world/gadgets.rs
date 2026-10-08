//! Note blocks (1507..=1556) and tripwire (hooks 1557..=1572, string 1573..=1580).
//!
//! A note block stores pitch 0..=24 and whether it is powered. The instrument
//! comes from the block below, as in Java. Tripwire string is placed by the
//! string item; hooks span up to 40 intervening string blocks.

use glam::{IVec3, Vec3};

use super::World;
use super::block::{Block, Facing, RenderKind};

pub const NOTE: Block = Block(1507);
pub const HOOK: Block = Block(1557);
pub const TRIPWIRE: Block = Block(1573);
/// Instrument substrates missing from the earlier block registry.
/// The axis-aware bone block of the Nether biomes, 1631..=1633.
pub const BONE_BLOCK: Block = super::nether_biome_blocks::BONE_BLOCK;
/// v0.4.0's bone block, before it gained an axis. Saves load it as the
/// upright one (`storage::decode`).
pub const LEGACY_BONE_BLOCK: Block = Block(1581);
pub const PACKED_ICE: Block = Block(1582);
const NOTE_FIRST: u16 = 1507;
const NOTE_LAST: u16 = 1556;
const HOOK_FIRST: u16 = 1557;
const HOOK_LAST: u16 = 1572;
const WIRE_FIRST: u16 = 1573;
const WIRE_LAST: u16 = 1580;
pub const NOTE_TEXTURE: u16 = 1151;
pub const HOOK_TEXTURE: u16 = 1152;
pub const WIRE_TEXTURE: u16 = 1153;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Instrument {
    Harp,
    Bass,
    Snare,
    Hat,
    BassDrum,
    Bell,
    Flute,
    Chime,
    Guitar,
    Xylophone,
    IronXylophone,
    CowBell,
    Didgeridoo,
    Bit,
    Banjo,
    Pling,
}

impl Instrument {
    /// Saved/event instrument identifier.
    pub const ALL: [Self; 16] = [
        Self::Harp,
        Self::Bass,
        Self::Snare,
        Self::Hat,
        Self::BassDrum,
        Self::Bell,
        Self::Flute,
        Self::Chime,
        Self::Guitar,
        Self::Xylophone,
        Self::IronXylophone,
        Self::CowBell,
        Self::Didgeridoo,
        Self::Bit,
        Self::Banjo,
        Self::Pling,
    ];

    pub fn from_u8(v: u8) -> Self {
        match v {
            1 => Self::Bass,
            2 => Self::Snare,
            3 => Self::Hat,
            4 => Self::BassDrum,
            5 => Self::Bell,
            6 => Self::Flute,
            7 => Self::Chime,
            8 => Self::Guitar,
            9 => Self::Xylophone,
            10 => Self::IronXylophone,
            11 => Self::CowBell,
            12 => Self::Didgeridoo,
            13 => Self::Bit,
            14 => Self::Banjo,
            15 => Self::Pling,
            _ => Self::Harp,
        }
    }
}

pub const fn is_note(b: Block) -> bool {
    b.0 >= NOTE_FIRST && b.0 <= NOTE_LAST
}

pub const fn is_hook(b: Block) -> bool {
    b.0 >= HOOK_FIRST && b.0 <= HOOK_LAST
}

pub const fn is_tripwire(b: Block) -> bool {
    b.0 >= WIRE_FIRST && b.0 <= WIRE_LAST
}

pub const fn participates(b: Block) -> bool {
    is_note(b) || is_hook(b) || is_tripwire(b)
}

pub fn note(pitch: u8, powered: bool) -> Block {
    Block(NOTE_FIRST + pitch.min(24) as u16 + if powered { 25 } else { 0 })
}

pub fn pitch(b: Block) -> Option<u8> {
    is_note(b).then(|| ((b.0 - NOTE_FIRST) % 25) as u8)
}

pub fn note_powered(b: Block) -> bool {
    is_note(b) && b.0 >= NOTE_FIRST + 25
}

pub fn hook(facing: Facing, attached: bool, powered: bool) -> Block {
    Block(HOOK_FIRST + facing as u16 + u16::from(attached) * 4 + u16::from(powered) * 8)
}

pub fn hook_state(b: Block) -> Option<(Facing, bool, bool)> {
    if !is_hook(b) {
        return None;
    }
    let i = b.0 - HOOK_FIRST;
    Some((Facing::ALL[(i % 4) as usize], i % 8 >= 4, i >= 8))
}

pub fn tripwire(powered: bool, attached: bool, disarmed: bool) -> Block {
    Block(WIRE_FIRST + u16::from(powered) + u16::from(attached) * 2 + u16::from(disarmed) * 4)
}

pub fn wire_state(b: Block) -> Option<(bool, bool, bool)> {
    is_tripwire(b).then(|| {
        let i = b.0 - WIRE_FIRST;
        (i % 2 == 1, i % 4 >= 2, i >= 4)
    })
}

pub fn base(b: Block) -> Option<Block> {
    if is_note(b) {
        Some(NOTE)
    } else if is_hook(b) {
        Some(HOOK)
    } else if is_tripwire(b) {
        Some(TRIPWIRE)
    } else if b == LEGACY_BONE_BLOCK {
        Some(BONE_BLOCK)
    } else {
        None
    }
}

pub fn palette_ids() -> impl Iterator<Item = u16> {
    [NOTE_FIRST, HOOK_FIRST, PACKED_ICE.0].into_iter()
}

pub const fn registry(id: u16) -> Option<(&'static str, RenderKind, [u16; 6])> {
    use RenderKind::*;
    if is_note(Block(id)) {
        return Some(("note block", Opaque, [NOTE_TEXTURE; 6]));
    }
    if is_hook(Block(id)) {
        return Some(("tripwire hook", Shaped, [HOOK_TEXTURE; 6]));
    }
    if is_tripwire(Block(id)) {
        return Some(("tripwire", Shaped, [WIRE_TEXTURE; 6]));
    }
    if id == LEGACY_BONE_BLOCK.0 {
        use super::block::tex::{BONE_BLOCK_SIDE as SIDE, BONE_BLOCK_TOP as TOP};
        return Some(("legacy bone block", Opaque, [SIDE, SIDE, TOP, TOP, SIDE, SIDE]));
    }
    if id == PACKED_ICE.0 {
        return Some(("packed ice", Opaque, [super::block::tex::ICE; 6]));
    }
    None
}

/// Java's note-block instrument table, limited to blocks this world has.
pub fn instrument(below: Block) -> Instrument {
    let below = below.stairs_base().or_else(|| below.slab_base()).unwrap_or(below).base();
    // Specific substrates must precede the broad material families (glowstone
    // is a pling, not stone; pumpkin is a didgeridoo, not wood).
    match below {
        Block::GOLD_BLOCK => Instrument::Bell,
        Block::CLAY => Instrument::Flute,
        BONE_BLOCK => Instrument::Xylophone,
        PACKED_ICE => Instrument::Chime,
        Block::IRON_BLOCK => Instrument::IronXylophone,
        Block::SOUL_SAND => Instrument::CowBell,
        Block::PUMPKIN => Instrument::Didgeridoo,
        Block::EMERALD_BLOCK => Instrument::Bit,
        Block::HAY_BALE => Instrument::Banjo,
        Block::GLOWSTONE => Instrument::Pling,
        b if b == Block::WOOL || b.wool_color().is_some() => Instrument::Guitar,
        b if b.is_log()
            || b.is_planks()
            || b.is_door()
            || is_note(b)
            || crate::world::forms::planks_of(b).is_some()
            || matches!(b, Block::BOOKSHELF | Block::CRAFTING_TABLE | Block::CHEST | Block::SMITHING_TABLE) =>
        {
            Instrument::Bass
        }
        b if b == Block::SAND || b == Block::RED_SAND || b == Block::GRAVEL || b.concrete_powder_color().is_some() => {
            Instrument::Snare
        }
        b if b == Block::GLASS || b.stained_glass_color().is_some() => Instrument::Hat,
        b if b == Block::STONE
            || b == Block::COBBLESTONE
            || b == Block::BEDROCK
            || b.is_opaque() && b.name().contains("stone") =>
        {
            Instrument::BassDrum
        }
        _ => Instrument::Harp,
    }
}

/// Semitone pitch around note 12, matching Java's `2^((note-12)/12)`.
pub fn pitch_hz(note: u8) -> f32 {
    2.0f32.powf((note.min(24) as f32 - 12.0) / 12.0)
}

pub fn placed_hook(normal: IVec3, look: Vec3) -> Block {
    let facing = Facing::from_offset(normal).unwrap_or_else(|| Facing::toward(look));
    hook(facing, false, false)
}

impl World {
    pub fn strike_note(&mut self, p: IVec3) -> bool {
        let Some(pitch) = self.get_block(p).and_then(pitch) else {
            return false;
        };
        self.play_note(p, pitch);
        true
    }

    pub(super) fn cycle_note(&mut self, p: IVec3, b: Block) {
        let Some(pitch) = pitch(b) else { return };
        let next = (pitch + 1) % 25;
        self.edit(p, note(next, note_powered(b)), false);
        self.play_note(p, next);
    }

    pub(super) fn note_neighbour(&mut self, p: IVec3, b: Block) {
        let powered = self.redstone_power(p) > 0;
        let was = note_powered(b);
        let pitch = pitch(b).unwrap_or(0);
        if powered != was {
            self.edit(p, note(pitch, powered), false);
            if powered {
                self.play_note(p, pitch);
            }
        }
    }

    fn play_note(&mut self, p: IVec3, pitch: u8) {
        let below = self.get_block(p - IVec3::Y).unwrap_or(Block::AIR);
        if self.get_block(p + IVec3::Y) != Some(Block::AIR) {
            return;
        }
        let instrument = instrument(below) as u8;
        self.notes.push((p, pitch, instrument));
    }

    /// Recompute both ends and all intermediate string cells. A Java line
    /// contains 1..40 wires; gaps detach it but still notify the other end.
    pub(super) fn connect_hook(&mut self, p: IVec3) {
        let Some((facing, _, _)) = self.get_block(p).and_then(hook_state) else { return };
        let mut end = None;
        let mut wires = Vec::new();
        let mut complete = true;
        let mut power = false;
        for distance in 1..=41 {
            let q = p + facing.offset() * distance;
            let Some(b) = self.get_block(q) else { return };
            if let Some((other, _, _)) = hook_state(b) {
                if other == facing.opposite() {
                    end = Some(q);
                }
                break;
            }
            if let Some((on, _, disarmed)) = wire_state(b) {
                wires.push(q);
                power |= on && !disarmed;
            } else {
                complete = false;
            }
        }
        let pulse = self.redstone.trip_pulse.get(&p).is_some_and(|&due| due > self.redstone.tick);
        let attached = (complete || pulse) && end.is_some() && !wires.is_empty();
        let on = attached && power || pulse;
        let next = hook(facing, attached, on);
        if self.get_block(p) != Some(next) {
            self.edit(p, next, false);
        }
        if let Some(end) = end {
            let other = hook(facing.opposite(), attached, on);
            if self.get_block(end) != Some(other) {
                self.edit(end, other, false);
            }
        }
        for q in wires {
            if let Some((on, was_attached, disarmed)) = self.get_block(q).and_then(wire_state)
                && was_attached != attached
            {
                self.edit(q, tripwire(on, attached, disarmed), false);
            }
        }
    }

    fn hooks_of(&self, start: IVec3) -> Vec<IVec3> {
        let mut out = Vec::new();
        for step in [IVec3::X, IVec3::NEG_X, IVec3::Z, IVec3::NEG_Z] {
            for distance in 1..=41 {
                let q = start + step * distance;
                let Some(b) = self.get_block(q) else { break };
                if let Some((facing, _, _)) = hook_state(b) {
                    if facing.offset() == -step {
                        out.push(q);
                    }
                    break;
                }
                if !is_tripwire(b) {
                    break;
                }
            }
        }
        out
    }

    /// Wake distant hooks on every string edit, including a gap or hook removal.
    pub(super) fn track_tripwire(&mut self, p: IVec3, old: Block, new: Block) {
        if old == new || !(is_tripwire(old) || is_tripwire(new) || is_hook(old) || is_hook(new)) {
            return;
        }
        if is_hook(old) && !is_hook(new) {
            self.redstone.trip_pulse.remove(&p);
        }
        let cut = wire_state(old).is_some_and(|(_, attached, disarmed)| attached && !disarmed) && !is_tripwire(new);
        for hook_pos in self.hooks_of(p) {
            if cut {
                self.redstone.trip_pulse.insert(hook_pos, self.redstone.tick + 10);
                self.schedule_redstone(hook_pos, 10, 0);
            }
            self.redstone_changed(hook_pos);
        }
    }

    /// String senses entities independently of whether it has a complete line.
    pub(super) fn refresh_tripwires(&mut self) {
        let contacts = std::mem::take(&mut self.redstone.contacts);
        for (&p, contact) in &contacts {
            if contact.all == 0 {
                continue;
            }
            if let Some((on, attached, disarmed)) = self.get_block(p).and_then(wire_state) {
                if !on {
                    self.edit(p, tripwire(true, attached, disarmed), false);
                }
                self.schedule_redstone(p, 10, 0);
            }
        }
        self.redstone.contacts = contacts;
    }

    pub(super) fn tripwire_tick(&mut self, p: IVec3) {
        let Some((on, attached, disarmed)) = self.get_block(p).and_then(wire_state) else { return };
        let occupied = self.redstone.contacts.get(&p).is_some_and(|c| c.all > 0);
        if on != occupied {
            self.edit(p, tripwire(occupied, attached, disarmed), false);
        }
        if occupied {
            self.schedule_redstone(p, 10, 0);
        }
    }

    pub(super) fn hook_tick(&mut self, p: IVec3) {
        self.redstone.trip_pulse.remove(&p);
        self.connect_hook(p);
    }

    /// Shears arm-safe removal. String still drops normally.
    pub fn disarm_tripwire(&mut self, p: IVec3) -> bool {
        let Some((on, attached, disarmed)) = self.get_block(p).and_then(wire_state) else { return false };
        if disarmed {
            return false;
        }
        self.edit(p, tripwire(on, attached, true), false);
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::world::chunk::ChunkData;
    use crate::world::terrain::Generator;
    use std::sync::Arc;

    fn world() -> World {
        let mut w = World::new_headless(Arc::new(Generator::new(5)), Default::default(), 2);
        w.insert_chunk(IVec3::new(0, 4, 0), Arc::new(ChunkData::Uniform(Block::AIR)), false);
        w
    }

    #[test]
    fn use_cycles_twenty_five_pitches_and_the_block_below_picks_the_instrument() {
        let mut w = world();
        let p = IVec3::new(4, 140, 4);
        w.set_block(p - IVec3::Y, Block::WOOL);
        w.set_block(p, NOTE);
        assert_eq!(instrument(Block::WOOL), Instrument::Guitar);
        assert!((pitch_hz(12) - 1.0).abs() < 1e-5);
        for i in 1..=25 {
            w.cycle_note(p, w.get_block(p).unwrap());
            assert_eq!(pitch(w.get_block(p).unwrap()), Some(i % 25));
        }
        assert_eq!(pitch(w.get_block(p).unwrap()), Some(0));
        assert_eq!(w.notes.len(), 25);
    }

    #[test]
    fn redstone_plays_a_note_once_per_rising_edge() {
        let mut w = world();
        let p = IVec3::new(4, 140, 4);
        w.set_block(p - IVec3::Y, Block::STONE);
        w.set_block(p, NOTE);
        w.set_block(p + IVec3::X, crate::world::redstone_blocks::REDSTONE_BLOCK);
        w.tick_redstone();
        assert!(note_powered(w.get_block(p).unwrap()));
        assert_eq!(w.notes.len(), 1);
        w.notes.clear();
        w.tick_redstone();
        assert!(w.notes.is_empty(), "holding power does not replay");
    }

    #[test]
    fn hooks_attach_across_string_and_an_entity_powers_them() {
        let mut w = world();
        let a = IVec3::new(2, 140, 4);
        let b = IVec3::new(5, 140, 4);
        w.set_block(a - IVec3::X, Block::STONE);
        w.set_block(b + IVec3::X, Block::STONE);
        w.set_block(a, hook(Facing::East, false, false));
        w.set_block(b, hook(Facing::West, false, false));
        for x in 3..5 {
            w.set_block(IVec3::new(x, 140, 4), TRIPWIRE);
        }
        w.tick_redstone();
        assert!(hook_state(w.get_block(a).unwrap()).unwrap().1, "west hook attached");
        assert!(wire_state(w.get_block(IVec3::new(3, 140, 4)).unwrap()).unwrap().1);
        // An entity overlapping the string: the contact map is filled by the caller.
        // Drive the refresh directly with a fake contact.
        w.redstone.contacts.insert(IVec3::new(3, 140, 4), Default::default());
        w.redstone.contacts.get_mut(&IVec3::new(3, 140, 4)).unwrap().all = 1;
        w.refresh_tripwires();
        w.tick_redstone();
        assert!(hook_state(w.get_block(a).unwrap()).unwrap().2);
        assert_eq!(w.redstone_power(a - IVec3::X * 2), 15, "the supporting block conducts strong output");
        w.redstone.contacts.clear();
        w.tripwire_tick(IVec3::new(3, 140, 4));
        w.tick_redstone();
        assert!(!hook_state(w.get_block(a).unwrap()).unwrap().2);
    }
    #[test]
    fn all_sixteen_instruments_have_distinct_substrates() {
        let blocks = [
            Block::AIR,
            Block::PLANKS,
            Block::SAND,
            Block::GLASS,
            Block::STONE,
            Block::GOLD_BLOCK,
            Block::CLAY,
            PACKED_ICE,
            Block::WOOL,
            BONE_BLOCK,
            Block::IRON_BLOCK,
            Block::SOUL_SAND,
            Block::PUMPKIN,
            Block::EMERALD_BLOCK,
            Block::HAY_BALE,
            Block::GLOWSTONE,
        ];
        for (block, expected) in blocks.into_iter().zip(Instrument::ALL) {
            assert_eq!(instrument(block), expected, "{}", block.name());
        }
    }

    #[test]
    fn notes_need_air_above_and_tuning_preserves_powered_edge() {
        let mut w = world();
        let p = IVec3::new(4, 140, 4);
        w.set_block(p, note(24, true));
        w.cycle_note(p, w.get_block(p).unwrap());
        assert_eq!(w.get_block(p), Some(note(0, true)));
        assert_eq!(w.notes.len(), 1, "air below is a harp substrate");
        w.notes.clear();
        w.set_block(p + IVec3::Y, Block::STONE);
        w.cycle_note(p, w.get_block(p).unwrap());
        assert!(w.notes.is_empty());
        assert_eq!(instrument(BONE_BLOCK), Instrument::Xylophone);
        assert_eq!(instrument(PACKED_ICE), Instrument::Chime);
        assert_eq!(instrument(Block::ICE), Instrument::Harp);
    }
    #[test]
    fn active_cut_alarm_and_wire_recheck_survive_save_restore() {
        let mut w = world();
        let a = IVec3::new(2, 140, 4);
        let b = IVec3::new(5, 140, 4);
        w.set_block(a - IVec3::X, Block::GLASS);
        w.set_block(b + IVec3::X, Block::STONE);
        w.set_block(a, hook(Facing::East, false, false));
        w.set_block(b, hook(Facing::West, false, false));
        w.set_block(a + IVec3::X, TRIPWIRE);
        w.set_block(a + IVec3::X * 2, TRIPWIRE);
        w.tick_redstone();
        w.set_block(a + IVec3::X, Block::AIR);
        w.tick_redstone();
        let remaining_wire = a + IVec3::X * 2;
        w.schedule_redstone(remaining_wire, 10, 0);
        let save = w.redstone_to_string();
        let mut restored = world();
        for (pos, slot) in &w.chunks {
            restored.insert_chunk(*pos, slot.data.clone(), true);
        }
        restored.load_redstone(&save);
        assert_eq!(restored.redstone.trip_pulse, w.redstone.trip_pulse);
        let mut before: Vec<_> = save.split('|').collect();
        let restored_save = restored.redstone_to_string();
        let mut after: Vec<_> = restored_save.split('|').collect();
        before.sort_unstable();
        after.sort_unstable();
        assert_eq!(after, before);
        for _ in 0..11 {
            restored.tick_redstone();
        }
        assert_eq!(hook_state(restored.get_block(a).unwrap()), Some((Facing::East, false, false)));
        assert!(restored.redstone.trip_pulse.is_empty());
    }

    #[test]
    fn long_wire_breaks_pulse_both_hooks_but_sheared_wire_does_not() {
        for shear in [false, true] {
            let mut w = world();
            w.insert_chunk(IVec3::new(1, 4, 0), Arc::new(ChunkData::Uniform(Block::AIR)), false);
            let a = IVec3::new(2, 140, 4);
            let b = IVec3::new(43, 140, 4);
            w.set_block(a - IVec3::X, Block::STONE);
            w.set_block(b + IVec3::X, Block::STONE);
            w.set_block(a, hook(Facing::East, false, false));
            w.set_block(b, hook(Facing::West, false, false));
            for x in 3..43 {
                w.set_block(IVec3::new(x, 140, 4), TRIPWIRE);
            }
            w.tick_redstone();
            assert!(hook_state(w.get_block(a).unwrap()).unwrap().1);
            let cut = IVec3::new(22, 140, 4);
            if shear {
                w.disarm_tripwire(cut);
            }
            w.set_block(cut, Block::AIR);
            w.tick_redstone();
            for p in [a, b] {
                let (_, attached, powered) = hook_state(w.get_block(p).unwrap()).unwrap();
                assert_eq!(attached, !shear, "an armed cut holds attachment until its recheck");
                assert_eq!(powered, !shear);
            }
            for _ in 0..10 {
                w.tick_redstone();
            }
            assert!(!hook_state(w.get_block(a).unwrap()).unwrap().2);
        }
    }
}
