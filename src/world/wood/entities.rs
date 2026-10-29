//! Block entities of the wood track, kept in `World` by position like
//! chests: sign text (Java's `SignBlockEntity`), shelf items
//! (`ShelfBlockEntity`) and chiseled bookshelf books
//! (`ChiseledBookShelfBlockEntity`). They appear when their block is
//! placed, spill their items when it goes, and are saved in the level file
//! under `wood`.

use glam::IVec3;
use rustc_hash::FxHashMap;

use super::super::World;
use super::super::block::Block;
use super::signs::{self, Sign};
use super::{bookshelf, shelves};
use crate::color::DyeColor;
use crate::inventory::{Stack, stack_from_str, stack_to_string};

/// Java's `SignText`: four lines, a dye colour and the glow flag.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SignText {
    pub lines: [String; 4],
    pub color: DyeColor,
    pub glowing: bool,
}

impl Default for SignText {
    fn default() -> Self {
        Self { lines: Default::default(), color: DyeColor::Black, glowing: false }
    }
}

impl SignText {
    /// Java's `hasMessage`: some line has text.
    pub fn has_message(&self) -> bool {
        self.lines.iter().any(|l| !l.is_empty())
    }
}

/// Java's `DyeColor.getTextColor`.
pub const fn text_color(c: DyeColor) -> [u8; 3] {
    const COLORS: [u32; 16] = [
        0xFFFFFF, 0xFF681F, 0xFF00FF, 0x9AC0CD, 0xFFFF00, 0xBFFF00, 0xFF69B4, 0x808080, 0xD3D3D3, 0x00FFFF, 0xA020F0,
        0x0000FF, 0x8B4513, 0x00FF00, 0xFF0000, 0x000000,
    ];
    let v = COLORS[c as usize];
    [(v >> 16) as u8, (v >> 8) as u8, v as u8]
}

/// Longest line a sign keeps, in characters (Java's sign update packet
/// allows 384; the 90- or 60-pixel width limit is the edit screen's and
/// the renderer's).
pub const MAX_LINE_CHARS: usize = 384;

/// Characters a sign line can hold: printable ASCII, what the font draws.
pub fn allowed(c: char) -> bool {
    c.is_ascii() && !c.is_ascii_control()
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct SignEntity {
    pub front: SignText,
    pub back: SignText,
    /// Waxing waits for honeycomb (v0.15); the flag is kept for saves.
    pub waxed: bool,
}

impl SignEntity {
    pub fn side(&self, front: bool) -> &SignText {
        if front { &self.front } else { &self.back }
    }
    pub fn side_mut(&mut self, front: bool) -> &mut SignText {
        if front { &mut self.front } else { &mut self.back }
    }
}

/// The chiseled bookshelf's books and the last slot used (Java's
/// `lastInteractedSlot`, read by comparators).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Bookshelf {
    pub books: [Option<Stack>; 6],
    pub last_slot: Option<u8>,
}

#[derive(Clone, Debug, Default)]
pub struct WoodEntities {
    pub(super) signs: FxHashMap<IVec3, SignEntity>,
    pub(super) shelves: FxHashMap<IVec3, [Option<Stack>; 3]>,
    pub(super) bookshelves: FxHashMap<IVec3, Bookshelf>,
    /// Bumped on every change, so renderers can cache what they built.
    pub(super) version: u64,
}

impl WoodEntities {
    pub fn version(&self) -> u64 {
        self.version
    }
    pub fn signs(&self) -> impl Iterator<Item = (&IVec3, &SignEntity)> {
        self.signs.iter()
    }
    pub fn shelves(&self) -> impl Iterator<Item = (&IVec3, &[Option<Stack>; 3])> {
        self.shelves.iter()
    }
    pub fn sign(&self, p: IVec3) -> Option<&SignEntity> {
        self.signs.get(&p)
    }
    pub fn shelf(&self, p: IVec3) -> Option<&[Option<Stack>; 3]> {
        self.shelves.get(&p)
    }
    pub fn bookshelf(&self, p: IVec3) -> Option<&Bookshelf> {
        self.bookshelves.get(&p)
    }
}

// ---- Saving ---------------------------------------------------------------

fn hex(s: &str) -> String {
    s.bytes().map(|b| format!("{b:02x}")).collect()
}

fn unhex(s: &str) -> Option<String> {
    if !s.is_ascii() || !s.len().is_multiple_of(2) {
        return None;
    }
    let bytes: Option<Vec<u8>> = (0..s.len()).step_by(2).map(|i| u8::from_str_radix(&s[i..i + 2], 16).ok()).collect();
    String::from_utf8(bytes?).ok().filter(|t| t.chars().all(allowed))
}

fn text_to_string(t: &SignText) -> String {
    format!("{}:{}:{}", t.color as u8, t.glowing as u8, t.lines.iter().map(|l| hex(l)).collect::<Vec<_>>().join("."))
}

fn text_from_str(s: &str) -> Option<SignText> {
    let mut parts = s.splitn(3, ':');
    let color = *DyeColor::ALL.get(parts.next()?.parse::<usize>().ok()?)?;
    let glowing = parts.next()? == "1";
    let lines: Vec<String> = parts.next()?.split('.').map(unhex).collect::<Option<_>>()?;
    let lines: [String; 4] = lines.try_into().ok()?;
    Some(SignText { lines, color, glowing })
}

fn stacks_to_string(s: &[Option<Stack>]) -> String {
    s.iter().map(|&s| stack_to_string(s)).collect::<Vec<_>>().join(",")
}

fn stacks_from_str<const N: usize>(s: &str) -> Option<[Option<Stack>; N]> {
    let parts: Vec<&str> = s.split(',').collect();
    if parts.len() != N {
        return None;
    }
    let mut out = [None; N];
    for (slot, part) in out.iter_mut().zip(parts) {
        *slot = stack_from_str(part)?;
    }
    Some(out)
}

impl World {
    pub fn wood_entities(&self) -> &WoodEntities {
        &self.wood
    }

    /// `s:x,y,z=front;back;waxed`, `h:x,y,z=items` and `b:x,y,z=books;last`
    /// entries joined by `|`, for the level file.
    pub fn wood_to_string(&self) -> String {
        self.wood_where(|_| true)
    }

    /// [`World::wood_to_string`]'s entries within `radius` of `at`, nearest
    /// first, up to `max_bytes` (long sign text could otherwise outgrow a
    /// LAN client's snapshot frame).
    pub fn wood_near(&self, at: glam::DVec3, radius: f64, max_bytes: usize) -> String {
        let distance = |p: &IVec3| (p.as_dvec3() + glam::DVec3::splat(0.5)).distance_squared(at);
        let all = self.wood_where(|p| distance(p) <= radius * radius);
        let mut entries: Vec<(f64, &str)> = all
            .split('|')
            .filter_map(|e| {
                let (head, _) = e.split_once('=')?;
                let c: Vec<i32> = head.split_once(':')?.1.split(',').filter_map(|v| v.parse().ok()).collect();
                let &[x, y, z] = &c[..] else { return None };
                Some((distance(&IVec3::new(x, y, z)), e))
            })
            .collect();
        entries.sort_by(|a, b| a.0.total_cmp(&b.0));
        let mut out = String::new();
        for (_, entry) in entries {
            if out.len() + entry.len() + 1 > max_bytes {
                break;
            }
            if !out.is_empty() {
                out.push('|');
            }
            out.push_str(entry);
        }
        out
    }

    fn wood_where(&self, keep: impl Fn(&IVec3) -> bool) -> String {
        let pos = |p: &IVec3| format!("{},{},{}", p.x, p.y, p.z);
        let signs = self.wood.signs.iter().filter(|(p, _)| keep(p)).map(|(p, s)| {
            format!("s:{}={};{};{}", pos(p), text_to_string(&s.front), text_to_string(&s.back), s.waxed as u8)
        });
        let shelves = self
            .wood
            .shelves
            .iter()
            .filter(|(p, _)| keep(p))
            .map(|(p, items)| format!("h:{}={}", pos(p), stacks_to_string(items)));
        let books = self.wood.bookshelves.iter().filter(|(p, _)| keep(p)).map(|(p, b)| {
            format!("b:{}={};{}", pos(p), stacks_to_string(&b.books), b.last_slot.map_or(-1, |s| s as i32))
        });
        signs.chain(shelves).chain(books).collect::<Vec<_>>().join("|")
    }

    /// A LAN client: replaces its signs, shelves and chiseled bookshelves
    /// with the host's ([`World::wood_near`]).
    pub fn replace_wood(&mut self, text: &str) {
        self.wood.signs.clear();
        self.wood.shelves.clear();
        self.wood.bookshelves.clear();
        self.load_wood(text);
    }

    /// Restores entities saved by [`World::wood_to_string`]; malformed
    /// entries are skipped.
    pub fn load_wood(&mut self, text: &str) {
        for entry in text.split('|').filter(|e| !e.is_empty()) {
            let Some((head, body)) = entry.split_once('=') else { continue };
            let Some((tag, pos)) = head.split_once(':') else { continue };
            let c: Vec<i32> = pos.split(',').filter_map(|v| v.parse().ok()).collect();
            let &[x, y, z] = &c[..] else { continue };
            let p = IVec3::new(x, y, z);
            match tag {
                "s" => {
                    let parts: Vec<&str> = body.split(';').collect();
                    if let [front, back, waxed] = parts[..]
                        && let (Some(front), Some(back)) = (text_from_str(front), text_from_str(back))
                    {
                        self.wood.signs.insert(p, SignEntity { front, back, waxed: waxed == "1" });
                    }
                }
                "h" => {
                    if let Some(items) = stacks_from_str::<3>(body) {
                        self.wood.shelves.insert(p, items);
                    }
                }
                "b" => {
                    if let Some((books, last)) = body.split_once(';')
                        && let Some(books) = stacks_from_str::<6>(books)
                    {
                        let last_slot = last.parse::<i8>().ok().filter(|&s| (0..6).contains(&s)).map(|s| s as u8);
                        self.wood.bookshelves.insert(p, Bookshelf { books, last_slot });
                    }
                }
                _ => {}
            }
        }
        self.wood.version += 1;
    }

    /// Keeps the entity tables in step with a block change at `p`: a block
    /// that stays the same family keeps its entity (a sign that turns or
    /// fills with water, a shelf powered or chained); one that goes spills
    /// its items.
    pub(in crate::world) fn track_wood(&mut self, p: IVec3, old: Block, new: Block) {
        if old == new {
            return;
        }
        let family = |b: Block| -> u8 {
            if signs::sign(b).is_some() {
                1
            } else if shelves::shelf(b).is_some() {
                2
            } else if bookshelf::slots(b).is_some() {
                3
            } else {
                0
            }
        };
        let (was, is) = (family(old), family(new));
        if was == 2 || is == 2 {
            self.shelf_changed(p, old, new);
            if was != 2 {
                self.shelf_power(p);
            }
        }
        if was == is {
            return;
        }
        match was {
            1 => {
                self.wood.signs.remove(&p);
            }
            2 => {
                if let Some(items) = self.wood.shelves.remove(&p)
                    && self.tile_drops
                {
                    self.drops.extend(items.into_iter().flatten().map(|s| (p, s)));
                }
            }
            3 => {
                if let Some(b) = self.wood.bookshelves.remove(&p)
                    && self.tile_drops
                {
                    self.drops.extend(b.books.into_iter().flatten().map(|s| (p, s)));
                }
            }
            _ => {}
        }
        match is {
            1 => {
                self.wood.signs.entry(p).or_default();
            }
            2 => {
                self.wood.shelves.entry(p).or_default();
            }
            3 => {
                self.wood.bookshelves.entry(p).or_default();
            }
            _ => {}
        }
        self.wood.version += 1;
    }

    /// The sign at `p`, if there is one.
    pub fn sign_at(&self, p: IVec3) -> Option<(Sign, &SignEntity)> {
        let s = signs::sign(self.get_block(p)?)?;
        Some((s, self.wood.signs.get(&p)?))
    }

    /// Java's `SignBlockEntity.updateSignText`: replaces one side's lines,
    /// keeping its colour and glow. Unprintable characters are dropped and
    /// lines are cut to [`MAX_LINE_CHARS`]. Waxed signs refuse.
    pub fn set_sign_text(&mut self, p: IVec3, front: bool, lines: [String; 4]) -> Result<(), &'static str> {
        if self.get_block(p).and_then(signs::sign).is_none() {
            return Err("there is no sign there");
        }
        let entity = self.wood.signs.entry(p).or_default();
        if entity.waxed {
            return Err("the sign is waxed");
        }
        let side = entity.side_mut(front);
        for (line, new) in side.lines.iter_mut().zip(lines) {
            *line = new.chars().filter(|&c| allowed(c)).take(MAX_LINE_CHARS).collect();
        }
        self.wood.version += 1;
        Ok(())
    }

    /// Changes one side of the sign at `p` with `f`; returns whether it
    /// changed (Java's `SignBlockEntity.updateText`).
    pub fn update_sign(&mut self, p: IVec3, front: bool, f: impl FnOnce(&mut SignText)) -> bool {
        let Some(entity) = self.wood.signs.get_mut(&p) else { return false };
        let side = entity.side_mut(front);
        let before = side.clone();
        f(side);
        let changed = *side != before;
        if changed {
            self.wood.version += 1;
        }
        changed
    }
}
