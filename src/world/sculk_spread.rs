//! The sculk catalyst's spreader (Java 26.3 `SculkSpreader` with its
//! `ChargeCursor`s, the `SculkBehaviour` of sculk, sculk veins and every
//! other block, and the sculk vein's `MultifaceSpreader`), for the level
//! spreader a catalyst uses: growth cost 10, no growth within 4 blocks of the
//! catalyst, charge decay 1 in 10, extra decay 1 in 5, `#sculk_replaceable`.
//!
//! Randomness comes from the world's own generator rather than Java's
//! `RandomSource`, so the same death spreads differently; the rules and odds
//! are Java's.

use glam::IVec3;
use serde_json::{Value, json};

use super::World;
use super::block::Block;
use super::cave_blocks::{self as cb, DIRS, Multiface, Phase};

/// `SculkSpreader.MAX_CHARGE`.
pub const MAX_CHARGE: i32 = 1000;
const MAX_CURSORS: usize = 32;
const MAX_CURSOR_DISTANCE: i32 = 1024;
const GROWTH_SPAWN_COST: i32 = 10;
const NO_GROWTH_RADIUS: i32 = 4;
const CHARGE_DECAY_RATE: u64 = 10;
const ADDITIONAL_DECAY_RATE: u64 = 5;
/// `SculkSpreader.MAX_GROWTH_RATE_RADIUS`.
const MAX_GROWTH_RATE_RADIUS: i32 = 24;

/// `SculkSpreader.ChargeCursor`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Cursor {
    pub pos: IVec3,
    pub charge: i32,
    pub update_delay: i32,
    pub decay_delay: i32,
    /// `facings`: unset, or the face bits ([`DIRS`] order) of the block it
    /// last moved onto.
    pub facings: Option<u8>,
}

impl Cursor {
    pub fn new(pos: IVec3, charge: i32) -> Self {
        Self { pos, charge, update_delay: 0, decay_delay: 1, facings: None }
    }

    pub fn save(&self) -> Value {
        json!({"pos":self.pos.to_array(),"charge":self.charge,"decay_delay":self.decay_delay,
            "update_delay":self.update_delay,"facings":self.facings})
    }

    pub fn load(v: &Value) -> Option<Self> {
        let pos: [i32; 3] = serde_json::from_value(v["pos"].clone()).ok()?;
        Some(Self {
            pos: IVec3::from_array(pos),
            charge: v["charge"].as_i64().unwrap_or(0).clamp(0, i64::from(MAX_CHARGE)) as i32,
            decay_delay: v["decay_delay"].as_i64().unwrap_or(1).clamp(0, 1) as i32,
            update_delay: v["update_delay"].as_i64().unwrap_or(0).clamp(0, i64::from(i32::MAX)) as i32,
            facings: v["facings"].as_u64().map(|f| (f & 63) as u8),
        })
    }
}

/// `SculkSpreader.addCursors`: `charge` in cursors of at most 1000, up to
/// 32 cursors.
pub fn add_cursors(cursors: &mut Vec<Cursor>, start: IVec3, mut charge: i32) {
    while charge > 0 {
        let c = charge.min(MAX_CHARGE);
        if cursors.len() < MAX_CURSORS {
            cursors.push(Cursor::new(start, c));
        }
        charge -= c;
    }
}

/// `#minecraft:sculk_replaceable`: overworld and Nether base stone, the
/// overworld substrates (dirt, mud, moss and grass blocks), terracotta,
/// nylium, sands, gravel, soul sand and soil, calcite, smooth basalt, clay,
/// dripstone, end stone and sandstones.
pub fn replaceable(b: Block) -> bool {
    let b = b.canonical();
    if matches!(
        b,
        Block::STONE
            | Block::GRANITE
            | Block::DIORITE
            | Block::ANDESITE
            | Block::DEEPSLATE
            | Block::DIRT
            | Block::GRASS
            | Block::SAND
            | Block::GRAVEL
            | Block::CALCITE
            | Block::CLAY
            | Block::TUFF
    ) {
        return true;
    }
    let name = b.name();
    (name.contains("terracotta") && !name.contains("glazed"))
        || matches!(
            name,
            "coarse dirt"
                | "rooted dirt"
                | "mud"
                | "moss block"
                | "pale moss block"
                | "crimson nylium"
                | "warped nylium"
                | "netherrack"
                | "basalt"
                | "blackstone"
                | "red sand"
                | "soul sand"
                | "soul soil"
                | "smooth basalt"
                | "dripstone block"
                | "end stone"
                | "red sandstone"
                | "sandstone"
        )
}

/// Which `SculkBehaviour` a block has.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Behaviour {
    Default,
    Sculk,
    Vein,
}

fn behaviour(b: Block) -> Behaviour {
    if b.canonical() == cb::SCULK {
        Behaviour::Sculk
    } else if vein_faces(b).is_some() {
        Behaviour::Vein
    } else {
        Behaviour::Default
    }
}

/// A sculk vein's faces and `waterlogged`.
fn vein_faces(b: Block) -> Option<(u8, bool)> {
    match cb::multiface_of(b) {
        Some((Multiface::SculkVein, faces, wet)) => Some((faces, wet)),
        _ => None,
    }
}

const fn opposite(i: usize) -> usize {
    i ^ 1
}

/// The eighteen offsets of a 3×3×3 cube that aren't corners or the centre.
fn non_corner_neighbours() -> [IVec3; 18] {
    let mut out = [IVec3::ZERO; 18];
    let mut n = 0;
    for x in -1..=1 {
        for y in -1..=1 {
            for z in -1..=1 {
                if (x == 0 || y == 0 || z == 0) && (x, y, z) != (0, 0, 0) {
                    out[n] = IVec3::new(x, y, z);
                    n += 1;
                }
            }
        }
    }
    out
}

impl World {
    fn shuffle<T>(&mut self, items: &mut [T]) {
        for i in (1..items.len()).rev() {
            let j = (self.roll() % (i as u64 + 1)) as usize;
            items.swap(i, j);
        }
    }

    fn below(&mut self, n: u64) -> u64 {
        self.roll() % n
    }

    /// `SculkSpreader.updateCursors` for the catalyst at `origin`.
    pub(super) fn update_sculk_cursors(&mut self, origin: IVec3) {
        let Some(mut cursors) = self.sculk.entities.get_mut(&origin.to_array()).map(|e| std::mem::take(&mut e.cursors))
        else {
            return;
        };
        let mut processed: Vec<Cursor> = Vec::with_capacity(cursors.len());
        let mut at: Vec<(IVec3, usize)> = Vec::new();
        for mut c in cursors.drain(..) {
            let d = (c.pos - origin).abs();
            if d.x.max(d.y).max(d.z) > MAX_CURSOR_DISTANCE {
                continue;
            }
            self.update_cursor(&mut c, origin);
            if c.charge <= 0 {
                continue;
            }
            match at.iter().position(|&(p, _)| p == c.pos) {
                None => {
                    at.push((c.pos, processed.len()));
                    processed.push(c);
                }
                Some(k) => {
                    let existing = at[k].1;
                    if c.charge + processed[existing].charge <= MAX_CHARGE {
                        // `mergeWith`.
                        processed[existing].charge += c.charge;
                        processed[existing].update_delay = processed[existing].update_delay.min(c.update_delay);
                    } else {
                        let smaller = c.charge < processed[existing].charge;
                        processed.push(c);
                        if smaller {
                            at[k].1 = processed.len() - 1;
                        }
                    }
                }
            }
        }
        if let Some(e) = self.sculk.entities.get_mut(&origin.to_array()) {
            e.cursors = processed;
        }
    }

    /// `ChargeCursor.update` (the level spreader, veins spreading).
    fn update_cursor(&mut self, c: &mut Cursor, origin: IVec3) {
        if c.charge <= 0 {
            return;
        }
        if c.update_delay > 0 {
            c.update_delay -= 1;
            return;
        }
        let Some(mut state) = self.get_block(c.pos) else { return };
        let mut beh = behaviour(state);
        if self.attempt_spread_vein(beh, c.pos, state, c.facings) {
            if beh != Behaviour::Sculk {
                state = self.get_block(c.pos).unwrap_or(Block::AIR);
                beh = behaviour(state);
            }
            self.sculk.sounds.push((c.pos.as_dvec3() + glam::DVec3::splat(0.5), super::sculk::SculkSound::Spread));
        }
        c.charge = self.attempt_use_charge(beh, c, origin);
        if c.charge <= 0 {
            self.on_discharged(beh, state, c.pos);
            return;
        }
        if let Some(to) = self.valid_movement_pos(c.pos) {
            self.on_discharged(beh, state, c.pos);
            c.pos = to;
            state = self.get_block(to).unwrap_or(Block::AIR);
        }
        match behaviour(state) {
            Behaviour::Sculk => c.facings = Some(0),
            Behaviour::Vein => c.facings = vein_faces(state).map(|(f, _)| f),
            Behaviour::Default => {}
        }
        c.decay_delay = if beh == Behaviour::Default { (c.decay_delay - 1).max(0) } else { 1 };
        c.update_delay = 1;
    }

    /// `SculkBehaviour.attemptSpreadVein`.
    fn attempt_spread_vein(&mut self, beh: Behaviour, pos: IVec3, state: Block, facings: Option<u8>) -> bool {
        match (beh, facings) {
            (Behaviour::Default, None) => {
                let here = self.get_block(pos).unwrap_or(Block::AIR);
                self.spread_veins(here, pos, true) > 0
            }
            (Behaviour::Default, Some(f)) if f != 0 => {
                if state != Block::AIR && !state.holds_water() {
                    return false;
                }
                self.regrow_vein(pos, state, f)
            }
            _ => self.spread_veins(state, pos, false) > 0,
        }
    }

    /// `SculkBehaviour.attemptUseCharge` of each behaviour.
    fn attempt_use_charge(&mut self, beh: Behaviour, c: &Cursor, origin: IVec3) -> i32 {
        let charge = c.charge;
        match beh {
            Behaviour::Default => {
                if c.decay_delay > 0 {
                    charge
                } else {
                    0
                }
            }
            Behaviour::Vein => {
                if self.attempt_place_sculk(c.pos) {
                    charge - 1
                } else if self.below(CHARGE_DECAY_RATE) == 0 {
                    (charge as f32 * 0.5).floor() as i32
                } else {
                    charge
                }
            }
            Behaviour::Sculk => {
                if charge == 0 || self.below(CHARGE_DECAY_RATE) != 0 {
                    return charge;
                }
                let at = c.pos;
                let close = (at - origin).length_squared() < NO_GROWTH_RADIUS * NO_GROWTH_RADIUS;
                if !close && self.can_place_growth(at) {
                    if (self.below(GROWTH_SPAWN_COST as u64) as i32) < charge {
                        self.place_growth(at + IVec3::Y);
                    }
                    (charge - GROWTH_SPAWN_COST).max(0)
                } else if self.below(ADDITIONAL_DECAY_RATE) != 0 {
                    charge
                } else {
                    charge - if close { 1 } else { decay_penalty(at, origin, charge) }
                }
            }
        }
    }

    /// `SculkBlock.canPlaceGrowth`: air or water above, and at most two
    /// sensors and shriekers in the 9×3×9 box from this height up.
    fn can_place_growth(&self, at: IVec3) -> bool {
        let above = self.get_block(at + IVec3::Y);
        if !matches!(above, Some(Block::AIR | Block::WATER)) {
            return false;
        }
        let mut found = 0;
        for x in -4..=4 {
            for y in 0..=2 {
                for z in -4..=4 {
                    if self
                        .get_block(at + IVec3::new(x, y, z))
                        .is_some_and(|b| cb::sensor_of(b).is_some() || cb::shrieker_of(b).is_some())
                    {
                        found += 1;
                        if found > 2 {
                            return false;
                        }
                    }
                }
            }
        }
        true
    }

    /// `SculkBlock.getRandomGrowthState`: a shrieker one time in eleven
    /// (never able to summon when grown in the level), else a sensor;
    /// waterlogged in water.
    fn place_growth(&mut self, at: IVec3) {
        let wet = self.get_block(at).is_some_and(|b| b.holds_water());
        let growth = if self.below(11) == 0 {
            cb::sculk_shrieker(false, false, wet)
        } else {
            cb::sculk_sensor(Phase::Inactive, wet)
        };
        self.edit(at, growth, false);
        self.sculk.sounds.push((at.as_dvec3() + glam::DVec3::splat(0.5), super::sculk::SculkSound::Grow));
    }

    /// `SculkVeinBlock.onDischarged`: faces lying on sculk go; a vein with no
    /// face left leaves air or water.
    fn on_discharged(&mut self, beh: Behaviour, state: Block, pos: IVec3) {
        if beh != Behaviour::Vein {
            return;
        }
        let Some((mut faces, wet)) = vein_faces(state) else { return };
        if self.get_block(pos) != Some(state) {
            return;
        }
        for (i, d) in DIRS.iter().enumerate() {
            if faces & (1 << i) != 0 && self.get_block(pos + *d) == Some(cb::SCULK) {
                faces &= !(1 << i);
            }
        }
        let next = if faces == 0 {
            if wet { Block::WATER } else { Block::AIR }
        } else {
            cb::multiface(Multiface::SculkVein, faces, wet)
        };
        if next != state {
            self.edit(pos, next, false);
        }
    }

    /// `SculkVeinBlock.attemptPlaceSculk`: a supporting block the vein lies
    /// on becomes sculk, veins spread out from it, and veins lying on it go.
    fn attempt_place_sculk(&mut self, pos: IVec3) -> bool {
        let Some(state) = self.get_block(pos) else { return false };
        let Some((faces, _)) = vein_faces(state) else { return false };
        let mut order = [0usize, 1, 2, 3, 4, 5];
        self.shuffle(&mut order);
        for support in order {
            if faces & (1 << support) == 0 {
                continue;
            }
            let at = pos + DIRS[support];
            if !self.get_block(at).is_some_and(replaceable) {
                continue;
            }
            self.edit(at, cb::SCULK, false);
            self.sculk.sounds.push((at.as_dvec3() + glam::DVec3::splat(0.5), super::sculk::SculkSound::Spread));
            self.spread_veins(cb::SCULK, at, false);
            let skip = opposite(support);
            for (i, d) in DIRS.iter().enumerate() {
                if i == skip {
                    continue;
                }
                let q = at + *d;
                if let Some(vein) = self.get_block(q).filter(|&b| vein_faces(b).is_some()) {
                    self.on_discharged(Behaviour::Vein, vein, q);
                }
            }
            return true;
        }
        false
    }

    /// `SculkVeinBlock.hasSubstrateAccess`.
    fn has_substrate_access(&self, state: Block, pos: IVec3) -> bool {
        let Some((faces, _)) = vein_faces(state) else { return false };
        (0..6).any(|i| faces & (1 << i) != 0 && self.get_block(pos + DIRS[i]).is_some_and(replaceable))
    }

    /// `ChargeCursor.getValidMovementPos`: a sculk block or vein among the
    /// shuffled non-corner neighbours, preferring a vein on replaceable
    /// ground.
    fn valid_movement_pos(&mut self, pos: IVec3) -> Option<IVec3> {
        let mut offsets = non_corner_neighbours();
        self.shuffle(&mut offsets);
        let mut found = None;
        for o in offsets {
            let n = pos + o;
            let Some(b) = self.get_block(n) else { continue };
            if behaviour(b) == Behaviour::Default || !self.movement_unobstructed(pos, n) {
                continue;
            }
            found = Some(n);
            if self.has_substrate_access(b, n) {
                break;
            }
        }
        found
    }

    /// `ChargeCursor.isMovementUnobstructed`.
    fn movement_unobstructed(&self, from: IVec3, to: IVec3) -> bool {
        let d = to - from;
        if d.abs().element_sum() == 1 {
            return true;
        }
        let open = |dir: IVec3| {
            let test = from + dir;
            !self.get_block(test).is_some_and(|b| super::mount_states::face_sturdy(b, -dir))
        };
        let (x, y, z) = (IVec3::X * d.x.signum(), IVec3::Y * d.y.signum(), IVec3::Z * d.z.signum());
        // Java builds a direction for a zero delta as positive.
        let (x, y, z) = (
            if d.x == 0 { IVec3::X } else { x },
            if d.y == 0 { IVec3::Y } else { y },
            if d.z == 0 { IVec3::Z } else { z },
        );
        if d.x == 0 {
            open(y) || open(z)
        } else if d.y == 0 {
            open(x) || open(z)
        } else {
            open(x) || open(y)
        }
    }

    /// `MultifaceSpreader.spreadAll` with the sculk vein's spreader: from
    /// each face (every face for a block that isn't a vein) toward each
    /// direction across it, the first of same-cell, same-plane and
    /// wrap-around spreads the vein can take. `same_cell` keeps to the
    /// same cell (`getSameSpaceSpreader`). Returns how many faces grew.
    fn spread_veins(&mut self, state: Block, pos: IVec3, same_cell: bool) -> u32 {
        let (faces, other) = match vein_faces(state) {
            Some((f, _)) => (f, false),
            None => (0, true),
        };
        let mut grown = 0;
        for (from, &from_dir) in DIRS.iter().enumerate() {
            if !other && faces & (1 << from) == 0 {
                continue;
            }
            for (toward, &toward_dir) in DIRS.iter().enumerate() {
                if toward / 2 == from / 2 {
                    continue;
                }
                if !other && faces & (1 << toward) != 0 {
                    continue;
                }
                let candidates =
                    [(pos, toward), (pos + toward_dir, from), (pos + toward_dir + from_dir, opposite(toward))];
                let tries = if same_cell { &candidates[..1] } else { &candidates[..] };
                if let Some(&(cell, face)) =
                    tries.iter().find(|(cell, face)| self.vein_can_spread_into(pos, *cell, *face))
                    && self.place_vein_face(cell, face)
                {
                    grown += 1;
                }
            }
        }
        grown
    }

    /// `SculkVeinSpreaderConfig.stateCanBeReplaced` and
    /// `MultifaceBlock.isValidStateForPlacement`.
    fn vein_can_spread_into(&self, source: IVec3, cell: IVec3, face: usize) -> bool {
        let Some(existing) = self.get_block(cell) else { return false };
        let Some(against) = self.get_block(cell + DIRS[face]) else { return false };
        if against.canonical() == cb::SCULK || cb::catalyst_bloom(against).is_some() {
            return false;
        }
        if (cell - source).abs().element_sum() == 2 {
            let n = source + DIRS[opposite(face)];
            if self.get_block(n).is_some_and(|b| super::mount_states::face_sturdy(b, DIRS[face])) {
                return false;
            }
        }
        if existing.is_lava() || existing.is_fire() {
            return false;
        }
        let vein = vein_faces(existing);
        let replaceable =
            existing.is_replaceable() || existing == Block::AIR || vein.is_some() || existing == Block::WATER;
        if !replaceable {
            return false;
        }
        // `isValidStateForPlacement`: a new face with a full face to lie on.
        vein.is_none_or(|(f, _)| f & (1 << face) == 0) && cb::supports(against, face)
    }

    /// `MultifaceSpreader.SpreadConfig.placeBlock`.
    fn place_vein_face(&mut self, cell: IVec3, face: usize) -> bool {
        let Some(old) = self.get_block(cell) else { return false };
        let (faces, wet) = match vein_faces(old) {
            Some((f, wet)) => (f, wet),
            None => (0, old == Block::WATER),
        };
        self.edit(cell, cb::multiface(Multiface::SculkVein, faces | 1 << face, wet), false)
    }

    /// `SculkVeinBlock.regrow`: a vein on whichever of `faces` have support.
    fn regrow_vein(&mut self, pos: IVec3, existing: Block, faces: u8) -> bool {
        let mut kept = 0u8;
        for (i, &d) in DIRS.iter().enumerate() {
            if faces & (1 << i) != 0 && self.get_block(pos + d).is_some_and(|n| cb::supports(n, i)) {
                kept |= 1 << i;
            }
        }
        if kept == 0 {
            return false;
        }
        self.edit(pos, cb::multiface(Multiface::SculkVein, kept, existing.holds_water()), false)
    }
}

/// `SculkBlock.getDecayPenalty`.
fn decay_penalty(at: IVec3, origin: IVec3, charge: i32) -> i32 {
    let outer = ((at - origin).length_squared() as f32).sqrt() - NO_GROWTH_RADIUS as f32;
    let reach = ((MAX_GROWTH_RATE_RADIUS - NO_GROWTH_RADIUS) * (MAX_GROWTH_RATE_RADIUS - NO_GROWTH_RADIUS)) as f32;
    let factor = (outer * outer / reach).min(1.0);
    ((charge as f32 * factor * 0.5) as i32).max(1)
}

/// A sculk sensor or shrieker of any state, which counts against growth.
pub fn is_growth(b: Block) -> bool {
    cb::sensor_of(b).is_some() || cb::shrieker_of(b).is_some()
}
