//! v0.17 Track D2 (AUT-05): a deterministic, seeded fuzz of storage and
//! vehicle transactions. Random operation sequences run through the code
//! the game uses: the storage screen layer (`session::storage`, which the
//! desktop host, controller seats and LAN peers reach through
//! `app::screen::Screen`) for clicks, shift-clicks, number-key swaps,
//! throws, cursor tosses and drops; the CLI and headless LAN `chest` and
//! `furnace` commands; player pickups; hoppers, droppers and dispensers;
//! chest and hopper carts; survival mining (spilling, and shulker boxes
//! keeping their contents); explosions; and a chunk unload or a full save
//! and reload in the middle of a sequence.
//!
//! After every step it checks that each item identity (item plus every
//! component, with bundle and shulker box contents counted item by item)
//! is conserved, that no stack exceeds its limit or sits in a slot that
//! refuses it, and that a comparator beside each station reads Java's
//! `AbstractContainerMenu.getRedstoneSignalFromContainer` for it once the
//! contents have been still for a few ticks.
//!
//! The default run is two seeds for CI. Set `VOXELCRAFT_STORAGE_FUZZ_SEEDS`
//! (and `VOXELCRAFT_STORAGE_FUZZ_STEPS`) for a longer one, or run the
//! ignored `storage_transactions_long` test (40 seeds of 600 steps from seed
//! 100; `VOXELCRAFT_STORAGE_FUZZ_FIRST` moves the first seed).
//!
//! New storage joins the fuzz as one entry in [`KINDS`].

use std::collections::BTreeMap;
use std::sync::Arc;

use glam::{DVec3, IVec3};
use voxelcraft::agent::{Agent, Command};
use voxelcraft::content::{bundle, shulker_box as boxes};
use voxelcraft::enchant::{Enchantment, Enchants};
use voxelcraft::entity::minecart::{CartKind, Minecart};
use voxelcraft::entity::{Ctx, Entities, PlayerId, Target};
use voxelcraft::inventory::components::{self, Payload};
use voxelcraft::inventory::{SLOTS, Stack, stack_to_string};
use voxelcraft::item::{ArmorMaterial, ArmorPiece, Item, Tier, ToolKind};
use voxelcraft::potion::Potion;
use voxelcraft::rules::{GameMode, GameRules};
use voxelcraft::session::storage::{Storage, StorageScreen, StorageSlot};
use voxelcraft::simulation::{self, TICK_SECONDS};
use voxelcraft::world::block::{Block, Facing};
use voxelcraft::world::brewing::{self, BrewingStand};
use voxelcraft::world::chest::Chest;
use voxelcraft::world::chunk::ChunkData;
use voxelcraft::world::double_chest::{self as dc, ChestState, ChestType};
use voxelcraft::world::furnace::{self, Furnace, FurnaceKind};
use voxelcraft::world::redstone_blocks as r;
use voxelcraft::world::storage::{LevelInfo, Storage as Disk};
use voxelcraft::world::terrain::Generator;
use voxelcraft::world::{World, copper, rails, shulker_box, utilities};

/// The platform is one chunk: x and z 0..32, y 128..160.
const PLATFORM: IVec3 = IVec3::new(0, 4, 0);
/// Container row height, the floor below it and the basin under the floor.
const Y: i32 = 150;
const FLOOR: i32 = Y - 1;
const BASIN: i32 = Y - 3;
const HOME: DVec3 = DVec3::new(16.5, Y as f64, 28.5);
/// Eight chunks east: the platform's chunk unloads.
const FAR: DVec3 = DVec3::new(16.5 + 256.0, Y as f64, 28.5);
const SEED: u64 = 7;

// ---------------------------------------------------------------------------
// The storage kinds under test.

#[derive(Clone, Copy, Debug)]
enum Form {
    /// One storage block at the station.
    Block(fn() -> Block),
    /// Two chests forming one 54-slot container, the second half east.
    DoubleChest,
    /// A cart on a detector rail (read by the comparator), or on a plain
    /// rail under a feeding hopper.
    Cart(CartKind, bool),
    /// The player's own ender inventory (no block, no comparator).
    Ender,
}

#[derive(Clone, Copy, Debug)]
struct Kind {
    name: &'static str,
    form: Form,
    /// A chest-fed hopper above pushing in, and a hopper below draining
    /// into a sink chest.
    feed: bool,
    /// Also a chest-fed hopper on its west side pushing in (furnace fuel).
    side_feed: bool,
    /// Whether a comparator in front of it is checked.
    comparator: bool,
    /// Breaking it keeps the contents inside its item (shulker boxes).
    keeps_contents: bool,
    /// The miner's hotbar slot: 0 pickaxe, 1 axe.
    tool: usize,
}

const fn kind(name: &'static str, form: Form) -> Kind {
    Kind { name, form, feed: true, side_feed: false, comparator: true, keeps_contents: false, tool: 0 }
}

fn chest() -> Block {
    Block::CHEST
}
fn trapped() -> Block {
    utilities::TRAPPED_CHEST
}
fn barrel() -> Block {
    Block::BARREL
}
fn copper_chest() -> Block {
    dc::block(ChestState {
        kind: dc::Kind::Copper(copper::Stage::Unaffected),
        facing: Facing::South,
        ty: ChestType::Single,
        wet: false,
    })
}
fn shulker() -> Block {
    shulker_box::placed(shulker_box::item_state(None), IVec3::Y).unwrap()
}
fn furnace_block() -> Block {
    Block::FURNACE
}
fn smoker() -> Block {
    Block::SMOKER
}
fn blast_furnace() -> Block {
    Block::BLAST_FURNACE
}
fn brewing_stand() -> Block {
    Block::BREWING_STAND
}
/// A hopper pushing down into the drain hopper.
fn hopper() -> Block {
    r::hopper(4, false)
}
/// A dispenser shooting south, onto the floor in front of the station.
fn dispenser() -> Block {
    r::dispenser(0, false)
}
/// A dropper dropping down into the drain hopper.
fn dropper() -> Block {
    r::dispenser(5, true)
}

/// Every storage kind the fuzz drives. A new container (or one another
/// track adds, such as the crafter) joins with one entry here.
const KINDS: &[Kind] = &[
    Kind { tool: 1, ..kind("chest", Form::Block(chest)) },
    Kind { tool: 1, ..kind("double chest", Form::DoubleChest) },
    Kind { tool: 1, ..kind("trapped chest", Form::Block(trapped)) },
    Kind { tool: 1, ..kind("barrel", Form::Block(barrel)) },
    kind("copper chest", Form::Block(copper_chest)),
    Kind { keeps_contents: true, ..kind("shulker box", Form::Block(shulker)) },
    Kind { side_feed: true, ..kind("furnace", Form::Block(furnace_block)) },
    Kind { side_feed: true, ..kind("smoker", Form::Block(smoker)) },
    Kind { side_feed: true, ..kind("blast furnace", Form::Block(blast_furnace)) },
    Kind { side_feed: true, ..kind("brewing stand", Form::Block(brewing_stand)) },
    kind("hopper", Form::Block(hopper)),
    // Nothing above: it collects dropped items instead.
    Kind { feed: false, ..kind("collecting hopper", Form::Block(hopper)) },
    kind("dispenser", Form::Block(dispenser)),
    kind("dropper", Form::Block(dropper)),
    Kind { feed: false, ..kind("chest cart on a detector rail", Form::Cart(CartKind::Chest, true)) },
    Kind { feed: false, ..kind("hopper cart on a detector rail", Form::Cart(CartKind::Hopper, true)) },
    Kind { comparator: false, ..kind("chest cart under a hopper", Form::Cart(CartKind::Chest, false)) },
    Kind { comparator: false, ..kind("hopper cart under a hopper", Form::Cart(CartKind::Hopper, false)) },
    Kind { feed: false, comparator: false, ..kind("ender chest", Form::Ender) },
];

// ---------------------------------------------------------------------------
// The item pool: everything stacks differently, carries components, or
// has a slot rule, and nothing smelts or brews with anything else in it
// (checked in `pool_never_transforms`), so only blaze powder is ever used
// up (as brewing fuel, accounted for).

fn potion(id: &str) -> Potion {
    Potion::from_id(id).unwrap()
}

fn pool(rng: &mut Rng) -> Stack {
    let dirt = Item::from_block(Block::DIRT);
    let n = |rng: &mut Rng, max: u8| 1 + rng.below(max as u64) as u8;
    match rng.below(17) {
        0 => Stack::new(dirt, n(rng, 64)),
        1 => Stack::new(Item::DIAMOND, n(rng, 64)),
        2 => Stack::new(Item::ENDER_PEARL, n(rng, 16)),
        3 => Stack::new(Item::DIAMOND, n(rng, 64)).with_name("Loot").unwrap(),
        4 => Stack {
            damage: rng.below(200) as u16,
            enchants: Enchants::NONE.with(Enchantment::Sharpness, 3).with(Enchantment::Unbreaking, 2),
            repair_cost: 3,
            ..Stack::new(Item::tool(ToolKind::Sword, Tier::Diamond), 1)
        }
        .with_name("Edge")
        .unwrap(),
        5 => components::with_dyed_color(
            Stack::new(Item::armor(ArmorPiece::Chestplate, ArmorMaterial::Leather), 1),
            Some([rng.below(256) as u8, 40, 200]),
        ),
        6 => Stack::new(Item::potion(potion("swiftness")), 1),
        7 => Stack::lingering_potion(potion("swiftness")),
        8 => Stack::new(Item::potion(potion("healing")), 1).with_name("Medicine").unwrap(),
        9 => Stack::new(Item::GLASS_BOTTLE, n(rng, 64)),
        10 => Stack::new(Item::NETHER_WART, n(rng, 64)),
        11 => Stack::new(Item::SUGAR, n(rng, 64)),
        12 => Stack::new(Item::BLAZE_POWDER, n(rng, 8)),
        13 => Stack::new(Item::COAL, n(rng, 64)),
        14 => Stack::new(Item::STICK, n(rng, 64)),
        15 => Stack::new(bundle::BUNDLE, 1)
            .with_payload(Payload::Bundle(vec![Stack::new(Item::DIAMOND, 3), Stack::new(dirt, 5)])),
        _ => boxes::with_contents(
            Stack::new(boxes::item(None), 1),
            &[Some(Stack::new(Item::DIAMOND, 10)), None, Some(Stack::new(Item::ENDER_PEARL, 4))],
        ),
    }
}

/// The item kinds the ledger follows; others (chests, hoppers, rails,
/// comparators, cart items) come and go with the stations.
fn tracked(item: Item) -> bool {
    [
        Item::from_block(Block::DIRT),
        Item::DIAMOND,
        Item::ENDER_PEARL,
        Item::tool(ToolKind::Sword, Tier::Diamond),
        Item::armor(ArmorPiece::Chestplate, ArmorMaterial::Leather),
        Item::potion(potion("swiftness")),
        Item::potion(potion("healing")),
        Stack::lingering_potion(potion("swiftness")).item,
        Item::GLASS_BOTTLE,
        Item::NETHER_WART,
        Item::SUGAR,
        Item::BLAZE_POWDER,
        Item::COAL,
        Item::STICK,
        bundle::BUNDLE,
    ]
    .contains(&item)
        || boxes::is_shulker_box(item)
}

// ---------------------------------------------------------------------------
// Deterministic randomness.

struct Rng(u64);

impl Rng {
    fn next(&mut self) -> u64 {
        // SplitMix64.
        self.0 = self.0.wrapping_add(0x9e37_79b9_7f4a_7c15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
        z ^ (z >> 31)
    }
    fn below(&mut self, n: u64) -> u64 {
        self.next() % n.max(1)
    }
    fn chance(&mut self, percent: u64) -> bool {
        self.below(100) < percent
    }
}

// ---------------------------------------------------------------------------
// Java's comparator reading and the item ledger.

/// `AbstractContainerMenu.getRedstoneSignalFromContainer` with
/// `Mth.lerpDiscrete(f, 0, 15)`, in Java's float arithmetic.
fn java_signal(slots: &[Option<Stack>], size: usize) -> u8 {
    let mut total = 0.0f32;
    for s in slots.iter().flatten() {
        total += s.count as f32 / s.item.max_stack().min(99) as f32;
    }
    total /= size as f32;
    (total * 14.0).floor() as u8 + u8::from(total > 0.0)
}

/// The identity a stack is counted under: everything but its count, with
/// bundle and shulker box contents counted separately.
fn key(stack: &Stack) -> String {
    let container = stack.item == bundle::BUNDLE || boxes::is_shulker_box(stack.item);
    let bare = if container { Stack { payload: None, ..*stack } } else { *stack };
    stack_to_string(Some(Stack { count: 1, ..bare }))
}

/// Adds `stack` (and what it holds) to `ledger`.
fn count(ledger: &mut BTreeMap<String, i64>, stack: &Stack) {
    if tracked(stack.item) {
        *ledger.entry(key(stack)).or_default() += i64::from(stack.count);
    }
    for inner in stack.bundle_contents() {
        count(ledger, inner);
    }
    if boxes::is_shulker_box(stack.item) {
        for inner in boxes::contents(stack).iter().flatten() {
            count(ledger, inner);
        }
    }
}

// ---------------------------------------------------------------------------
// Stations and the harness.

struct Station {
    kind: Kind,
    /// The storage cell (the west half of a double chest, a cart's rail).
    at: IVec3,
    cart: Option<u32>,
    /// Ticks since the expected comparator reading last changed.
    still: u32,
    expected: Option<u8>,
}

impl Station {
    fn comparator(&self) -> IVec3 {
        self.at - IVec3::Z
    }
    /// The air cell the comparator powers.
    fn output(&self) -> IVec3 {
        self.at - IVec3::Z * 2
    }
    /// Where a player stands to use or break it (two cells south).
    fn front(&self) -> DVec3 {
        self.at.as_dvec3() + DVec3::new(0.5, 0.0, 2.5)
    }
}

#[derive(Clone, Copy, Debug)]
enum Op {
    Open(usize),
    Click(StorageSlot, bool),
    QuickMove(StorageSlot),
    Swap(StorageSlot, usize),
    Throw(StorageSlot, bool),
    TossCursor(bool),
    DropSlot(usize, bool),
    /// Stand above a collecting station looking down and toss one slot.
    TossOnto(usize, usize),
    Pickup,
    Cli(usize, bool, usize),
    Pulse(usize),
    Break(usize, bool),
    BreakCart(usize),
    Explode(usize),
    Rebuild,
    Wait(u32),
    Unload,
    Reload,
}

struct Harness {
    world: World,
    entities: Entities,
    player: Agent,
    miner: Agent,
    rules: GameRules,
    stations: Vec<Station>,
    open: Option<usize>,
    /// Item entities the harness picked up off the ground before a blast.
    bin: Vec<Stack>,
    expected: BTreeMap<String, i64>,
    rng: Rng,
    seed: u64,
    tick: u64,
    center: DVec3,
    log: Vec<String>,
    dir: std::path::PathBuf,
    /// Brewing stands load blaze powder as fuel; that one item is used up.
    burned_powder: i64,
    /// Per operation: how often it ran and how often it changed the ledger's
    /// placement of items (a fingerprint of where everything is).
    stats: BTreeMap<&'static str, (usize, usize)>,
    comparator_checks: usize,
    timings: BTreeMap<&'static str, std::time::Duration>,
    /// Unloads and reloads at random (the long run); otherwise one of them
    /// halfway through.
    random_reloads: bool,
}

/// Empty chunks around home and the far point, so nothing is generated.
fn air_chunks() -> impl Iterator<Item = (IVec3, Arc<ChunkData>)> {
    let air = Arc::new(ChunkData::Uniform(Block::AIR));
    let rows = voxelcraft::world::terrain::Dimension::Overworld.chunk_rows();
    (-4..=4).flat_map(move |z| {
        let air = air.clone();
        let rows = rows.clone();
        (-4..=12).flat_map(move |x| {
            let air = air.clone();
            rows.clone().map(move |y| (IVec3::new(x, y, z), air.clone()))
        })
    })
}

fn platform() -> ChunkData {
    let mut data = ChunkData::Uniform(Block::AIR);
    let local = |y: i32| (y - PLATFORM.y * 32) as usize;
    for z in 0..32 {
        for x in 0..32 {
            data.set(x, local(BASIN), z, Block::BEDROCK);
            data.set(x, local(FLOOR), z, Block::BEDROCK);
            if x == 0 || z == 0 || x == 31 || z == 31 {
                for y in [BASIN + 1, Y, Y + 1, Y + 2] {
                    data.set(x, local(y), z, Block::BEDROCK);
                }
            }
        }
    }
    data
}

/// Streams chunks until the player's surroundings are in: storage needs
/// the blocks, not finished lighting.
fn settle(world: &mut World, at: DVec3) {
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(60);
    let cell = at.floor().as_ivec3();
    loop {
        world.update(at);
        let around = [IVec3::ZERO, IVec3::X * 40, IVec3::NEG_X * 40, IVec3::Z * 40, IVec3::NEG_Z * 40];
        if world.pending_jobs() == 0 && around.iter().all(|&d| world.is_loaded(cell + d)) {
            return;
        }
        assert!(std::time::Instant::now() < deadline, "headless streaming did not finish");
        if world.pending_jobs() > 0 {
            std::thread::sleep(std::time::Duration::from_millis(1));
        }
    }
}

fn new_world(saved: Vec<(IVec3, Arc<ChunkData>)>) -> World {
    let mut chunks: Vec<(IVec3, Arc<ChunkData>)> = air_chunks().collect();
    chunks.extend(saved);
    let mut world = World::new_headless(Arc::new(Generator::new(SEED)), chunks.into_iter().collect(), 2);
    world.set_tile_drops(true);
    settle(&mut world, HOME);
    world
}

fn ctx(at: DVec3) -> Ctx {
    Ctx {
        spawn_light: voxelcraft::simulation::lighting::SpawnLight::DAY,
        players: vec![Target::new(PlayerId::HOST, at, false)],
        daylight: 1.0,
        raining: false,
        spawning: false,
        dimension: voxelcraft::world::terrain::Dimension::Overworld,
    }
}

/// Points `agent` from `from` at `target`, a cell or a point, until its
/// crosshair `hits`. Returns whether some aim did.
fn aim(agent: &mut Agent, from: DVec3, target: DVec3, hits: impl Fn(&Agent) -> bool) -> bool {
    agent.session.player.pos = from;
    let eye = agent.session.player.eye();
    for offset in [DVec3::ZERO, DVec3::new(0.0, 0.3, 0.0), DVec3::new(0.0, -0.3, 0.0), DVec3::new(0.3, 0.2, 0.0)] {
        let d = (target + offset - eye).normalize();
        agent.session.player.yaw = d.z.atan2(d.x) as f32;
        agent.session.player.pitch = d.y.asin() as f32;
        if hits(agent) {
            return true;
        }
    }
    false
}

impl Harness {
    fn new(seed: u64) -> Self {
        let world = new_world(vec![(PLATFORM, Arc::new(platform()))]);
        let mut player = Agent::new(HOME);
        player.set_mode(GameMode::Survival);
        let mut miner = Agent::new(HOME);
        miner.set_mode(GameMode::Survival);
        let tool = |kind| Stack {
            enchants: Enchants::NONE.with(Enchantment::Efficiency, 5).with(Enchantment::Unbreaking, 3),
            ..Stack::new(Item::tool(kind, Tier::Netherite), 1)
        };
        miner.session.inventory.slots[0] = Some(tool(ToolKind::Pickaxe));
        miner.session.inventory.slots[1] = Some(tool(ToolKind::Axe));
        let dir = std::env::temp_dir().join(format!(
            "voxelcraft-storage-fuzz-{}-{seed}-{}",
            std::process::id(),
            std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos()
        ));
        let mut h = Harness {
            world,
            entities: Entities::new(SEED),
            player,
            miner,
            rules: GameRules::default(),
            stations: Vec::new(),
            open: None,
            bin: Vec::new(),
            expected: BTreeMap::new(),
            rng: Rng(seed.wrapping_mul(0x2545_f491_4f6c_dd1d) ^ 0x5eed),
            seed,
            tick: 0,
            center: HOME,
            log: Vec::new(),
            dir,
            burned_powder: 0,
            stats: BTreeMap::new(),
            comparator_checks: 0,
            timings: BTreeMap::new(),
            random_reloads: false,
        };
        // Five stations a row, rows six cells apart.
        for (i, &kind) in KINDS.iter().enumerate() {
            let at = IVec3::new(3 + (i % 5) as i32 * 6, Y, 4 + (i / 5) as i32 * 6);
            h.stations.push(Station { kind, at, cart: None, still: 0, expected: None });
        }
        h.rebuild(false);
        for _ in 0..4 {
            h.step_world();
        }
        h.fill();
        h.expected = h.ledger();
        h
    }

    // -- building --------------------------------------------------------

    fn put(&mut self, p: IVec3, b: Block) {
        if self.world.get_block(p) == Some(Block::AIR) {
            self.world.set_block(p, b);
        }
    }

    /// Places whatever is missing at every station. `count` adds a new
    /// shulker box block to the expected ledger.
    fn rebuild(&mut self, count: bool) {
        for i in 0..self.stations.len() {
            let (kind, at) = (self.stations[i].kind, self.stations[i].at);
            if kind.comparator {
                self.put(self.stations[i].comparator(), r::comparator(Facing::North, false, false));
            }
            if kind.feed {
                let above = at + IVec3::Y;
                self.put(above, r::hopper(4, false));
                self.put(above + IVec3::Y, Block::CHEST);
                if !matches!(kind.form, Form::Cart(..)) {
                    let below = at - IVec3::Y;
                    if self.world.get_block(below) != Some(r::hopper(4, false)) {
                        self.world.set_block(below, r::hopper(4, false));
                    }
                    self.put(below - IVec3::Y, Block::CHEST);
                }
            }
            if kind.side_feed {
                let side = at - IVec3::X;
                self.put(side, r::hopper(2, false));
                self.put(side + IVec3::Y, Block::CHEST);
            }
            match kind.form {
                Form::Block(block) => {
                    let b = block();
                    if self.world.get_block(at) == Some(Block::AIR) {
                        self.world.set_block(at, b);
                        if count && shulker_box::owns(b.0) {
                            *self.expected.entry(key(&Stack::new(boxes::item(None), 1))).or_default() += 1;
                        }
                    }
                }
                Form::DoubleChest => {
                    // The half whose partner lies east is the left one.
                    let facing = Facing::ALL
                        .into_iter()
                        .find(|&f| dc::connected(f, ChestType::Left).offset() == IVec3::X)
                        .unwrap();
                    let half = |ty| dc::block(ChestState { kind: dc::Kind::Plain, facing, ty, wet: false });
                    if self.world.get_block(at) == Some(Block::AIR)
                        && self.world.get_block(at + IVec3::X) == Some(Block::AIR)
                    {
                        self.world.set_block(at, half(ChestType::Left));
                        self.world.set_block(at + IVec3::X, half(ChestType::Right));
                    } else if self.world.get_block(at).is_some_and(|b| dc::state(b).is_none())
                        || self.world.get_block(at + IVec3::X).is_some_and(|b| dc::state(b).is_none())
                    {
                        // One half survived as a single chest; leave it be.
                    }
                }
                Form::Cart(cart, detector) => {
                    self.put(at, if detector { rails::DETECTOR_RAIL } else { Block::RAIL });
                    let alive = self.stations[i].cart.is_some_and(|id| self.entities.cart(id).is_some());
                    if !alive {
                        let id = self.entities.spawn_cart(cart, at.as_dvec3() + DVec3::new(0.5, 0.0625, 0.5));
                        self.stations[i].cart = Some(id);
                    }
                }
                Form::Ender => {}
            }
        }
    }

    /// Random contents for every station and the player.
    fn fill(&mut self) {
        for i in 0..self.stations.len() {
            let Some(storage) = self.storage(i) else { continue };
            let mut screen = StorageScreen {
                session: &mut self.player.session,
                world: &mut self.world,
                entities: &mut self.entities,
                storage,
            };
            for slot in screen.slots() {
                if matches!(slot, StorageSlot::Inventory(_)) || !self.rng.chance(40) {
                    continue;
                }
                // Up to ten draws for something the slot takes.
                let mut stack = pool(&mut self.rng);
                for _ in 0..10 {
                    if screen.may_place(slot, &stack) {
                        break;
                    }
                    stack = pool(&mut self.rng);
                }
                // Placed through the cursor, so every slot rule applies.
                screen.session.inventory.cursor = Some(stack);
                screen.click(slot, false);
                screen.session.inventory.cursor = None;
            }
        }
        for slot in 0..SLOTS {
            if self.rng.chance(55) {
                self.player.session.inventory.slots[slot] = Some(pool(&mut self.rng));
            }
        }
        for slot in 0..27 {
            if self.rng.chance(30) {
                self.player.session.ender.slots[slot] = Some(pool(&mut self.rng));
            }
        }
    }

    fn storage(&self, i: usize) -> Option<Storage> {
        let s = &self.stations[i];
        match s.kind.form {
            Form::Block(_) | Form::DoubleChest => {
                let b = self.world.get_block(s.at)?;
                if self.world.chest(s.at).is_some() {
                    Some(Storage::Block(s.at))
                } else if self.world.furnace(s.at).is_some() {
                    Some(Storage::Furnace(s.at))
                } else if b == Block::BREWING_STAND && self.world.brewing_stand(s.at).is_some() {
                    Some(Storage::Brewing(s.at))
                } else {
                    None
                }
            }
            Form::Cart(..) => s.cart.filter(|&id| self.entities.cart(id).is_some()).map(Storage::Vehicle),
            Form::Ender => Some(Storage::Ender),
        }
    }

    fn screen(&mut self) -> Option<StorageScreen<'_>> {
        let storage = self.storage(self.open?)?;
        Some(StorageScreen {
            session: &mut self.player.session,
            world: &mut self.world,
            entities: &mut self.entities,
            storage,
        })
    }

    // -- simulation --------------------------------------------------------

    /// One game tick in `Game::tick`'s order: carts, world rules,
    /// automation entities, entity simulation, block drops, pickups.
    fn step_world(&mut self) {
        let stands: Vec<(IVec3, u8)> =
            self.stations.iter().filter_map(|s| self.world.brewing_stand(s.at).map(|b| (s.at, b.fuel_left))).collect();
        self.entities.tick_minecarts(&mut self.world, &[]);
        // Detector rails and plates, then the open screens' lids (trapped
        // chests power while open), as `Game::tick` does.
        let player = &self.player.session.player;
        self.world.redstone_contacts([(player.pos, player.collision_shape())], &self.entities);
        let open: Vec<IVec3> = match self.open.and_then(|i| self.storage(i)) {
            Some(Storage::Block(p)) => vec![p],
            _ => Vec::new(),
        };
        self.world.set_shulker_viewers(&open);
        self.world.set_chest_viewers(open);
        simulation::tick_world(&mut self.world, self.center);
        for (p, before) in stands {
            if before == 0 && self.world.brewing_stand(p).is_some_and(|b| b.fuel_left == brewing::FUEL_USES) {
                self.burned_powder += 1;
                *self.expected.entry(key(&Stack::new(Item::BLAZE_POWDER, 1))).or_default() -= 1;
            }
        }
        self.world.tick_automation_entities(&mut self.entities);
        self.entities.sync_chunk_entities(&mut self.world);
        let _ = self.entities.update(TICK_SECONDS, &self.world, &ctx(self.center));
        for (cell, stack) in std::mem::take(&mut self.world.drops) {
            self.entities.drop_from_block(stack, cell);
        }
        self.world.xp_drops.clear();
        self.entities.orbs.clear();
        for stack in self.player.session.inventory.take_spill() {
            voxelcraft::session::storage::toss(&self.player.session, &mut self.entities, stack);
        }
        if self.center == HOME {
            let at = self.player.session.player.pos;
            self.entities.pick_up_items(at, &mut self.player.session.inventory);
        }
        self.tick += 1;
        if self.center == HOME {
            self.check_comparators();
        }
    }

    // -- the checks -----------------------------------------------------------

    fn fail(&self, what: String) -> ! {
        let tail: Vec<&String> = self.log.iter().rev().take(40).collect::<Vec<_>>().into_iter().rev().collect();
        panic!(
            "seed {} tick {}: {what}\nlast operations:\n{}",
            self.seed,
            self.tick,
            tail.iter().map(|s| s.as_str()).collect::<Vec<_>>().join("\n")
        );
    }

    /// The comparator reading Java gives the station's storage now.
    fn java_reading(&self, s: &Station) -> Option<u8> {
        let p = s.at;
        match s.kind.form {
            Form::Ender => None,
            Form::Cart(_, true) => {
                let cart = self.entities.minecarts.iter().find(|c| c.pos.floor().as_ivec3() == p && c.slot_count() > 0);
                Some(cart.map_or(0, |c| java_signal(&c.slots[..c.slot_count()], c.slot_count())))
            }
            Form::Cart(..) => None,
            _ => {
                if self.world.chest(p).is_some() {
                    let block = self.world.get_block(p)?;
                    // `ChestBlock.isChestBlockedAt` over both halves.
                    if dc::state(block).is_some() {
                        let (first, second) = self.world.chest_halves(p);
                        let blocked = |q: IVec3| self.world.get_block(q + IVec3::Y).is_some_and(r::conductor);
                        if blocked(first) || second.is_some_and(blocked) {
                            return Some(0);
                        }
                    }
                    let n = self.world.container_slots(p);
                    let slots: Vec<Option<Stack>> = (0..n).map(|i| self.world.chest_slot(p, i)).collect();
                    Some(java_signal(&slots, n))
                } else if let Some(f) = self.world.furnace(p) {
                    Some(java_signal(&[f.input, f.fuel, f.output], 3))
                } else {
                    let b = self.world.brewing_stand(p)?;
                    Some(java_signal(&[b.bottles[0], b.bottles[1], b.bottles[2], b.ingredient, b.fuel], 5))
                }
            }
        }
    }

    /// Once a station's expected reading has held for four ticks, the
    /// comparator in front of it must show it.
    fn check_comparators(&mut self) {
        for i in 0..self.stations.len() {
            if !self.stations[i].kind.comparator {
                continue;
            }
            let expected = self.java_reading(&self.stations[i]);
            let s = &mut self.stations[i];
            if expected != s.expected {
                s.expected = expected;
                s.still = 0;
                continue;
            }
            s.still += 1;
            let (comparator, output, still, name) = (s.comparator(), s.output(), s.still, s.kind.name);
            let Some(expected) = expected else { continue };
            let present = matches!(
                self.world.get_block(comparator).and_then(r::component),
                Some(r::Component::Comparator { .. })
            );
            if !present {
                // A comparator placed by a rebuild needs its own four ticks.
                self.stations[i].still = 0;
                continue;
            }
            if still >= 4 {
                self.comparator_checks += 1;
                let shown = self.world.redstone_power(output);
                if shown != expected {
                    self.fail(format!("{name}: comparator shows {shown}, Java reads {expected}"));
                }
            }
        }
    }

    /// Every stack in the world, the player, the harness bin.
    fn ledger(&self) -> BTreeMap<String, i64> {
        let mut ledger = BTreeMap::new();
        let session = &self.player.session;
        let inv = &session.inventory;
        for s in inv.slots.iter().chain(&inv.armor).chain([&inv.offhand, &inv.cursor]).flatten() {
            count(&mut ledger, s);
        }
        for s in session.ender.slots.iter().flatten().chain(&self.bin) {
            count(&mut ledger, s);
        }
        for (p, text) in entries(&self.world.chests_to_string()) {
            let chest = Chest::deserialize(text).unwrap_or_else(|| self.fail(format!("unreadable chest at {p}")));
            for s in chest.slots.iter().flatten() {
                count(&mut ledger, s);
            }
            // A placed shulker box is its item.
            if let Some(state) = self.world.get_block(p).and_then(shulker_box::decode) {
                count(&mut ledger, &Stack::new(boxes::item(state.colour), 1));
            }
        }
        for (p, text) in entries(&self.world.furnaces_to_string()) {
            let f = Furnace::deserialize(text).unwrap_or_else(|| self.fail(format!("unreadable furnace at {p}")));
            for s in [f.input, f.fuel, f.output].iter().flatten() {
                count(&mut ledger, s);
            }
        }
        for (p, text) in entries(&self.world.brewing_stands_to_string()) {
            let b = BrewingStand::deserialize(text).unwrap_or_else(|| self.fail(format!("unreadable stand at {p}")));
            for s in b.bottles.iter().chain([&b.ingredient, &b.fuel]).flatten() {
                count(&mut ledger, s);
            }
        }
        let mut parsed = Entities::new(0);
        parsed.load_items(&self.entities.items_to_string());
        for item in &parsed.items {
            count(&mut ledger, &item.stack);
        }
        for text in self.entities.minecarts_to_string().split(';').filter(|t| !t.is_empty()) {
            let cart = Minecart::deserialize(text).unwrap_or_else(|| self.fail("unreadable cart".into()));
            for s in cart.slots.iter().flatten() {
                count(&mut ledger, s);
            }
        }
        ledger.retain(|_, n| *n != 0);
        ledger
    }

    /// Stack limits and slot rules everywhere.
    fn check_slots(&self) {
        let fail = |what: String| self.fail(what);
        let valid = |s: &Stack, place: &str| {
            if s.count == 0 || s.count > s.item.max_stack() {
                fail(format!("{place}: {} x{} exceeds its limit {}", s.display_name(), s.count, s.item.max_stack()));
            }
            for inner in s.bundle_contents() {
                if boxes::is_shulker_box(inner.item) {
                    fail(format!("{place}: a shulker box inside a bundle"));
                }
            }
        };
        let inv = &self.player.session.inventory;
        for s in inv.slots.iter().chain([&inv.cursor]).chain(&self.player.session.ender.slots).flatten() {
            valid(s, "player");
        }
        for (p, text) in entries(&self.world.chests_to_string()) {
            let chest = Chest::deserialize(text).unwrap();
            let block = self.world.get_block(p);
            let size = match block.and_then(r::component) {
                Some(r::Component::Hopper { .. }) => 5,
                Some(r::Component::Dispenser { .. }) => 9,
                _ => 27,
            };
            for (i, s) in chest.slots.iter().enumerate() {
                let Some(s) = s else { continue };
                let place = format!("container at {p} slot {i}");
                valid(s, &place);
                if i >= size {
                    fail(format!("{place}: past its {size} slots"));
                }
                if block.is_some_and(|b| shulker_box::owns(b.0)) && boxes::is_shulker_box(s.item) {
                    fail(format!("{place}: a shulker box inside a shulker box"));
                }
            }
        }
        for (p, text) in entries(&self.world.furnaces_to_string()) {
            let f = Furnace::deserialize(text).unwrap();
            for s in [f.input, f.fuel].iter().flatten() {
                valid(s, &format!("furnace at {p}"));
            }
            if let Some(s) = f.fuel
                && furnace::burn_time(s.item).is_none()
                && s.item != Item::BUCKET
            {
                fail(format!("furnace at {p}: {} in the fuel slot", s.display_name()));
            }
            if let Some(s) = f.output {
                fail(format!("furnace at {p}: {} in the output though nothing smelts", s.display_name()));
            }
        }
        for (p, text) in entries(&self.world.brewing_stands_to_string()) {
            let b = BrewingStand::deserialize(text).unwrap();
            for s in b.bottles.iter().flatten() {
                if s.count != 1 || !brewing::fits_bottle_slot(s.item) {
                    fail(format!("stand at {p}: {} x{} in a bottle slot", s.display_name(), s.count));
                }
            }
            if let Some(s) = b.ingredient {
                valid(&s, &format!("stand at {p}"));
                if !brewing::is_ingredient(s.item) {
                    fail(format!("stand at {p}: {} as an ingredient", s.display_name()));
                }
            }
            if let Some(s) = b.fuel
                && (s.item != Item::BLAZE_POWDER || s.count > s.item.max_stack())
            {
                fail(format!("stand at {p}: {} x{} as fuel", s.display_name(), s.count));
            }
        }
        for cart in &self.entities.minecarts {
            for (i, s) in cart.slots.iter().enumerate() {
                let Some(s) = s else { continue };
                valid(s, &format!("cart {} slot {i}", cart.id));
                if i >= cart.slot_count() {
                    fail(format!("cart {} slot {i}: past its {} slots", cart.id, cart.slot_count()));
                }
            }
        }
        for item in &self.entities.items {
            valid(&item.stack, "dropped item");
        }
    }

    fn check(&self) {
        self.check_slots();
        let ledger = self.ledger();
        if ledger != self.expected {
            let mut diff = Vec::new();
            for k in ledger.keys().chain(self.expected.keys()) {
                let (have, want) = (ledger.get(k).copied().unwrap_or(0), self.expected.get(k).copied().unwrap_or(0));
                if have != want && !diff.iter().any(|d: &String| d.contains(k.as_str())) {
                    diff.push(format!("  {k}: {have} (expected {want})"));
                }
            }
            self.fail(format!("items not conserved:\n{}", diff.join("\n")));
        }
    }

    // -- operations -------------------------------------------------------------

    /// A random station among those `pick` accepts (any station if none).
    fn station(&mut self, pick: impl Fn(&Kind) -> bool) -> usize {
        let fit: Vec<usize> = (0..self.stations.len()).filter(|&i| pick(&self.stations[i].kind)).collect();
        if fit.is_empty() {
            return self.rng.below(self.stations.len() as u64) as usize;
        }
        fit[self.rng.below(fit.len() as u64) as usize]
    }

    /// A slot of the open screen; with nothing held, usually a full one.
    fn slot(&mut self) -> StorageSlot {
        let prefer_full = self.rng.chance(75);
        let slots: Vec<StorageSlot> = match self.screen() {
            Some(s) => {
                let full: Vec<StorageSlot> = s.slots().into_iter().filter(|&x| s.get(x).is_some()).collect();
                if s.session.inventory.cursor.is_none() && !full.is_empty() && prefer_full { full } else { s.slots() }
            }
            None => (0..SLOTS).map(StorageSlot::Inventory).collect(),
        };
        slots[self.rng.below(slots.len() as u64) as usize]
    }

    fn choose(&mut self) -> Op {
        let blocks = |k: &Kind| matches!(k.form, Form::Block(_) | Form::DoubleChest);
        let roll = self.rng.below(100);
        match roll {
            0..=9 => Op::Open(self.station(|_| true)),
            10..=29 => Op::Click(self.slot(), self.rng.chance(30)),
            30..=44 => Op::QuickMove(self.slot()),
            45..=51 => Op::Swap(self.slot(), self.rng.below(9) as usize),
            52..=55 => Op::Throw(self.slot(), self.rng.chance(50)),
            56..=58 => Op::TossCursor(self.rng.chance(50)),
            59..=60 => Op::DropSlot(self.rng.below(SLOTS as u64) as usize, self.rng.chance(50)),
            61..=63 => Op::TossOnto(
                self.station(|k| k.name == "collecting hopper" || matches!(k.form, Form::Cart(CartKind::Hopper, true))),
                self.rng.below(9) as usize,
            ),
            64..=68 => Op::Pickup,
            69..=74 => Op::Cli(
                self.station(|k| !matches!(k.form, Form::Ender)),
                self.rng.chance(50),
                self.rng.below(27) as usize,
            ),
            75..=78 => Op::Pulse(self.station(|k| k.name == "dispenser" || k.name == "dropper" || k.name == "hopper")),
            79..=80 => Op::Break(self.station(blocks), self.rng.chance(80)),
            81 => Op::BreakCart(self.station(|k| matches!(k.form, Form::Cart(..)))),
            82 => Op::Explode(self.station(|_| true)),
            83..=86 => Op::Rebuild,
            98 if self.random_reloads => Op::Unload,
            99 if self.random_reloads => Op::Reload,
            _ => Op::Wait(1 + self.rng.below(20) as u32),
        }
    }

    fn apply(&mut self, op: Op) {
        match op {
            Op::Open(i) => {
                self.close();
                self.open = Some(i);
            }
            Op::Click(slot, right) => {
                if let Some(mut s) = self.screen() {
                    s.click(slot, right);
                } else if let StorageSlot::Inventory(i) = slot {
                    self.player.session.inventory.click(i, right);
                }
            }
            Op::QuickMove(slot) => {
                // The desktop only quick-moves with nothing held.
                if self.player.session.inventory.cursor.is_none()
                    && let Some(mut s) = self.screen()
                {
                    s.quick_move(slot);
                }
            }
            Op::Swap(slot, hotbar) => {
                if let Some(mut s) = self.screen() {
                    s.swap(slot, hotbar);
                } else if let StorageSlot::Inventory(i) = slot {
                    voxelcraft::session::storage::swap_in_inventory(&mut self.player.session.inventory, i, hotbar);
                }
            }
            Op::Throw(slot, whole) => {
                if let Some(mut s) = self.screen() {
                    s.throw(slot, whole);
                }
            }
            Op::TossCursor(one) => {
                voxelcraft::session::storage::toss_cursor(&mut self.player.session, &mut self.entities, one);
            }
            Op::DropSlot(slot, whole) => {
                voxelcraft::session::storage::drop_slot(&mut self.player.session, &mut self.entities, slot, whole);
            }
            Op::TossOnto(i, slot) => {
                let s = &self.stations[i];
                let collects =
                    matches!(s.kind.form, Form::Cart(CartKind::Hopper, true)) || s.kind.name == "collecting hopper";
                if collects {
                    let above = s.at.as_dvec3() + DVec3::new(0.5, 1.0, 0.5);
                    self.player.session.player.pos = above;
                    self.player.session.player.pitch = -1.55;
                    voxelcraft::session::storage::drop_slot(&mut self.player.session, &mut self.entities, slot, true);
                    self.player.session.player.pos = HOME;
                    self.player.session.player.pitch = 0.0;
                }
            }
            Op::Pickup => {
                let found = self.entities.items.iter().find(|e| e.pickup_delay <= 0.0).map(|e| e.pos);
                if let Some(at) = found {
                    self.player.session.player.pos = at;
                    self.entities.pick_up_items(at, &mut self.player.session.inventory);
                    self.player.session.player.pos = HOME;
                }
            }
            Op::Cli(i, take, slot) => self.cli(i, take, slot),
            Op::Pulse(i) => {
                let side = self.stations[i].at + IVec3::X;
                if self.world.get_block(side) == Some(Block::AIR)
                    && !matches!(self.stations[i].kind.form, Form::DoubleChest | Form::Ender | Form::Cart(..))
                {
                    self.world.set_block(side, r::REDSTONE_BLOCK);
                    self.step_world();
                    self.step_world();
                    self.world.set_block(side, Block::AIR);
                }
            }
            Op::Break(i, drops) => self.break_station(i, drops),
            Op::BreakCart(i) => {
                if let Some(id) = self.stations[i].cart
                    && let Some(cart) = self.entities.cart(id)
                {
                    let target = cart.pos + DVec3::new(0.0, 0.35, 0.0);
                    let from = self.stations[i].front();
                    self.miner.session.player.pos = from;
                    let eye = self.miner.session.player.eye();
                    let dir = (target - eye).normalize();
                    for _ in 0..6 {
                        self.entities.hurt_cart(eye, dir, 6.0, false);
                    }
                }
            }
            Op::Explode(i) => {
                // Items already lying around would be blown up legitimately.
                for item in std::mem::take(&mut self.entities.items) {
                    self.bin.push(item.stack);
                }
                let center = self.stations[i].at.as_dvec3() + DVec3::splat(0.5);
                let cells = self.world.exploded_cells(center, 3.0);
                self.entities.explode_with(&self.world, center, 3.0, false, true);
                let drops = self.rules.bool("block_drops");
                self.world.detonate(&cells, 3.0, drops, false, false);
                self.world.primed_tnt.clear();
                for (cell, stack) in std::mem::take(&mut self.world.drops) {
                    self.entities.drop_from_block(stack, cell);
                }
            }
            Op::Rebuild => self.rebuild(true),
            Op::Wait(n) => {
                for _ in 0..n {
                    self.step_world();
                }
            }
            Op::Unload => {
                self.close();
                self.center = FAR;
                settle(&mut self.world, FAR);
                self.entities.sync_chunk_entities(&mut self.world);
                assert!(!self.world.is_loaded(self.stations[0].at), "the platform should unload");
                for _ in 0..5 {
                    self.step_world();
                }
                self.center = HOME;
                settle(&mut self.world, HOME);
                self.entities.sync_chunk_entities(&mut self.world);
            }
            Op::Reload => self.reload(),
        }
    }

    /// Closing a screen returns the held stack to the inventory, or drops it.
    fn close(&mut self) {
        self.player.session.inventory.return_stacks([]);
        self.open = None;
    }

    fn cli(&mut self, i: usize, take: bool, slot: usize) {
        let s = &self.stations[i];
        let (from, target) = (s.front(), s.at);
        let furnace = self.world.furnace(target).is_some();
        let cart = s.cart.filter(|&id| self.entities.cart(id).is_some());
        let entities = &self.entities;
        let world = &self.world;
        let aimed = match (s.kind.form, cart) {
            (Form::Ender, _) => return,
            (Form::Cart(..), Some(id)) => {
                let at = entities.cart(id).unwrap().pos + DVec3::new(0.0, 0.35, 0.0);
                aim(&mut self.player, from, at, |a| {
                    let eye = a.session.player.eye();
                    let dir = a.session.player.forward().as_dvec3();
                    let reach =
                        voxelcraft::entity::minecart::interaction_reach(world, eye, dir, a.session.entity_reach());
                    entities.vehicle_container(eye, dir, reach) == Some(id)
                })
            }
            (Form::Cart(..), None) => return,
            _ => aim(&mut self.player, from, target.as_dvec3() + DVec3::splat(0.5), |a| {
                a.session.block_target(world).is_some_and(|(p, _)| p == target)
            }),
        };
        if aimed {
            self.player.selected = self.rng.below(9) as usize;
            let command = if furnace { Command::Furnace(take, slot % 3) } else { Command::Chest(take, slot) };
            let _ = self.player.execute(command, &mut self.world, &mut self.entities, &[]);
        }
        self.player.session.player.pos = HOME;
        self.player.session.player.pitch = 0.0;
    }

    /// Survival mining with the right tool, `block_drops` on or off.
    fn break_station(&mut self, i: usize, drops: bool) {
        let s = &self.stations[i];
        if matches!(s.kind.form, Form::Ender | Form::Cart(..)) {
            return;
        }
        // A shulker box broken without drops is lost with what it holds,
        // as in Java (its contents ride on the loot table's drop).
        let drops = drops || s.kind.keeps_contents;
        let (from, target, tool) = (s.front(), s.at, s.kind.tool);
        if self.world.get_block(target).is_none_or(|b| b == Block::AIR) {
            return;
        }
        let world = &self.world;
        if !aim(&mut self.miner, from, target.as_dvec3() + DVec3::splat(0.5), |a| {
            a.session.block_target(world).is_some_and(|(p, _)| p == target)
        }) {
            return;
        }
        self.rules.set("block_drops", if drops { "true" } else { "false" }).unwrap();
        self.world.set_tile_drops(drops);
        self.miner.session.cancel_mining();
        for _ in 0..400 {
            match self.miner.session.tick_mining(tool, &mut self.world, &mut self.entities, &self.rules) {
                Ok(voxelcraft::session::mining::Outcome::Broken { .. }) | Err(_) => break,
                Ok(_) => {}
            }
            self.step_world();
        }
        self.rules.set("block_drops", "true").unwrap();
        self.world.set_tile_drops(true);
    }

    /// Saves to disk and loads into a new world, as quitting and reopening
    /// the game does (`Game::save` and `restore_dimension`).
    fn reload(&mut self) {
        self.close();
        for stack in self.player.session.inventory.take_spill() {
            voxelcraft::session::storage::toss(&self.player.session, &mut self.entities, stack);
        }
        let mut props = BTreeMap::new();
        props.insert("redstone".to_string(), self.world.redstone_to_string());
        props.insert("automation".to_string(), self.world.automation_to_string());
        props.insert("furnaces".to_string(), self.world.furnaces_to_string());
        props.insert("brewing".to_string(), self.world.brewing_stands_to_string());
        props.insert("chests".to_string(), self.world.chests_to_string());
        self.entities.save_props(&mut props);
        let disk = Disk::new(&self.dir);
        disk.save(&LevelInfo { seed: SEED, player: None, props }, &self.world.modified_chunks()).unwrap();
        let level = disk.load_level().unwrap();
        let chunks = disk.load_chunks().unwrap();
        let mut world = new_world(chunks.into_iter().collect());
        let props = &level.props;
        world.load_redstone(&props["redstone"]);
        world.load_automation(&props["automation"]);
        world.load_brewing_stands(&props["brewing"]);
        world.load_furnaces(&props["furnaces"]);
        world.load_chests(&props["chests"]);
        let mut entities = Entities::new(SEED);
        entities.load_props(props);
        self.world = world;
        self.entities = entities;
        self.entities.sync_chunk_entities(&mut self.world);
    }

    /// Where every stack is, to tell whether an operation did anything.
    fn fingerprint(&self) -> String {
        let inv = &self.player.session.inventory;
        format!(
            "{}{}{}{}{}{}{:?}{:?}",
            self.world.chests_to_string(),
            self.world.furnaces_to_string(),
            self.world.brewing_stands_to_string(),
            self.entities.minecarts_to_string(),
            self.entities.items.len(),
            inv.serialize(),
            inv.cursor,
            self.player.session.ender.slots,
        )
    }

    fn run(&mut self, steps: usize) {
        self.check();
        for step in 0..steps {
            let op = match step {
                _ if self.random_reloads => self.choose(),
                // Each costs a second or two of chunk loading: even seeds
                // unload the platform halfway, odd seeds save and reload.
                s if s == steps / 2 && self.seed.is_multiple_of(2) => Op::Unload,
                s if s == steps / 2 => Op::Reload,
                _ => self.choose(),
            };
            self.log.push(format!("step {step} tick {}: {op:?}", self.tick));
            let name = format!("{op:?}");
            let name: &'static str = OP_NAMES.iter().find(|n| name.starts_with(*n)).copied().unwrap_or("?");
            let before = self.fingerprint();
            let started = std::time::Instant::now();
            self.apply(op);
            *self.timings.entry(name).or_default() += started.elapsed();
            let stat = self.stats.entry(name).or_default();
            stat.0 += 1;
            if self.fingerprint() != before {
                self.stats.get_mut(name).unwrap().1 += 1;
            }
            for _ in 0..1 + self.rng.below(3) {
                self.step_world();
            }
            self.check();
        }
        // Let everything settle and check the comparators once more.
        for _ in 0..40 {
            self.step_world();
        }
        self.check();
        let _ = std::fs::remove_dir_all(&self.dir);
        eprintln!(
            "seed {}: {} ticks, {} comparator checks, {} blaze powder burned",
            self.seed, self.tick, self.comparator_checks, self.burned_powder
        );
        for (name, (ran, changed)) in &self.stats {
            eprintln!("  {name}: ran {ran}, changed something {changed}, {:?}", self.timings[name]);
        }
    }
}

const OP_NAMES: [&str; 18] = [
    "Open",
    "Click",
    "QuickMove",
    "Swap",
    "Throw",
    "TossCursor",
    "DropSlot",
    "TossOnto",
    "Pickup",
    "Cli",
    "Pulse",
    "BreakCart",
    "Break",
    "Explode",
    "Rebuild",
    "Wait",
    "Unload",
    "Reload",
];

/// `x,y,z=text` entries of a `|`-separated save string.
fn entries(text: &str) -> Vec<(IVec3, &str)> {
    text.split('|')
        .filter(|e| !e.is_empty())
        .filter_map(|e| {
            let (pos, rest) = e.split_once('=')?;
            let c: Vec<i32> = pos.split(',').filter_map(|v| v.parse().ok()).collect();
            (c.len() == 3).then(|| (IVec3::new(c[0], c[1], c[2]), rest))
        })
        .collect()
}

fn env(name: &str, default: usize) -> usize {
    std::env::var(name).ok().and_then(|v| v.parse().ok()).unwrap_or(default)
}

fn fuzz(seeds: std::ops::Range<u64>, steps: usize, random_reloads: bool) {
    for seed in seeds {
        let started = std::time::Instant::now();
        let mut h = Harness::new(seed);
        h.random_reloads = random_reloads;
        eprintln!("seed {seed}: set up in {:?}", started.elapsed());
        h.run(steps);
    }
}

#[test]
fn pool_never_transforms() {
    // Nothing in the pool smelts, and no bottle brews with any ingredient,
    // so the only items used up are blaze powder loaded as brewing fuel.
    let mut rng = Rng(1);
    let stacks: Vec<Stack> = (0..400).map(|_| pool(&mut rng)).collect();
    for s in &stacks {
        for kind in [FurnaceKind::Normal, FurnaceKind::Smoker, FurnaceKind::Blast] {
            assert!(kind.smelt(s.item).is_none(), "{} smelts", s.display_name());
        }
        for i in &stacks {
            assert!(brewing::brew_stack(*s, i.item).is_none(), "{} brews with {}", s.display_name(), i.display_name());
        }
    }
}

#[test]
fn storage_transactions_conserve_items_and_comparators() {
    let seeds = env("VOXELCRAFT_STORAGE_FUZZ_SEEDS", 2) as u64;
    fuzz(0..seeds, env("VOXELCRAFT_STORAGE_FUZZ_STEPS", 150), false);
}

#[test]
#[ignore = "long fuzz run; CI runs the short one"]
fn storage_transactions_long() {
    let first = env("VOXELCRAFT_STORAGE_FUZZ_FIRST", 100) as u64;
    let seeds = env("VOXELCRAFT_STORAGE_FUZZ_SEEDS", 40) as u64;
    fuzz(first..first + seeds, env("VOXELCRAFT_STORAGE_FUZZ_STEPS", 600), true);
}

// ---------------------------------------------------------------------------
// Minimal reproductions of what the fuzz found, and the Java menu facts.

fn fixture() -> serde_json::Value {
    let text = std::fs::read_to_string("tests/fixtures/java/26.3/storage_transactions.json").unwrap();
    serde_json::from_str(&text).unwrap()
}

fn bench() -> (World, Entities, voxelcraft::session::PlayerSession) {
    let world = new_world(vec![(PLATFORM, Arc::new(platform()))]);
    (world, Entities::new(SEED), voxelcraft::session::PlayerSession::new(PlayerId(1), HOME))
}

fn screen<'a>(
    world: &'a mut World,
    entities: &'a mut Entities,
    session: &'a mut voxelcraft::session::PlayerSession,
    storage: Storage,
) -> StorageScreen<'a> {
    StorageScreen { session, world, entities, storage }
}

#[test]
fn storage_menus_match_the_java_fixture() {
    let f = fixture();
    let moves = |menu: &str| -> Vec<(Option<u64>, Option<u64>, bool)> {
        f["quick_move"][menu]
            .as_array()
            .unwrap()
            .iter()
            .map(|m| (m[0].as_u64(), m[1].as_u64(), m[2].as_bool().unwrap()))
            .collect()
    };
    // Container slots go back to the player backwards: the hotbar from the
    // right first (`TO_PLAYER`); the furnace's input and fuel forwards,
    // the main grid from the top left first (`TO_PLAYER_FORWARDS`).
    for menu in ["chest", "shulker_box", "hopper", "dispenser", "brewing_stand"] {
        assert!(moves(menu)[0].2, "{menu} moves its slots to the player backwards");
    }
    assert_eq!(moves("furnace")[..2], [(Some(3), Some(39), true), (Some(3), Some(39), false)]);
    use voxelcraft::session::storage::{TO_PLAYER, TO_PLAYER_FORWARDS, slot_max};
    assert_eq!((TO_PLAYER[0], TO_PLAYER[8], TO_PLAYER[9], TO_PLAYER[35]), (8, 0, 35, 9));
    assert_eq!((TO_PLAYER_FORWARDS[0], TO_PLAYER_FORWARDS[26], TO_PLAYER_FORWARDS[27]), (9, 35, 0));
    // Blaze powder tries the fuel slot, and the ingredient slot only when
    // the fuel took none; ingredients and bottles go only to their slots.
    assert_eq!(
        moves("brewing_stand")[1..5],
        [(Some(4), Some(5), false), (Some(3), Some(4), false), (Some(3), Some(4), false), (Some(0), Some(3), false)]
    );
    let potion = Stack::new(Item::potion(potion("swiftness")), 1);
    assert_eq!(f["potion_slot_max"], u64::from(slot_max(StorageSlot::BrewBottle(0), &potion)));
    assert_eq!(
        f["furnace_fuel_bucket_max"],
        u64::from(slot_max(StorageSlot::FurnaceFuel, &Stack::new(Item::BUCKET, 1)))
    );
    assert_eq!(f["hopper_cooldown"], 8);
    // The fuzz's oracle (`java_signal`) uses Java's float arithmetic.
    assert_eq!(f["comparator_float"], true);
    // Each fix below follows one of these.
    assert_eq!(f["contents_drop_without_rules"], true);
    assert_eq!(f["brewing_fuel_load_sets_changed"], true);
    assert_eq!(f["furnace_input_compares_components"], true);
    assert_eq!(f["shulker_box_loot_conditions"], serde_json::json!([]));
    assert_eq!(f["chest_loot_conditions"], serde_json::json!(["minecraft:survives_explosion"]));
}

/// Breaking a chest, furnace or brewing stand with `block_drops` off spills
/// its contents: Java drops them through `Containers.dropContents`, which no
/// game rule gates. They used to vanish.
#[test]
fn contents_spill_with_block_drops_off() {
    let (mut w, _, _) = bench();
    let p = IVec3::new(6, Y, 26);
    w.set_tile_drops(false);
    let diamonds = Stack::new(Item::DIAMOND, 7);
    w.set_block(p, Block::CHEST);
    w.chest_mut(p).unwrap().slots[3] = Some(diamonds);
    w.set_block(p, Block::AIR);
    w.set_block(p, Block::FURNACE);
    w.furnace_mut(p).unwrap().fuel = Some(Stack::new(Item::COAL, 5));
    w.set_block(p, Block::AIR);
    w.set_block(p, Block::BREWING_STAND);
    w.brewing_stand_mut(p).unwrap().ingredient = Some(Stack::new(Item::NETHER_WART, 3));
    w.set_block(p, Block::AIR);
    let dropped: Vec<Stack> = w.drops.iter().map(|&(_, s)| s).collect();
    assert_eq!(dropped, [diamonds, Stack::new(Item::COAL, 5), Stack::new(Item::NETHER_WART, 3)]);
}

/// A brewing stand loading blaze powder into its fuel bar wakes the
/// comparators (`BrewingStandBlockEntity.serverTick` calls `setChanged`).
/// Before, a comparator that read the stand the tick before kept showing
/// the powder.
#[test]
fn loading_brewing_fuel_wakes_comparators() {
    let (mut w, mut e, mut s) = bench();
    let p = IVec3::new(9, Y, 26);
    let comparator = p - IVec3::Z;
    w.set_block(p, Block::BREWING_STAND);
    w.set_block(comparator, r::comparator(Facing::North, false, false));
    let stand = w.brewing_stand_mut(p).unwrap();
    stand.fuel_left = 1;
    stand.fuel = Some(Stack::new(Item::BLAZE_POWDER, 1));
    for _ in 0..10 {
        simulation::tick_world(&mut w, HOME);
    }
    assert_eq!(w.redstone_power(comparator - IVec3::Z), 1);
    // A water bottle, then four nether wart the next tick: the brew starts
    // and uses the last fuel, and the tick after the stand loads the powder.
    s.inventory.cursor = Some(Stack::new(Item::potion(potion("water")), 1));
    screen(&mut w, &mut e, &mut s, Storage::Brewing(p)).click(StorageSlot::BrewBottle(0), false);
    simulation::tick_world(&mut w, HOME);
    s.inventory.cursor = Some(Stack::new(Item::NETHER_WART, 4));
    screen(&mut w, &mut e, &mut s, Storage::Brewing(p)).click(StorageSlot::BrewIngredient, false);
    for _ in 0..10 {
        simulation::tick_world(&mut w, HOME);
    }
    let stand = *w.brewing_stand(p).unwrap();
    assert_eq!((stand.fuel, stand.fuel_left), (None, brewing::FUEL_USES));
    let java = java_signal(&[stand.bottles[0], stand.bottles[1], stand.bottles[2], stand.ingredient, stand.fuel], 5);
    assert_eq!(java, 3);
    assert_eq!(w.redstone_power(comparator - IVec3::Z), java);
}

/// A hopper topping up a furnace's input keeps the cook going
/// (`AbstractFurnaceBlockEntity.setItem` restarts it only for a different
/// item). Every insertion used to restart it, so a hopper-fed furnace never
/// finished an item while the hopper kept feeding it.
#[test]
fn hopper_fed_furnaces_keep_cooking() {
    let (mut w, mut e, _) = bench();
    let p = IVec3::new(12, Y, 26);
    w.set_block(p, Block::FURNACE);
    w.set_block(p + IVec3::Y, r::hopper(4, false));
    let f = w.furnace_mut(p).unwrap();
    f.input = Some(Stack::new(Item::RAW_IRON, 1));
    f.fuel = Some(Stack::new(Item::COAL, 8));
    w.chest_mut(p + IVec3::Y).unwrap().slots[0] = Some(Stack::new(Item::RAW_IRON, 40));
    for _ in 0..230 {
        simulation::tick_world(&mut w, HOME);
        w.tick_automation_entities(&mut e);
    }
    let f = w.furnace(p).unwrap();
    assert!(f.output.is_some_and(|s| s.item == Item::IRON_INGOT), "{:?}", f.output);
}

/// Bottle slots keep a potion's components (its name, a lingering potion's
/// contents), by click and by shift-click. They used to keep only the item.
#[test]
fn bottle_slots_keep_potion_components() {
    let (mut w, mut e, mut s) = bench();
    let p = IVec3::new(15, Y, 26);
    w.set_block(p, Block::BREWING_STAND);
    let named = Stack::new(Item::potion(potion("healing")), 1).with_name("Medicine").unwrap();
    let lingering = Stack::lingering_potion(potion("swiftness"));
    s.inventory.cursor = Some(named);
    screen(&mut w, &mut e, &mut s, Storage::Brewing(p)).click(StorageSlot::BrewBottle(0), false);
    s.inventory.slots[4] = Some(lingering);
    screen(&mut w, &mut e, &mut s, Storage::Brewing(p)).quick_move(StorageSlot::Inventory(4));
    let stand = w.brewing_stand(p).unwrap();
    assert_eq!(stand.bottles, [Some(named), Some(lingering), None]);
}

/// The storage screens follow Java's menus where they used to differ.
#[test]
fn storage_screens_follow_java_menus() {
    let (mut w, mut e, mut s) = bench();
    let (furnace_at, stand_at, chest_at) = (IVec3::new(18, Y, 26), IVec3::new(21, Y, 26), IVec3::new(24, Y, 26));
    w.set_block(furnace_at, Block::FURNACE);
    w.set_block(stand_at, Block::BREWING_STAND);
    w.set_block(chest_at, Block::CHEST);
    // `FurnaceResultSlot`: the right button takes half the output.
    w.furnace_mut(furnace_at).unwrap().output = Some(Stack::new(Item::IRON_INGOT, 5));
    screen(&mut w, &mut e, &mut s, Storage::Furnace(furnace_at)).click(StorageSlot::FurnaceOutput, true);
    assert_eq!(s.inventory.cursor, Some(Stack::new(Item::IRON_INGOT, 3)));
    assert_eq!(w.furnace(furnace_at).unwrap().output, Some(Stack::new(Item::IRON_INGOT, 2)));
    s.inventory.cursor = None;
    // `AbstractFurnaceMenu` moves fuel to the main grid first; the chest's
    // slots go to the hotbar from the right.
    w.furnace_mut(furnace_at).unwrap().fuel = Some(Stack::new(Item::COAL, 9));
    screen(&mut w, &mut e, &mut s, Storage::Furnace(furnace_at)).quick_move(StorageSlot::FurnaceFuel);
    assert_eq!(s.inventory.slots[9], Some(Stack::new(Item::COAL, 9)));
    w.chest_mut(chest_at).unwrap().slots[0] = Some(Stack::new(Item::DIAMOND, 2));
    screen(&mut w, &mut e, &mut s, Storage::Block(chest_at)).quick_move(StorageSlot::Container(0));
    assert_eq!(s.inventory.slots[8], Some(Stack::new(Item::DIAMOND, 2)));
    // `BrewingStandMenu`: blaze powder fills the fuel slot and stops there.
    w.brewing_stand_mut(stand_at).unwrap().fuel = Some(Stack::new(Item::BLAZE_POWDER, 60));
    s.inventory.slots[20] = Some(Stack::new(Item::BLAZE_POWDER, 10));
    screen(&mut w, &mut e, &mut s, Storage::Brewing(stand_at)).quick_move(StorageSlot::Inventory(20));
    assert_eq!(s.inventory.slots[20], Some(Stack::new(Item::BLAZE_POWDER, 6)));
    let stand = w.brewing_stand(stand_at).unwrap();
    assert_eq!((stand.fuel, stand.ingredient), (Some(Stack::new(Item::BLAZE_POWDER, 64)), None));
    // An ingredient the ingredient slot can't take stays where it is.
    w.brewing_stand_mut(stand_at).unwrap().ingredient = Some(Stack::new(Item::SUGAR, 1));
    s.inventory.slots[21] = Some(Stack::new(Item::NETHER_WART, 5));
    screen(&mut w, &mut e, &mut s, Storage::Brewing(stand_at)).quick_move(StorageSlot::Inventory(21));
    assert_eq!(s.inventory.slots[21], Some(Stack::new(Item::NETHER_WART, 5)));
    // A number key swaps a slot with the hotbar (`SWAP`); a stack bigger
    // than the slot allows leaves the rest on the hotbar.
    s.inventory.slots[2] = Some(Stack::new(Item::GLASS_BOTTLE, 3));
    screen(&mut w, &mut e, &mut s, Storage::Brewing(stand_at)).swap(StorageSlot::BrewBottle(1), 2);
    assert_eq!(w.brewing_stand(stand_at).unwrap().bottles[1], Some(Stack::new(Item::GLASS_BOTTLE, 1)));
    assert_eq!(s.inventory.slots[2], Some(Stack::new(Item::GLASS_BOTTLE, 2)));
    screen(&mut w, &mut e, &mut s, Storage::Block(chest_at)).swap(StorageSlot::Container(5), 8);
    assert_eq!(w.chest(chest_at).unwrap().slots[5], Some(Stack::new(Item::DIAMOND, 2)));
    assert_eq!(s.inventory.slots[8], None);
}
