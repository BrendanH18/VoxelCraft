//! Nether mobs beyond the original roster: piglins and their gold, the
//! piglin brutes guarding bastion remnants, hoglins and zoglins, and
//! striders.
//!
//! Each such mob carries a [`NetherMob`] with its gear, admired gold, pocket,
//! anger and zombification timer. Per-tick movement and attacks live in
//! `mob::nether_ai`; this module holds the parts that need the whole entity
//! list: periodic sensing of players, mobs and dropped items, picking items
//! up, bartering, anger broadcasts, mob-on-mob hits, zombification and
//! saving the persistent ones. See `docs/nether-mobs.md` for the Java
//! references.

use std::f32::consts::TAU;

use glam::{DVec3, IVec3};
use serde_json::{Value, json};

use super::item::{ItemEntity, PICKUP_DELAY};
use super::{Ctx, Entities, EntityEvent, Mob, MobKind, MobSound, MobWorld, PlayerId, Rng};
use crate::inventory::Stack;
use crate::item::{ArmorMaterial, ArmorPiece, Item, Tier};
use crate::world::block::Block;
use crate::world::terrain::Dimension;

/// Seconds a piglin admires gold before bartering (Java's 119 ticks).
pub const ADMIRE_TIME: f32 = 119.0 / 20.0;
/// Seconds an angered piglin holds a grudge (Java's 600 ticks).
pub const ANGER_TIME: f32 = 30.0;
/// Seconds outside the Nether before a piglin zombifies (Java's 300 ticks).
pub const ZOMBIFY_TIME: f32 = 15.0;
/// Piglins this close to a player opening a chest or breaking gold notice it
/// (Java's 16-block box), and broadcast anger to adults this close.
pub const ANGER_RANGE: f64 = 16.0;
/// After a player's hit, a piglin ignores gold for Java's 400 ticks.
const ADMIRE_DISABLED_TIME: f32 = 20.0;
/// Piglins walk to wanted items within Java's 9 blocks.
const ITEM_RANGE: f64 = 9.0;
/// Seconds before a piglin that ate a porkchop eats again (Java's 200 ticks).
const EAT_COOLDOWN: f32 = 10.0;
/// A hit baby piglin runs from its attacker for Java's 100 ticks.
const BABY_FLEE_TIME: f32 = 5.0;
/// Piglins keep Java's 6 blocks away from zombified piglins.
const AVOID_ZOMBIFIED: f64 = 6.0;
/// Seconds between a nether mob's looks around (Java's sensors run every
/// 20 ticks; this is a little quicker so thrown gold is noticed promptly).
const SENSE_INTERVAL: f32 = 0.5;
/// Chance of each piece of golden armor (Java's 0.1), of a baby (0.2) and
/// of a crossbow instead of a sword (0.5).
const ARMOR_CHANCE: f32 = 0.1;
const BABY_CHANCE: f32 = 0.2;
const CROSSBOW_CHANCE: f32 = 0.5;
/// Java's equipment drop chance (0.085, plus 0.01 per Looting level).
const EQUIPMENT_DROP: f32 = 0.085;
/// Bastion residents wander back once this far from home.
const HOME_RANGE: f64 = 12.0;
/// Seconds between checks for bastions that need their residents.
const RESIDENT_INTERVAL: f32 = 1.0;
/// Bastions within this many blocks of a player get their residents.
const RESIDENT_RANGE: i32 = 48;
/// Hoglins stay passive near repellents within Java's 8 blocks across and
/// 4 up or down, for 200 ticks, and look for them about once a second.
const REPEL_RANGE: (i32, i32) = (8, 4);
const PACIFY_TIME: f32 = 10.0;
const SCAN_INTERVAL: f32 = 1.0;
/// A hoglin holds a grudge for Java's 200-tick attack memory.
const HOGLIN_ANGER: f32 = 10.0;
/// Java's 5-20 s retreats.
const RETREAT: (f32, f32) = (5.0, 20.0);
/// Baby hoglins trail adults further than 5 blocks off (Java's 5-16).
const FOLLOW_ADULT: f64 = 5.0;
/// Most stacks a piglin's pocket holds (Java's inventory has 8 slots).
const POCKET_SLOTS: usize = 8;

/// What a nether mob holds in its main hand.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Weapon {
    None,
    GoldenSword,
    Crossbow,
    GoldenAxe,
}

impl Weapon {
    fn item(self) -> Option<Item> {
        match self {
            Weapon::None => None,
            Weapon::GoldenSword => Some(Item::tool(crate::item::ToolKind::Sword, Tier::Gold)),
            Weapon::Crossbow => Some(Item::CROSSBOW),
            Weapon::GoldenAxe => Some(Item::tool(crate::item::ToolKind::Axe, Tier::Gold)),
        }
    }

    fn name(self) -> &'static str {
        match self {
            Weapon::None => "none",
            Weapon::GoldenSword => "golden_sword",
            Weapon::Crossbow => "crossbow",
            Weapon::GoldenAxe => "golden_axe",
        }
    }

    fn from_name(name: &str) -> Self {
        [Weapon::GoldenSword, Weapon::Crossbow, Weapon::GoldenAxe]
            .into_iter()
            .find(|w| w.name() == name)
            .unwrap_or(Weapon::None)
    }
}

/// Who a nether mob is angry at.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Foe {
    Player(PlayerId),
    /// Another mob, by [`Mob::uid`].
    Mob(u32),
}

/// What a nether mob decided to do at its last look around.
#[derive(Clone, Copy, PartialEq, Debug)]
pub(super) enum Goal {
    None,
    /// Go after `foe`, last seen with its feet at `pos`.
    Attack {
        foe: Foe,
        pos: DVec3,
    },
    /// Keep away from this point.
    Flee(DVec3),
    /// Walk to a wanted item lying here.
    Fetch(DVec3),
    /// Head back toward this point (a brute's bastion).
    Walk(DVec3),
}

/// Per-mob state for piglins and brutes.
#[derive(Clone, Debug, PartialEq)]
pub struct NetherMob {
    pub weapon: Weapon,
    /// Gold held in the off hand while admiring it.
    pub offhand: Option<Stack>,
    pub admire_left: f32,
    /// Items picked up and kept; dropped on death.
    pub pocket: Vec<Stack>,
    /// Seconds spent outside the Nether.
    pub zombify: f32,
    /// Never zombifies (Java's `IsImmuneToZombification`).
    pub immune: bool,
    /// Bastion piglins don't hunt hoglins (Java's `CannotHunt`), and
    /// bastion hoglins can't be hunted (`CannotBeHunted`).
    pub cannot_hunt: bool,
    /// Where a bastion resident stays near (Java's brute `HOME` memory).
    pub home: Option<DVec3>,
    pub(super) foe: Option<Foe>,
    pub(super) foe_left: f32,
    pub(super) flee_from: Option<Foe>,
    pub(super) flee_left: f32,
    pub(super) goal: Goal,
    pub(super) sense_timer: f32,
    pub(super) admire_disabled: f32,
    pub(super) hunt_cooldown: f32,
    pub(super) ate: f32,
    /// Hoglins: seconds left passive after seeing a repellent and the
    /// nearest one seen; hoglins and striders: seconds to the next block scan.
    pub(super) pacified: f32,
    pub(super) repellent: Option<DVec3>,
    pub(super) scan_timer: f32,
    /// Striders: out of lava, shivering and slowed.
    pub(super) cold: bool,
    /// Seconds spent loading the crossbow toward the next shot.
    pub(super) charge: f32,
    /// Loading plus the once-per-shot random aiming delay, in seconds.
    pub(super) aim_at: f32,
    /// Admiring is over: barter (or keep the item) at the next upkeep.
    pub(super) barter: bool,
    /// Zombified: becomes its zombie form at the next upkeep.
    pub(super) convert: bool,
    /// This corpse's carried items have already been emitted by death loot.
    loot_dropped: bool,
}

impl NetherMob {
    /// Fresh state for the kinds that use it.
    pub fn for_kind(kind: MobKind) -> Option<Box<Self>> {
        matches!(kind, MobKind::Piglin | MobKind::PiglinBrute | MobKind::Hoglin | MobKind::Zoglin | MobKind::Strider)
            .then(|| {
                Box::new(Self {
                    weapon: Weapon::None,
                    offhand: None,
                    admire_left: 0.0,
                    pocket: Vec::new(),
                    zombify: 0.0,
                    immune: false,
                    cannot_hunt: false,
                    home: None,
                    foe: None,
                    foe_left: 0.0,
                    flee_from: None,
                    flee_left: 0.0,
                    goal: Goal::None,
                    sense_timer: 0.0,
                    admire_disabled: 0.0,
                    hunt_cooldown: 0.0,
                    ate: 0.0,
                    pacified: 0.0,
                    repellent: None,
                    scan_timer: 0.0,
                    cold: false,
                    charge: 0.0,
                    aim_at: 0.0,
                    barter: false,
                    convert: false,
                    loot_dropped: false,
                })
            })
    }

    /// Angry at `foe` for [`ANGER_TIME`].
    pub fn anger(&mut self, foe: Foe) {
        self.foe = Some(foe);
        self.foe_left = ANGER_TIME;
    }

    pub fn is_admiring(&self) -> bool {
        self.offhand.is_some()
    }

    /// Java's "idle" piglin, the only kind a chest or gold block upsets:
    /// not admiring, fighting or running away.
    fn idle(&self) -> bool {
        self.offhand.is_none() && self.foe.is_none() && self.flee_left <= 0.0
    }
}

/// One entry of Java's `gameplay/piglin_bartering` loot table.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Barter {
    Item(Item),
    /// A potion or splash potion by Java id.
    Potion(&'static str),
    Splash(&'static str),
    /// Java's item, which doesn't exist here: the roll gives nothing.
    Missing(&'static str),
}

/// Java 1.21's bartering table: (output, weight, min count, max count).
/// Iron boots come without Soul Speed, which isn't an enchantment here.
pub const BARTERING: [(Barter, u32, u8, u8); 18] = [
    (Barter::Missing("enchanted book (soul speed)"), 5, 1, 1),
    (Barter::Item(Item::armor(ArmorPiece::Boots, ArmorMaterial::Iron)), 8, 1, 1),
    (Barter::Potion("fire_resistance"), 8, 1, 1),
    (Barter::Splash("fire_resistance"), 8, 1, 1),
    (Barter::Potion("water"), 10, 1, 1),
    (Barter::Item(Item::IRON_NUGGET), 10, 10, 36),
    (Barter::Item(Item::ENDER_PEARL), 10, 2, 4),
    (Barter::Item(Item::STRING), 20, 3, 9),
    (Barter::Item(Item::NETHER_QUARTZ), 20, 5, 12),
    (Barter::Item(Item::from_block(Block::OBSIDIAN)), 40, 1, 1),
    (Barter::Item(Item::from_block(Block::CRYING_OBSIDIAN)), 40, 1, 3),
    (Barter::Item(Item::FIRE_CHARGE), 40, 1, 1),
    (Barter::Item(Item::LEATHER), 40, 2, 4),
    (Barter::Item(Item::from_block(Block::SOUL_SAND)), 40, 2, 8),
    (Barter::Item(Item::NETHER_BRICK), 40, 2, 8),
    (Barter::Missing("spectral arrow"), 40, 6, 12),
    (Barter::Item(Item::from_block(Block::GRAVEL)), 40, 8, 16),
    (Barter::Item(Item::from_block(Block::BLACKSTONE)), 40, 8, 16),
];

/// Sum of the bartering weights (Java's 459).
pub const BARTER_TOTAL: u32 = {
    let mut sum = 0;
    let mut i = 0;
    while i < BARTERING.len() {
        sum += BARTERING[i].1;
        i += 1;
    }
    sum
};

/// One bartering roll: Java's weighted pick, then a uniform count. `None`
/// for an entry whose item doesn't exist yet.
pub fn barter(rng: &mut Rng) -> Option<Stack> {
    let mut r = rng.next_int(BARTER_TOTAL);
    let &(output, _, lo, hi) = BARTERING
        .iter()
        .find(|e| {
            let hit = r < e.1;
            r -= e.1.min(r);
            hit
        })
        .unwrap_or(&BARTERING[0]);
    let count = lo + rng.next_int((hi - lo) as u32 + 1) as u8;
    let item = match output {
        Barter::Item(item) => item,
        Barter::Potion(id) => Item::potion(crate::potion::Potion::from_id(id)?),
        Barter::Splash(id) => Item::splash_potion(crate::potion::Potion::from_id(id)?),
        Barter::Missing(_) => return None,
    };
    Some(Stack::new(item, count))
}

/// The currency piglins barter for (Java's `BARTERING_ITEM`).
pub fn is_barter_currency(item: Item) -> bool {
    item == Item::GOLD_INGOT
}

/// Java's `piglin_loved` tag, for the items that exist.
pub fn is_loved(item: Item) -> bool {
    use std::sync::OnceLock;
    // Named lookups pick up gold items other branches add later.
    static NAMED: OnceLock<Vec<Item>> = OnceLock::new();
    let named = NAMED.get_or_init(|| {
        [
            "golden apple",
            "enchanted golden apple",
            "golden carrot",
            "bell",
            "light weighted pressure plate",
            "nether gold ore",
            "golden horse armor",
        ]
        .into_iter()
        .filter_map(Item::from_name)
        .collect()
    });
    is_barter_currency(item)
        || item.as_tool().is_some_and(|(_, tier)| tier == Tier::Gold)
        || item.as_armor().is_some_and(|(_, m)| m == ArmorMaterial::Gold)
        || matches!(item, Item::RAW_GOLD | Item::CLOCK | Item::GLISTERING_MELON_SLICE)
        || item.block().is_some_and(|b| {
            matches!(
                b,
                Block::GOLD_BLOCK
                    | Block::GILDED_BLACKSTONE
                    | Block::GOLD_ORE
                    | Block::DEEPSLATE_GOLD_ORE
                    | Block::RAW_GOLD_BLOCK
            )
        })
        || named.contains(&item)
}

/// Java's `piglin_food` tag.
fn is_food(item: Item) -> bool {
    matches!(item, Item::RAW_PORKCHOP | Item::COOKED_PORKCHOP)
}

/// Blocks in Java's `guarded_by_piglins` tag: opening or breaking one angers
/// idle piglins nearby.
pub fn guarded_by_piglins(block: Block) -> bool {
    matches!(block.base(), Block::CHEST | Block::BARREL)
        || matches!(
            block,
            Block::GOLD_BLOCK
                | Block::GILDED_BLACKSTONE
                | Block::GOLD_ORE
                | Block::DEEPSLATE_GOLD_ORE
                | Block::RAW_GOLD_BLOCK
        )
}

/// Java's `isWearingGold`: any golden armor piece keeps piglins calm.
pub fn wears_gold(armor: &[Option<Stack>; 4]) -> bool {
    armor.iter().flatten().any(|s| s.item.as_armor().is_some_and(|(_, m)| m == ArmorMaterial::Gold))
}

/// Piglins and brutes, which stand together (Java's `AbstractPiglin`).
fn is_piglin(kind: MobKind) -> bool {
    matches!(kind, MobKind::Piglin | MobKind::PiglinBrute)
}

/// Mobs piglins run from (Java's `isZombified`).
fn is_zombified(kind: MobKind) -> bool {
    matches!(kind, MobKind::ZombifiedPiglin | MobKind::Zoglin)
}

/// Whether a mob that hit someone was a piglin or brute.
fn is_piglin_kind(kind: Option<MobKind>) -> bool {
    kind.is_some_and(is_piglin)
}

/// Kinds whose babies drop no loot (Java's baby animals).
pub(super) fn babies_drop_nothing(kind: MobKind) -> bool {
    matches!(kind, MobKind::Hoglin | MobKind::Strider)
}

/// The item that breeds `kind` in Java (crimson fungus for hoglins). There
/// is no animal breeding yet: this is the hook for when there is.
pub fn breeding_item(kind: MobKind) -> Option<&'static str> {
    match kind {
        MobKind::Hoglin => Some("crimson fungus"),
        MobKind::Strider => Some("warped fungus"),
        _ => None,
    }
}

/// Blocks in Java's `hoglin_repellents` tag: the nether portal and warped
/// fungus, plus potted fungus and respawn anchors once they exist.
pub fn is_hoglin_repellent(block: Block) -> bool {
    use std::sync::OnceLock;
    static NAMED: OnceLock<Vec<Block>> = OnceLock::new();
    let named = NAMED.get_or_init(|| {
        ["warped fungus", "potted warped fungus", "respawn anchor"].into_iter().filter_map(Block::from_name).collect()
    });
    block == Block::NETHER_PORTAL || named.iter().any(|b| b.base() == block.base())
}

/// Nearest hoglin repellent around `cell`, scanning Java's box.
fn find_repellent<W: MobWorld + ?Sized>(world: &W, cell: IVec3) -> Option<DVec3> {
    let (h, v) = REPEL_RANGE;
    let mut best: Option<(i32, IVec3)> = None;
    for dy in -v..=v {
        for dz in -h..=h {
            for dx in -h..=h {
                let p = cell + IVec3::new(dx, dy, dz);
                let d = dx * dx + dy * dy + dz * dz;
                if best.is_none_or(|(bd, _)| d < bd) && world.block(p).is_some_and(is_hoglin_repellent) {
                    best = Some((d, p));
                }
            }
        }
    }
    best.map(|(_, p)| p.as_dvec3() + DVec3::splat(0.5))
}

/// Adult piglins and adult hoglins within 16 blocks of `pos` (Java's
/// visible-adult counts; sight lines aren't checked).
fn adult_counts(view: &[Seen], pos: DVec3) -> (usize, usize) {
    let near = view.iter().filter(|s| !s.baby && s.pos.distance_squared(pos) <= ANGER_RANGE * ANGER_RANGE);
    near.fold((0, 0), |(p, h), s| (p + is_piglin(s.kind) as usize, h + (s.kind == MobKind::Hoglin) as usize))
}

/// A living mob as nether mobs see it during one update.
#[derive(Clone, Copy, Debug)]
pub(super) struct Seen {
    pub uid: u32,
    pub kind: MobKind,
    pub pos: DVec3,
    pub baby: bool,
    /// A hoglin piglins may hunt.
    pub huntable: bool,
}

/// Gear a freshly spawned piglin rolls (Java's `finalizeSpawn`).
pub(super) fn on_spawn(mob: &mut Mob, rng: &mut Rng) {
    let Some(n) = mob.nether.as_mut() else { return };
    if mob.kind == MobKind::Hoglin {
        // Java: a fifth of spawned hoglins are babies.
        mob.baby = rng.chance(BABY_CHANCE);
        return;
    }
    if mob.kind == MobKind::PiglinBrute {
        n.weapon = Weapon::GoldenAxe;
        return;
    }
    if mob.kind != MobKind::Piglin {
        return;
    }
    if rng.chance(BABY_CHANCE) {
        mob.baby = true;
        return;
    }
    n.weapon = if rng.next_f32() < CROSSBOW_CHANCE { Weapon::Crossbow } else { Weapon::GoldenSword };
    for slot in &mut mob.armor {
        if rng.chance(ARMOR_CHANCE) {
            *slot = Some(super::armor::ArmorKind::Gold);
        }
    }
}

/// Experience for killing a nether mob (Java's piglins give 5, babies too).
pub(super) fn xp(kind: MobKind, baby: bool) -> Option<u32> {
    match kind {
        MobKind::Piglin => Some(5),
        MobKind::PiglinBrute => Some(20),
        MobKind::Hoglin if baby => Some(3),
        MobKind::Hoglin | MobKind::Zoglin => Some(5),
        // Baby animals give nothing; adults use the usual 1-3.
        MobKind::Strider if baby => Some(0),
        _ => None,
    }
}

impl Entities {
    /// Gives new mobs an identity, and lets nether mobs look around: who to
    /// fight or flee and which dropped items they want. Runs before the
    /// mobs move each update.
    pub(super) fn nether_sense<W: MobWorld + ?Sized>(&mut self, dt: f32, world: &W, ctx: &Ctx) {
        let mut any = false;
        for m in &mut self.mobs {
            if m.uid == 0 {
                self.next_uid = self.next_uid.max(1);
                m.uid = self.next_uid;
                self.next_uid = self.next_uid.wrapping_add(1).max(1);
            }
            any |= m.nether.is_some();
        }
        self.nether_view.clear();
        if !any {
            return;
        }
        self.nether_view.extend(self.mobs.iter().filter(|m| m.alive()).map(|m| Seen {
            uid: m.uid,
            kind: m.kind,
            pos: m.pos,
            baby: m.baby,
            huntable: m.kind == MobKind::Hoglin && m.nether.as_ref().is_some_and(|n| !n.cannot_hunt),
        }));
        let (mobs, view, items, rng) = (&mut self.mobs, &self.nether_view, &self.items, &mut self.rng);
        let allow_pickup = self.villager_griefing;
        let mut hunts = Vec::new();
        for m in mobs.iter_mut() {
            let alive = m.alive();
            let Some(n) = m.nether.as_mut() else { continue };
            n.sense_timer -= dt;
            if n.sense_timer > 0.0 || !alive {
                continue;
            }
            // Spread the looks so the work doesn't land on one tick.
            n.sense_timer = SENSE_INTERVAL * rng.range(0.8, 1.2);
            match m.kind {
                MobKind::Hoglin => sense_hoglin(m, view, world, ctx),
                MobKind::Zoglin => sense_zoglin(m, view, world, ctx),
                MobKind::Strider => sense_strider(m, world),
                _ => {
                    if let Some(prey) = sense(m, view, items, world, ctx, rng, allow_pickup) {
                        hunts.push((m.pos, m.uid, prey));
                    }
                }
            }
        }
        // Java's `StartHuntingHoglin`: the piglins around a hunter join in
        // and none of them hunts again for a while.
        for (pos, hunter, prey) in hunts {
            self.broadcast_anger(pos, Foe::Mob(prey), hunter);
            for m in &mut self.mobs {
                if is_piglin(m.kind)
                    && !m.baby
                    && m.pos.distance_squared(pos) <= ANGER_RANGE * ANGER_RANGE
                    && let Some(n) = m.nether.as_mut()
                {
                    n.hunt_cooldown = n.hunt_cooldown.max(self.rng.range(30.0, 120.0));
                }
            }
        }
    }

    /// After the mobs have moved: mob-on-mob hits, finished admiring,
    /// item pickups and zombification.
    pub(super) fn nether_upkeep(&mut self, ctx: &Ctx, events: &mut Vec<EntityEvent>) {
        let hits: Vec<_> = events
            .iter()
            .filter_map(|e| match *e {
                EntityEvent::MobHit { target, attacker, damage, knockback } => {
                    Some((target, attacker, damage, knockback))
                }
                _ => None,
            })
            .collect();
        if !hits.is_empty() {
            events.retain(|e| !matches!(e, EntityEvent::MobHit { .. }));
            for (target, attacker, damage, knockback) in hits {
                let Some(i) = self.mobs.iter().position(|m| m.uid == target && m.alive()) else { continue };
                let (kind, pos, burning) = (self.mobs[i].kind, self.mobs[i].pos, self.mobs[i].burning);
                let killed = self.mobs[i].damage(damage, Some(knockback), &mut self.rng);
                self.nether_hurt(i, Foe::Mob(attacker));
                if let Some(owner) = self.mobs[i].animal.as_ref().and_then(|a| a.owner) {
                    self.animals_defend_owner(attacker, owner);
                }
                let wolf_kill = self.mobs.iter().any(|m| {
                    m.uid == attacker && m.kind == MobKind::Wolf && m.animal.as_ref().is_some_and(|a| a.owner.is_some())
                });
                if killed {
                    self.drop_loot_with_fire(kind, pos, 0, burning, wolf_kill);
                }
            }
        }
        if self.nether_view.is_empty() {
            return;
        }
        let mut i = 0;
        while i < self.mobs.len() {
            let Some(n) = self.mobs[i].nether.as_mut() else {
                i += 1;
                continue;
            };
            if std::mem::take(&mut n.convert) {
                self.zombify(i, events);
            }
            if self.mobs[i].nether.as_ref().is_some_and(|n| n.barter) {
                self.finish_admiring(i, ctx);
            }
            if self.mobs[i].alive() && self.mobs[i].nether.as_ref().is_some_and(|n| matches!(n.goal, Goal::Fetch(_))) {
                self.pick_up(i, events);
            }
            i += 1;
        }
    }

    /// A player opened a container or broke a block piglins guard at `at`:
    /// idle piglins within 16 blocks turn on them (only those that can see
    /// it happen when `only_if_seen`, as for opening a chest).
    pub fn piglins_notice<W: MobWorld + ?Sized>(&mut self, player: PlayerId, at: DVec3, only_if_seen: bool, world: &W) {
        for m in &mut self.mobs {
            if m.kind != MobKind::Piglin || !m.alive() || (at - m.pos).abs().cmpgt(DVec3::splat(ANGER_RANGE)).any() {
                continue;
            }
            let eye = m.pos + DVec3::Y * (m.shape().height * 0.9);
            let Some(n) = m.nether.as_mut() else { continue };
            if n.idle() && (!only_if_seen || super::mob::line_of_sight(world, eye, at + DVec3::Y * 1.5)) {
                n.anger(Foe::Player(player));
                n.sense_timer = 0.0;
            }
        }
    }

    /// Java's `wasHurtBy` for the mob at `index`, hurt by `foe`: it drops
    /// what it was admiring, then a baby runs and alerts the adults while an
    /// adult fights back alongside the adults around it.
    pub(super) fn nether_hurt(&mut self, index: usize, foe: Foe) {
        let attacker = match foe {
            Foe::Mob(a) => self.nether_view.iter().find(|s| s.uid == a).map(|s| s.kind),
            Foe::Player(_) => None,
        };
        let Some(m) = self.mobs.get_mut(index) else { return };
        let (kind, pos, baby, uid, alive) = (m.kind, m.pos, m.baby, m.uid, m.alive());
        let Some(n) = m.nether.as_mut() else { return };
        if foe == Foe::Mob(uid) {
            return;
        }
        match kind {
            MobKind::Zoglin => {
                n.anger(foe);
                n.sense_timer = 0.0;
                return;
            }
            MobKind::Hoglin => {
                n.pacified = 0.0;
                n.sense_timer = 0.0;
                let (piglins, hoglins) = adult_counts(&self.nether_view, pos);
                if baby || is_piglin_kind(attacker) && piglins > hoglins {
                    // Java's `retreatFromNearestTarget`.
                    n.flee_from = Some(foe);
                    n.flee_left = self.rng.range(RETREAT.0, RETREAT.1);
                } else if attacker != Some(MobKind::Hoglin) {
                    n.anger(foe);
                    n.foe_left = HOGLIN_ANGER;
                    self.broadcast_hoglins(pos, foe, uid);
                }
                return;
            }
            k if !is_piglin(k) => return,
            _ => {}
        }
        if is_piglin_kind(attacker) {
            return;
        }
        // Outnumbered by the hoglin's herd, a piglin backs off instead.
        if attacker == Some(MobKind::Hoglin) && !baby {
            let (piglins, hoglins) = adult_counts(&self.nether_view, pos);
            if hoglins > piglins {
                n.flee_from = Some(foe);
                n.flee_left = self.rng.range(RETREAT.0, RETREAT.1);
                n.foe = None;
                return;
            }
        }
        // Interrupted admiring: no barter; anything but currency is kept.
        // A lethal hit leaves the offhand intact for the death-loot path.
        let interrupted = if alive { n.offhand.take() } else { None };
        let overflow = if let Some(stack) = interrupted {
            n.admire_left = 0.0;
            if !is_barter_currency(stack.item) { pocket(n, stack) } else { None }
        } else {
            None
        };
        if matches!(foe, Foe::Player(_)) {
            n.admire_disabled = ADMIRE_DISABLED_TIME;
        }
        if baby {
            n.flee_from = Some(foe);
            n.flee_left = BABY_FLEE_TIME;
        } else {
            n.anger(foe);
        }
        n.sense_timer = 0.0;
        if let Some(stack) = overflow {
            self.throw_from_mob(stack, pos, None);
        }
        self.broadcast_anger(pos, foe, uid);
    }

    /// Java's hoglin `broadcastAttackTarget`: adult hoglins around `pos`
    /// that aren't pacified or busy join the attack.
    fn broadcast_hoglins(&mut self, pos: DVec3, foe: Foe, except: u32) {
        let r2 = ANGER_RANGE * ANGER_RANGE;
        for m in &mut self.mobs {
            if m.kind != MobKind::Hoglin || m.baby || m.uid == except || !m.alive() {
                continue;
            }
            if m.pos.distance_squared(pos) <= r2
                && let Some(n) = m.nether.as_mut()
                && n.foe.is_none()
                && n.pacified <= 0.0
            {
                n.anger(foe);
                n.foe_left = HOGLIN_ANGER;
                n.sense_timer = 0.0;
            }
        }
    }

    /// Java's `broadcastAngerTarget`: adult piglins around `pos` that aren't
    /// already angry take up the fight.
    fn broadcast_anger(&mut self, pos: DVec3, foe: Foe, except: u32) {
        let r2 = ANGER_RANGE * ANGER_RANGE;
        for m in &mut self.mobs {
            if !is_piglin(m.kind) || m.baby || m.uid == except || !m.alive() {
                continue;
            }
            if m.pos.distance_squared(pos) <= r2
                && let Some(n) = m.nether.as_mut()
                && n.foe.is_none()
                && !n.is_admiring()
            {
                n.anger(foe);
                n.sense_timer = 0.0;
            }
        }
    }

    /// Admiring is over. Adults barter gold ingots away; other loved items
    /// (and babies' gold) are kept.
    fn finish_admiring(&mut self, index: usize, ctx: &Ctx) {
        let m = &mut self.mobs[index];
        let (pos, baby) = (m.pos, m.baby);
        let Some(n) = m.nether.as_mut() else { return };
        n.barter = false;
        let Some(stack) = n.offhand.take() else { return };
        if baby || !is_barter_currency(stack.item) {
            if let Some(overflow) = pocket(n, stack) {
                self.throw_from_mob(overflow, pos, None);
            }
            return;
        }
        // Java throws the loot toward the nearest player in view, otherwise
        // somewhere random.
        let toward = ctx
            .players
            .iter()
            .filter(|t| t.alive && t.pos.distance_squared(pos) < ANGER_RANGE * ANGER_RANGE)
            .min_by(|a, b| a.pos.distance_squared(pos).total_cmp(&b.pos.distance_squared(pos)))
            .map(|t| t.pos);
        if let Some(stack) = barter(&mut self.rng) {
            self.throw_from_mob(stack, pos, toward);
        }
    }

    /// Java's `BehaviorUtils.throwItem`: tossed from the mob's chest at 0.3
    /// blocks a tick toward `toward` (or a random direction).
    fn throw_from_mob(&mut self, stack: Stack, pos: DVec3, toward: Option<DVec3>) {
        let from = pos + DVec3::Y * 1.2;
        let dir = match toward {
            Some(t) => (t + DVec3::Y - from).normalize_or(DVec3::X),
            None => {
                let a = self.rng.range(0.0, TAU) as f64;
                DVec3::new(a.cos(), 0.3, a.sin()).normalize()
            }
        };
        let vel = dir * 6.0 + DVec3::Y * 1.0;
        self.items.push(ItemEntity::new(stack, from, vel, PICKUP_DELAY * 4.0, &mut self.rng));
    }

    /// A fetching piglin picks up the wanted item it reached.
    fn pick_up(&mut self, index: usize, events: &mut Vec<EntityEvent>) {
        // The shared flag follows the world's mobGriefing gamerule.
        if !self.villager_griefing {
            return;
        }
        let m = &self.mobs[index];
        let reach = m.shape().half_width + 1.0;
        let Some(j) = self.items.iter().position(|it| {
            let d = it.pos - m.pos;
            it.pickup_delay <= 0.0
                && d.x.abs() <= reach
                && d.z.abs() <= reach
                && (-0.5..m.shape().height + 0.5).contains(&d.y)
                && m.nether.as_ref().is_some_and(|n| wants(n, m.baby, it.stack.item))
        }) else {
            return;
        };
        let m = &mut self.mobs[index];
        let Some(n) = m.nether.as_mut() else { return };
        let item = self.items[j].stack.item;
        let take = if item == Item::GOLD_NUGGET { self.items[j].stack.count } else { 1 };
        let carried = Stack { count: take, ..self.items[j].stack };
        let mut overflow = None;
        if is_food(item) {
            n.ate = EAT_COOLDOWN;
        } else if is_loved(item) {
            n.offhand = Some(carried);
            n.admire_left = ADMIRE_TIME;
            events.push(EntityEvent::Sound { sound: MobSound::Ambient(m.kind), pos: m.pos + DVec3::Y * 1.6 });
        } else {
            overflow = pocket(n, carried);
        }
        n.goal = Goal::None;
        n.sense_timer = 0.0;
        // Java marks mobs that pick items up persistent.
        m.persistent = true;

        let pos = m.pos;
        let stack = &mut self.items[j].stack;
        if stack.count <= take {
            self.items.swap_remove(j);
        } else {
            stack.count -= take;
        }
        if let Some(stack) = overflow {
            self.throw_from_mob(stack, pos, None);
        }
    }

    /// Java's `convertTo`: a piglin outside the Nether becomes a zombified
    /// piglin in place, keeping its age, persistence and armor. Piglin
    /// inventory and an admired item are dropped before conversion.
    fn zombify(&mut self, index: usize, events: &mut Vec<EntityEvent>) {
        let old = &self.mobs[index];
        let into = match old.kind {
            MobKind::Piglin | MobKind::PiglinBrute => MobKind::ZombifiedPiglin,
            MobKind::Hoglin => MobKind::Zoglin,
            _ => return,
        };
        let carried: Vec<Stack> =
            old.nether.as_ref().map(|n| n.pocket.iter().copied().chain(n.offhand).collect()).unwrap_or_default();
        let pos = old.pos;
        let mut mob = Mob::new(into, old.pos, old.yaw);
        mob.baby = old.baby;
        mob.persistent = old.persistent;
        mob.uid = old.uid;
        mob.vel = old.vel;
        mob.previous_pos = old.previous_pos;
        mob.armor = old.armor;
        events.push(EntityEvent::Sound { sound: MobSound::Ambient(into), pos: old.pos + DVec3::Y * 1.6 });
        self.mobs[index] = mob;
        for stack in carried {
            self.items.push(ItemEntity::new(stack, pos + DVec3::Y * 0.5, DVec3::ZERO, PICKUP_DELAY, &mut self.rng));
        }
    }

    /// Equipment and pocket drops for a nether mob that died at `pos`:
    /// carried items always, worn gear 8.5% (+1% per Looting level) of the
    /// time on a player kill.
    pub(super) fn equipment_drops(&mut self, kind: MobKind, pos: DVec3, looting: u8, player_kill: bool) {
        let Some(m) = self.mobs.iter_mut().find(|m| {
            m.kind == kind
                && !m.alive()
                && m.pos.distance_squared(pos) < 0.01
                && m.nether.as_ref().is_some_and(|n| !n.loot_dropped)
        }) else {
            return;
        };
        let n = m.nether.as_mut().unwrap();
        n.loot_dropped = true;
        let mut out: Vec<Stack> = n.pocket.clone();
        out.extend(n.offhand);
        let worn: Vec<Item> = n
            .weapon
            .item()
            .into_iter()
            .chain(
                m.armor
                    .iter()
                    .zip(ArmorPiece::ALL)
                    .filter(|(a, _)| **a == Some(super::armor::ArmorKind::Gold))
                    .map(|(_, piece)| Item::armor(piece, ArmorMaterial::Gold)),
            )
            .collect();
        for item in worn {
            if player_kill && (self.rng.next_f32() - 0.01 * looting as f32) < EQUIPMENT_DROP {
                // Java damages dropped gear by a random amount.
                let mut stack = Stack::new(item, 1);
                if let Some(max) = item.durability() {
                    stack.damage = self.rng.next_int(max as u32 / 2 + 1) as u16 + max / 4;
                }
                out.push(stack);
            }
        }
        for stack in out {
            let vel = DVec3::new(self.rng.range(-1.5, 1.5) as f64, 4.0, self.rng.range(-1.5, 1.5) as f64);
            self.items.push(ItemEntity::new(stack, pos + DVec3::Y * 0.5, vel, PICKUP_DELAY, &mut self.rng));
        }
    }

    /// Persistent nether mobs, as JSON for the level file.
    pub fn nether_mobs_to_string(&self) -> String {
        let mobs: Vec<Value> =
            self.mobs.iter().filter(|m| m.persistent && m.alive() && m.villager.is_none()).map(save_mob).collect();
        let mut bastions: Vec<[i32; 3]> = self.bastions_populated.iter().map(|p| p.to_array()).collect();
        bastions.sort_unstable();
        let mut structures: Vec<_> = self.structures_populated.iter().map(|p| p.to_array()).collect();
        structures.sort_unstable();
        json!({ "mobs": mobs, "bastions": bastions, "structures": structures }).to_string()
    }

    /// Restores [`Entities::nether_mobs_to_string`], skipping bad entries.
    pub fn load_nether_mobs(&mut self, text: &str) {
        let Ok(root) = serde_json::from_str::<Value>(text) else { return };
        if let Some(done) = root["bastions"].as_array() {
            self.bastions_populated.extend(done.iter().filter_map(|p| {
                let p = p.as_array()?;
                let c = |i: usize| p.get(i)?.as_i64().and_then(|v| i32::try_from(v).ok());
                Some(IVec3::new(c(0)?, c(1)?, c(2)?))
            }));
        }
        if let Some(done) = root["structures"].as_array() {
            self.structures_populated.extend(done.iter().filter_map(|p| {
                let p = p.as_array()?;
                Some(IVec3::new(
                    i32::try_from(p.first()?.as_i64()?).ok()?,
                    i32::try_from(p.get(1)?.as_i64()?).ok()?,
                    i32::try_from(p.get(2)?.as_i64()?).ok()?,
                ))
            }));
        }
        let Some(mobs) = root["mobs"].as_array() else { return };
        self.mobs.extend(mobs.iter().filter_map(load_mob));
    }
}

impl Entities {
    /// Bastion remnants come with residents (Java places them as the
    /// structure generates): once a bastion near a player has loaded, its
    /// piglins and brutes spawn, persistent and staying near their piece.
    /// Populated bastions are remembered in the level file.
    pub(super) fn populate_bastions<W: MobWorld + ?Sized>(&mut self, dt: f32, world: &W, ctx: &Ctx) {
        if ctx.dimension != Dimension::Nether {
            return;
        }
        self.bastion_timer -= dt;
        if self.bastion_timer > 0.0 {
            return;
        }
        self.bastion_timer = RESIDENT_INTERVAL;
        for p in ctx.players.iter().map(|t| t.pos.floor().as_ivec3()) {
            for b in world.bastions_near(p, RESIDENT_RANGE) {
                let key = b.bounds.min;
                let (lo, hi) = (b.bounds.min, b.bounds.max);
                let mid = (lo + hi) / 2;
                let loaded = [lo, hi, mid, IVec3::new(lo.x, mid.y, hi.z), IVec3::new(hi.x, mid.y, lo.z)]
                    .into_iter()
                    .all(|c| world.loaded(c));
                if !loaded || !self.bastions_populated.insert(key) {
                    continue;
                }
                // Each bastion rolls its own residents, whoever finds it.
                let seed = crate::world::noise::hash3(key.x, key.y, key.z, world.seed() ^ 0xB1_6110);
                let mut rng = Rng::new(seed);
                for piece in &b.pieces {
                    for &kind in residents(piece.kind) {
                        self.place_resident(world, kind, &piece.bounds, &mut rng);
                    }
                }
            }
        }
    }

    /// Puts one resident on a free floor spot inside `bounds`.
    fn place_resident<W: MobWorld + ?Sized>(
        &mut self,
        world: &W,
        kind: Option<MobKind>,
        bounds: &crate::world::structure::Bounds,
        rng: &mut Rng,
    ) {
        let Some(kind) = kind else { return };
        for _ in 0..10 {
            let x = bounds.min.x + 1 + rng.next_int((bounds.max.x - bounds.min.x - 1).max(1) as u32) as i32;
            let z = bounds.min.z + 1 + rng.next_int((bounds.max.z - bounds.min.z - 1).max(1) as u32) as i32;
            let Some(pos) = floor_in(world, kind, x, z, bounds.min.y, bounds.max.y - 1) else { continue };
            let mut mob = Mob::new(kind, pos, rng.range(0.0, TAU));
            mob.persistent = true;
            if let Some(n) = mob.nether.as_mut() {
                n.home = Some(pos);
                // Java's bastion mob pools: melee 1, sword 4, crossbow 4.
                n.weapon = match kind {
                    MobKind::PiglinBrute => Weapon::GoldenAxe,
                    _ => match rng.next_int(9) {
                        0 => Weapon::None,
                        1..=4 => Weapon::GoldenSword,
                        _ => Weapon::Crossbow,
                    },
                };
                // Java's `initMemories`: no hunting for the first 30-120 s.
                n.hunt_cooldown = rng.range(30.0, 120.0);
                // Stable hoglins can't be hunted.
                n.cannot_hunt = kind == MobKind::Hoglin;
            }
            self.mobs.push(mob);
            return;
        }
    }
}

/// Residents of one bastion piece: Java fills its pieces from mob pools
/// with an empty option (`None`), roughly one or two piglins per room,
/// brutes guarding housing, towers, the treasure and the rampart, and
/// hoglins penned in the stables.
fn residents(kind: crate::world::bastion::PieceKind) -> &'static [Option<MobKind>] {
    use crate::world::bastion::PieceKind;
    const P: Option<MobKind> = Some(MobKind::Piglin);
    const B: Option<MobKind> = Some(MobKind::PiglinBrute);
    const H: Option<MobKind> = Some(MobKind::Hoglin);
    match kind {
        PieceKind::Housing => &[P, P, None, B],
        PieceKind::Court => &[P, P, None],
        PieceKind::Stable => &[P, P, None, H, H, H, H],
        PieceKind::Tower => &[P, B],
        PieceKind::Treasure => &[B, B, P, P],
        PieceKind::Bridge => &[P, None],
        PieceKind::Face => &[P, P, B],
    }
}

/// Feet position on a solid floor in column (x, z) between `lo` and `hi`,
/// out of lava and with room for `kind`.
fn floor_in<W: MobWorld + ?Sized>(world: &W, kind: MobKind, x: i32, z: i32, lo: i32, hi: i32) -> Option<DVec3> {
    let floor = (lo..=hi).rev().find(|&y| {
        world.block(IVec3::new(x, y, z)).is_some_and(|b| b.is_solid())
            && world.block(IVec3::new(x, y + 1, z)).is_some_and(|b| !b.is_solid() && !b.is_fluid())
    })?;
    let pos = DVec3::new(x as f64 + 0.5, floor as f64 + 1.0, z as f64 + 0.5);
    let shape = kind.shape();
    let clear = world.loaded(pos.floor().as_ivec3())
        && !crate::physics::overlaps_solid(world, pos, shape)
        && !crate::physics::touches_block(world, pos, shape, Block::is_fluid);
    clear.then_some(pos)
}

/// Adds a stack to a piglin's eight-slot inventory. Any remainder is
/// returned so the caller can throw it rather than losing existing items.
fn pocket(n: &mut NetherMob, mut stack: Stack) -> Option<Stack> {
    for slot in &mut n.pocket {
        if slot.stacks_with(&stack) {
            let moved = (slot.max() - slot.count).min(stack.count);
            slot.count += moved;
            stack.count -= moved;
            if stack.count == 0 {
                return None;
            }
        }
    }
    if n.pocket.len() < POCKET_SLOTS {
        n.pocket.push(stack);
        None
    } else {
        Some(stack)
    }
}

/// Java's `wantsToPickup` for a piglin.
fn wants(n: &NetherMob, baby: bool, item: Item) -> bool {
    if baby && item == Item::LEATHER {
        return false;
    }
    if n.admire_disabled > 0.0 && n.foe.is_some() {
        return false;
    }
    if is_barter_currency(item) || is_loved(item) {
        return n.offhand.is_none();
    }
    if item == Item::GOLD_NUGGET {
        return n.pocket.len() < POCKET_SLOTS;
    }
    is_food(item) && n.ate <= 0.0
}

/// One look around for the piglin or brute `m`: picks its goal. Returns the
/// adult hoglin it started hunting, if it did.
fn sense<W: MobWorld + ?Sized>(
    m: &mut Mob,
    view: &[Seen],
    items: &[ItemEntity],
    world: &W,
    ctx: &Ctx,
    rng: &mut Rng,
    allow_pickup: bool,
) -> Option<u32> {
    let (pos, baby, kind, uid) = (m.pos, m.baby, m.kind, m.uid);
    let brute = kind == MobKind::PiglinBrute;
    let eye = pos + DVec3::Y * (m.shape().height * 0.9);
    let n = m.nether.as_mut()?;
    let locate = |foe: Foe| locate(foe, view, ctx);
    // Grudges end when the foe is gone or far away.
    if let Some(foe) = n.foe
        && locate(foe).is_none_or(|p| p.distance_squared(pos) > 32.0 * 32.0)
    {
        n.foe = None;
    }
    if n.offhand.is_some() {
        n.goal = Goal::None;
        return None;
    }
    // Gold first: a piglin sees gold even mid-fight unless a player's hit
    // put it off gold for a while. Brutes don't care for gold.
    let fetch = items
        .iter()
        .filter(|_| !brute && allow_pickup)
        .filter(|it| {
            let d = it.pos - pos;
            d.x * d.x + d.z * d.z <= ITEM_RANGE * ITEM_RANGE && d.y.abs() <= 4.0 && wants(n, baby, it.stack.item)
        })
        .filter(|it| n.foe.is_none() || is_barter_currency(it.stack.item) || is_loved(it.stack.item))
        .min_by(|a, b| a.pos.distance_squared(pos).total_cmp(&b.pos.distance_squared(pos)))
        .map(|it| it.pos);
    if let Some(at) = fetch {
        n.goal = Goal::Fetch(at);
        return None;
    }
    if n.flee_left > 0.0
        && let Some(at) = n.flee_from.and_then(locate)
    {
        n.goal = Goal::Flee(at);
        return None;
    }
    if let Some(foe) = n.foe
        && !baby
        && let Some(at) = locate(foe)
    {
        n.goal = Goal::Attack { foe, pos: at };
        return None;
    }
    let near = |s: &&Seen, r: f64| s.uid != uid && s.pos.distance_squared(pos) <= r * r;
    if !brute && let Some(z) = view.iter().find(|s| is_zombified(s.kind) && near(s, AVOID_ZOMBIFIED)) {
        n.goal = Goal::Flee(z.pos);
        return None;
    }
    if !baby {
        // Players in sight: for piglins, only those without gold armor.
        let player = ctx
            .players
            .iter()
            .filter(|t| {
                t.targetable
                    && (brute || !t.gold_armor)
                    && t.pos.distance_squared(pos) <= ANGER_RANGE * ANGER_RANGE
                    && super::mob::line_of_sight(world, eye, t.pos + DVec3::Y * 1.5)
            })
            .min_by(|a, b| a.pos.distance_squared(pos).total_cmp(&b.pos.distance_squared(pos)));
        if let Some(t) = player {
            n.goal = Goal::Attack { foe: Foe::Player(t.id), pos: t.pos };
            return None;
        }
        // Wither skeletons are piglins' nemesis.
        if let Some(s) = view.iter().find(|s| s.kind == MobKind::WitherSkeleton && near(s, ANGER_RANGE)) {
            n.goal = Goal::Attack { foe: Foe::Mob(s.uid), pos: s.pos };
            return None;
        }
        // Java's `StartHuntingHoglin`: adults hunt adult hoglins now and then.
        if !brute
            && !n.cannot_hunt
            && n.hunt_cooldown <= 0.0
            && let Some(s) =
                view.iter().find(|s| s.kind == MobKind::Hoglin && !s.baby && s.huntable && near(s, ANGER_RANGE))
        {
            n.anger(Foe::Mob(s.uid));
            n.hunt_cooldown = rng.range(30.0, 120.0);
            n.goal = Goal::Attack { foe: Foe::Mob(s.uid), pos: s.pos };
            return Some(s.uid);
        }
    }
    n.goal = match n.home {
        Some(home) if home.distance_squared(pos) > HOME_RANGE * HOME_RANGE => Goal::Walk(home),
        _ => Goal::None,
    };
    None
}

/// Where a foe is now: a targetable player, or a living mob in `view`.
fn locate(foe: Foe, view: &[Seen], ctx: &Ctx) -> Option<DVec3> {
    match foe {
        Foe::Player(id) => ctx.players.iter().find(|t| t.id == id && t.targetable).map(|t| t.pos),
        Foe::Mob(u) => view.iter().find(|s| s.uid == u).map(|s| s.pos),
    }
}

/// The nearest targetable player within 16 blocks that `eye` can see.
fn visible_player<'a, W: MobWorld + ?Sized>(
    ctx: &'a Ctx,
    world: &W,
    pos: DVec3,
    eye: DVec3,
) -> Option<&'a super::Target> {
    ctx.players
        .iter()
        .filter(|t| {
            t.targetable
                && t.pos.distance_squared(pos) <= ANGER_RANGE * ANGER_RANGE
                && super::mob::line_of_sight(world, eye, t.pos + DVec3::Y * 1.5)
        })
        .min_by(|a, b| a.pos.distance_squared(pos).total_cmp(&b.pos.distance_squared(pos)))
}

/// One look around for the hoglin `m` (Java's `HoglinAi`): steer clear of
/// repellents and of outnumbering piglins, keep up a grudge, charge the
/// nearest visible player, and as a baby trail the adults.
fn sense_hoglin<W: MobWorld + ?Sized>(m: &mut Mob, view: &[Seen], world: &W, ctx: &Ctx) {
    let (pos, baby, uid) = (m.pos, m.baby, m.uid);
    let eye = pos + DVec3::Y * (m.shape().height * 0.8);
    let Some(n) = m.nether.as_mut() else { return };
    n.scan_timer -= SENSE_INTERVAL;
    if n.scan_timer <= 0.0 {
        n.scan_timer = SCAN_INTERVAL;
        n.repellent = find_repellent(world, pos.floor().as_ivec3());
        if n.repellent.is_some() {
            n.pacified = PACIFY_TIME;
        }
    }
    if let Some(foe) = n.foe
        && locate(foe, view, ctx).is_none_or(|p| p.distance_squared(pos) > 32.0 * 32.0)
    {
        n.foe = None;
    }
    if let Some(r) = n.repellent {
        n.goal = Goal::Flee(r);
        return;
    }
    if n.flee_left > 0.0
        && let Some(at) = n.flee_from.and_then(|f| locate(f, view, ctx))
    {
        n.goal = Goal::Flee(at);
        return;
    }
    let (piglins, hoglins) = adult_counts(view, pos);
    if !baby
        && piglins > hoglins
        && let Some(p) = view.iter().find(|s| is_piglin(s.kind) && !s.baby && s.pos.distance_squared(pos) <= 256.0)
    {
        n.goal = Goal::Flee(p.pos);
        return;
    }
    if n.pacified <= 0.0 {
        if let Some(foe) = n.foe
            && let Some(at) = locate(foe, view, ctx)
        {
            n.goal = Goal::Attack { foe, pos: at };
            return;
        }
        if let Some(t) = visible_player(ctx, world, pos, eye) {
            n.goal = Goal::Attack { foe: Foe::Player(t.id), pos: t.pos };
            return;
        }
    }
    n.goal = Goal::None;
    if baby
        && let Some(adult) = view
            .iter()
            .filter(|s| s.kind == MobKind::Hoglin && !s.baby && s.uid != uid)
            .min_by(|a, b| a.pos.distance_squared(pos).total_cmp(&b.pos.distance_squared(pos)))
        && (FOLLOW_ADULT * FOLLOW_ADULT..=ANGER_RANGE * ANGER_RANGE).contains(&adult.pos.distance_squared(pos))
    {
        n.goal = Goal::Walk(adult.pos);
    }
}

/// Java's `StriderGoToLavaGoal`: a strider out of lava heads for the
/// nearest open lava within 8 blocks across and 2 up or down.
fn sense_strider<W: MobWorld + ?Sized>(m: &mut Mob, world: &W) {
    let pos = m.pos;
    let Some(n) = m.nether.as_mut() else { return };
    if !n.cold {
        n.goal = Goal::None;
        return;
    }
    n.scan_timer -= SENSE_INTERVAL;
    if n.scan_timer > 0.0 && n.goal != Goal::None {
        return;
    }
    n.scan_timer = SCAN_INTERVAL;
    let cell = pos.floor().as_ivec3();
    let mut best: Option<(i32, IVec3)> = None;
    for dy in -2..=2 {
        for dz in -8..=8 {
            for dx in -8..=8 {
                let p = cell + IVec3::new(dx, dy, dz);
                let d = dx * dx + dy * dy + dz * dz;
                if best.is_none_or(|(bd, _)| d < bd)
                    && world.block(p).is_some_and(|b| b.is_lava())
                    && world.block(p + IVec3::Y) == Some(Block::AIR)
                {
                    best = Some((d, p));
                }
            }
        }
    }
    n.goal = best.map_or(Goal::None, |(_, p)| Goal::Walk(p.as_dvec3() + DVec3::new(0.5, 1.0, 0.5)));
}

/// Feet position on a lava sea's surface in column (x, z): Java's strider
/// rule wants open air right above the lava.
pub(super) fn lava_spot<W: MobWorld + ?Sized>(world: &W, x: i32, z: i32) -> Option<DVec3> {
    let sea = crate::world::nether::LAVA_SEA;
    let y = (sea - 8..=sea + 8).rev().find(|&y| {
        world.block(IVec3::new(x, y, z)).is_some_and(|b| b.is_lava())
            && world.block(IVec3::new(x, y + 1, z)) == Some(Block::AIR)
    })?;
    let pos = DVec3::new(x as f64 + 0.5, y as f64 + 1.0, z as f64 + 0.5);
    (!crate::physics::overlaps_solid(world, pos, MobKind::Strider.shape())).then_some(pos)
}

/// One look around for the zoglin `m`: anything alive nearby but creepers
/// and other zoglins is a target (Java's `Zoglin.isTargetable`).
fn sense_zoglin<W: MobWorld + ?Sized>(m: &mut Mob, view: &[Seen], world: &W, ctx: &Ctx) {
    let (pos, uid) = (m.pos, m.uid);
    let eye = pos + DVec3::Y * (m.shape().height * 0.8);
    let Some(n) = m.nether.as_mut() else { return };
    if let Some(foe) = n.foe
        && let Some(at) = locate(foe, view, ctx).filter(|p| p.distance_squared(pos) <= 32.0 * 32.0)
    {
        n.goal = Goal::Attack { foe, pos: at };
        return;
    }
    n.foe = None;
    let mob = view
        .iter()
        .filter(|s| {
            s.uid != uid
                && !matches!(s.kind, MobKind::Zoglin | MobKind::Creeper)
                && (s.pos.y - pos.y).abs() < 4.0
                && s.pos.distance_squared(pos) <= ANGER_RANGE * ANGER_RANGE
        })
        .min_by(|a, b| a.pos.distance_squared(pos).total_cmp(&b.pos.distance_squared(pos)))
        .map(|s| (Foe::Mob(s.uid), s.pos));
    let player = visible_player(ctx, world, pos, eye).map(|t| (Foe::Player(t.id), t.pos));
    let target = [mob, player]
        .into_iter()
        .flatten()
        .min_by(|a, b| a.1.distance_squared(pos).total_cmp(&b.1.distance_squared(pos)));
    n.goal = match target {
        // Players were only picked if seen.
        Some((foe @ Foe::Player(_), at)) => Goal::Attack { foe, pos: at },
        Some((foe, at)) if super::mob::line_of_sight(world, eye, at + DVec3::Y) => Goal::Attack { foe, pos: at },
        _ => Goal::None,
    };
}

fn save_mob(m: &Mob) -> Value {
    let n = m.nether.as_deref();
    let armor: Vec<bool> = m.armor.iter().map(|a| a.is_some()).collect();
    json!({
        "variant": m.aquatic.as_ref().map(|a| a.variant),
        "color": m.wool_color as u8,
        "kind": m.kind.name(),
        "pos": m.pos.to_array(),
        "yaw": m.yaw,
        "health": m.health,
        "baby": m.baby,
        "grow": m.grow,
        "age": m.age,
        "egg_timer": m.egg_timer,
        "sheared": m.sheared,
        "animal": m.animal.as_deref().map(super::animals::save),
        "armor": armor,
        "weapon": n.map_or("none", |n| n.weapon.name()),
        "offhand": n.and_then(|n| n.offhand).map(|s| crate::inventory::stack_to_string(Some(s))),
        "admire": n.map_or(0.0, |n| n.admire_left),
        "pocket": n.map_or(String::new(), |n| {
            n.pocket.iter().map(|s| crate::inventory::stack_to_string(Some(*s))).collect::<Vec<_>>().join(",")
        }),
        "zombify": n.map_or(0.0, |n| n.zombify),
        "immune": n.is_some_and(|n| n.immune),
        "cannot_hunt": n.is_some_and(|n| n.cannot_hunt),
        "home": n.and_then(|n| n.home).map(|h| h.to_array()),
    })
}

fn load_mob(v: &Value) -> Option<Mob> {
    let kind = MobKind::from_name(v["kind"].as_str()?)?;
    let p = v["pos"].as_array()?;
    let pos = DVec3::new(p.first()?.as_f64()?, p.get(1)?.as_f64()?, p.get(2)?.as_f64()?);
    if !pos.is_finite() {
        return None;
    }
    let mut m = Mob::new(kind, pos, v["yaw"].as_f64().unwrap_or(0.0) as f32);
    m.persistent = true;
    if let Some(a) = &mut m.aquatic {
        a.variant = v["variant"].as_u64().unwrap_or(0).min(255) as u8;
    }
    m.baby = v["baby"].as_bool().unwrap_or(false);
    m.age =
        v["age"].as_i64().unwrap_or(if m.baby && kind.is_breedable() { -24000 } else { 0 }).clamp(-24000, 6000) as i32;
    m.egg_timer =
        v["egg_timer"].as_f64().filter(|t| t.is_finite() && *t >= 0.0).unwrap_or(m.egg_timer as f64).min(600.0) as f32;
    m.sheared = v["sheared"].as_bool().unwrap_or(false);
    if let Some(a) = m.animal.as_mut() {
        super::animals::load(a, &v["animal"]);
    }
    m.grow = v["grow"].as_f64().filter(|g| g.is_finite() && *g >= 0.0).unwrap_or(0.0) as f32;
    m.wool_color = crate::color::DyeColor::ALL.get(v["color"].as_u64().unwrap_or(0).min(15) as usize).copied().unwrap();
    if let Some(h) = v["health"].as_f64().filter(|h| *h > 0.0) {
        m.health = (h as f32).min(m.max_health());
    }
    if let Some(armor) = v["armor"].as_array() {
        for (slot, worn) in m.armor.iter_mut().zip(armor) {
            *slot = worn.as_bool().unwrap_or(false).then_some(super::armor::ArmorKind::Gold);
        }
    }
    if let Some(n) = m.nether.as_mut() {
        n.weapon = Weapon::from_name(v["weapon"].as_str().unwrap_or("none"));
        n.offhand = v["offhand"].as_str().and_then(|s| crate::inventory::stack_from_str(s).flatten()).or_else(|| {
            // Earlier v0.5 saves kept only the numeric item id.
            let id = u16::try_from(v["offhand"].as_u64()?).ok()?;
            Item(id).is_valid().then(|| Stack::new(Item(id), 1))
        });
        n.admire_left = v["admire"].as_f64().unwrap_or(0.0) as f32;
        if n.offhand.is_some() && n.admire_left <= 0.0 {
            n.admire_left = ADMIRE_TIME;
        }
        n.pocket = v["pocket"]
            .as_str()
            .unwrap_or("")
            .split(',')
            .filter_map(|s| crate::inventory::stack_from_str(s).flatten())
            .take(POCKET_SLOTS)
            .collect();
        n.zombify = v["zombify"].as_f64().unwrap_or(0.0) as f32;
        n.immune = v["immune"].as_bool().unwrap_or(false);
        n.cannot_hunt = v["cannot_hunt"].as_bool().unwrap_or(false);
        n.home = v["home"].as_array().and_then(|h| {
            let h = DVec3::new(h.first()?.as_f64()?, h.get(1)?.as_f64()?, h.get(2)?.as_f64()?);
            h.is_finite().then_some(h)
        });
    }
    Some(m)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::entity::{Target, item::THROWN_PICKUP_DELAY};
    use crate::physics::test_util::Grid;
    use crate::world::terrain::Dimension;

    fn nether(players: Vec<Target>) -> Ctx {
        Ctx { players, daylight: 0.0, spawning: false, raining: false, dimension: Dimension::Nether }
    }

    fn player(at: DVec3) -> Target {
        Target::new(PlayerId::HOST, at, true)
    }

    #[test]
    fn melee_and_sweep_anger_piglins_at_the_attacker_not_a_closer_player() {
        let mut e = Entities::new(9);
        let hitter = PlayerId(2);
        e.player_spots = vec![(PlayerId::HOST, DVec3::new(0.5, 10.0, 0.5)), (hitter, DVec3::new(2.5, 10.0, 0.5))];
        let target = swordsman(&mut e);
        for x in [1.0, 5.0] {
            let i = swordsman(&mut e);
            e.mobs[i].pos.x = x;
        }
        e.nether_sense(0.0, &Grid::flat(10), &nether(vec![]));
        let sword = Stack::new(Item::tool(crate::item::ToolKind::Sword, Tier::Wood), 1);
        e.melee_for(target, DVec3::Z, Some(sword), 0.0, false, Some(DVec3::new(2.5, 10.0, 0.5)), hitter);
        for m in &e.mobs {
            assert_eq!(m.nether.as_ref().unwrap().foe, Some(Foe::Player(hitter)));
        }
        assert!(e.mobs[1].health < MobKind::Piglin.max_health(), "the sweep also hit a piglin");
        assert_eq!(e.mobs[2].health, MobKind::Piglin.max_health(), "the broadcast reached an unharmed piglin");
    }

    #[test]
    fn player_projectiles_anger_piglins_at_their_owner() {
        let world = Grid::flat(10);
        let hitter = PlayerId(2);
        for potion in [false, true] {
            let mut e = Entities::new(9);
            let target = swordsman(&mut e);
            e.mobs[target].pos.x = 3.5;
            let ally = swordsman(&mut e);
            e.mobs[ally].pos.x = 6.5;
            let mut bystander = player(DVec3::new(3.5, 10.0, 2.5));
            bystander.gold_armor = true;
            let mut attacker = Target::new(hitter, DVec3::new(0.5, 10.0, 0.5), true);
            attacker.gold_armor = true;
            let ctx = nether(vec![bystander, attacker]);
            if potion {
                e.potions.push(crate::entity::potion::ThrownPotion::new(
                    crate::potion::Potion::from_id("harming").unwrap(),
                    Some(hitter),
                    e.mobs[target].pos + DVec3::Y,
                    DVec3::ZERO,
                ));
            } else {
                // Creative/Infinity arrows still have an owner despite being unrecoverable.
                e.shoot_enchanted_for(DVec3::new(0.5, 11.0, 0.5), DVec3::X, 0.5, false, Default::default(), hitter);
            }
            let events = run(&mut e, &world, &ctx, 0.3);
            assert!(events.iter().any(|e| matches!(e, EntityEvent::MobShot { owner: Some(id), .. } if *id == hitter)));
            for m in &e.mobs {
                assert_eq!(m.nether.as_ref().unwrap().foe, Some(Foe::Player(hitter)));
            }
        }
    }

    #[test]
    fn piglin_crossbow_hits_keep_the_piglin_death_message() {
        let world = Grid::flat(10);
        let mut e = Entities::new(9);
        let i = swordsman(&mut e);
        e.mobs[i].nether.as_mut().unwrap().weapon = Weapon::Crossbow;
        let ctx = nether(vec![player(DVec3::new(6.5, 10.0, 0.5))]);
        let events = run(&mut e, &world, &ctx, 10.0);
        let causes: Vec<_> = events
            .iter()
            .filter_map(|e| match e {
                EntityEvent::PlayerHit { cause, .. } => Some(*cause),
                _ => None,
            })
            .collect();
        assert!(!causes.is_empty(), "the crossbow hit the player");
        assert!(causes.iter().all(|&c| c == "was shot by a piglin"), "{causes:?}");
    }

    /// One adult piglin with a golden sword at the origin of a flat floor.
    fn swordsman(e: &mut Entities) -> usize {
        e.mobs.push(Mob::new(MobKind::Piglin, DVec3::new(0.5, 10.0, 0.5), 0.0));
        let i = e.mobs.len() - 1;
        e.mobs[i].nether.as_mut().unwrap().weapon = Weapon::GoldenSword;
        i
    }

    fn run<W: MobWorld>(e: &mut Entities, world: &W, ctx: &Ctx, secs: f32) -> Vec<EntityEvent> {
        let mut all = Vec::new();
        for _ in 0..(secs * 20.0) as usize {
            all.extend(e.update(0.05, world, ctx));
        }
        all
    }

    fn hits(events: &[EntityEvent]) -> usize {
        events.iter().filter(|e| matches!(e, EntityEvent::PlayerHit { .. })).count()
    }

    #[test]
    fn bartering_uses_javas_weights_and_counts_deterministically() {
        assert_eq!(BARTER_TOTAL, 459);
        let weights: Vec<u32> = BARTERING.iter().map(|e| e.1).collect();
        assert_eq!(weights, [5, 8, 8, 8, 10, 10, 10, 20, 20, 40, 40, 40, 40, 40, 40, 40, 40, 40]);
        // Same seed, same trades.
        let a: Vec<_> = (0..64)
            .map({
                let mut r = Rng::new(42);
                move |_| barter(&mut r)
            })
            .collect();
        let b: Vec<_> = (0..64)
            .map({
                let mut r = Rng::new(42);
                move |_| barter(&mut r)
            })
            .collect();
        assert_eq!(a, b);
        let mut rng = Rng::new(7);
        let rolls = 45_900;
        let mut obsidian = 0;
        let mut pearls = 0;
        let mut empty = 0;
        for _ in 0..rolls {
            match barter(&mut rng) {
                Some(s) if s.item == Item::from_block(Block::OBSIDIAN) => {
                    assert_eq!(s.count, 1);
                    obsidian += 1;
                }
                Some(s) if s.item == Item::ENDER_PEARL => {
                    assert!((2..=4).contains(&s.count));
                    pearls += 1;
                }
                Some(s) if s.item == Item::IRON_NUGGET => assert!((10..=36).contains(&s.count)),
                Some(s) if s.item == Item::from_block(Block::GRAVEL) => assert!((8..=16).contains(&s.count)),
                Some(_) => {}
                None => empty += 1,
            }
        }
        // Expected 4000 obsidian, 1000 pearls and 4500 missing-item rolls.
        assert!((3700..4300).contains(&obsidian), "{obsidian}");
        assert!((850..1150).contains(&pearls), "{pearls}");
        assert!((4200..4800).contains(&empty), "{empty}");
        // Every existing output is a real item.
        let mut rng = Rng::new(1);
        assert!((0..2000).filter_map(|_| barter(&mut rng)).all(|s| s.item.is_valid()));
    }

    #[test]
    fn piglins_attack_players_without_gold_armor_only() {
        let world = Grid::flat(10);
        let mut e = Entities::new(3);
        swordsman(&mut e);
        let mut gold = player(DVec3::new(3.5, 10.0, 0.5));
        gold.gold_armor = true;
        let calm = run(&mut e, &world, &nether(vec![gold]), 4.0);
        assert_eq!(hits(&calm), 0, "a golden helmet keeps piglins neutral");
        let bare = run(&mut e, &world, &nether(vec![player(DVec3::new(3.5, 10.0, 0.5))]), 4.0);
        let damage = bare.iter().find_map(|e| match e {
            EntityEvent::PlayerHit { damage, .. } => Some(*damage),
            _ => None,
        });
        assert_eq!(damage, Some(8.0), "golden sword: 5 + 3");
        assert!(wears_gold(&[
            None,
            Some(Stack::new(Item::armor(ArmorPiece::Chestplate, ArmorMaterial::Gold), 1)),
            None,
            None
        ]));
        assert!(!wears_gold(&[
            Some(Stack::new(Item::armor(ArmorPiece::Helmet, ArmorMaterial::Iron), 1)),
            None,
            None,
            None
        ]));
    }

    #[test]
    fn chests_and_gold_anger_idle_piglins_and_hits_are_broadcast() {
        let world = Grid::flat(10);
        let mut gold = player(DVec3::new(4.5, 10.0, 0.5));
        gold.gold_armor = true;
        let ctx = nether(vec![gold]);
        let mut e = Entities::new(5);
        let a = swordsman(&mut e);
        let b = swordsman(&mut e);
        e.mobs[b].pos.x = 30.0;
        e.update(0.05, &world, &ctx);
        assert!(guarded_by_piglins(Block::CHEST) && guarded_by_piglins(Block::GOLD_BLOCK));
        assert!(!guarded_by_piglins(Block::STONE));
        e.piglins_notice(PlayerId::HOST, gold.pos, true, &world);
        assert_eq!(e.mobs[a].nether.as_ref().unwrap().foe, Some(Foe::Player(PlayerId::HOST)));
        assert_eq!(e.mobs[b].nether.as_ref().unwrap().foe, None, "too far away to notice");
        assert!(hits(&run(&mut e, &world, &ctx, 3.0)) > 0, "gold armor doesn't calm an angry piglin");

        // An admiring piglin isn't idle, so opening a chest doesn't upset it.
        let mut e = Entities::new(5);
        let a = swordsman(&mut e);
        e.mobs[a].nether.as_mut().unwrap().offhand = Some(Stack::new(Item::GOLD_INGOT, 1));
        e.piglins_notice(PlayerId::HOST, gold.pos, false, &world);
        assert_eq!(e.mobs[a].nether.as_ref().unwrap().foe, None);

        // Hitting one piglin turns the adults around it; a hit baby runs.
        let mut e = Entities::new(6);
        let a = swordsman(&mut e);
        let b = swordsman(&mut e);
        e.mobs[b].pos.x = 8.0;
        e.mobs.push(Mob::new(MobKind::Piglin, DVec3::new(-6.0, 10.0, 0.5), 0.0));
        e.mobs[2].baby = true;
        e.update(0.05, &world, &ctx);
        e.attack(a, DVec3::X, 1.0);
        for i in [a, b, 2] {
            assert_eq!(e.mobs[i].nether.as_ref().unwrap().foe, (i != 2).then_some(Foe::Player(PlayerId::HOST)));
        }
        e.attack(2, DVec3::X, 1.0);
        let baby = e.mobs[2].nether.as_ref().unwrap();
        assert!(baby.flee_left > 0.0 && baby.foe.is_none());
    }

    #[test]
    fn thrown_gold_is_admired_then_bartered_and_babies_keep_it() {
        let world = Grid::flat(10);
        let mut gold = player(DVec3::new(6.5, 10.0, 0.5));
        gold.gold_armor = true;
        let ctx = nether(vec![gold]);
        let mut e = Entities::new(9);
        swordsman(&mut e);
        let mut rng = Rng::new(1);
        let ingot = Stack::new(Item::GOLD_INGOT, 2);
        e.items.push(ItemEntity::new(ingot, DVec3::new(3.5, 10.0, 0.5), DVec3::ZERO, THROWN_PICKUP_DELAY, &mut rng));
        run(&mut e, &world, &ctx, 3.5);
        let n = e.mobs[0].nether.as_ref().unwrap();
        assert_eq!(n.offhand, Some(Stack::new(Item::GOLD_INGOT, 1)), "walked over and picked one up");
        assert!(e.mobs[0].persistent);
        assert_eq!(e.items.iter().find(|i| i.stack.item == Item::GOLD_INGOT).map(|i| i.stack.count), Some(1));
        // Admiring takes about six seconds; the second ingot follows.
        run(&mut e, &world, &ctx, 3.0);
        assert!(e.mobs[0].nether.as_ref().unwrap().is_admiring(), "still admiring after 3 s");
        run(&mut e, &world, &ctx, 12.0);
        assert!(e.items.iter().all(|i| i.stack.item != Item::GOLD_INGOT), "both ingots traded");
        assert!(!e.mobs[0].nether.as_ref().unwrap().pocket.iter().any(|s| s.item == Item::GOLD_INGOT));

        let mut e = Entities::new(9);
        swordsman(&mut e);
        e.mobs[0].baby = true;
        e.items.push(ItemEntity::new(
            Stack::new(Item::GOLD_INGOT, 1),
            DVec3::new(2.5, 10.0, 0.5),
            DVec3::ZERO,
            0.0,
            &mut rng,
        ));
        run(&mut e, &world, &ctx, 10.0);
        let n = e.mobs[0].nether.as_ref().unwrap();
        assert_eq!(n.pocket.first().map(|s| s.item), Some(Item::GOLD_INGOT), "babies don't barter");
    }

    #[test]
    fn mob_griefing_disables_piglin_item_pickup_and_bartering() {
        let world = Grid::flat(10);
        let mut gold = player(DVec3::new(6.5, 10.0, 0.5));
        gold.gold_armor = true;
        let ctx = nether(vec![gold]);
        let mut e = Entities::new(19);
        swordsman(&mut e);
        e.villager_griefing = false;
        let gift = Stack::new(Item::GOLD_INGOT, 1);
        e.items.push(ItemEntity::new(gift, e.mobs[0].pos, DVec3::ZERO, 0.0, &mut e.rng));
        run(&mut e, &world, &ctx, 10.0);
        assert_eq!(e.items.len(), 1);
        assert_eq!(e.items[0].stack, gift);
        assert!(!e.mobs[0].persistent);
        assert!(!e.mobs[0].nether.as_ref().unwrap().is_admiring());
        e.villager_griefing = true;
        e.mobs[0].pos = e.items[0].pos;
        e.mobs[0].vel = DVec3::ZERO;
        e.mobs[0].nether.as_mut().unwrap().sense_timer = 0.0;
        run(&mut e, &world, &ctx, 1.0);
        assert!(e.mobs[0].nether.as_ref().unwrap().is_admiring());
        assert!(e.items.is_empty());
    }

    #[test]
    fn admired_gold_gear_keeps_components_through_pickup_save_and_death() {
        let mut e = Entities::new(21);
        let i = swordsman(&mut e);
        let mut gift =
            Stack::new(Item::tool(crate::item::ToolKind::Sword, Tier::Gold), 1).with_name("Named gift").unwrap();
        gift.damage = 12;
        gift.repair_cost = 3;
        gift.enchants = crate::enchant::Enchants::NONE.with(crate::enchant::Enchantment::Sharpness, 2);
        e.items.push(ItemEntity::new(gift, e.mobs[i].pos, DVec3::ZERO, 0.0, &mut e.rng));
        e.pick_up(i, &mut Vec::new());
        assert_eq!(e.mobs[i].nether.as_ref().unwrap().offhand, Some(gift));
        let saved = e.nether_mobs_to_string();
        let mut loaded = Entities::new(21);
        loaded.load_nether_mobs(&saved);
        assert_eq!(loaded.mobs[0].nether.as_ref().unwrap().offhand, Some(gift));
        loaded.finish_admiring(0, &nether(vec![]));
        assert_eq!(loaded.mobs[0].nether.as_ref().unwrap().pocket, vec![gift]);
        loaded.attack(0, DVec3::X, 100.0);
        loaded.drop_loot(MobKind::Piglin, loaded.mobs[0].pos);
        assert!(loaded.items.iter().any(|i| i.stack == gift));

        // The first v0.5 saves represented the admired item by its numeric id.
        let mut old: Value = serde_json::from_str(&saved).unwrap();
        old["mobs"][0]["offhand"] = json!(Item::GOLD_INGOT.0);
        let mut loaded = Entities::new(21);
        loaded.load_nether_mobs(&old.to_string());
        assert_eq!(loaded.mobs[0].nether.as_ref().unwrap().offhand, Some(Stack::new(Item::GOLD_INGOT, 1)));
    }

    #[test]
    fn full_piglin_pockets_throw_overflow_without_destroying_older_items() {
        let mut e = Entities::new(22);
        let i = swordsman(&mut e);
        let kept: Vec<_> = (0..POCKET_SLOTS)
            .map(|j| Stack::new(Item::GOLD_NUGGET, 1).with_name(&format!("Nugget {j}")).unwrap())
            .collect();
        let gift = Stack::new(Item::CLOCK, 1).with_name("Overflow").unwrap();
        let n = e.mobs[i].nether.as_mut().unwrap();
        n.pocket = kept.clone();
        n.offhand = Some(gift);
        e.finish_admiring(i, &nether(vec![]));
        assert_eq!(e.mobs[i].nether.as_ref().unwrap().pocket, kept);
        assert_eq!(e.items.len(), 1);
        assert_eq!(e.items[0].stack, gift);

        // Same item ids with different components use separate slots.
        let n = e.mobs[i].nether.as_mut().unwrap();
        n.pocket.truncate(1);
        let distinct = Stack::new(Item::GOLD_NUGGET, 1).with_name("Different name").unwrap();
        assert_eq!(pocket(n, distinct), None);
        assert_eq!(n.pocket, vec![kept[0], distinct]);
    }

    #[test]
    fn nether_melee_attacks_trigger_thorns_and_credit_its_kills() {
        let world = Grid::flat(10);
        for kind in [MobKind::Piglin, MobKind::PiglinBrute, MobKind::Hoglin, MobKind::Zoglin] {
            let mut e = Entities::new(23);
            e.mobs.push(Mob::new(kind, DVec3::new(0.5, 10.0, 0.5), 0.0));
            let mut target = player(DVec3::new(1.5, 10.0, 0.5));
            // A test-only level above VI makes a retaliation certain.
            target.thorns = [7, 0, 0, 0];
            let ctx = nether(vec![target]);
            let events = e.update(0.05, &world, &ctx);
            assert_eq!(hits(&events), 1, "{kind:?} struck the player");
            let hurt = kind.max_health() - e.mobs[0].health;
            assert!((1.0..5.0).contains(&hurt), "{kind:?} took Thorns damage: {hurt}");

            let mut e = Entities::new(23);
            e.mobs.push(Mob::new(kind, DVec3::new(0.5, 10.0, 0.5), 0.0));
            e.mobs[0].health = 0.5;
            let pocket_stack = Stack::new(Item::GOLD_NUGGET, 3);
            e.mobs[0].nether.as_mut().unwrap().pocket.push(pocket_stack);
            e.mobs[0].pos = DVec3::new(0.5, 10.0, 0.5);
            // Strong existing upward knockback moves the corpse beyond the
            // loot lookup tolerance during the same tick as lethal Thorns.
            let struck_at = e.mobs[0].pos;
            e.mobs[0].vel = DVec3::Y * 20.0;
            e.update(0.05, &world, &ctx);
            assert!(e.mobs[0].pos.distance(struck_at) > 0.1);
            assert!(!e.mobs[0].alive(), "{kind:?} killed by Thorns");
            assert!(!e.orbs.is_empty(), "{kind:?} kill awarded player experience");
            assert!(e.items.iter().any(|i| i.stack == pocket_stack), "{kind:?} kill spilled inventory");
        }
    }

    #[test]
    fn lethal_hits_drop_the_ingot_a_piglin_is_admiring() {
        let mut e = Entities::new(25);
        let i = swordsman(&mut e);
        let gift = Stack::new(Item::GOLD_INGOT, 1).with_name("Last gift").unwrap();
        e.mobs[i].nether.as_mut().unwrap().offhand = Some(gift);
        e.attack(i, DVec3::X, 100.0);
        e.drop_loot(MobKind::Piglin, e.mobs[i].pos);
        assert!(e.items.iter().any(|i| i.stack == gift));
    }

    #[test]
    fn colocated_piglin_deaths_drop_each_inventory_once() {
        let mut e = Entities::new(26);
        let pos = DVec3::new(0.5, 10.0, 0.5);
        let gifts = [
            Stack::new(Item::CLOCK, 1).with_name("First piglin").unwrap(),
            Stack::new(Item::CLOCK, 1).with_name("Second piglin").unwrap(),
        ];
        for gift in gifts {
            let i = swordsman(&mut e);
            e.mobs[i].nether.as_mut().unwrap().pocket.push(gift);
            e.attack(i, DVec3::X, 100.0);
        }
        // Death events identify mobs by kind and position. Co-located corpses
        // must each be consumed once instead of duplicating the first pocket.
        for _ in 0..2 {
            e.drop_loot(MobKind::Piglin, pos);
        }
        for gift in gifts {
            assert_eq!(e.items.iter().filter(|i| i.stack == gift).count(), 1);
        }
    }

    #[test]
    fn baby_zoglins_drop_rotten_flesh() {
        let mut e = Entities::new(24);
        let mut m = Mob::new(MobKind::Zoglin, DVec3::new(0.5, 10.0, 0.5), 0.0);
        m.baby = true;
        e.mobs.push(m);
        e.attack(0, DVec3::X, 100.0);
        e.drop_loot(MobKind::Zoglin, e.mobs[0].pos);
        let flesh: u32 =
            e.items.iter().filter(|i| i.stack.item == Item::ROTTEN_FLESH).map(|i| i.stack.count as u32).sum();
        assert!((1..=3).contains(&flesh));
    }

    #[test]
    fn piglins_zombify_after_fifteen_seconds_outside_the_nether() {
        let world = Grid::flat(10);
        let far = player(DVec3::new(60.0, 10.0, 0.5));
        let mut e = Entities::new(2);
        swordsman(&mut e);
        e.mobs[0].baby = true;
        run(&mut e, &world, &nether(vec![far]), 20.0);
        assert_eq!(e.mobs[0].kind, MobKind::Piglin, "safe in the Nether");
        let overworld = Ctx { dimension: Dimension::Overworld, daylight: 0.0, ..nether(vec![far]) };
        run(&mut e, &world, &overworld, 14.5);
        assert_eq!(e.mobs[0].kind, MobKind::Piglin);
        let pocket_stack = Stack::new(Item::CLOCK, 1).with_name("Kept clock").unwrap();
        let admired = Stack::new(Item::GOLD_INGOT, 1).with_name("Gift").unwrap();
        let n = e.mobs[0].nether.as_mut().unwrap();
        n.pocket.push(pocket_stack);
        n.offhand = Some(admired);
        n.admire_left = 2.0;
        run(&mut e, &world, &overworld, 1.0);
        assert_eq!(e.mobs[0].kind, MobKind::ZombifiedPiglin);
        assert!(e.items.iter().any(|i| i.stack == pocket_stack), "conversion spills its inventory");
        assert!(e.items.iter().any(|i| i.stack == admired), "conversion cancels admiring and drops the gift");
        assert!(e.mobs[0].baby, "keeps its age");
        let mut e = Entities::new(2);
        swordsman(&mut e);
        e.mobs[0].nether.as_mut().unwrap().immune = true;
        run(&mut e, &world, &overworld, 20.0);
        assert_eq!(e.mobs[0].kind, MobKind::Piglin, "immune piglins stay");
    }

    #[test]
    fn persistent_piglins_round_trip_through_the_level_file() {
        let mut e = Entities::new(4);
        swordsman(&mut e);
        e.spawn(MobKind::Zombie, DVec3::ZERO);
        let m = &mut e.mobs[0];
        m.persistent = true;
        m.health = 11.0;
        m.armor[0] = Some(super::super::armor::ArmorKind::Gold);
        let n = m.nether.as_mut().unwrap();
        n.weapon = Weapon::Crossbow;
        n.offhand = Some(Stack::new(Item::GOLD_INGOT, 1));
        n.admire_left = 2.5;
        n.pocket = vec![Stack::new(Item::GOLD_NUGGET, 12), Stack::new(Item::CLOCK, 1)];
        n.zombify = 4.0;
        n.cannot_hunt = true;
        let text = e.nether_mobs_to_string();
        let mut loaded = Entities::new(4);
        loaded.load_nether_mobs(&text);
        assert_eq!(loaded.mobs.len(), 1, "only persistent nether mobs are saved");
        let (a, b) = (&e.mobs[0], &loaded.mobs[0]);
        assert_eq!((a.kind, a.pos, a.health, a.baby, a.armor), (b.kind, b.pos, b.health, b.baby, b.armor));
        let (na, nb) = (a.nether.as_ref().unwrap(), b.nether.as_ref().unwrap());
        assert_eq!(
            (na.weapon, na.offhand, na.admire_left, &na.pocket, na.zombify, na.cannot_hunt),
            (nb.weapon, nb.offhand, nb.admire_left, &nb.pocket, nb.zombify, nb.cannot_hunt)
        );
        assert!(b.persistent);
        // Old levels without the key, and junk, load nothing.
        loaded.load_nether_mobs("");
        loaded.load_nether_mobs("{\"mobs\":[{\"kind\":\"dragon\"},{\"kind\":\"piglin\",\"pos\":[1,\"x\",2]}]}");
        assert_eq!(loaded.mobs.len(), 1);
    }

    #[test]
    fn piglins_spawn_in_the_nether_with_javas_gear_rolls() {
        assert!(MobKind::Piglin.spawns_in(Dimension::Nether));
        assert!(!MobKind::Piglin.spawns_in(Dimension::Overworld));
        assert_eq!(MobKind::from_name("piglin"), Some(MobKind::Piglin));
        let mut e = Entities::new(11);
        for _ in 0..2000 {
            e.spawn(MobKind::Piglin, DVec3::ZERO);
        }
        let babies = e.mobs.iter().filter(|m| m.baby).count();
        assert!((340..460).contains(&babies), "20% babies: {babies}");
        assert!(e.mobs.iter().filter(|m| m.baby).all(|m| m.nether.as_ref().unwrap().weapon == Weapon::None));
        let crossbows = e.mobs.iter().filter(|m| m.nether.as_ref().unwrap().weapon == Weapon::Crossbow).count();
        assert!((700..900).contains(&crossbows), "half the adults: {crossbows}");
        let helmets = e.mobs.iter().filter(|m| m.armor[0].is_some()).count();
        assert!((120..200).contains(&helmets), "10% of adults: {helmets}");
        assert_eq!(MobKind::Piglin.max_health(), 16.0);
    }

    #[test]
    fn dead_piglins_drop_their_pocket_and_sometimes_their_gear() {
        let mut e = Entities::new(12);
        swordsman(&mut e);
        e.mobs[0].nether.as_mut().unwrap().pocket.push(Stack::new(Item::GOLD_NUGGET, 5));
        assert!(e.attack(0, DVec3::X, 100.0).is_some());
        e.drop_loot(MobKind::Piglin, e.mobs[0].pos);
        assert!(e.items.iter().any(|i| i.stack == Stack::new(Item::GOLD_NUGGET, 5)));
        let swords = (0..2000)
            .filter(|_| {
                let mut e = Entities::new(e.rng.next_int(u32::MAX) as u64);
                swordsman(&mut e);
                e.attack(0, DVec3::X, 100.0);
                e.drop_loot_with(MobKind::Piglin, e.mobs[0].pos, 0);
                e.items.iter().any(|i| i.stack.item.as_tool().is_some())
            })
            .count();
        assert!((120..230).contains(&swords), "8.5%: {swords}");
    }

    /// A flat Nether floor at y = 33 under one generated bastion.
    struct Remnant(Grid, std::sync::Arc<crate::world::bastion::Bastion>);

    impl crate::physics::BlockSource for Remnant {
        fn block(&self, p: IVec3) -> Option<Block> {
            self.0.block(p)
        }
    }

    impl MobWorld for Remnant {
        fn loaded(&self, _: IVec3) -> bool {
            true
        }
        fn surface(&self, _: i32, _: i32) -> Option<i32> {
            None
        }
        fn exposed(&self, _: IVec3) -> bool {
            false
        }
        fn bastions_near(&self, _: IVec3, _: i32) -> Vec<std::sync::Arc<crate::world::bastion::Bastion>> {
            vec![self.1.clone()]
        }
    }

    fn remnant() -> Remnant {
        use crate::world::bastion::{Bastion, Kind};
        let b = Bastion::generate(77, glam::IVec2::new(0, 0), Kind::Housing, crate::world::block::Facing::South);
        Remnant(Grid::flat(34), std::sync::Arc::new(b))
    }

    #[test]
    fn brutes_hit_hard_even_players_in_gold_and_never_spawn_naturally() {
        let world = Grid::flat(10);
        let mut e = Entities::new(3);
        e.spawn(MobKind::PiglinBrute, DVec3::new(0.5, 10.0, 0.5));
        assert_eq!(e.mobs[0].health, 50.0);
        assert_eq!(e.mobs[0].nether.as_ref().unwrap().weapon, Weapon::GoldenAxe);
        let mut gold = player(DVec3::new(3.5, 10.0, 0.5));
        gold.gold_armor = true;
        let events = run(&mut e, &world, &nether(vec![gold]), 4.0);
        let damage = events.iter().find_map(|e| match e {
            EntityEvent::PlayerHit { damage, .. } => Some(*damage),
            _ => None,
        });
        assert_eq!(damage, Some(13.0), "golden axe: 7 + 6");
        assert!(!MobKind::PiglinBrute.spawns_in(Dimension::Nether));
        assert_eq!(xp(MobKind::PiglinBrute, false), Some(20));
        // Gold thrown at a brute stays on the floor.
        let mut e = Entities::new(3);
        e.spawn(MobKind::PiglinBrute, DVec3::new(0.5, 10.0, 0.5));
        let mut rng = Rng::new(2);
        e.items.push(ItemEntity::new(
            Stack::new(Item::GOLD_INGOT, 1),
            DVec3::new(2.5, 10.0, 0.5),
            DVec3::ZERO,
            0.0,
            &mut rng,
        ));
        run(&mut e, &world, &nether(vec![player(DVec3::new(60.0, 10.0, 0.5))]), 4.0);
        assert_eq!(e.items.len(), 1);
        assert!(!e.mobs[0].nether.as_ref().unwrap().is_admiring());
    }

    #[test]
    fn bastions_get_their_residents_once_and_remember_it() {
        let world = remnant();
        let bounds = world.1.bounds;
        let centre = ((bounds.min + bounds.max) / 2).as_dvec3();
        let mut far = player(DVec3::new(centre.x, 34.0, bounds.max.z as f64 + 40.0));
        far.targetable = false;
        let ctx = nether(vec![far]);
        let mut e = Entities::new(8);
        e.update(0.05, &world, &ctx);
        let residents = e.mobs.len();
        assert!(residents >= 10, "a housing bastion is well populated: {residents}");
        let brutes = e.count(MobKind::PiglinBrute);
        assert!(brutes >= 2, "{brutes} brutes");
        for m in &e.mobs {
            assert!(m.persistent && m.nether.as_ref().unwrap().home.is_some());
            assert!(bounds.contains(m.pos.floor().as_ivec3()), "{:?} outside at {:?}", m.kind, m.pos);
            assert!(!m.baby);
        }
        assert!(
            e.mobs
                .iter()
                .all(|m| m.kind != MobKind::PiglinBrute || m.nether.as_ref().unwrap().weapon == Weapon::GoldenAxe)
        );
        run(&mut e, &world, &ctx, 3.0);
        assert_eq!(e.mobs.len(), residents, "no second batch");
        // The same bastion rolls the same residents in another world copy.
        let mut again = Entities::new(99);
        again.update(0.05, &world, &ctx);
        assert_eq!(again.mobs.len(), residents);
        // A saved level knows the bastion is done, and keeps its residents.
        let text = e.nether_mobs_to_string();
        let mut loaded = Entities::new(8);
        loaded.load_nether_mobs(&text);
        assert_eq!(loaded.mobs.len(), residents);
        assert!(loaded.mobs.iter().all(|m| m.nether.as_ref().unwrap().home.is_some()));
        run(&mut loaded, &world, &ctx, 2.0);
        assert_eq!(loaded.mobs.len(), residents);
        // No residents outside the Nether, or on Peaceful.
        let mut e = Entities::new(8);
        let overworld = Ctx { dimension: Dimension::Overworld, ..nether(vec![far]) };
        e.update(0.05, &world, &overworld);
        e.update_difficulty(0.05, &world, &ctx, crate::simulation::difficulty::Difficulty::Peaceful);
        assert!(e.mobs.is_empty());
    }

    #[test]
    fn residents_wander_back_home() {
        let world = Grid::flat(10);
        let mut e = Entities::new(4);
        e.spawn(MobKind::PiglinBrute, DVec3::new(20.5, 10.0, 0.5));
        e.mobs[0].nether.as_mut().unwrap().home = Some(DVec3::new(0.5, 10.0, 0.5));
        let mut away = player(DVec3::new(60.0, 10.0, 0.5));
        away.targetable = false;
        run(&mut e, &world, &nether(vec![away]), 8.0);
        assert!(e.mobs[0].pos.x < 13.0, "walked home: {}", e.mobs[0].pos.x);
    }

    fn hoglin_at(e: &mut Entities, x: f64, baby: bool) -> usize {
        e.mobs.push(Mob::new(MobKind::Hoglin, DVec3::new(x, 10.0, 0.5), 0.0));
        let i = e.mobs.len() - 1;
        e.mobs[i].baby = baby;
        i
    }

    #[test]
    fn hoglins_charge_players_and_toss_them() {
        let world = Grid::flat(10);
        let mut e = Entities::new(5);
        hoglin_at(&mut e, 0.5, false);
        assert_eq!(e.mobs[0].health, 40.0);
        let mut gold = player(DVec3::new(3.5, 10.0, 0.5));
        gold.gold_armor = true;
        let events = run(&mut e, &world, &nether(vec![gold]), 7.0);
        let blows: Vec<(f32, f32)> = events
            .iter()
            .filter_map(|e| match e {
                EntityEvent::PlayerHit { damage, knockback, .. } => Some((*damage, knockback.y)),
                _ => None,
            })
            .collect();
        assert!((3..=4).contains(&blows.len()), "one charge every two seconds: {blows:?}");
        assert!(blows.iter().all(|&(d, _)| (3.0..=8.0).contains(&d)), "{blows:?}");
        assert!(blows.iter().all(|&(_, up)| up >= 5.0), "thrown upward: {blows:?}");
        // Babies nip for half a heart and don't toss.
        let mut e = Entities::new(5);
        hoglin_at(&mut e, 0.5, true);
        let events = run(&mut e, &world, &nether(vec![player(DVec3::new(2.0, 10.0, 0.5))]), 3.0);
        assert!(events.iter().any(|e| matches!(e, EntityEvent::PlayerHit { damage, .. } if *damage == 0.5)));
        // Knockback resistance: a hoglin barely budges.
        let mut e = Entities::new(5);
        hoglin_at(&mut e, 0.5, false);
        e.mobs.push(Mob::new(MobKind::Pig, DVec3::new(9.5, 10.0, 0.5), 0.0));
        e.attack(0, DVec3::X, 1.0);
        e.attack(1, DVec3::X, 1.0);
        assert!((e.mobs[0].vel.x - e.mobs[1].vel.x * 0.4).abs() < 1e-6);
    }

    #[test]
    fn hoglins_shun_portals_and_outnumbering_piglins() {
        assert!(is_hoglin_repellent(Block::NETHER_PORTAL) && !is_hoglin_repellent(Block::STONE));
        assert!(is_hoglin_repellent(crate::world::nether_biome_blocks::WARPED_FUNGUS));
        let mut world = Grid::flat(10);
        world.set(IVec3::new(-3, 10, 0), Block::NETHER_PORTAL);
        let mut e = Entities::new(6);
        hoglin_at(&mut e, 0.5, false);
        let events = run(&mut e, &world, &nether(vec![player(DVec3::new(3.5, 10.0, 0.5))]), 4.0);
        assert_eq!(hits(&events), 0, "pacified by the portal");
        assert!(e.mobs[0].pos.x > 1.5, "backed away from it: {}", e.mobs[0].pos.x);

        let world = Grid::flat(10);
        let mut e = Entities::new(6);
        hoglin_at(&mut e, 0.5, false);
        for z in [3.0, 5.0] {
            e.mobs.push(Mob::new(MobKind::Piglin, DVec3::new(4.5, 10.0, z), 0.0));
        }
        let mut gold = player(DVec3::new(30.0, 10.0, 0.5));
        gold.gold_armor = true;
        run(&mut e, &world, &nether(vec![gold]), 3.0);
        assert!(e.mobs[0].pos.x < 0.0, "ran from two piglins: {}", e.mobs[0].pos.x);
    }

    #[test]
    fn hit_hoglins_rally_the_herd_and_babies_run() {
        let world = Grid::flat(10);
        let mut far = player(DVec3::new(40.0, 10.0, 0.5));
        far.targetable = true;
        let ctx = nether(vec![far]);
        let mut e = Entities::new(7);
        let a = hoglin_at(&mut e, 0.5, false);
        let b = hoglin_at(&mut e, 6.5, false);
        let c = hoglin_at(&mut e, -6.5, true);
        e.update(0.05, &world, &ctx);
        e.attack(a, DVec3::X, 1.0);
        assert_eq!(e.mobs[a].nether.as_ref().unwrap().foe, Some(Foe::Player(PlayerId::HOST)));
        assert_eq!(e.mobs[b].nether.as_ref().unwrap().foe, Some(Foe::Player(PlayerId::HOST)));
        assert_eq!(e.mobs[c].nether.as_ref().unwrap().foe, None, "babies aren't rallied");
        e.attack(c, DVec3::X, 1.0);
        assert!(e.mobs[c].nether.as_ref().unwrap().flee_left >= RETREAT.0);
    }

    #[test]
    fn hoglins_become_zoglins_that_attack_anything() {
        let world = Grid::flat(10);
        let mut far = player(DVec3::new(60.0, 10.0, 0.5));
        far.targetable = false;
        let overworld = Ctx { dimension: Dimension::Overworld, ..nether(vec![far]) };
        let mut e = Entities::new(8);
        hoglin_at(&mut e, 0.5, false);
        run(&mut e, &world, &overworld, 15.2);
        assert_eq!(e.mobs[0].kind, MobKind::Zoglin);
        assert!(MobKind::Zoglin.fire_immune() && !MobKind::Hoglin.fire_immune());
        assert_eq!(MobKind::Zoglin.creature(), crate::enchant::Creature::Undead);
        e.mobs.push(Mob::new(MobKind::Cow, DVec3::new(3.5, 10.0, 0.5), 0.0));
        e.mobs.push(Mob::new(MobKind::Creeper, DVec3::new(-2.5, 10.0, 0.5), 0.0));
        run(&mut e, &world, &overworld, 4.0);
        let cow = e.mobs.iter().find(|m| m.kind == MobKind::Cow).unwrap();
        assert!(cow.health < MobKind::Cow.max_health(), "the cow was gored");
        let creeper = e.mobs.iter().find(|m| m.kind == MobKind::Creeper).unwrap();
        assert_eq!(creeper.health, MobKind::Creeper.max_health(), "creepers are left alone");
    }

    #[test]
    fn piglins_hunt_adult_hoglins_but_not_babies_or_stable_ones() {
        let world = Grid::flat(10);
        let mut gold = player(DVec3::new(40.0, 10.0, 0.5));
        gold.gold_armor = true;
        let ctx = nether(vec![gold]);
        let mut e = Entities::new(9);
        let hunter = swordsman(&mut e);
        let friend = swordsman(&mut e);
        e.mobs[friend].pos.z = 4.0;
        let prey = hoglin_at(&mut e, 6.5, false);
        run(&mut e, &world, &ctx, 1.0);
        let uid = e.mobs[prey].uid;
        assert_eq!(e.mobs[hunter].nether.as_ref().unwrap().foe, Some(Foe::Mob(uid)));
        assert_eq!(e.mobs[friend].nether.as_ref().unwrap().foe, Some(Foe::Mob(uid)), "joins the hunt");
        assert!(
            e.mobs
                .iter()
                .filter(|m| m.kind == MobKind::Piglin)
                .all(|m| m.nether.as_ref().unwrap().hunt_cooldown >= 29.0)
        );

        let mut baby_only = Entities::new(9);
        let hunter = swordsman(&mut baby_only);
        let baby = hoglin_at(&mut baby_only, 6.5, true);
        run(&mut baby_only, &world, &ctx, 2.0);
        assert_eq!(baby_only.mobs[hunter].nether.as_ref().unwrap().foe, None);
        assert_eq!(baby_only.mobs[baby].health, 40.0);

        let mut e = Entities::new(9);
        let hunter = swordsman(&mut e);
        let prey = hoglin_at(&mut e, 6.5, false);
        e.mobs[prey].nether.as_mut().unwrap().cannot_hunt = true;
        run(&mut e, &world, &ctx, 2.0);
        assert_eq!(e.mobs[hunter].nether.as_ref().unwrap().foe, None);
        assert_eq!(e.mobs[prey].health, 40.0);
    }

    #[test]
    fn hoglin_loot_experience_and_spawn_rules() {
        let mut rng = Rng::new(3);
        for _ in 0..200 {
            for (item, n) in MobKind::Hoglin.drops(&mut rng, 0) {
                match item {
                    Item::RAW_PORKCHOP => assert!((2..=4).contains(&n)),
                    Item::LEATHER => assert_eq!(n, 1),
                    other => panic!("{other:?}"),
                }
            }
            assert!(
                MobKind::Zoglin
                    .drops(&mut rng, 0)
                    .iter()
                    .all(|&(i, n)| i == Item::ROTTEN_FLESH && (1..=3).contains(&n))
            );
        }
        assert_eq!(
            (xp(MobKind::Hoglin, false), xp(MobKind::Hoglin, true), xp(MobKind::Zoglin, false)),
            (Some(5), Some(3), Some(5))
        );
        let mut e = Entities::new(4);
        let baby = hoglin_at(&mut e, 0.5, true);
        e.attack(baby, DVec3::X, 100.0);
        e.drop_loot(MobKind::Hoglin, e.mobs[baby].pos);
        assert!(e.items.is_empty(), "baby hoglins drop nothing");
        let mut e = Entities::new(4);
        hoglin_at(&mut e, 0.5, false);
        e.mobs[0].burning = true;
        e.attack(0, DVec3::X, 100.0);
        e.drop_loot_with_fire(MobKind::Hoglin, e.mobs[0].pos, 0, true, true);
        assert!(e.items.iter().any(|i| i.stack.item == Item::COOKED_PORKCHOP));
        assert_eq!(breeding_item(MobKind::Hoglin), Some("crimson fungus"));
        let mut e = Entities::new(5);
        let babies = (0..1000)
            .filter(|_| {
                e.spawn(MobKind::Hoglin, DVec3::ZERO);
                e.mobs.last().unwrap().baby
            })
            .count();
        assert!((150..250).contains(&babies), "{babies}");
    }

    #[test]
    fn bastion_stables_pen_hoglins_that_cannot_be_hunted() {
        use crate::world::bastion::{Bastion, Kind};
        let b = Bastion::generate(5, glam::IVec2::new(0, 0), Kind::Stables, crate::world::block::Facing::East);
        let world = Remnant(Grid::flat(34), std::sync::Arc::new(b));
        let mut far = player(DVec3::new(0.0, 34.0, -60.0));
        far.targetable = false;
        let mut e = Entities::new(8);
        e.update(0.05, &world, &nether(vec![far]));
        let hoglins: Vec<_> = e.mobs.iter().filter(|m| m.kind == MobKind::Hoglin).collect();
        assert!(hoglins.len() >= 2, "{}", hoglins.len());
        assert!(hoglins.iter().all(|m| m.nether.as_ref().unwrap().cannot_hunt && m.persistent));
    }

    /// Stone floor at y < 10 with a lava lake (its surface at y = 9) over
    /// x, z in -6..=6, and dry land around it.
    fn lava_lake() -> Grid {
        let mut world = Grid::flat(10);
        for x in -6..=6 {
            for z in -6..=6 {
                world.set(IVec3::new(x, 9, z), Block::LAVA);
                world.set(IVec3::new(x, 8, z), Block::LAVA);
            }
        }
        world
    }

    fn strider_at(e: &mut Entities, pos: DVec3) -> usize {
        e.mobs.push(Mob::new(MobKind::Strider, pos, 0.0));
        e.mobs.len() - 1
    }

    #[test]
    fn striders_walk_on_lava_and_rise_out_of_it() {
        let world = lava_lake();
        let ctx = nether(vec![player(DVec3::new(60.0, 10.0, 0.5))]);
        let mut e = Entities::new(3);
        strider_at(&mut e, DVec3::new(0.5, 10.0, 0.5));
        strider_at(&mut e, DVec3::new(2.5, 8.2, 2.5));
        run(&mut e, &world, &ctx, 20.0);
        for m in &e.mobs {
            assert!((m.pos.y - 10.0).abs() < 0.05, "standing on the surface: {}", m.pos.y);
            assert!(m.pos.x.abs() < 7.0 && m.pos.z.abs() < 7.0, "stays on the lake: {:?}", m.pos);
            assert_eq!(m.health, 20.0, "lava doesn't hurt a strider");
            assert!(!m.nether.as_ref().unwrap().cold);
            assert!(!m.burning);
        }
    }

    #[test]
    fn cold_striders_shiver_slow_down_and_head_for_lava() {
        let world = lava_lake();
        let ctx = nether(vec![player(DVec3::new(60.0, 10.0, 0.5))]);
        let mut e = Entities::new(4);
        let i = strider_at(&mut e, DVec3::new(11.5, 10.0, 0.5));
        e.update(0.05, &world, &ctx);
        assert!(e.mobs[i].nether.as_ref().unwrap().cold);

        run(&mut e, &world, &ctx, 15.0);
        assert!(!e.mobs[i].nether.as_ref().unwrap().cold, "found the lake: {:?}", e.mobs[i].pos);
        // Water and rain hurt them.
        let mut wet = Grid::flat(10);
        wet.set(IVec3::new(0, 10, 0), Block::WATER);
        let mut e = Entities::new(4);
        strider_at(&mut e, DVec3::new(0.5, 10.0, 0.5));
        run(&mut e, &wet, &ctx, 1.0);
        assert!(e.mobs[0].health < 20.0);
        assert!(MobKind::Strider.fire_immune() && !MobKind::Strider.is_hostile());
    }

    #[test]
    fn striders_spawn_on_lava_seas_and_drop_string() {
        let mut world = Grid::flat(20);
        let sea = crate::world::nether::LAVA_SEA;
        for y in 20..=sea {
            world.set(IVec3::new(3, y, 4), Block::LAVA);
        }
        assert_eq!(lava_spot(&world, 3, 4), Some(DVec3::new(3.5, sea as f64 + 1.0, 4.5)));
        assert_eq!(lava_spot(&world, 0, 0), None, "no lava, no strider");
        world.set(IVec3::new(3, sea + 1, 4), Block::STONE);
        assert_eq!(lava_spot(&world, 3, 4), None, "needs open air above the lava");
        assert!(MobKind::Strider.spawns_in(Dimension::Nether) && !MobKind::Strider.spawns_in(Dimension::Overworld));
        let mut rng = Rng::new(6);
        for _ in 0..100 {
            let drops = MobKind::Strider.drops(&mut rng, 0);
            assert!(drops.iter().all(|&(i, n)| i == Item::STRING && (2..=5).contains(&n)), "{drops:?}");
        }
        assert_eq!(xp(MobKind::Strider, true), Some(0));
        assert_eq!(xp(MobKind::Strider, false), None, "adults give the usual 1-3");
        assert_eq!(breeding_item(MobKind::Strider), Some("warped fungus"));
    }

    #[test]
    fn crying_obsidian_is_bartered_and_behaves_like_javas() {
        use crate::item::{Tier, ToolKind};
        let crying = Block::CRYING_OBSIDIAN;
        assert_eq!(Item::from_name("crying obsidian"), Some(Item::from_block(crying)));
        assert_eq!((crying.hardness(), crying.emission()), (50.0, 10));
        assert!(!crate::mining::can_harvest(crying, Some(Item::tool(ToolKind::Pickaxe, Tier::Iron))));
        assert!(crate::mining::can_harvest(crying, Some(Item::tool(ToolKind::Pickaxe, Tier::Diamond))));
        assert!(Block::creative_palette().any(|b| b == crying));
        assert!(BARTERING.contains(&(Barter::Item(Item::from_block(crying)), 40, 1, 3)));
    }
}
