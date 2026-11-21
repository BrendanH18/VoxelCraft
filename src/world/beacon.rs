//! v0.16 Track C: beacons (Java 26.3 `BeaconBlock`, `BeaconBlockEntity`,
//! `BeaconBeamBlock` and `BeaconMenu`'s rules). See the dated verification
//! record for sources and differences.
//!
//! The block is state 12726; texture layers 708–710 (glass shell, core,
//! beam). Each beacon is a block entity kept by position, saved in the
//! dimension's `beacons` property and ticked in loaded chunks only:
//!
//! - every tick the beam check climbs at most ten cells
//!   (`BLOCKS_CHECK_PER_TICK`), collecting coloured sections from stained
//!   glass and panes and stopping at any block that dampens light fully
//!   (except bedrock), so a beacon never scans its column at once;
//! - on game ticks divisible by 80 it re-reads its pyramid
//!   (`updateBase`, at most 4 layers of `#beacon_base_blocks`) while it has
//!   a beam, and with levels and a beam applies its powers to players
//!   within `10 + 10 * levels` blocks horizontally (and any height above
//!   that range's bottom) for `(9 + 2 * levels) * 20` ticks.
use std::collections::BTreeMap;

use glam::IVec3;
use serde_json::{Value, json};

use super::block::{Block, RenderKind};
use super::shape::{Box16, Boxes};
use crate::simulation::effects::Effect;

pub const BEACON: Block = Block(12726);
pub const GLASS_TEX: u16 = 708;
pub const CORE_TEX: u16 = 709;
pub const BEAM_TEX: u16 = 710;
pub const TEX_END: u16 = 711;
/// `BeaconBlockEntity.MAX_LEVELS`.
pub const MAX_LEVELS: u8 = 4;
/// Game ticks between pyramid and power updates.
pub const UPDATE_INTERVAL: u64 = 80;
/// `BLOCKS_CHECK_PER_TICK`.
pub const CHECKS_PER_TICK: i32 = 10;

/// `BeaconBlockEntity.BEACON_EFFECTS`: the powers each pyramid level
/// unlocks, in Java's order. Regeneration (level 4) is secondary only.
pub const POWERS: [&[Effect]; 4] = [
    &[Effect::Speed, Effect::Haste],
    &[Effect::Resistance, Effect::JumpBoost],
    &[Effect::Strength],
    &[Effect::Regeneration],
];

/// `#beacon_base_blocks`.
pub fn is_base(b: Block) -> bool {
    matches!(b, Block::IRON_BLOCK | Block::GOLD_BLOCK | Block::EMERALD_BLOCK | Block::NETHERITE_BLOCK)
        || b == super::stone_blocks::DIAMOND_BLOCK
}

/// `#beacon_payment_items`.
pub fn is_payment(item: crate::item::Item) -> bool {
    use crate::item::Item;
    matches!(item, Item::NETHERITE_INGOT | Item::EMERALD | Item::DIAMOND | Item::GOLD_INGOT | Item::IRON_INGOT)
}

/// `getRequiredLevelsFor`: the pyramid level a power needs (`None` for an
/// effect a beacon can't give).
pub fn required_levels(effect: Effect) -> Option<u8> {
    POWERS.iter().position(|tier| tier.contains(&effect)).map(|i| i as u8 + 1)
}

/// `BeaconBlockEntity.validateEffects`.
pub fn valid_powers(primary: Option<Effect>, secondary: Option<Effect>, levels: u8) -> bool {
    if secondary.is_some() && levels < 4 {
        return false;
    }
    let need = |e: Option<Effect>| e.map_or(Some(0), required_levels);
    let (Some(p), Some(s)) = (need(primary), need(secondary)) else { return false };
    if p > levels || s > levels {
        return false;
    }
    if p >= 4 {
        return false;
    }
    s == 0 || s >= 4 || primary == secondary
}

/// `applyEffects`: the range, the amplifier of the primary power and the
/// duration in ticks for a beacon of `levels`.
pub fn range(levels: u8) -> f64 {
    f64::from(levels) * 10.0 + 10.0
}
pub fn duration(levels: u8) -> u32 {
    (9 + u32::from(levels) * 2) * 20
}

/// Whether a player whose box spans `min`..`max` is inside the box
/// `applyEffects` gathers: the beacon's cell grown by the range, stretched
/// up by the world's height.
pub fn reaches(pos: IVec3, levels: u8, height: f64, min: glam::DVec3, max: glam::DVec3) -> bool {
    let r = range(levels);
    let lo = pos.as_dvec3() - glam::DVec3::splat(r);
    let hi = pos.as_dvec3() + glam::DVec3::splat(1.0 + r) + glam::DVec3::Y * height;
    min.cmplt(hi).all() && max.cmpgt(lo).all()
}

/// One coloured stretch of a beacon's beam (`BeaconBeamOwner.Section`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Section {
    /// `DyeColor.getTextureDiffuseColor` (RGB), averaged where colours mix.
    pub color: [u8; 3],
    pub height: i32,
}

/// What a beacon remembers.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Beacon {
    pub levels: u8,
    pub primary: Option<Effect>,
    pub secondary: Option<Effect>,
    /// `CustomName`, copied from and back to the item.
    pub name: crate::inventory::StackName,
    beam: Vec<Section>,
    checking: Vec<Section>,
    /// `lastCheckY`; `None` before the first check (`level.getMinY() - 1`).
    last_check_y: Option<i32>,
}

impl Beacon {
    /// `getBeamSections`: no beam while the pyramid is missing.
    pub fn beam(&self) -> &[Section] {
        if self.levels == 0 { &[] } else { &self.beam }
    }

    fn save(&self) -> Value {
        let mut v = json!({"levels": self.levels});
        if let Some(p) = self.primary {
            v["primary_effect"] = json!(format!("minecraft:{}", p.id()));
        }
        if let Some(s) = self.secondary {
            v["secondary_effect"] = json!(format!("minecraft:{}", s.id()));
        }
        if let Some(name) = self.name.as_str() {
            v["CustomName"] = json!(name);
        }
        v
    }

    fn load(v: &Value) -> Self {
        // `loadEffect` keeps only the beacon's own powers.
        let effect = |key: &str| {
            v[key]
                .as_str()
                .and_then(|s| Effect::from_id(s.trim_start_matches("minecraft:")))
                .filter(|&e| required_levels(e).is_some())
        };
        Beacon {
            levels: v["levels"].as_u64().map_or(0, |l| l.min(u64::from(MAX_LEVELS)) as u8),
            primary: effect("primary_effect"),
            secondary: effect("secondary_effect"),
            name: v["CustomName"].as_str().and_then(crate::inventory::StackName::new).unwrap_or_default(),
            ..Default::default()
        }
    }
}

/// Beacons by position, kept while their chunk is unloaded.
#[derive(Default, Debug)]
pub struct State {
    pub beacons: BTreeMap<[i32; 3], Beacon>,
    /// A LAN replica's own tick count, for its beams' cadence.
    replica_ticks: u64,
}

/// The beacon's sounds (`SoundEvents.BEACON_*`, played as `BLOCKS`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Cue {
    Activate,
    Ambient,
    PowerSelect,
    Deactivate,
}

impl Cue {
    pub const ALL: [Cue; 4] = [Cue::Activate, Cue::Ambient, Cue::PowerSelect, Cue::Deactivate];
}

/// What beacons did this tick, for the game to apply and play.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Event {
    /// Apply `primary` (amplifier `amplifier`) and, if any, `secondary` to
    /// players inside the box for `ticks`.
    Powers { pos: IVec3, levels: u8, primary: Effect, amplifier: u8, secondary: Option<Effect>, ticks: u32 },
    /// `BEACON_AMBIENT` with each power pulse.
    Ambient(IVec3),
    /// `BEACON_POWER_SELECT`: new powers chosen while the beam is up.
    PowerSelect(IVec3),
    /// `BEACON_ACTIVATE`: a beam and pyramid came together.
    Activate(IVec3),
    /// `BEACON_DEACTIVATE`: the beam or pyramid went (or the beacon).
    Deactivate(IVec3),
}

/// The beam colour a block adds (`BeaconBeamBlock`): stained glass and
/// panes their dye, the beacon white.
fn beam_colour(b: Block) -> Option<[u8; 3]> {
    if b == BEACON {
        return Some(crate::color::DyeColor::White.rgb());
    }
    let c = b.canonical();
    c.stained_glass_color().or_else(|| c.stained_pane_color()).map(|d| d.rgb())
}

/// `ARGB.average`.
fn average(a: [u8; 3], b: [u8; 3]) -> [u8; 3] {
    [0, 1, 2].map(|i| ((u16::from(a[i]) + u16::from(b[i])) / 2) as u8)
}

/// Name, kind and layers: a glass shell (+X, -X, +Y, -Y, +Z, -Z all the
/// same; the inner boxes take theirs from [`box_layer`]).
pub const fn registry() -> (&'static str, RenderKind, [u16; 6]) {
    ("beacon", RenderKind::Shaped, [GLASS_TEX; 6])
}

/// Java's beacon model: the glass shell, the obsidian plinth and the
/// glowing core.
pub fn boxes() -> Boxes {
    let mut out = Boxes::from_box(Box16::FULL);
    out.push(Box16 { min: [2, 0, 2], max: [14, 3, 14] });
    out.push(Box16 { min: [3, 3, 3], max: [13, 14, 13] });
    out
}

/// The shell covers the cell (`Shapes.block()` collision and outline).
pub fn collision() -> Boxes {
    Boxes::from_box(Box16::FULL)
}

/// Layers of the plinth and core boxes.
pub fn box_layer(b: Block, i: usize) -> Option<u16> {
    (b == BEACON).then_some(match i {
        1 => super::block::tex::OBSIDIAN,
        2 => CORE_TEX,
        _ => GLASS_TEX,
    })
}

/// Original art: a clear shell with a pale frame, a cyan-white core, and a
/// soft vertical streak for the beam.
pub fn pixel(layer: u16, x: usize, y: usize) -> [u8; 4] {
    let n = ((x * 5 + y * 11 + x * y) % 5) as u8;
    match layer {
        GLASS_TEX => {
            let edge = x == 0 || y == 0 || x == 15 || y == 15;
            let glint = (x + y == 5 || x + y == 6) && x < 6;
            if edge {
                [196, 226, 232, 255]
            } else if glint {
                [236, 248, 250, 255]
            } else {
                [0; 4]
            }
        }
        CORE_TEX => {
            let (cx, cy) = (x as i32 - 8, y as i32 - 8);
            let r2 = (cx * cx + cy * cy) as u8;
            let k = 255u8.saturating_sub(r2.saturating_mul(2));
            [k / 2 + 100 + n, k / 4 + 190, 230 + n * 4, 255]
        }
        _ => {
            let centre = (x as i32 - 8).unsigned_abs() as u8;
            let k = 255u8.saturating_sub(centre * 20);
            [k, k, k, 255]
        }
    }
}

impl super::World {
    pub(super) fn track_beacon(&mut self, p: IVec3, old: Block, new: Block) {
        let key = p.to_array();
        if new == BEACON {
            if old != BEACON {
                self.beacons.beacons.entry(key).or_default();
            }
        } else if old == BEACON && self.beacons.beacons.remove(&key).is_some_and(|b| b.levels > 0) {
            // `setRemoved` plays the deactivation sound.
            self.beacon_events.push(Event::Deactivate(p));
        }
    }

    pub fn beacon(&self, p: IVec3) -> Option<&Beacon> {
        self.beacons.beacons.get(&p.to_array())
    }

    pub fn beacon_mut(&mut self, p: IVec3) -> Option<&mut Beacon> {
        self.beacons.beacons.get_mut(&p.to_array())
    }

    /// Beacons in loaded chunks with a beam to draw.
    pub fn beacon_beams(&self) -> impl Iterator<Item = (IVec3, &[Section])> + '_ {
        self.beacons
            .beacons
            .iter()
            .map(|(p, b)| (IVec3::from_array(*p), b.beam()))
            .filter(|(p, beam)| !beam.is_empty() && self.chunk_loaded(super::chunk::chunk_of(*p)))
    }

    /// `ServerboundSetBeaconPacket` through `BeaconMenu.updateEffects` once
    /// the payment is checked: sets the powers if the pyramid allows them,
    /// with `BEACON_POWER_SELECT` while the beam is up. Returns whether
    /// they were accepted (the caller then takes the payment).
    pub fn set_beacon_powers(&mut self, p: IVec3, primary: Option<Effect>, secondary: Option<Effect>) -> bool {
        let Some(b) = self.beacons.beacons.get_mut(&p.to_array()) else { return false };
        if !valid_powers(primary, secondary, b.levels) {
            return false;
        }
        b.primary = primary;
        b.secondary = secondary;
        if !b.beam.is_empty() {
            self.beacon_events.push(Event::PowerSelect(p));
        }
        true
    }

    /// Takes the beacon events since the last call.
    pub fn take_beacon_events(&mut self) -> Vec<Event> {
        std::mem::take(&mut self.beacon_events)
    }

    /// `BeaconBlockEntity.tick` for every beacon in a loaded chunk, at game
    /// time `time`. A LAN replica first mirrors its beacons from the
    /// block-entity models it holds and only draws the beam (no events).
    pub fn tick_beacons(&mut self, time: u64) {
        if self.remote {
            let present: Vec<[i32; 3]> =
                self.banners().filter(|(_, b)| *b == BEACON).map(|(p, _)| p.to_array()).collect();
            self.beacons.beacons.retain(|k, _| present.contains(k));
            for k in present {
                self.beacons.beacons.entry(k).or_default();
            }
        }
        let keys: Vec<[i32; 3]> = self.beacons.beacons.keys().copied().collect();
        for key in keys {
            let p = IVec3::from_array(key);
            if !self.chunk_loaded(super::chunk::chunk_of(p)) || self.get_block(p) != Some(BEACON) {
                continue;
            }
            let Some(mut b) = self.beacons.beacons.remove(&key) else { continue };
            self.tick_beacon(p, &mut b, time);
            self.beacons.beacons.insert(key, b);
        }
    }

    fn tick_beacon(&mut self, p: IVec3, b: &mut Beacon, time: u64) {
        let min_y = self.min_y();
        // `lastSetBlock`: Java climbs to the WORLD_SURFACE heightmap; this
        // climbs to the top of the world (the cells above the highest
        // block are air, which only lengthens the last section).
        let top = self.max_y() - 1;
        let mut check = match b.last_check_y {
            Some(y) if y >= p.y => IVec3::new(p.x, y + 1, p.z),
            _ => {
                b.checking.clear();
                b.last_check_y = Some(p.y - 1);
                p
            }
        };
        let mut last = b.checking.len().checked_sub(1);
        let mut i = 0;
        while i < CHECKS_PER_TICK && check.y <= top {
            // Java's column is always loaded with the beacon; an unloaded
            // cube above (VoxelCraft streams 32-block cubes) reads as air.
            let block = self.get_block(check).unwrap_or(Block::AIR);
            if let Some(colour) = beam_colour(block) {
                if b.checking.len() <= 1 {
                    b.checking.push(Section { color: colour, height: 1 });
                    last = Some(b.checking.len() - 1);
                } else if let Some(l) = last {
                    if b.checking[l].color == colour {
                        b.checking[l].height += 1;
                    } else {
                        let mixed = average(b.checking[l].color, colour);
                        b.checking.push(Section { color: mixed, height: 1 });
                        last = Some(b.checking.len() - 1);
                    }
                }
            } else {
                let Some(l) = last.filter(|_| block.light_opacity() < 15 || block == Block::BEDROCK) else {
                    b.checking.clear();
                    b.last_check_y = Some(top);
                    break;
                };
                b.checking[l].height += 1;
            }
            check.y += 1;
            b.last_check_y = Some(b.last_check_y.unwrap_or(p.y - 1) + 1);
            i += 1;
        }
        let previous = b.levels;
        if time.is_multiple_of(UPDATE_INTERVAL) {
            if !b.beam.is_empty() {
                b.levels = self.beacon_levels(p);
            }
            if b.levels > 0
                && !b.beam.is_empty()
                && !self.remote
                && let Some(primary) = b.primary
            {
                let amplifier = u8::from(b.levels >= 4 && b.primary == b.secondary);
                let secondary = b.secondary.filter(|&s| b.levels >= 4 && Some(s) != b.primary);
                let ticks = duration(b.levels);
                self.beacon_events.push(Event::Powers {
                    pos: p,
                    levels: b.levels,
                    primary,
                    amplifier,
                    secondary,
                    ticks,
                });
            }
            if b.levels > 0 && !b.beam.is_empty() && !self.remote {
                self.beacon_events.push(Event::Ambient(p));
            }
        }
        if b.last_check_y.is_some_and(|y| y >= top) {
            b.last_check_y = Some(min_y - 1);
            b.beam = std::mem::take(&mut b.checking);
            if !self.remote {
                let (was, is) = (previous > 0, b.levels > 0);
                if !was && is {
                    self.beacon_events.push(Event::Activate(p));
                } else if was && !is {
                    self.beacon_events.push(Event::Deactivate(p));
                }
            }
        }
    }

    /// A LAN replica's beams: its beacons tick on its own clock (no powers,
    /// no sounds), so the beam shows as the host's would.
    pub fn tick_beacon_replicas(&mut self) {
        self.beacons.replica_ticks = self.beacons.replica_ticks.wrapping_add(1);
        let t = self.beacons.replica_ticks;
        self.tick_beacons(t);
    }

    /// `updateBase`: full square layers of base blocks under the beacon,
    /// at most four.
    pub fn beacon_levels(&self, p: IVec3) -> u8 {
        let mut levels = 0;
        for step in 1..=i32::from(MAX_LEVELS) {
            let y = p.y - step;
            if y < self.min_y() {
                break;
            }
            for x in p.x - step..=p.x + step {
                for z in p.z - step..=p.z + step {
                    if !self.get_block(IVec3::new(x, y, z)).is_some_and(is_base) {
                        return levels;
                    }
                }
            }
            levels = step as u8;
        }
        levels
    }

    /// `beacons` dimension property.
    pub fn beacons_to_string(&self) -> String {
        let list: Vec<Value> = self.beacons.beacons.iter().map(|(p, b)| json!([p, b.save()])).collect();
        json!({"version": 1, "beacons": list}).to_string()
    }

    pub fn load_beacons(&mut self, text: &str) {
        let Ok(v) = serde_json::from_str::<Value>(text) else { return };
        for e in v["beacons"].as_array().into_iter().flatten() {
            let Some(p) = e[0].as_array().filter(|a| a.len() == 3).and_then(|a| {
                let c = |i: usize| a[i].as_i64().and_then(|n| i32::try_from(n).ok());
                Some([c(0)?, c(1)?, c(2)?])
            }) else {
                continue;
            };
            self.beacons.beacons.insert(p, Beacon::load(&e[1]));
        }
    }
}

#[cfg(test)]
mod tests;
