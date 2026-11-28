//! End portals: putting an eye of ender in a frame, and opening the portal
//! once twelve frames with eyes ring a 3x3 hole, all facing in (Java's
//! `EndPortalFrameBlock` pattern).

use glam::IVec3;

use super::World;
use super::block::{Block, Facing};

/// The twelve frame cells around the portal centred on `center`, each
/// with the way it must face (toward the middle).
pub fn ring(center: IVec3) -> [(IVec3, Facing); 12] {
    let mut out = [(IVec3::ZERO, Facing::South); 12];
    for (i, d) in (-1..=1).enumerate() {
        out[i * 4] = (center + IVec3::new(d, 0, -2), Facing::South);
        out[i * 4 + 1] = (center + IVec3::new(d, 0, 2), Facing::North);
        out[i * 4 + 2] = (center + IVec3::new(-2, 0, d), Facing::East);
        out[i * 4 + 3] = (center + IVec3::new(2, 0, d), Facing::West);
    }
    out
}

/// Whether `b` is an End portal frame holding an eye.
pub fn has_eye(b: Block) -> bool {
    (204..=207).contains(&b.0)
}

/// The centre of a complete ring of eyed frames that includes the frame at
/// `frame`, if there is one.
pub fn find_portal(frame: IVec3, get: impl Fn(IVec3) -> Option<Block>) -> Option<IVec3> {
    centers(frame).find(|&c| ring(c).iter().all(|&(p, facing)| get(p).is_some_and(|b| fits(b, facing, false))))
}

/// Whether `b` is a frame facing `facing`, with an eye unless `eyeless_ok`.
fn fits(b: Block, facing: Facing, eyeless_ok: bool) -> bool {
    b.base() == Block::END_PORTAL_FRAME && (eyeless_ok || has_eye(b)) && b.oriented().is_some_and(|(_, f)| f == facing)
}

/// Centres of the rings the frame at `frame` could be part of.
fn centers(frame: IVec3) -> impl Iterator<Item = IVec3> {
    // The frame is on one side of the ring: its centre is within two cells.
    (-2..=2)
        .flat_map(move |dz| (-2..=2).map(move |dx| frame + IVec3::new(dx, 0, dz)))
        .filter(move |c| ring(*c).iter().any(|&(p, _)| p == frame))
}

/// Whether every ring an eye in `frame` could complete is fully loaded, so
/// [`find_portal`] sees it. Rings that a loaded cell already rules out (a
/// missing eye, wrong block or facing) don't count.
pub fn rings_loaded(frame: IVec3, get: impl Fn(IVec3) -> Option<Block>) -> bool {
    centers(frame).all(|c| {
        let cells = ring(c).map(|(p, facing)| (get(p), facing, p == frame));
        // The frame getting the eye only needs to face the right way.
        let possible = cells.iter().all(|&(b, facing, is_frame)| b.is_none_or(|b| fits(b, facing, is_frame)));
        !possible || cells.iter().all(|(b, ..)| b.is_some())
    })
}

/// `TheEndGatewayBlockEntity`'s exit search: 1024 blocks out, stepping a
/// chunk (16) at a time at most 16 times, end stone from y 30, the island
/// at y 75, the tallest block within 16, the exit ten above it; arrivals
/// stand on the tallest block within 5 of the exit gateway's cap.
pub const EXIT_DISTANCE: f64 = 1024.0;
pub const EXIT_STEPS: i32 = 16;
pub const SPAWN_MIN_Y: i32 = 30;
pub const ISLAND_Y: i32 = 75;
pub const TALLEST_RADIUS: i32 = 16;
pub const EXIT_ABOVE: i32 = 10;
pub const ARRIVAL_RADIUS: i32 = 5;

/// v0.16 Track G: where a new End gateway's exit goes
/// (`TheEndGatewayBlockEntity.findOrCreateValidTeleportPos`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct GatewayExit {
    /// The exit gateway's cell: ten above the tallest block found.
    pub exit: IVec3,
    /// Where an `EndIslandFeature` island must be built first, when the
    /// search found no end stone to stand on.
    pub island: Option<IVec3>,
}

/// `BlockPos.asLong`: 26 bits of x at 38, 26 of z at 12, 12 of y at 0.
pub fn block_pos_long(p: IVec3) -> i64 {
    const H: i64 = (1 << 26) - 1;
    const V: i64 = (1 << 12) - 1;
    ((i64::from(p.x) & H) << 38) | ((i64::from(p.z) & H) << 12) | (i64::from(p.y) & V)
}

/// The end stone cells of `EndIslandFeature.place` at `origin`, with the
/// gateway's `RandomSource.create(origin.asLong())` (a legacy LCG): a disc
/// of radius 4–6 shrinking by 0.5–1.5 per layer down.
pub fn java_island(origin: IVec3) -> Vec<IVec3> {
    let mut rng = crate::enchant::JavaRandom::new(block_pos_long(origin));
    island_with(origin, &mut |n| rng.next_bounded(n))
}

/// `EndIslandFeature.place` at `origin` drawing from `int` (`nextInt`).
pub fn island_with(origin: IVec3, int: &mut impl FnMut(i32) -> i32) -> Vec<IVec3> {
    let mut size = int(3) as f32 + 4.0;
    let mut out = Vec::new();
    let mut y = 0;
    while size > 0.5 {
        let (lo, hi) = ((-size).floor() as i32, size.ceil() as i32);
        for x in lo..=hi {
            for z in lo..=hi {
                if (x * x + z * z) as f32 <= (size + 1.0) * (size + 1.0) {
                    out.push(origin + IVec3::new(x, y, z));
                }
            }
        }
        size -= int(2) as f32 + 0.5;
        y -= 1;
    }
    out
}

/// The End as it stands for the gateway search: loaded or saved chunks,
/// else the generator's, plus an island about to be built.
struct EndView<'a> {
    end: &'a super::end::EndGen,
    loaded: &'a dyn Fn(IVec3) -> Option<std::sync::Arc<super::chunk::ChunkData>>,
    sections: std::cell::RefCell<rustc_hash::FxHashMap<IVec3, std::sync::Arc<super::chunk::ChunkData>>>,
    island: Vec<IVec3>,
    max_y: i32,
}

impl EndView<'_> {
    fn section(&self, cpos: IVec3) -> std::sync::Arc<super::chunk::ChunkData> {
        if let Some(d) = self.sections.borrow().get(&cpos) {
            return d.clone();
        }
        let d = (self.loaded)(cpos).unwrap_or_else(|| std::sync::Arc::new(self.end.generate(cpos)));
        self.sections.borrow_mut().insert(cpos, d.clone());
        d
    }

    fn get(&self, p: IVec3) -> Block {
        if p.y < 0 || p.y >= self.max_y {
            return Block::AIR;
        }
        if self.island.contains(&p) {
            return Block::END_STONE;
        }
        let l = super::chunk::local_of(p);
        self.section(super::chunk::chunk_of(p)).get(l.x as usize, l.y as usize, l.z as usize)
    }

    /// For Java's 16-block chunk column `cx, cz`, the top of its highest
    /// non-empty 16-high section, if any (`getHighestSectionPosition() +
    /// 15`). VoxelCraft's chunks are larger cubes, so the column's own
    /// cells are read.
    fn highest(&self, cx: i32, cz: i32) -> Option<i32> {
        use super::chunk::CHUNK_SIZE_I as S;
        let (x0, z0) = (cx * 16, cz * 16);
        for cy in (0..self.max_y.div_euclid(S)).rev() {
            let cpos = IVec3::new(x0.div_euclid(S), cy, z0.div_euclid(S));
            if self.section(cpos).uniform() == Some(Block::AIR) {
                continue;
            }
            for y in (cy * S..cy * S + S).rev() {
                let filled = (z0..z0 + 16).any(|z| (x0..x0 + 16).any(|x| self.get(IVec3::new(x, y, z)) != Block::AIR));
                if filled {
                    return Some(y.div_euclid(16) * 16 + 15);
                }
            }
        }
        None
    }
}

impl crate::physics::BlockSource for EndView<'_> {
    fn block(&self, p: IVec3) -> Option<Block> {
        Some(self.get(p))
    }
}

/// `findTallestBlock`: over the columns within `dist` of `around` (x
/// outer, z inner; its own column only `with_bedrock`), the highest
/// full-collision block, a later column only when strictly taller;
/// `around` itself if there's none.
pub fn tallest_block<W: crate::physics::BlockSource + ?Sized>(
    world: &W,
    around: IVec3,
    dist: i32,
    with_bedrock: bool,
    (min_y, max_y): (i32, i32),
) -> IVec3 {
    let mut tallest: Option<IVec3> = None;
    for xd in -dist..=dist {
        for zd in -dist..=dist {
            if xd == 0 && zd == 0 && !with_bedrock {
                continue;
            }
            let floor = tallest.map_or(min_y, |t| t.y);
            let mut y = max_y - 1;
            while y > floor {
                let p = IVec3::new(around.x + xd, y, around.z + zd);
                let b = world.block(p).unwrap_or(Block::AIR);
                if b != Block::AIR && crate::physics::full_collision(world, p) && (with_bedrock || b != Block::BEDROCK)
                {
                    tallest = Some(p);
                    break;
                }
                y -= 1;
            }
        }
    }
    tallest.unwrap_or(around)
}

/// `findOrCreateValidTeleportPos` for the gateway at `entry`: 1024 blocks
/// straight out from the centre, back past chunks with blocks (up to 16)
/// then on past empty ones (up to 16); in that chunk, the end stone from
/// y 30 up with two non-full blocks above nearest the world origin (else
/// an island at y 75); then the tallest block within 16 of it, and ten up.
/// `loaded` gives the chunk sections as they stand where they exist.
pub fn gateway_exit(
    end: &super::end::EndGen,
    loaded: &dyn Fn(IVec3) -> Option<std::sync::Arc<super::chunk::ChunkData>>,
    entry: IVec3,
    max_y: i32,
) -> GatewayExit {
    let mut view = EndView { end, loaded, sections: Default::default(), island: Vec::new(), max_y };
    let dir = glam::DVec2::new(f64::from(entry.x), f64::from(entry.z)).normalize_or_zero();
    let mut at = dir * EXIT_DISTANCE;
    let chunk = |at: glam::DVec2| ((at.x / 16.0).floor() as i32, (at.y / 16.0).floor() as i32);
    let empty = |view: &EndView, at: glam::DVec2| {
        let (cx, cz) = chunk(at);
        view.highest(cx, cz).is_none()
    };
    let mut limit = EXIT_STEPS;
    while !empty(&view, at) && limit > 0 {
        limit -= 1;
        at -= dir * 16.0;
    }
    let mut limit = EXIT_STEPS;
    while empty(&view, at) && limit > 0 {
        limit -= 1;
        at += dir * 16.0;
    }
    // `findValidSpawnInChunk`: x fastest, then y, then z; the first of
    // the nearest kept.
    let (cx, cz) = chunk(at);
    let mut found: Option<(f64, IVec3)> = None;
    if let Some(top) = view.highest(cx, cz) {
        for z in cz * 16..cz * 16 + 16 {
            for y in SPAWN_MIN_Y..=top {
                for x in cx * 16..cx * 16 + 16 {
                    let p = IVec3::new(x, y, z);
                    if view.get(p) != Block::END_STONE
                        || crate::physics::full_collision(&view, p + IVec3::Y)
                        || crate::physics::full_collision(&view, p + IVec3::Y * 2)
                    {
                        continue;
                    }
                    let d = (p.as_dvec3() + glam::DVec3::splat(0.5)).length_squared();
                    if found.is_none_or(|(best, _)| d < best) {
                        found = Some((d, p));
                    }
                }
            }
        }
    }
    let (start, island) = match found {
        Some((_, p)) => (p, None),
        None => {
            let origin = IVec3::new((at.x + 0.5).floor() as i32, ISLAND_Y, (at.y + 0.5).floor() as i32);
            view.island = java_island(origin);
            (origin, Some(origin))
        }
    };
    let tallest = tallest_block(&view, start, TALLEST_RADIUS, true, (0, max_y));
    GatewayExit { exit: tallest + IVec3::Y * EXIT_ABOVE, island }
}

/// Java draws X, Y, then Z, each as the difference of two bounded draws.
fn egg_offset(mut draw: impl FnMut(u32) -> i32) -> IVec3 {
    IVec3::new(draw(16) - draw(16), draw(8) - draw(8), draw(16) - draw(16))
}

impl World {
    /// Builds an End gateway centred on `origin`.
    pub fn build_gateway(&mut self, origin: IVec3) {
        for dy in -2..=2 {
            for dz in -1..=1 {
                for dx in -1..=1 {
                    let p = origin + IVec3::new(dx, dy, dz);
                    if let Some(b) = super::end::gateway_block(origin, p)
                        && self.get_block(p) != Some(b)
                    {
                        self.set_block(p, b);
                    }
                }
            }
        }
    }

    /// Generation 7: where the return gateway at `cell` leads
    /// (`end_gateway_return`'s exact exit), if it is one.
    pub fn return_gateway_exit(&self, cell: IVec3) -> Option<IVec3> {
        self.generator.end()?.return_gateway_at(cell)
    }

    /// Notes the return gateways a newly generated section holds.
    pub(super) fn register_return_gateways(&mut self, pos: IVec3, data: &super::chunk::ChunkData) {
        let Some(end) = self.generator.end() else { return };
        let base = pos * super::chunk::CHUNK_SIZE_I;
        let size = super::chunk::CHUNK_SIZE_I;
        for cz in (base.z..base.z + size).step_by(16) {
            for cx in (base.x..base.x + size).step_by(16) {
                let Some(g) = end.return_gateway_at_chunk(glam::IVec2::new(cx >> 4, cz >> 4)) else { continue };
                let l = g - base;
                if l.cmpge(IVec3::ZERO).all()
                    && l.cmplt(IVec3::splat(size)).all()
                    && data.get(l.x as usize, l.y as usize, l.z as usize) == Block::END_GATEWAY
                {
                    self.generated_gateways.push(g);
                }
            }
        }
    }

    /// v0.16 Track G: the gateway exit search for the gateway at `entry`
    /// against the End as it stands (see [`gateway_exit`]).
    pub fn find_gateway_exit(&self, entry: IVec3) -> Option<GatewayExit> {
        let end = self.generator.end()?;
        Some(gateway_exit(end, &|c| self.section_data(c), entry, self.max_y()))
    }

    /// The `EndIslandFeature` island at `origin` (see [`java_island`]),
    /// for gateways that lead out over the void.
    pub fn build_end_island(&mut self, origin: IVec3) {
        for p in java_island(origin) {
            self.set_block(p, Block::END_STONE);
        }
    }

    /// Where someone coming out of the gateway at `gateway` stands
    /// (`findExitPosition`): on the highest full block, not bedrock, within
    /// five of it (its own column aside), or on its cap if there's none.
    pub fn gateway_arrival(&self, gateway: IVec3) -> glam::DVec3 {
        let from = gateway + IVec3::Y * 2;
        let stand = tallest_block(self, from, ARRIVAL_RADIUS, false, (self.min_y(), self.max_y())) + IVec3::Y;
        stand.as_dvec3() + glam::DVec3::new(0.5, 0.0, 0.5)
    }

    /// Hitting or using the dragon egg makes it jump to a random empty cell
    /// up to 15 blocks away (Java's `DragonEggBlock.teleport`). Returns
    /// where it went.
    pub fn teleport_egg(&mut self, pos: IVec3) -> Option<IVec3> {
        if self.get_block(pos) != Some(Block::DRAGON_EGG) {
            return None;
        }
        for _ in 0..1000 {
            let to = pos + egg_offset(|bound| (self.roll() % u64::from(bound)) as i32);
            // Java 26.3 also rejects unsupported air, outside the default
            // world border, and positions outside this dimension's height.
            // Unknown/unloaded support never counts as a non-air block.
            if self.contains_y(to.y)
                && (-29_999_984..29_999_984).contains(&to.x)
                && (-29_999_984..29_999_984).contains(&to.z)
                && self.get_block(to) == Some(Block::AIR)
                && self.get_block(to - IVec3::Y).is_some_and(|b| b != Block::AIR)
            {
                self.set_block(pos, Block::AIR);
                self.set_block(to, Block::DRAGON_EGG);
                return Some(to);
            }
        }
        None
    }

    /// Opens the exit portal in the End's podium once the dragon is dead,
    /// with the dragon egg on its pillar after the first kill.
    pub fn open_exit_portal(&mut self, egg: bool) {
        self.set_exit_portal(true, egg);
    }

    /// v0.16 Track G: builds the exit portal open (`END_PODIUM_ACTIVE`) or
    /// shut (`END_PODIUM_INACTIVE`, the respawn ritual's), with the dragon
    /// egg on top if `egg`. Shutting it is `respawnDragon`'s: every bedrock
    /// and portal block of the podium turns to end stone, then the shut
    /// podium is built over it, which leaves the bowl empty.
    pub fn set_exit_portal(&mut self, active: bool, egg: bool) {
        let Some(origin) = self.generator.end().map(|e| e.podium()) else { return };
        for dy in -1..=3 {
            for dz in -4..=4 {
                for dx in -4..=4 {
                    let p = origin + IVec3::new(dx, dy, dz);
                    if let Some(b) = super::end::podium_block(origin, p, active)
                        && self.get_block(p) != Some(b)
                    {
                        self.set_block(p, b);
                    }
                }
            }
        }
        if egg {
            self.set_block(origin + IVec3::Y * 4, Block::DRAGON_EGG);
        }
    }

    /// v0.16 Track G: the first half of `SUMMONING_PILLARS`' rebuild: every
    /// block within 10 of the pillar's bedrock cap goes (`removeBlock`).
    pub fn clear_spike_top(&mut self, p: &super::end::Pillar) {
        let top = IVec3::new(p.x, p.top, p.z);
        let r = crate::entity::dragon::PILLAR_CLEAR;
        for dy in -r..=r {
            for dz in -r..=r {
                for dx in -r..=r {
                    let at = top + IVec3::new(dx, dy, dz);
                    if self.get_block(at).is_some_and(|b| b != Block::AIR) {
                        self.set_block(at, Block::AIR);
                    }
                }
            }
        }
    }

    /// v0.16 Track G: the second half, `EndSpikeFeature.placeSpike`: the
    /// pillar, its cage and bedrock cap rebuilt, with fire where its
    /// crystal stands (the caller adds the crystal).
    pub fn build_spike(&mut self, p: &super::end::Pillar) {
        let r = p.radius.max(2);
        for y in self.min_y()..=p.top + 10 {
            for dz in -r..=r {
                for dx in -r..=r {
                    let at = IVec3::new(p.x + dx, y, p.z + dz);
                    if let Some(b) = p.block_at(at)
                        && self.get_block(at).is_some_and(|old| old != b)
                    {
                        self.set_block(at, b);
                    }
                }
            }
        }
        let cell = p.crystal().floor().as_ivec3();
        if self.get_block(cell) != Some(Block::AIR) {
            self.set_block(cell, Block::AIR);
        }
        self.crystal_fire(cell);
    }

    /// Puts an eye of ender in the empty frame at `pos`. Returns `None` if
    /// there's no empty frame there (or part of its ring hasn't loaded, so
    /// the last eye can't be spent without opening the portal), else
    /// whether this opened a portal.
    pub fn insert_eye(&mut self, pos: IVec3) -> Option<bool> {
        let frame = self.get_block(pos).filter(|b| b.base() == Block::END_PORTAL_FRAME && !has_eye(*b))?;
        if !rings_loaded(pos, |p| self.get_block(p)) {
            return None;
        }
        self.set_block(pos, Block(frame.0 + 4));
        let Some(center) = find_portal(pos, |p| self.get_block(p)) else { return Some(false) };
        for dz in -1..=1 {
            for dx in -1..=1 {
                self.set_block(center + IVec3::new(dx, 0, dz), Block::END_PORTAL);
            }
        }
        Some(true)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rustc_hash::FxHashMap;

    #[test]
    fn egg_draws_match_the_java_fixture_in_xyz_order() {
        let fixture: serde_json::Value =
            serde_json::from_str(include_str!("../../tests/fixtures/java/26.3/mob_integrations.json")).unwrap();
        assert_eq!(fixture["dragon_egg"]["attempts"], 1000);
        assert_eq!(fixture["dragon_egg"]["requires_non_air_below"], true);
        let mut bounds = Vec::new();
        let mut values = [15, 0, 0, 7, 5, 2].into_iter();
        assert_eq!(
            egg_offset(|bound| {
                bounds.push(bound);
                values.next().unwrap()
            }),
            IVec3::new(15, -7, 3)
        );
        assert_eq!(serde_json::json!(bounds), fixture["dragon_egg"]["draw_bounds_xyz"]);
    }

    fn eyed(facing: Facing) -> Block {
        Block(Block::END_PORTAL_FRAME.0 + 4).with_facing(facing)
    }

    #[test]
    fn a_full_inward_ring_opens_from_any_frame() {
        let center = IVec3::new(10, 30, -4);
        let mut blocks: FxHashMap<IVec3, Block> = ring(center).iter().map(|&(p, f)| (p, eyed(f))).collect();
        for &(p, _) in &ring(center) {
            assert_eq!(find_portal(p, |q| blocks.get(&q).copied()), Some(center));
        }
        // Not from somewhere that isn't on the ring.
        assert_eq!(find_portal(center, |q| blocks.get(&q).copied()), None);

        // One frame turned outward, or missing its eye, keeps it shut.
        let (p, f) = ring(center)[5];
        blocks.insert(p, eyed(f.opposite()));
        assert_eq!(find_portal(ring(center)[0].0, |q| blocks.get(&q).copied()), None);
        blocks.insert(p, Block::END_PORTAL_FRAME.with_facing(f));
        assert_eq!(find_portal(ring(center)[0].0, |q| blocks.get(&q).copied()), None);
    }

    #[test]
    fn unloaded_ring_cells_hold_off_the_last_eye() {
        let center = IVec3::new(0, 64, 0);
        let (frame, facing) = ring(center)[0];
        // Eleven eyed frames and an empty one, all facing in.
        let mut blocks: FxHashMap<IVec3, Block> = ring(center).iter().map(|&(p, f)| (p, eyed(f))).collect();
        blocks.insert(frame, Block::END_PORTAL_FRAME.with_facing(facing));
        let get = |q: IVec3| Some(blocks.get(&q).copied().unwrap_or(Block::AIR));
        assert!(rings_loaded(frame, get));

        // Part of the ring unloaded: wait for it.
        let far = ring(center)[7].0;
        assert!(!rings_loaded(frame, |q| if q == far { None } else { get(q) }));

        // Another candidate ring crossing unloaded cells doesn't matter
        // once a loaded cell rules it out (here, plain air).
        let other = center + IVec3::new(-1, 0, 0);
        assert!(ring(other).iter().any(|&(p, _)| p == frame));
        let unloaded = ring(other).iter().map(|&(p, _)| p).find(|p| p.x == -3).unwrap();
        assert!(rings_loaded(frame, |q| if q == unloaded { None } else { get(q) }));
    }

    #[test]
    fn ring_frames_face_the_middle() {
        let center = IVec3::new(0, 0, 0);
        for (p, f) in ring(center) {
            let inside = p + f.offset();
            assert!(inside.x.abs() <= 1 && inside.z.abs() <= 1, "{p} facing {f:?}");
        }
    }
}
