//! The headless rig: a flat test world, the game's tick order, the three
//! interruptions and the per-tick probe trace.
use glam::{DVec3, IVec2, IVec3};
use std::collections::BTreeMap;
use std::hash::{Hash, Hasher};
use std::sync::Arc;
use voxelcraft::{
    entity::{Ctx, Entities},
    inventory::Stack,
    item::Item,
    simulation::{self, TICK_SECONDS, difficulty::Difficulty, lighting::SpawnLight},
    world::{
        World,
        block::Block,
        storage::{LevelInfo, Storage},
        terrain::{Dimension, Generator},
    },
};

pub const SEED: u64 = 7;
/// Flat test terrain is a single stone layer at the bottom of the world;
/// every build brings its own supports, high above it.
pub const GROUND: i32 = -64;
/// Relative (0, 0, 0) of every build, well inside storage column (0, 0).
pub const ORIGIN: IVec3 = IVec3::new(4, 100, 4);
/// Game ticks the chunks stay unloaded. Odd and not a multiple of any
/// corpus period, so a phase slip shows.
pub const UNLOAD_GAP: u64 = 37;

/// What one probe reads after each game tick.
#[derive(Clone, Copy, Debug)]
pub enum Probe {
    /// The block id at a relative position.
    Block(IVec3),
    /// [`World::redstone_power`] at a relative position.
    Power(IVec3),
    /// Total items in the chest-backed container or furnace there.
    Items(IVec3),
    /// Items of one kind in the container or furnace there.
    Count(IVec3, Item),
    /// The smallest relative x (0..32) in row (y, z) holding the block, or -1.
    Locate(Block, i32, i32),
    /// The cart's x position in thousandths of a block.
    CartX(usize),
    /// Items in the cart.
    CartItems(usize),
    /// Items in loose item entities.
    LooseItems,
    /// Blocks in piston motion (heads included).
    Moving,
    /// The remaining delay of the scheduled tick at a relative position, or -1.
    Scheduled(IVec3),
    /// A hash of the pending scheduled ticks: position, remaining delay and
    /// priority, as the `redstone` save records them.
    Pending,
}

/// One reference contraption.
pub struct Spec {
    pub name: &'static str,
    /// Relative positions in placement order.
    pub blocks: Vec<(IVec3, Block)>,
    /// Chest-backed container contents (chests, hoppers, droppers), slot by slot.
    pub contents: Vec<(IVec3, Vec<Stack>)>,
    /// Extra set-up after placing blocks and contents.
    pub setup: fn(&mut Rig),
    /// Player-like actions before machine tick `m` (1-based).
    pub script: fn(&mut Rig, u64),
    pub probes: Vec<Probe>,
    pub ticks: u64,
    pub random_tick_speed: u32,
    /// Extra room the build moves into (the flying machine), relative.
    pub reach: IVec3,
    /// Blocks counted with the items in [`Rig::total`].
    pub counted: fn(Block) -> bool,
    /// Whether the counted total must stay constant (a closed system).
    pub closed: bool,
    /// Whether runs repeat exactly (no random ticks or random drops), so an
    /// interrupted trace must equal the reference trace.
    pub deterministic: bool,
    /// Machine ticks at which to unload and to save and restart.
    pub unload_at: u64,
    pub restart_at: u64,
    /// Restart at the first tick from here on with blocks in piston motion,
    /// or, for builds without pistons, with scheduled ticks pending.
    pub motion_after: u64,
}

fn nothing(_: &mut Rig) {}
fn no_script(_: &mut Rig, _: u64) {}
fn none(_: Block) -> bool {
    false
}

impl Default for Spec {
    fn default() -> Self {
        Self {
            name: "",
            blocks: Vec::new(),
            contents: Vec::new(),
            setup: nothing,
            script: no_script,
            probes: Vec::new(),
            ticks: 200,
            random_tick_speed: 0,
            reach: IVec3::ZERO,
            counted: none,
            closed: false,
            deterministic: true,
            unload_at: 50,
            restart_at: 70,
            motion_after: 20,
        }
    }
}

/// Blocks from ASCII layers: `layers[y][z]` is a row whose characters are x.
/// Spaces and dots are air (left untouched).
pub fn grid(layers: &[&[&str]], legend: &[(char, Block)]) -> Vec<(IVec3, Block)> {
    let mut blocks = Vec::new();
    for (y, rows) in layers.iter().enumerate() {
        for (z, row) in rows.iter().enumerate() {
            for (x, c) in row.chars().enumerate() {
                if c == ' ' || c == '.' {
                    continue;
                }
                let block = legend.iter().find(|(k, _)| *k == c).unwrap_or_else(|| panic!("no legend for {c:?}")).1;
                blocks.push((IVec3::new(x as i32, y as i32, z as i32), block));
            }
        }
    }
    blocks
}

pub fn abs(rel: IVec3) -> IVec3 {
    ORIGIN + rel
}

pub struct Rig {
    pub world: World,
    pub entities: Entities,
    /// Machine ticks run so far.
    pub tick: u64,
    columns: Vec<IVec2>,
    centre: DVec3,
    speed: u32,
    min: IVec3,
    max: IVec3,
}

impl Rig {
    pub fn new(spec: &Spec) -> Self {
        let (mut min, mut max) = (IVec3::ZERO, IVec3::ZERO);
        for &(p, _) in &spec.blocks {
            min = min.min(p);
            max = max.max(p);
        }
        max += spec.reach.max(IVec3::ZERO);
        min += spec.reach.min(IVec3::ZERO);
        let (min, max) = (abs(min) - IVec3::ONE, abs(max) + IVec3::ONE);
        let mut columns = Vec::new();
        for x in min.x.div_euclid(32)..=max.x.div_euclid(32) {
            for z in min.z.div_euclid(32)..=max.z.div_euclid(32) {
                columns.push(IVec2::new(x, z));
            }
        }
        let centre = ((min + max).as_dvec3()) * 0.5;
        let world = World::new_headless(Arc::new(Generator::new(SEED)), Default::default(), 2);
        let mut rig = Self {
            world,
            entities: Entities::new(SEED),
            tick: 0,
            columns,
            centre,
            speed: spec.random_tick_speed,
            min,
            max,
        };
        rig.world.set_simulation_players([centre]);
        rig.load_columns();
        for &(p, b) in &spec.blocks {
            rig.world.set_block(abs(p), b);
        }
        for (p, slots) in &spec.contents {
            let chest = rig.world.chest_mut(abs(*p)).unwrap_or_else(|| panic!("{}: no container at {p}", spec.name));
            for (slot, stack) in chest.slots.iter_mut().zip(slots) {
                *slot = Some(*stack);
            }
        }
        (spec.setup)(&mut rig);
        rig
    }

    fn load_columns(&mut self) {
        for &c in &self.columns {
            self.world.load_test_column(c, GROUND);
        }
        // Skylight is exact only where a column and its eight neighbours are
        // loaded (`World::sky_light`); elsewhere growth sees block light
        // alone, and a crop or stem under open sky never grows. A player's
        // surroundings are always loaded, so the ring around the build is
        // loaded too, as empty air (uniform chunks get no random ticks),
        // and stays loaded while the build's own columns unload.
        for &c in &self.columns {
            for dx in -1..=1 {
                for dz in -1..=1 {
                    let n = c + IVec2::new(dx, dz);
                    if !self.columns.contains(&n) && self.world.get_block(IVec3::new(n.x * 32, 0, n.y * 32)).is_none() {
                        self.world.load_test_column(n, i32::MIN);
                    }
                }
            }
        }
        self.world.update_block_light();
        self.entities.sync_chunk_entities(&mut self.world);
    }

    pub fn get(&self, rel: IVec3) -> Option<Block> {
        self.world.get_block(abs(rel))
    }

    pub fn set(&mut self, rel: IVec3, block: Block) {
        self.world.set_block(abs(rel), block);
    }

    /// A player's right click on the block (a button, lever, repeater...).
    pub fn press(&mut self, rel: IVec3) {
        assert!(self.world.use_redstone(abs(rel)), "nothing to use at {rel}");
    }

    /// One game tick in the order `Game::tick` runs these systems: carts,
    /// contact inputs, world rules (redstone, fluids, furnaces, random
    /// ticks), automation entities, entity storage, entities, then drops.
    fn game_tick(&mut self) {
        self.entities.tick_minecarts(&mut self.world, &[]);
        self.world.redstone_contacts(std::iter::empty(), &self.entities);
        simulation::tick_world_rules(&mut self.world, self.centre, false, self.speed);
        self.world.tick_automation_entities(&mut self.entities);
        self.entities.sync_chunk_entities(&mut self.world);
        let ctx = Ctx {
            players: Vec::new(),
            daylight: 1.0,
            spawn_light: SpawnLight::DAY,
            raining: false,
            spawning: false,
            dimension: Dimension::Overworld,
        };
        self.entities.update_world(TICK_SECONDS, &mut self.world, &ctx, Difficulty::Normal);
        for (cell, stack) in std::mem::take(&mut self.world.drops) {
            self.entities.drop_from_block(stack, cell);
        }
        self.world.xp_drops.clear();
    }

    /// Unload every column the build touches, keep the world ticking for
    /// `gap` game ticks, then load them back.
    pub fn unload_for(&mut self, gap: u64) {
        for &c in &self.columns {
            self.world.unload_test_column(c);
        }
        self.entities.sync_chunk_entities(&mut self.world);
        for _ in 0..gap {
            self.game_tick();
        }
        self.load_columns();
    }

    /// Save to disk, drop the world, and load a fresh one from the save.
    pub fn restart(self, name: &str) -> Self {
        let dir = std::env::temp_dir().join(format!(
            "voxelcraft-contraption-{name}-{}-{}",
            std::process::id(),
            std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos()
        ));
        let storage = Storage::new(&dir);
        let mut props = dimension_props(&self.world);
        self.entities.save_props(&mut props);
        {
            let _lock = storage.lock().unwrap();
            storage.save(&LevelInfo { seed: SEED, player: None, props }, &self.world.modified_chunks()).unwrap();
        }
        let Self { columns, centre, speed, min, max, tick, .. } = self;
        drop(self.world);
        drop(self.entities);
        let level = storage.load_level().unwrap();
        let chunks = storage.load_chunks().unwrap();
        let mut world = World::new_headless(Arc::new(Generator::new(level.seed)), chunks, 2);
        world.set_simulation_players([centre]);
        restore_dimension(&mut world, &level.props);
        let mut entities = Entities::new(SEED);
        entities.load_props(&level.props);
        let mut rig = Self { world, entities, tick, columns, centre, speed, min, max };
        rig.load_columns();
        std::fs::remove_dir_all(dir).unwrap();
        rig
    }

    fn container_items(&self, p: IVec3, only: Option<Item>) -> i64 {
        let keep = |s: &Option<Stack>| s.filter(|s| only.is_none_or(|i| s.item == i)).map_or(0, |s| s.count as i64);
        if let Some(c) = self.world.chest(p) {
            return c.slots.iter().map(keep).sum();
        }
        if let Some(f) = self.world.furnace(p) {
            return keep(&f.input) + keep(&f.fuel) + keep(&f.output);
        }
        -1
    }

    pub fn read(&self, probe: Probe) -> i64 {
        match probe {
            Probe::Block(p) => self.get(p).map_or(-1, |b| b.0 as i64),
            Probe::Power(p) => self.world.redstone_power(abs(p)) as i64,
            Probe::Items(p) => self.container_items(abs(p), None),
            Probe::Count(p, item) => self.container_items(abs(p), Some(item)),
            Probe::Locate(block, y, z) => {
                (0..32).find(|&x| self.get(IVec3::new(x, y, z)) == Some(block)).map_or(-1, |x| x as i64)
            }
            Probe::CartX(i) => self.entities.minecarts.get(i).map_or(i64::MIN, |c| (c.pos.x * 1000.0).round() as i64),
            Probe::CartItems(i) => self
                .entities
                .minecarts
                .get(i)
                .map_or(-1, |c| c.slots.iter().map(|s| s.map_or(0, |s| s.count as i64)).sum()),
            Probe::LooseItems => self.entities.items.iter().map(|e| e.stack.count as i64).sum(),
            Probe::Moving => self.world.moving_piston_blocks(0.0).count() as i64,
            Probe::Scheduled(p) => {
                let p = abs(p);
                pending_ticks(&self.world)
                    .iter()
                    .find(|t| t[..3] == [p.x as i64, p.y as i64, p.z as i64])
                    .map_or(-1, |t| t[3])
            }
            Probe::Pending => {
                let mut ticks = pending_ticks(&self.world);
                ticks.sort_unstable();
                let mut h = std::hash::DefaultHasher::new();
                ticks.hash(&mut h);
                h.finish() as i64
            }
        }
    }

    /// Items in every container, furnace, cart and loose item, plus the
    /// counted blocks in the build's box (moving ones included).
    pub fn total(&self, counted: fn(Block) -> bool) -> i64 {
        let mut total = 0;
        for x in self.min.x..=self.max.x {
            for y in self.min.y..=self.max.y {
                for z in self.min.z..=self.max.z {
                    let p = IVec3::new(x, y, z);
                    total += self.container_items(p, None).max(0);
                    total += self.world.get_block(p).is_some_and(counted) as i64;
                }
            }
        }
        total += self.world.moving_piston_blocks(0.0).filter(|(b, _)| counted(*b)).count() as i64;
        total += self.read(Probe::LooseItems);
        total +=
            self.entities.minecarts.iter().flat_map(|c| c.slots).map(|s| s.map_or(0, |s| s.count as i64)).sum::<i64>();
        total
    }
}

/// Pending scheduled ticks as the `redstone` save records them: position,
/// remaining delay and priority.
pub fn pending_ticks(world: &World) -> Vec<[i64; 5]> {
    world
        .redstone_to_string()
        .split('|')
        .filter_map(|e| e.strip_prefix("t,"))
        .map(|e| {
            let f: Vec<i64> = e.split(',').map(|v| v.parse().unwrap()).collect();
            [f[0], f[1], f[2], f[3], f[4]]
        })
        .collect()
}

/// The dimension properties `Game::dimension_props` saves.
fn dimension_props(w: &World) -> BTreeMap<String, String> {
    let mut props = BTreeMap::new();
    for (k, v) in [
        ("redstone", w.redstone_to_string()),
        ("automation", w.automation_to_string()),
        ("falling", w.falling_to_string()),
        ("furnaces", w.furnaces_to_string()),
        ("brewing", w.brewing_stands_to_string()),
        ("chests", w.chests_to_string()),
        ("wood", w.wood_to_string()),
        ("beehives", w.beehives_to_string()),
        ("archaeology", w.archaeology_to_string()),
        ("jukeboxes", w.jukeboxes_to_string()),
        ("creaking_hearts", w.creaking_hearts_to_string()),
        ("sculk", w.sculk_to_string()),
        ("copper_golem_statues", w.copper_golem_statues_to_string()),
        ("trial", w.trial_to_string()),
        ("spawners", w.spawners_to_string()),
        ("utilities", w.utilities_to_string()),
        ("workstations", w.workstations_to_string()),
        ("beacons", w.beacons_to_string()),
        ("conduits", w.conduits_to_string()),
        ("potent_sulfur", w.potent_sulfur_to_string()),
    ] {
        props.insert(k.to_string(), v);
    }
    props
}

/// `Game::restore_dimension`, in its order.
fn restore_dimension(w: &mut World, props: &BTreeMap<String, String>) {
    let get = |k: &str| props.get(k).map(String::as_str);
    if let Some(v) = get("redstone") {
        w.load_redstone(v);
    }
    if let Some(v) = get("automation") {
        w.load_automation(v);
    }
    if let Some(v) = get("falling") {
        w.load_falling(v);
    }
    if let Some(v) = get("brewing") {
        w.load_brewing_stands(v);
    }
    if let Some(v) = get("furnaces") {
        w.load_furnaces(v);
    }
    if let Some(v) = get("chests") {
        w.load_chests(v);
    }
    if let Some(v) = get("archaeology") {
        w.load_archaeology(v);
    }
    if let Some(v) = get("jukeboxes") {
        w.load_jukeboxes(v);
    }
    if let Some(v) = get("beehives") {
        w.load_beehives(v);
    }
    if let Some(v) = get("creaking_hearts") {
        w.load_creaking_hearts(v);
    }
    if let Some(v) = get("sculk") {
        w.load_sculk(v);
    }
    if let Some(v) = get("copper_golem_statues") {
        w.load_copper_golem_statues(v);
    }
    if let Some(v) = get("trial") {
        w.load_trial(v);
    }
    if let Some(v) = get("wood") {
        w.load_wood(v);
    }
    if let Some(v) = get("spawners") {
        w.load_spawners(v);
    }
    if let Some(v) = get("utilities") {
        w.load_utilities(v);
    }
    if let Some(v) = get("workstations") {
        w.load_workstations(v);
    }
    if let Some(v) = get("beacons") {
        w.load_beacons(v);
    }
    if let Some(v) = get("conduits") {
        w.load_conduits(v);
    }
    if let Some(v) = get("potent_sulfur") {
        w.load_potent_sulfur(v);
    }
}

/// Where a run is interrupted.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Cut {
    None,
    /// Unload every column before machine tick `at`, for [`UNLOAD_GAP`] game ticks.
    Unload(u64),
    /// Save, drop and reload the world from disk before machine tick `at`.
    Restart(u64),
}

/// A run's per-tick record: `lines[i]` is every probe after machine tick `i + 1`.
pub struct Run {
    pub cut: Cut,
    pub lines: Vec<Vec<i64>>,
    pub totals: Vec<i64>,
    pub moving: Vec<bool>,
    pub pending: Vec<bool>,
}

impl Run {
    /// One probe's values, indexed by machine tick - 1.
    pub fn column(&self, probe: usize) -> Vec<i64> {
        self.lines.iter().map(|l| l[probe]).collect()
    }
}

pub fn run(spec: &Spec, cut: Cut) -> Run {
    let mut rig = Rig::new(spec);
    let mut out = Run { cut, lines: Vec::new(), totals: Vec::new(), moving: Vec::new(), pending: Vec::new() };
    for m in 1..=spec.ticks {
        match cut {
            Cut::Unload(at) if at == m => rig.unload_for(UNLOAD_GAP),
            Cut::Restart(at) if at == m => rig = rig.restart(spec.name),
            _ => {}
        }
        (spec.script)(&mut rig, m);
        rig.game_tick();
        rig.tick = m;
        out.lines.push(spec.probes.iter().map(|&p| rig.read(p)).collect());
        out.totals.push(rig.total(spec.counted));
        out.moving.push(rig.world.moving_piston_blocks(0.0).next().is_some());
        out.pending.push(!pending_ticks(&rig.world).is_empty());
    }
    // `CONTRAPTION_TRACE=<build name>` prints every run's trace, a line per tick.
    if std::env::var("CONTRAPTION_TRACE").is_ok_and(|n| n == spec.name) {
        eprintln!("{} {cut:?}: probes {:?}", spec.name, spec.probes);
        for (i, (line, total)) in out.lines.iter().zip(&out.totals).enumerate() {
            eprintln!("{:4} {line:?} total {total}", i + 1);
        }
    }
    out
}

/// Runs the reference and the three interruptions. `verify` checks a run
/// against the build's documented Java behaviour; deterministic builds
/// must also repeat the reference trace exactly after each interruption
/// (Java pauses an unloaded chunk's ticks, block entities and entities,
/// and a save keeps all of them), and closed builds keep their total.
pub fn check(spec: &Spec, verify: fn(&Run) -> Result<(), String>) {
    exercise(spec, verify, "differs from Java");
}

/// The persistence half of [`check`] alone, for builds whose Java timing
/// waits on another track: interrupted runs repeat the uninterrupted trace,
/// closed builds keep their total, and `invariants` (accounting that holds
/// whatever the timing, or nothing) holds on every run.
pub fn persists(spec: &Spec, invariants: fn(&Run) -> Result<(), String>) {
    exercise(spec, invariants, "breaks an invariant");
}

pub fn no_invariants(_: &Run) -> Result<(), String> {
    Ok(())
}

fn exercise(spec: &Spec, verify: fn(&Run) -> Result<(), String>, what: &str) {
    let name = spec.name;
    let reference = run(spec, Cut::None);
    if let Err(e) = verify(&reference) {
        panic!("{name}: the uninterrupted run {what}: {e}");
    }
    conserved(spec, &reference);
    let motion = (spec.motion_after as usize..reference.moving.len())
        .find(|&i| reference.moving[i])
        .or_else(|| (spec.motion_after as usize..reference.pending.len()).find(|&i| reference.pending[i]))
        .map(|i| i as u64 + 2);
    let mut cuts = vec![Cut::Unload(spec.unload_at), Cut::Restart(spec.restart_at)];
    cuts.extend(motion.filter(|&m| m <= spec.ticks).map(Cut::Restart));
    for cut in cuts {
        let r = run(spec, cut);
        if let Err(e) = verify(&r) {
            panic!("{name} after {cut:?} {what}: {e}");
        }
        if spec.deterministic {
            compare(spec, &reference, &r);
        }
        conserved(spec, &r);
    }
}

/// A closed build keeps its items and counted blocks.
fn conserved(spec: &Spec, r: &Run) {
    if !spec.closed {
        return;
    }
    let first = r.totals[0];
    if let Some(i) = r.totals.iter().position(|&t| t != first) {
        panic!("{} ({:?}): items/blocks {first} became {} after machine tick {}", spec.name, r.cut, r.totals[i], i + 1);
    }
}

/// The interrupted trace must equal the reference tick for tick.
pub fn compare(spec: &Spec, reference: &Run, r: &Run) {
    for (i, (a, b)) in reference.lines.iter().zip(&r.lines).enumerate() {
        if a != b {
            let probes: Vec<String> = spec
                .probes
                .iter()
                .zip(a.iter().zip(b))
                .filter(|(_, (x, y))| x != y)
                .map(|(p, (x, y))| format!("{p:?}: expected {x}, got {y}"))
                .collect();
            panic!(
                "{} after {:?}: machine tick {} differs from the uninterrupted run: {}",
                spec.name,
                r.cut,
                i + 1,
                probes.join("; ")
            );
        }
    }
    if reference.totals != r.totals {
        let i = reference.totals.iter().zip(&r.totals).position(|(a, b)| a != b).unwrap();
        panic!(
            "{} after {:?}: item/block total differs at machine tick {}: expected {}, got {}",
            spec.name,
            r.cut,
            i + 1,
            reference.totals[i],
            r.totals[i]
        );
    }
}

/// Machine ticks (1-based) at which a boolean column changes, with the new value.
pub fn edges(values: &[bool]) -> Vec<(u64, bool)> {
    values.windows(2).enumerate().filter(|(_, w)| w[0] != w[1]).map(|(i, w)| (i as u64 + 2, w[1])).collect()
}

/// Machine ticks (1-based) at which a boolean column turns on.
pub fn rises(values: &[bool]) -> Vec<u64> {
    edges(values).into_iter().filter(|e| e.1).map(|e| e.0).collect()
}

/// Machine ticks (1-based) at which a boolean column turns off.
pub fn falls(values: &[bool]) -> Vec<u64> {
    edges(values).into_iter().filter(|e| !e.1).map(|e| e.0).collect()
}

pub fn ensure(ok: bool, what: impl FnOnce() -> String) -> Result<(), String> {
    if ok { Ok(()) } else { Err(what()) }
}
