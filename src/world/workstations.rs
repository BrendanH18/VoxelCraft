//! v0.15 Track B block entities and states: banner patterns
//! (`BannerBlockEntity`: patterns and custom name) and lecterns
//! (`LecternBlock`, `LecternBlockEntity`: the book, the open page, the
//! comparator output and the two-tick pulse when a page turns).
//!
//! Lectern states: the four facings at 937..=940 keep their meaning
//! (`has_book=false, powered=false`); the other twelve are state ids
//! 11776..=11787 (`11776 + combo * 4 + facing`, combo 0 book, 1 powered,
//! 2 book and powered), inside Track B's reservation 11776..=12223.
//!
//! Saved in each dimension's `workstations` level property:
//! `b:x,y,z=<name hex>;<payload field or ->` per patterned or named banner
//! and `l:x,y,z=<book stack>;<page>` per lectern holding a book.

use glam::IVec3;
use rustc_hash::FxHashMap;

use super::World;
use super::block::{Block, Facing};
use super::colour_blocks::banner;
use crate::content::books;
use crate::inventory::components::{Payload, StackPayload};
use crate::inventory::{Stack, StackName, stack_from_str, stack_to_string};

/// The plain lectern states (`has_book=false, powered=false`).
pub const LECTERN_BASE: u16 = 937;
pub const LECTERN_FIRST: u16 = 11776;
pub const LECTERN_LAST: u16 = LECTERN_FIRST + 11;
const _: () = assert!(LECTERN_FIRST >= 11776 && LECTERN_LAST <= 12223);

/// Texture layers 3859..=3951 are Track B's: the banner pattern masks
/// (two per pattern, top then bottom half of the flag), then the lectern's
/// open book.
pub const TEX: u16 = 3859;
pub const PATTERN_TEX: u16 = TEX;
pub const BOOK_TOP: u16 = PATTERN_TEX + 2 * crate::inventory::components::BANNER_PATTERNS.len() as u16;
pub const TEX_END: u16 = BOOK_TOP + 1;
const _: () = assert!(TEX_END <= 3952);

/// The layer holding half `half` (0 top, 1 bottom) of pattern `p`'s mask.
pub fn pattern_layer(p: crate::inventory::components::BannerPattern, half: u8) -> u16 {
    PATTERN_TEX
        + 2 * crate::inventory::components::BannerPattern::all().position(|q| q == p).unwrap_or(0) as u16
        + u16::from(half)
}

/// Track B's texture layers: white pattern masks (tinted when drawn) and
/// the lectern's open book seen from above. Original art.
pub fn pixel(layer: u16, x: usize, y: usize) -> [u8; 4] {
    use crate::inventory::components::BannerPattern;
    if layer < BOOK_TOP {
        let i = (layer - PATTERN_TEX) as usize;
        let Some(p) = BannerPattern::all().nth(i / 2) else { return [0; 4] };
        let on = crate::content::banner_patterns::texel(p, (i % 2) as u8, x, y, 16);
        return if on { [255, 255, 255, 255] } else { [0, 0, 0, 0] };
    }
    // The lectern's open book (`BOOK_TOP`) is drawn by the client's
    // `render::material_textures` (v0.18 Track 3).
    [0; 4]
}

/// `LecternBlock`'s `PAGE_CHANGE_IMPULSE_TICKS`.
pub const PAGE_PULSE_TICKS: u64 = 2;

/// A lectern state's facing, `has_book` and `powered`.
pub const fn lectern(id: u16) -> Option<(Facing, bool, bool)> {
    if id >= LECTERN_BASE && id < LECTERN_BASE + 4 {
        return Some((Facing::ALL[(id - LECTERN_BASE) as usize], false, false));
    }
    if id >= LECTERN_FIRST && id <= LECTERN_LAST {
        let i = id - LECTERN_FIRST;
        let combo = i / 4;
        return Some((Facing::ALL[(i % 4) as usize], combo != 1, combo != 0));
    }
    None
}

/// The lectern state with these properties.
pub const fn lectern_state(facing: Facing, has_book: bool, powered: bool) -> Block {
    let f = facing as u16;
    match (has_book, powered) {
        (false, false) => Block(LECTERN_BASE + f),
        (true, false) => Block(LECTERN_FIRST + f),
        (false, true) => Block(LECTERN_FIRST + 4 + f),
        (true, true) => Block(LECTERN_FIRST + 8 + f),
    }
}

pub const fn is_lectern(b: Block) -> bool {
    lectern(b.0).is_some()
}

/// A placed banner's `patterns` and `CustomName`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct BannerEntity {
    pub patterns: Option<StackPayload>,
    pub name: StackName,
}

impl BannerEntity {
    fn is_empty(&self) -> bool {
        self.patterns.is_none() && self.name.as_str().is_none()
    }
}

/// A lectern's book and open page.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Lectern {
    pub book: Stack,
    pub page: usize,
}

impl Lectern {
    pub fn pages(&self) -> usize {
        books::page_count(&self.book)
    }

    /// `getRedstoneSignal`.
    pub fn signal(&self) -> u8 {
        books::lectern_signal(self.page, self.pages())
    }
}

#[derive(Clone, Debug, Default)]
pub struct State {
    pub(in crate::world) banners: FxHashMap<IVec3, BannerEntity>,
    /// Banners removed since the last tick, so the drop that follows the
    /// removal carries their patterns (Java's loot copies them from the
    /// block entity).
    removed: FxHashMap<IVec3, BannerEntity>,
    pub(in crate::world) lecterns: FxHashMap<IVec3, Lectern>,
    /// Bumped on every change, for renderers and LAN snapshots.
    version: u64,
}

impl State {
    pub fn version(&self) -> u64 {
        self.version
    }
}

/// What a page turn asks of the game: Java's level event 1043.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PageTurn(pub IVec3);

impl World {
    pub fn workstation_entities(&self) -> &State {
        &self.workstations
    }

    /// A placed banner's patterns and name, if it has any.
    pub fn banner_entity(&self, p: IVec3) -> Option<&BannerEntity> {
        self.workstations.banners.get(&p)
    }

    /// `BannerBlockEntity.applyImplicitComponents`: the placed banner takes
    /// the item's patterns (and the ominous banner's other components) and
    /// custom name.
    pub fn restore_banner_item(&mut self, p: IVec3, item: &Stack) {
        if !self.get_block(p).is_some_and(|b| banner::decode(b.0).is_some()) {
            return;
        }
        let entity =
            BannerEntity { patterns: item.payload.filter(|p| matches!(p.get(), Payload::Banner(_))), name: item.name };
        if entity.is_empty() {
            self.workstations.banners.remove(&p);
        } else {
            self.workstations.banners.insert(p, entity);
        }
        self.workstations.version += 1;
    }

    /// The item a banner drops, with what its block entity held (the loot
    /// table's `copy_components`). Consumes the record of a banner just
    /// removed at `p`.
    pub(crate) fn banner_drop(&mut self, p: IVec3, block: Block) -> Option<Stack> {
        let b = banner::decode(block.0)?;
        let mut stack = Stack::new(banner::standing(b.colour, 0), 1);
        if let Some(e) = self.workstations.removed.remove(&p).or_else(|| self.workstations.banners.get(&p).copied()) {
            stack.payload = e.patterns;
            stack.name = e.name;
        }
        Some(stack)
    }

    /// The banner item a banner at `p` would drop now (pick block, tests).
    pub fn banner_item_at(&self, p: IVec3) -> Option<Stack> {
        let b = banner::decode(self.get_block(p)?.0)?;
        let mut stack = Stack::new(banner::standing(b.colour, 0), 1);
        if let Some(e) = self.workstations.banners.get(&p) {
            stack.payload = e.patterns;
            stack.name = e.name;
        }
        Some(stack)
    }

    /// Keeps the tables in step with a block change: a banner that goes
    /// leaves its entity for the drop that follows; a lectern that goes
    /// drops its book (`LecternBlockEntity.preRemoveSideEffects`).
    pub(super) fn track_workstations(&mut self, p: IVec3, old: Block, new: Block) {
        if old == new {
            return;
        }
        let banner_old = banner::decode(old.0);
        let banner_new = banner::decode(new.0);
        if banner_old.is_some()
            && banner_new.is_none_or(|n| Some(n.colour) != banner_old.map(|o| o.colour))
            && let Some(e) = self.workstations.banners.remove(&p)
        {
            self.workstations.removed.insert(p, e);
            self.workstations.version += 1;
        }
        if is_lectern(old) && !is_lectern(new) {
            if let Some(l) = self.workstations.lecterns.remove(&p) {
                if self.tile_drops {
                    self.drops.push((p + IVec3::Y, l.book));
                }
                self.workstations.version += 1;
            }
        } else if let Some((_, false, _)) = lectern(new.0)
            && self.workstations.lecterns.remove(&p).is_some()
        {
            self.workstations.version += 1;
        }
    }

    /// Forgets removal records nobody claimed (a banner broken with drops
    /// off).
    pub fn clear_removed_banners(&mut self) {
        self.workstations.removed.clear();
    }

    pub fn lectern_at(&self, p: IVec3) -> Option<&Lectern> {
        self.workstations.lecterns.get(&p)
    }

    /// `LecternBlock.tryPlaceBook`: an empty lectern takes one book (any
    /// `#lectern_books` item). Returns whether it did.
    pub fn place_lectern_book(&mut self, p: IVec3, book: Stack) -> bool {
        let Some((facing, false, _)) = self.get_block(p).and_then(|b| lectern(b.0)) else { return false };
        if !books::lectern_book(book.item) {
            return false;
        }
        self.workstations.lecterns.insert(p, Lectern { book: Stack { count: 1, ..book }, page: 0 });
        self.workstations.version += 1;
        // `resetBookState`: has_book, unpowered.
        self.set_block(p, lectern_state(facing, true, false));
        self.redstone_changed(p);
        true
    }

    /// `LecternMenu` button 3: the book comes out and the lectern resets.
    pub fn take_lectern_book(&mut self, p: IVec3) -> Option<Stack> {
        let l = self.workstations.lecterns.remove(&p)?;
        self.workstations.version += 1;
        if let Some((facing, _, _)) = self.get_block(p).and_then(|b| lectern(b.0)) {
            self.set_block(p, lectern_state(facing, false, false));
            self.redstone_changed(p);
        }
        Some(l.book)
    }

    /// `LecternBlockEntity.setPage`: clamps to the book; a change pulses the
    /// lectern for two ticks and turns the page audibly. Returns whether
    /// the page changed.
    pub fn set_lectern_page(&mut self, p: IVec3, page: i64) -> bool {
        let Some(l) = self.workstations.lecterns.get_mut(&p) else { return false };
        let page = books::clamp_page(page, l.pages());
        if page == l.page {
            return false;
        }
        l.page = page;
        self.workstations.version += 1;
        // `signalPageChange`.
        if let Some((facing, book, _)) = self.get_block(p).and_then(|b| lectern(b.0)) {
            self.set_block(p, lectern_state(facing, book, true));
            self.redstone_changed(p);
            self.schedule_redstone(p, PAGE_PULSE_TICKS, 0);
        }
        if self.page_turns.len() < 64 {
            self.page_turns.push(PageTurn(p));
        }
        true
    }

    /// The scheduled end of a page-turn pulse (`LecternBlock.tick`).
    pub(super) fn end_lectern_pulse(&mut self, p: IVec3) -> bool {
        let Some((facing, book, true)) = self.get_block(p).and_then(|b| lectern(b.0)) else { return false };
        self.set_block(p, lectern_state(facing, book, false));
        self.redstone_changed(p);
        true
    }

    /// `getAnalogOutputSignal`.
    pub fn lectern_comparator(&self, p: IVec3) -> Option<u8> {
        let (_, book, _) = lectern(self.get_block(p)?.0)?;
        Some(if book { self.workstations.lecterns.get(&p).map_or(0, Lectern::signal) } else { 0 })
    }

    /// `LecternBlock.getSignal` / `getDirectSignal`: a pulsing lectern
    /// weakly powers every side and strongly powers the block below.
    pub(super) fn lectern_signal(b: Block, direction: IVec3, strong: bool) -> Option<u8> {
        let (_, _, powered) = lectern(b.0)?;
        Some(if powered && (!strong || direction == IVec3::NEG_Y) { 15 } else { 0 })
    }

    pub fn workstations_to_string(&self) -> String {
        self.workstations_where(|_| true)
    }

    /// The entries within `radius` of `at`, for a LAN client.
    pub fn workstations_near(&self, at: glam::DVec3, radius: f64) -> String {
        self.workstations_where(|p| (p.as_dvec3() + glam::DVec3::splat(0.5)).distance_squared(at) <= radius * radius)
    }

    fn workstations_where(&self, keep: impl Fn(&IVec3) -> bool) -> String {
        let pos = |p: &IVec3| format!("{},{},{}", p.x, p.y, p.z);
        let mut out: Vec<String> = self
            .workstations
            .banners
            .iter()
            .filter(|(p, _)| keep(p))
            .map(|(p, e)| {
                let name: String = e.name.as_str().unwrap_or("").bytes().map(|b| format!("{b:02x}")).collect();
                format!("b:{}={name};{}", pos(p), e.patterns.map_or("-", |f| f.field()))
            })
            .collect();
        out.extend(
            self.workstations
                .lecterns
                .iter()
                .filter(|(p, _)| keep(p))
                .map(|(p, l)| format!("l:{}={};{}", pos(p), stack_to_string(Some(l.book)), l.page)),
        );
        out.sort();
        out.join("|")
    }

    /// Restores [`World::workstations_to_string`]; malformed entries are
    /// skipped.
    pub fn load_workstations(&mut self, text: &str) {
        for entry in text.split('|').filter(|e| !e.is_empty()) {
            let Some((head, body)) = entry.split_once('=') else { continue };
            let Some((tag, at)) = head.split_once(':') else { continue };
            let c: Vec<i32> = at.split(',').filter_map(|v| v.parse().ok()).collect();
            let &[x, y, z] = &c[..] else { continue };
            let p = IVec3::new(x, y, z);
            let Some((a, b)) = body.split_once(';') else { continue };
            match tag {
                "b" => {
                    let Some(name) = unhex(a).and_then(|n| StackName::new(&n)) else { continue };
                    let patterns = match b {
                        "-" => None,
                        f => match Payload::from_field(f) {
                            Some(p) => p,
                            None => continue,
                        },
                    };
                    let e = BannerEntity { patterns, name };
                    if !e.is_empty() {
                        self.workstations.banners.insert(p, e);
                    }
                }
                "l" => {
                    let (Some(Some(book)), Ok(page)) = (stack_from_str(a), b.parse::<i64>()) else { continue };
                    let page = books::clamp_page(page, books::page_count(&book));
                    self.workstations.lecterns.insert(p, Lectern { book, page });
                }
                _ => {}
            }
        }
        self.workstations.version += 1;
    }

    /// A LAN client: replaces its entities with the host's.
    pub fn replace_workstations(&mut self, text: &str) {
        self.workstations.banners.clear();
        self.workstations.lecterns.clear();
        self.load_workstations(text);
    }
}

fn unhex(s: &str) -> Option<String> {
    if !s.is_ascii() || !s.len().is_multiple_of(2) {
        return None;
    }
    let bytes: Option<Vec<u8>> = (0..s.len()).step_by(2).map(|i| u8::from_str_radix(&s[i..i + 2], 16).ok()).collect();
    String::from_utf8(bytes?).ok()
}

#[cfg(test)]
mod tests;
