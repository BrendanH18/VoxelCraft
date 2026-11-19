//! Persistent villagers, POI claims, daily work/rest and atomic merchant transactions.
//! Java 26.3's trades are in `villager_trades` ([`trades`]); offers naming items
//! the game lacks are redrawn like ones Java discards.
use crate::inventory::{Inventory, SLOTS, Stack, move_into, stack_from_str, stack_to_string};
use crate::item::Item;
use crate::world::block::Block;
use glam::{DVec3, IVec3};
use serde_json::{Value, json};
fn hash(mut seed: u64) -> u64 {
    crate::world::noise::splitmix64(&mut seed)
}
use super::{Entities, EntityEvent, Mob, MobKind, MobWorld};
#[path = "villager_trades.rs"]
pub mod trades;
use trades::TradeDef;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Profession {
    #[default]
    None,
    Nitwit,
    Farmer,
    Fisherman,
    Shepherd,
    Fletcher,
    Librarian,
    Cartographer,
    Cleric,
    Armorer,
    Weaponsmith,
    Toolsmith,
    Butcher,
    Leatherworker,
    Mason,
}
impl Profession {
    pub const ALL: [Self; 15] = [
        Self::None,
        Self::Nitwit,
        Self::Farmer,
        Self::Fisherman,
        Self::Shepherd,
        Self::Fletcher,
        Self::Librarian,
        Self::Cartographer,
        Self::Cleric,
        Self::Armorer,
        Self::Weaponsmith,
        Self::Toolsmith,
        Self::Butcher,
        Self::Leatherworker,
        Self::Mason,
    ];
    pub fn name(self) -> &'static str {
        match self {
            Self::None => "Unemployed",
            Self::Nitwit => "Nitwit",
            Self::Farmer => "Farmer",
            Self::Fisherman => "Fisherman",
            Self::Shepherd => "Shepherd",
            Self::Fletcher => "Fletcher",
            Self::Librarian => "Librarian",
            Self::Cartographer => "Cartographer",
            Self::Cleric => "Cleric",
            Self::Armorer => "Armorer",
            Self::Weaponsmith => "Weaponsmith",
            Self::Toolsmith => "Toolsmith",
            Self::Butcher => "Butcher",
            Self::Leatherworker => "Leatherworker",
            Self::Mason => "Mason",
        }
    }
    pub fn of(b: Block) -> Option<Self> {
        Some(match b.base() {
            b if crate::world::composter::level(b).is_some() => Self::Farmer,
            Block::BARREL => Self::Fisherman,
            Block::LOOM => Self::Shepherd,
            Block::FLETCHING_TABLE => Self::Fletcher,
            Block::LECTERN => Self::Librarian,
            Block::CARTOGRAPHY_TABLE => Self::Cartographer,
            Block::BREWING_STAND => Self::Cleric,
            Block::BLAST_FURNACE | Block::LIT_BLAST_FURNACE => Self::Armorer,
            Block::GRINDSTONE => Self::Weaponsmith,
            Block::SMITHING_TABLE => Self::Toolsmith,
            Block::SMOKER | Block::LIT_SMOKER => Self::Butcher,
            Block::STONECUTTER => Self::Mason,
            _ => return None,
        })
    }
}
pub fn is_poi(b: Block) -> bool {
    b.is_bed_head()
        || b.base() == Block::BELL
        || (b == Block::BREWING_STAND
            || b == Block::SMITHING_TABLE
            || ((904..=948).contains(&b.0) || crate::world::composter::level(b).is_some()))
            && Profession::of(b).is_some()
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Offer {
    pub cost: Stack,
    pub second: Option<Stack>,
    pub output: Stack,
    pub uses: u8,
    pub max_uses: u8,
    pub xp: u16,
    pub demand: i16,
    pub multiplier: f32,
}
impl Offer {
    /// The first cost after demand (`MerchantOffer.getCostA` with no special price).
    pub fn price(self) -> Stack {
        self.price_with(0)
    }
    /// `MerchantOffer.getModifiedCostCount`: the base price plus
    /// `max(0, floor(base * demand * priceMultiplier))` plus the special
    /// price difference, clamped once to `1..=max stack`.
    pub fn price_with(self, special: i32) -> Stack {
        let base = self.cost.count as i32;
        let extra = ((base as f32 * self.demand as f32 * self.multiplier).floor() as i32).max(0);
        Stack { count: (base + extra + special).clamp(1, self.cost.item.max_stack() as i32) as u8, ..self.cost }
    }
    /// `MerchantOffer.updateDemand`: `demand + uses - (maxUses - uses)`.
    pub fn update_demand(&mut self) {
        self.demand = self.demand.saturating_add(2 * self.uses as i16 - self.max_uses as i16);
    }
    pub fn stocked(self) -> bool {
        self.uses < self.max_uses
    }
    fn save(self) -> Value {
        json!([
            stack_to_string(Some(self.cost)),
            stack_to_string(self.second),
            stack_to_string(Some(self.output)),
            self.uses,
            self.max_uses,
            self.xp,
            self.demand,
            self.multiplier
        ])
    }
    fn load(v: &Value) -> Option<Self> {
        let a = v.as_array()?;
        if a.len() != 8 {
            return None;
        }
        let out = Self {
            cost: stack_from_str(a[0].as_str()?)??,
            second: stack_from_str(a[1].as_str()?)?,
            output: stack_from_str(a[2].as_str()?)??,
            uses: u8::try_from(a[3].as_u64()?).ok()?,
            max_uses: u8::try_from(a[4].as_u64()?).ok()?,
            xp: u16::try_from(a[5].as_u64()?).ok()?,
            demand: i16::try_from(a[6].as_i64()?).ok()?,
            multiplier: a[7].as_f64()? as f32,
        };
        (out.max_uses > 0
            && out.uses <= out.max_uses
            && out.multiplier.is_finite()
            && (0.0..=1.0).contains(&out.multiplier))
        .then_some(out)
    }
}
#[derive(Clone, Debug)]
pub struct Villager {
    pub id: u64,
    pub profession: Profession,
    pub level: u8,
    pub xp: u16,
    pub job: Option<IVec3>,
    pub home: Option<IVec3>,
    pub offers: [Option<Offer>; 10],
    pub sleeping: bool,
    pub trading: bool,
    pub goal: Option<DVec3>,
    pub fleeing: bool,
    pub(crate) growth: f32,
    pub(crate) active: bool,
    /// Host-only discount retained for compatibility with old saves.
    pub reputation: i16,
    pub(super) last_slept: Option<i64>,
    pub(super) golem_seen: f32,
    pub(super) food: [Option<Stack>; 8],
    pub(super) food_level: u8,
    pub(super) courtship: u16,
    pub(crate) bell_hide: f32,
    pub wandering: bool,
    /// Biome variant used by Java 26.3 cartographer predicates.
    pub map_variant: Option<String>,
    pub(super) gossip: super::villager_gossip::Gossip,
    /// Java's `MEETING_POINT` memory: the bell this villager meets at.
    pub meeting: Option<IVec3>,
    /// Java `lastGossipTime` (absolute ticks); gossip waits 1200 ticks.
    pub(super) last_gossip: Option<i64>,
    /// Java `WorkAtPoi.lastCheck` (absolute ticks): a work check every 300.
    pub(super) work_check: Option<i64>,
    seed: u64,
    /// Java `lastRestockCheckDay`: the day the last restock check saw.
    restock_day: i64,
    /// Java `numberOfRestocksToday`.
    restocks: u8,
    /// Java `lastRestockGameTime` (absolute ticks).
    last_restock: i64,
    /// v0.15 Track E: players with Hero of the Village and its amplifier,
    /// copied in by the raid tick (not saved); [`Villager::priced_for`]
    /// passes the owner's to [`Villager::priced_with_hero`].
    pub heroes: Vec<(super::PlayerId, u8)>,
}
impl Mob {
    /// What a LAN client's trading screen shows: merchant id, profession, level, xp and offers.
    pub fn trade_view(&self) -> Option<Value> {
        let v = self.villager.as_ref()?;
        Some(json!({
            "id": v.id,
            "profession": Profession::ALL.iter().position(|p| *p == v.profession).unwrap_or(0),
            "level": v.level,
            "xp": v.xp,
            "wandering": v.wandering,
            "offers": v.offers.map(|o| o.map(Offer::save)),
        }))
    }
    /// Applies [`Mob::trade_view`] to a client's copy of the merchant.
    pub fn apply_trade_view(&mut self, t: &Value) {
        let Some(v) = self.villager.as_mut() else { return };
        v.id = t["id"].as_u64().unwrap_or(v.id);
        v.profession =
            t["profession"].as_u64().and_then(|i| Profession::ALL.get(i as usize).copied()).unwrap_or(v.profession);
        v.level = t["level"].as_u64().unwrap_or(1).clamp(1, 5) as u8;
        v.xp = t["xp"].as_u64().unwrap_or(0).min(u16::MAX as u64) as u16;
        v.wandering = t["wandering"] == true;
        if let Some(offers) = t["offers"].as_array() {
            for (slot, o) in v.offers.iter_mut().zip(offers) {
                *slot = Offer::load(o);
            }
        }
    }
}

impl Villager {
    pub fn new(id: u64, seed: u64) -> Self {
        Self {
            id,
            profession: Profession::None,
            level: 1,
            xp: 0,
            job: None,
            home: None,
            offers: [None; 10],
            sleeping: false,
            trading: false,
            goal: None,
            fleeing: false,
            seed,
            growth: 0.0,
            active: true,
            reputation: 0,
            last_slept: None,
            golem_seen: 0.0,
            food: [None; 8],
            food_level: 0,
            courtship: 0,
            bell_hide: 0.0,
            wandering: false,
            map_variant: None,
            gossip: Default::default(),
            meeting: None,
            last_gossip: None,
            work_check: None,
            restock_day: 0,
            restocks: 0,
            last_restock: 0,
            heroes: Vec::new(),
        }
    }
    pub fn observation(&self) -> Value {
        self.observation_for(super::PlayerId::HOST)
    }
    pub fn observation_for(&self, owner: super::PlayerId) -> Value {
        json!({"id":self.id,"profession":self.profession.name(),"level":self.level,"rank":self.level_name(),"xp":self.xp,
            "job":self.job.map(|p|p.to_array()),"home":self.home.map(|p|p.to_array()),
            "offers":self.offers.iter().enumerate().filter_map(|(i,o)|o.map(|o|json!({"offer":i+1,
                "cost":{"item":o.cost.item.name(),"count":self.priced_for(o,owner).count},"second":o.second.map(|s|json!({"item":s.item.name(),"count":s.count})),"result":{"item":o.output.item.name(),"count":o.output.count,"stack":stack_to_string(Some(o.output))},
                "uses":o.uses,"max_uses":o.max_uses,"stocked":o.stocked(),"villager_xp":o.xp}))).collect::<Vec<_>>()})
    }
    pub fn level_name(&self) -> &'static str {
        ["Novice", "Apprentice", "Journeyman", "Expert", "Master"][(self.level - 1) as usize]
    }
    /// `price` after the reputation discount, for the host.
    pub fn priced(&self, offer: Offer) -> Stack {
        self.priced_for(offer, super::PlayerId::HOST)
    }
    /// Java `getPlayerReputation`: the gossip total (plus the host-only legacy discount).
    pub fn reputation_of(&self, owner: super::PlayerId) -> i16 {
        self.gossip.reputation(owner) + if owner == super::PlayerId::HOST { self.reputation } else { 0 }
    }
    /// What `owner` pays: [`Self::priced_with_hero`] with the owner's Hero of
    /// the Village level, which the raid tick copies into [`Self::heroes`].
    pub fn priced_for(&self, offer: Offer, owner: super::PlayerId) -> Stack {
        let hero = self.heroes.iter().find(|h| h.0 == owner).map(|h| h.1);
        self.priced_with_hero(offer, owner, hero)
    }
    /// Java `updateSpecialPrices` then `getCostA`: the reputation and Hero
    /// of the Village (`hero` is its amplifier) discounts become the offer's
    /// special price difference. Wandering traders have none.
    pub fn priced_with_hero(&self, offer: Offer, owner: super::PlayerId, hero: Option<u8>) -> Stack {
        if self.wandering {
            return offer.price();
        }
        offer.price_with(special_price_diff(offer, self.reputation_of(owner) as i32, hero))
    }
    pub fn set_profession(&mut self, p: Profession) {
        if self.profession != p {
            self.profession = p;
            self.level = 1;
            self.xp = 0;
            self.offers = [None; 10];
            self.unlock();
        }
    }
    /// The villager type (`VillagerType`), from its biome: `plains` until known.
    pub fn variant(&self) -> &str {
        self.map_variant.as_deref().unwrap_or("plains")
    }
    /// `AbstractVillager.addOffersFromTradeSet` without duplicates: draws
    /// the level's `amount` offers from its tag, removing each entry it
    /// tries; an entry that yields no offer (wrong villager type, discarded
    /// result, or an item the game lacks) is skipped and another drawn.
    /// Offers append after the last one, as Java's offer list does.
    fn unlock(&mut self) {
        let seed = hash(self.seed ^ ((self.profession as u64) << 32) ^ self.level as u64);
        let mut rng = crate::enchant::JavaRandom::new(seed as i64);
        let mut left: Vec<&TradeDef> = trades::TRADES
            .iter()
            .filter(|(p, l, _)| *p == self.profession && *l == self.level)
            .map(|(_, _, d)| d)
            .collect();
        let mut found = 0;
        let want = trades::amount(self.profession, self.level);
        while found < want && !left.is_empty() {
            let d = left.remove(rng.next_bounded(left.len() as i32) as usize);
            if let Some(offer) = d.offer(hash(seed ^ rng.next_int() as u64), self.variant()) {
                self.push_offer(offer);
                found += 1;
            }
        }
    }
    /// Appends after the last offer; offers past the tenth are dropped.
    pub(super) fn push_offer(&mut self, offer: Offer) {
        let next = self.offers.iter().rposition(Option::is_some).map_or(0, |i| i + 1);
        if let Some(slot) = self.offers.get_mut(next) {
            *slot = Some(offer);
        }
    }
    /// Java `shouldRestock` and `restock`, run when `WorkAtPoi` starts at
    /// the job site (`now` is the absolute tick, `day` the day number). A
    /// new day (12000 ticks since the last restock, or a later day) first
    /// catches up the demand updates missed that day (`catchUpDemand`).
    /// Then a restock needs a used offer and either no restock yet today or
    /// fewer than two and 2400 ticks since the last.
    pub fn restock(&mut self, now: i64, day: i64) -> bool {
        let mut new_day = now > self.last_restock + 12000;
        new_day |= self.restock_day > 0 && day > self.restock_day;
        self.restock_day = day;
        if new_day {
            self.last_restock = now;
            self.catch_up_demand();
            self.restocks = 0;
        }
        let allowed = self.restocks == 0 || self.restocks < 2 && now > self.last_restock + 2400;
        if !allowed || !self.offers.iter().flatten().any(|o| o.uses > 0) {
            return false;
        }
        for o in self.offers.iter_mut().flatten() {
            o.update_demand();
            o.uses = 0;
        }
        self.last_restock = now;
        self.restocks += 1;
        true
    }
    /// Java `catchUpDemand`: the restocks a day missed still update demand.
    fn catch_up_demand(&mut self) {
        let missed = 2 - self.restocks.min(2) as i32;
        if missed > 0 {
            for o in self.offers.iter_mut().flatten() {
                o.uses = 0;
            }
        }
        for _ in 0..missed {
            for o in self.offers.iter_mut().flatten() {
                o.update_demand();
            }
        }
    }
    /// All costs and the result must fit on a scratch inventory before any state changes.
    pub fn trade(&mut self, index: usize, inv: &mut Inventory) -> Result<u32, &'static str> {
        self.trade_for(index, inv, super::PlayerId::HOST)
    }
    pub fn trade_for(
        &mut self,
        index: usize,
        inv: &mut Inventory,
        owner: super::PlayerId,
    ) -> Result<u32, &'static str> {
        let offer = self.offers.get(index).copied().flatten().ok_or("no such offer")?;
        if !offer.stocked() {
            return Err("offer out of stock");
        }
        // An explorer map whose destination `prepare_merchant_maps` hasn't
        // found yet (an offer unlocked while the screen was open) isn't sold.
        if offer.output.item.is_map() && offer.output.item != Item::MAP && offer.output.map_id.is_none() {
            return Err("map not ready");
        }
        let mut slots = inv.slots;
        for cost in [Some(self.priced_for(offer, owner)), offer.second].into_iter().flatten() {
            let mut left = cost.count;
            for s in &mut slots {
                if let Some(stack) = s
                    && stack.item == cost.item
                    && stack.enchants == cost.enchants
                    && stack.damage == 0
                {
                    let n = left.min(stack.count);
                    left -= n;
                    stack.count -= n;
                    if stack.count == 0 {
                        *s = None
                    }
                    if left == 0 {
                        break;
                    }
                }
            }
            if left > 0 {
                return Err("missing trade payment");
            }
        }
        const ORDER: [usize; SLOTS] = {
            let mut a = [0; SLOTS];
            let mut i = 0;
            while i < SLOTS {
                a[i] = i;
                i += 1;
            }
            a
        };
        if move_into(offer.output, &mut slots, &ORDER).is_some() {
            return Err("inventory full");
        }
        inv.slots = slots;
        self.offers[index].as_mut().unwrap().uses += 1;
        if self.wandering {
            return Ok(3 + (hash(self.seed ^ self.offers[index].unwrap().uses as u64) % 4) as u32);
        }
        // Java's `TRADE` reputation event: two trading gossip.
        self.gossip.add(owner, super::villager_gossip::TRADING, 2);
        self.xp = self.xp.saturating_add(offer.xp);
        // Java 26.3 `rewardTradeXp`: crossing the level's threshold promotes
        // at once (`increaseMerchantCareer`) and adds 5 XP to the orb; the
        // caller grants the Regeneration (`Entities::trade_for`).
        let promoted = self.level < 5 && self.xp >= LEVEL_XP[self.level as usize];
        if promoted {
            self.level += 1;
            self.unlock();
        }
        Ok(3 + (hash(self.seed ^ self.xp as u64 ^ self.offers[index].unwrap().uses as u64) % 4) as u32
            + if promoted { 5 } else { 0 })
    }
}

/// Java `Villager.updateSpecialPrices` for one offer: `-floor(reputation *
/// priceMultiplier)` when the reputation is not zero, and for Hero of the
/// Village `-max(floor((0.3 + 0.0625 * amplifier) * base cost), 1)`.
pub fn special_price_diff(offer: Offer, reputation: i32, hero: Option<u8>) -> i32 {
    let mut diff = 0;
    if reputation != 0 {
        diff -= (reputation as f32 * offer.multiplier).floor() as i32;
    }
    diff -= super::raid::hero_price_cut(offer.cost.count, hero);
    diff
}

/// `PoiTypes.MEETING`'s tickets: villagers that can share one bell.
pub const MEETING_TICKETS: u8 = 32;
/// `getMeetPackage`: villagers walk until this close to their bell.
const MEETING_CLOSE: f64 = 6.0;
/// `WorkAtPoi.CHECK_COOLDOWN` (ticks) and `DISTANCE` (blocks from the
/// job site's centre).
pub const WORK_CHECK_COOLDOWN: i64 = 300;
pub const WORK_DISTANCE: f64 = 1.73;

/// The villager brain activities the schedules choose between.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Activity {
    Idle,
    Work,
    Meet,
    Rest,
    Play,
}
/// Java 26.3's `timeline/villager_schedule.json`: each track's keyframes
/// (tick of the day, activity); an activity lasts until the next keyframe,
/// and the last one wraps past midnight to the first.
pub const ADULT_SCHEDULE: [(u32, Activity); 5] = [
    (10, Activity::Idle),
    (2000, Activity::Work),
    (9000, Activity::Meet),
    (11000, Activity::Idle),
    (12000, Activity::Rest),
];
pub const BABY_SCHEDULE: [(u32, Activity); 5] = [
    (10, Activity::Idle),
    (3000, Activity::Play),
    (6000, Activity::Idle),
    (10000, Activity::Play),
    (12000, Activity::Rest),
];
/// The scheduled activity at `tick` (0..24000) of the day.
pub fn schedule_activity(tick: u32, baby: bool) -> Activity {
    let track = if baby { &BABY_SCHEDULE } else { &ADULT_SCHEDULE };
    track.iter().rev().find(|(at, _)| *at <= tick).unwrap_or(&track[track.len() - 1]).1
}

/// `VillagerData.NEXT_LEVEL_XP_THRESHOLDS`: the XP that ends each level.
pub const LEVEL_XP: [u16; 5] = [0, 10, 70, 150, 250];

/// How close (blocks) a farmer must be to cut a crop.
const HARVEST_REACH: f64 = 1.5;
/// Farmers look for ripe crops this far from their job site.
const HARVEST_RADIUS: i32 = 5;
/// The ripe crop closest to `from` around a farmer's job site (wheat, carrots, potatoes and beetroot at their maximum age).
fn ripe_crop<W: MobWorld + ?Sized>(world: &W, job: IVec3, from: DVec3) -> Option<IVec3> {
    let mut best: Option<(f64, IVec3)> = None;
    for dy in -1..=1 {
        for dz in -HARVEST_RADIUS..=HARVEST_RADIUS {
            for dx in -HARVEST_RADIUS..=HARVEST_RADIUS {
                let cell = job + IVec3::new(dx, dy, dz);
                let ripe =
                    world.block(cell).is_some_and(|b| b.as_crop().is_some_and(|(crop, stage)| stage == crop.max_age()));
                if ripe && world.block(cell - IVec3::Y).is_some_and(|b| b.is_farmland()) {
                    let d = (cell.as_dvec3() + DVec3::new(0.5, 0.0, 0.5)).distance_squared(from);
                    if best.is_none_or(|(bd, _)| d < bd) {
                        best = Some((d, cell));
                    }
                }
            }
        }
    }
    best.map(|(_, cell)| cell)
}

/// Where a sleeping villager lies: the head half's centre on top of the mattress, turned so
/// the body stretches toward the foot (Java's `SLEEPING` pose; the renderer lays the head
/// toward the pillow). `None` if the bed's foot half cannot be found.
fn bed_pose<W: MobWorld + ?Sized>(world: &W, head: IVec3) -> Option<(DVec3, f32)> {
    let color = world.block(head)?.bed_color()?;
    let foot = crate::world::block::Block::colored_bed(color, false);
    let toward = crate::world::block::Facing::ALL
        .into_iter()
        .map(crate::world::block::Facing::offset)
        .find(|o| o.y == 0 && world.block(head + *o) == Some(foot))?;
    let at = DVec3::new(
        head.x as f64 + 0.5,
        head.y as f64 + crate::world::block::Block::BED_FOOT.height(),
        head.z as f64 + 0.5,
    );
    // The model lies with its head along (-sin yaw, cos yaw); the pillow end is away from the foot.
    Some((at, (toward.x as f32).atan2(-(toward.z as f32))))
}
impl Entities {
    /// Resolve deferred exploration modifiers before the merchant screen opens.
    pub fn prepare_merchant_maps(&mut self, world: &mut crate::world::World, id: u64) {
        let Some(m) = self.mobs.iter_mut().find(|m| m.villager.as_ref().is_some_and(|v| v.id == id)) else { return };
        let v = m.villager.as_mut().unwrap();
        if v.map_variant.is_none() {
            v.map_variant = Some(map_variant(world.generator.surface_biome(m.pos.x as i32, m.pos.z as i32)).into());
        }
        for slot in &mut v.offers {
            if let Some(offer) = slot
                && offer.output.item.is_map()
                && offer.output.item != Item::MAP
                && offer.output.map_id.is_none()
            {
                if !trades::map_allowed(offer.output.item, v.map_variant.as_deref().unwrap_or("plains")) {
                    *slot = None;
                    continue;
                }
                if let Some(stack) =
                    world.maps.explorer(&world.generator, m.pos.floor().as_ivec3(), offer.output.item, 100)
                {
                    offer.output = stack;
                } else {
                    *slot = None;
                }
            }
        }
    }
    pub fn merchant(&self, id: u64) -> Option<&Mob> {
        self.mobs.iter().find(|m| m.alive() && m.villager.as_ref().is_some_and(|v| v.id == id))
    }
    /// Java `AbstractVillager.stillValid`: alive, able to trade, and its box
    /// within `range` of the eye.
    pub fn merchant_in_reach(&self, id: u64, eye: DVec3, range: f64) -> bool {
        self.merchant(id).is_some_and(|m| {
            let (min, max) = m.aabb();
            matches!(m.kind, MobKind::Villager | MobKind::WanderingTrader)
                && crate::physics::box_distance_squared(eye, min, max) < range * range
                && m.age >= 0
                && !m.villager.as_ref().unwrap().sleeping
        })
    }
    pub fn trade(&mut self, id: u64, index: usize, inv: &mut Inventory) -> Result<u32, &'static str> {
        self.trade_for(id, index, inv, super::PlayerId::HOST)
    }
    pub fn trade_for(
        &mut self,
        id: u64,
        index: usize,
        inv: &mut Inventory,
        owner: super::PlayerId,
    ) -> Result<u32, &'static str> {
        let m = self
            .mobs
            .iter_mut()
            .find(|m| m.alive() && m.villager.as_ref().is_some_and(|v| v.id == id))
            .ok_or("villager gone")?;
        if !matches!(m.kind, MobKind::Villager | MobKind::WanderingTrader)
            || m.age < 0
            || m.villager.as_ref().unwrap().sleeping
        {
            return Err("villager cannot trade now");
        }
        let v = m.villager.as_mut().unwrap();
        let level = v.level;
        let xp = v.trade_for(index, inv, owner)?;
        // `rewardTradeXp`: a promotion comes with Regeneration for 200 ticks.
        if m.kind == MobKind::Villager && m.villager.as_ref().unwrap().level > level {
            m.effects.add(crate::simulation::effects::Effect::Regeneration, 0, 200);
        }
        Ok(xp)
    }
    pub fn target_merchant(&self, world: &crate::world::World, eye: DVec3, dir: DVec3, reach: f64) -> Option<u64> {
        let (i, t) = self.raycast(eye, dir, reach)?;
        // Use the shared voxel ray to prevent trading through walls.
        if world.raycast(eye, dir, reach).is_some_and(|h| {
            let hit = if world.get_block(h.0).is_some_and(|b| b.kind() == crate::world::block::RenderKind::Shaped) {
                crate::physics::ray_shape(world, h.0, eye, dir).map(|(hit, _)| hit)
            } else {
                crate::physics::ray_aabb(eye, dir, h.0.as_dvec3(), h.0.as_dvec3() + 1.0)
            };
            hit.is_some_and(|hit| hit < t)
        }) {
            return None;
        }
        let m = &self.mobs[i];
        let v = m.villager.as_ref()?;
        (matches!(m.kind, MobKind::Villager | MobKind::WanderingTrader) && m.age >= 0 && !v.sleeping).then_some(v.id)
    }
    /// v0.15 Track G: spawns each generation 4 village resident (Java's
    /// villager, cat, iron golem, animal and camel spawn pieces) once, when
    /// its cell ticks. The spawn cell joins `villager_births`, which is
    /// saved, so a resident never respawns; zombie villagers wait out
    /// Peaceful. Armor stands (the taiga armorer's) are not spawned.
    pub(super) fn populate_village_residents<W: MobWorld + ?Sized>(
        &mut self,
        world: &W,
        difficulty: crate::simulation::difficulty::Difficulty,
    ) {
        use crate::world::village::Resident as R;
        let mut due = Vec::new();
        world.village_residents(&mut |p, r| {
            if !self.villager_births.contains(&p) && world.entity_ticking(p) {
                due.push((p, r));
            }
        });
        for (p, r) in due {
            let kind = match r {
                R::Villager { .. } => MobKind::Villager,
                R::ZombieVillager { .. } => MobKind::ZombieVillager,
                R::Cat(_) => MobKind::Cat,
                R::IronGolem => MobKind::IronGolem,
                R::Cow => MobKind::Cow,
                R::Pig => MobKind::Pig,
                R::Sheep => MobKind::Sheep,
                R::Horse(_) => MobKind::Horse,
                R::Camel => MobKind::Camel,
                R::ArmorStand => {
                    self.villager_births.insert(p);
                    continue;
                }
            };
            if difficulty == crate::simulation::difficulty::Difficulty::Peaceful && kind.is_hostile() {
                continue;
            }
            self.villager_births.insert(p);
            self.spawn(kind, p.as_dvec3() + DVec3::new(0.5, 0.0, 0.5));
            let m = self.mobs.last_mut().unwrap();
            match r {
                R::Villager { nitwit, baby } => {
                    // The baby piece's villager is `Age -21359`.
                    if baby {
                        m.age = -21359;
                    }
                    if nitwit && let Some(v) = &mut m.villager {
                        v.set_profession(Profession::Nitwit);
                    }
                }
                R::ZombieVillager { nitwit } => {
                    m.persistent = true;
                    if nitwit && let Some(v) = &mut m.villager {
                        v.set_profession(Profession::Nitwit);
                    }
                }
                R::Cat(variant) => {
                    m.persistent = true;
                    if let Some(a) = &mut m.animal {
                        a.variant = variant;
                    }
                }
                R::Horse(variant) => {
                    m.persistent = true;
                    if let Some(s) = &mut m.mount {
                        s.variant = (variant & 0xFF).min(6) as u8;
                        s.marking = (variant >> 8).min(4) as u8;
                    }
                }
                R::Camel => {}
                _ => m.persistent = true,
            }
        }
    }

    pub(super) fn village_upkeep<W: MobWorld + ?Sized>(&mut self, dt: f32, world: &W) {
        self.village_timer -= dt;
        if self.village_timer > 0.0 {
            return;
        }
        self.village_timer = 1.0;
        world.village_homes(&mut |home, spawn| {
            if world.entity_ticking(spawn) && self.villager_births.insert(home) {
                self.spawn(MobKind::Villager, spawn.as_dvec3() + DVec3::new(0.5, 0.0, 0.5));
                let m = self.mobs.last_mut().unwrap();
                m.villager.as_mut().unwrap().home = Some(home);
                if hash(world.seed() ^ home.x as u64 ^ ((home.z as u64) << 32)).is_multiple_of(5) {
                    m.age = -24000;
                }
                // Java's desert village houses sometimes keep a camel; here one home in four.
                if world.biome(home.x, home.z) == crate::world::terrain::Biome::Desert
                    && hash(world.seed() ^ 0xCA3E1 ^ home.x as u64 ^ ((home.z as u64) << 32)).is_multiple_of(4)
                {
                    self.spawn(MobKind::Camel, spawn.as_dvec3() + DVec3::new(0.5, 0.0, 0.5));
                }
            }
        });
        self.mob_index.rebuild(&self.mobs);
        self.claimed_beds.clear();
        self.claimed_jobs.clear();
        self.claimed_meetings.clear();
        for m in &self.mobs {
            if m.alive()
                && m.kind == MobKind::Villager
                && let Some(v) = &m.villager
            {
                if let Some(p) = v.home {
                    self.claimed_beds.insert(p);
                }
                if let Some(p) = v.job {
                    self.claimed_jobs.insert(p);
                }
                if let Some(p) = v.meeting {
                    *self.claimed_meetings.entry(p).or_default() += 1;
                }
            }
        }
        let tick = (self.village_time.rem_euclid(1.0) * 24000.0) as u32;
        let now = self.village_day * 24000 + tick as i64;
        for i in 0..self.mobs.len() {
            let pos = self.mobs[i].pos;
            let threat = self
                .mob_index
                .nearest(&self.mobs, pos, 15.0, |o| {
                    super::illager::villager_fear(o.kind).is_some_and(|d| o.pos.distance_squared(pos) <= d * d)
                })
                .map(|j| self.mobs[j].pos);
            let m = &mut self.mobs[i];
            if !m.alive() || !world.entity_ticking(m.pos.floor().as_ivec3()) {
                continue;
            }
            let Some(v) = &mut m.villager else { continue };
            if v.map_variant.is_none() {
                v.map_variant = Some(map_variant(world.biome(m.pos.x as i32, m.pos.z as i32)).into());
            }
            v.gossip.maybe_decay(now);
            if m.kind != MobKind::Villager {
                continue;
            }

            // Java `ValidateNearbyPoi` then `ResetProfession`: losing the job
            // site fires only a villager that has never traded (no XP, level 1).
            if let Some(p) = v.job
                && world.loaded(p)
                && world.block(p).and_then(Profession::of) != Some(v.profession)
            {
                self.claimed_jobs.remove(&p);
                v.job = None;
                if v.xp == 0 && v.level <= 1 {
                    v.set_profession(Profession::None)
                }
            }
            if let Some(p) = v.home
                && world.loaded(p)
                && world.block(p).is_none_or(|b| !b.is_bed_head())
            {
                self.claimed_beds.remove(&p);
                v.home = None;
            }
            if let Some(p) = v.meeting
                && world.loaded(p)
                && world.block(p).is_none_or(|b| b.base() != Block::BELL)
            {
                v.meeting = None;
            }
            // Java `AcquirePoi` (`SCAN_RANGE` 48, closest first): beds at any
            // age, job sites and meeting bells (32 tickets each) for adults.
            let adult = m.age >= 0;
            let wants_job = v.job.is_none() && adult && v.profession != Profession::Nitwit;
            let wants_meeting = v.meeting.is_none() && adult;
            if v.home.is_none() || wants_job || wants_meeting {
                let mut home = None;
                let mut job = None;
                let mut meeting = None;
                let mut hd = 48.0f64.powi(2);
                let mut jd = hd;
                let mut md = hd;
                world.village_pois(&mut |p, b| {
                    let dist = m.pos.distance_squared(p.as_dvec3());
                    let prof = Profession::of(b);
                    if b.is_bed_head()
                        && (dist < hd || dist == hd && home.is_some_and(|h: IVec3| p.to_array() < h.to_array()))
                        && !self.claimed_beds.contains(&p)
                        && !self.storage.claims_bed(p)
                    {
                        hd = dist;
                        home = Some(p);
                    } else if b.base() == Block::BELL
                        && (dist < md || dist == md && meeting.is_some_and(|h: IVec3| p.to_array() < h.to_array()))
                        && self.claimed_meetings.get(&p).copied().unwrap_or(0) < MEETING_TICKETS
                    {
                        md = dist;
                        meeting = Some(p);
                    } else if let Some(prof) = prof
                        && (v.xp == 0 || prof == v.profession)
                        && (dist < jd
                            || dist == jd && job.is_some_and(|j: (IVec3, Profession)| p.to_array() < j.0.to_array()))
                        && !self.claimed_jobs.contains(&p)
                        && !self.storage.claims_job(p)
                    {
                        jd = dist;
                        job = Some((p, prof));
                    }
                });
                if v.home.is_none() {
                    v.home = home;
                    if let Some(p) = home {
                        self.claimed_beds.insert(p);
                    }
                }
                if wants_meeting && let Some(p) = meeting {
                    v.meeting = Some(p);
                    *self.claimed_meetings.entry(p).or_default() += 1;
                }
                if wants_job && let Some((p, prof)) = job {
                    v.job = Some(p);
                    self.claimed_jobs.insert(p);
                    v.set_profession(prof);
                }
            }
            let activity = schedule_activity(tick, m.age < 0);
            v.fleeing = threat.is_some();
            v.sleeping = false;
            v.goal = if let Some(z) = threat {
                let d = (m.pos - z) * DVec3::new(1.0, 0.0, 1.0);
                Some(m.pos + d.normalize_or_zero() * 10.0)
            } else if v.bell_hide > 0.0 {
                v.home.map(|p| p.as_dvec3() + DVec3::new(0.5, 0.6, 0.5))
            } else if v.trading {
                None
            } else {
                match activity {
                    Activity::Rest => v.home.map(|p| p.as_dvec3() + DVec3::new(0.5, 0.6, 0.5)),
                    Activity::Work if adult => v.job.map(|p| p.as_dvec3() + DVec3::new(0.5, 0.0, 0.5)),
                    // `SetWalkTargetFromBlockMemory(MEETING_POINT, .., 6, ..)`:
                    // walk until within six blocks of the bell.
                    Activity::Meet if adult => v
                        .meeting
                        .map(|p| p.as_dvec3() + DVec3::new(0.5, 0.0, 0.5))
                        .filter(|at| at.distance_squared(m.pos) > MEETING_CLOSE * MEETING_CLOSE),
                    _ => None,
                }
            };
            // Java's `HarvestFarmland`: farmers on shift cut ripe crops near their composter.
            if v.profession == Profession::Farmer
                && self.villager_griefing
                && m.age >= 0
                && !v.fleeing
                && !v.trading
                && v.bell_hide <= 0.0
                && activity == Activity::Work
                && let Some(job) = v.job
                && let Some(cell) = ripe_crop(world, job, m.pos)
            {
                let at = cell.as_dvec3() + DVec3::new(0.5, 0.0, 0.5);
                if ((at - m.pos) * DVec3::new(1.0, 0.0, 1.0)).length_squared() < HARVEST_REACH * HARVEST_REACH
                    && (at.y - m.pos.y).abs() < 2.0
                {
                    self.pending_sounds.push(EntityEvent::VillagerHarvest { cell });
                } else {
                    v.goal = Some(at);
                }
            }
            if activity == Activity::Rest
                && !v.fleeing
                && v.bell_hide <= 0.0
                && !v.trading
                && let Some(p) = v.home
            {
                v.sleeping = m.pos.distance_squared(p.as_dvec3() + DVec3::new(0.5, 0.6, 0.5)) < 2.25;
            }
            if v.sleeping {
                v.last_slept = Some(self.village_day * 24000 + tick as i64);
                if let Some(bed) = v.home
                    && let Some((at, yaw)) = bed_pose(world, bed)
                {
                    m.pos = at;
                    m.previous_pos = at;
                    m.yaw = yaw;
                    m.vel = DVec3::ZERO;
                }
            }
            // Java `WorkAtPoi`: while working, at most every 300 ticks and on a
            // coin flip, a villager within 1.73 blocks of its job site's centre
            // works it, which restocks when `shouldRestock` allows.
            if activity == Activity::Work
                && !v.fleeing
                && !v.sleeping
                && v.work_check.is_none_or(|t| now - t >= WORK_CHECK_COOLDOWN)
                && hash(v.seed ^ now as u64).is_multiple_of(2)
            {
                v.work_check = Some(now);
                if v.job.is_some_and(|p| m.pos.distance_squared(p.as_dvec3() + 0.5) < WORK_DISTANCE * WORK_DISTANCE) {
                    v.restock(now, self.village_day);
                }
            }
        }
        self.breed_villagers(world);
        self.life_tick(world);
    }
    pub fn villagers_to_string(&self) -> String {
        let mobs: Vec<Value> = self
            .mobs
            .iter()
            .chain(self.storage.mobs())
            .filter(|m| m.alive())
            .filter_map(|m| match m.kind {
                MobKind::IronGolem | MobKind::SnowGolem | MobKind::TraderLlama => {
                    let mut saved = json!({
                    "identity":super::leash::save(m), "kind": m.kind.name(), "p": m.pos.to_array(), "yaw": m.yaw,
                    "health": m.health, "effects":m.effects.serialize(), "built": m.built, "anger": m.player_hit_left, "anger_player":m.angry_player.map(|p|p.0), "trader": m.trader.as_ref().map(|t|t.save()),
                    });
                    // Angry at every player (`universal_anger`); written only when set.
                    if m.neutral_universal {
                        saved["anger_all"] = json!(true);
                    }
                    if m.attributes() != super::zombie::Attrs::default() {
                        saved["attributes"] = m.attrs.save();
                    }
                    Some(saved)
                }
                MobKind::Villager | MobKind::ZombieVillager | MobKind::WanderingTrader => {
                    let v = m.villager.as_ref()?;
                    let mut saved = json!({"identity":super::leash::save(m),"trader":m.trader.as_ref().map(|t|t.save()),"kind":m.kind.name(),"id":v.id,"seed":v.seed,"p":m.pos.to_array(),"yaw":m.yaw,
                    "health":m.health,"effects":m.effects.serialize(),"age":m.age,"built":m.built,"profession":v.profession as u8,"level":v.level,"xp":v.xp,
                    "job":v.job.map(|p|p.to_array()),"home":v.home.map(|p|p.to_array()),
                    "offers":v.offers.map(|o|o.map(Offer::save)),"day":v.restock_day,"restocks":v.restocks,
                    "last_restock":v.last_restock,"meeting":v.meeting.map(|p|p.to_array()),"slept":v.last_slept,"golem_seen":v.golem_seen,"gossip":v.gossip.save(),"food":v.food.map(stack_to_string),"food_level":v.food_level,"bell_hide":v.bell_hide,"reputation":v.reputation,"weakness":m.weakness_left,"convert":m.convert_left,"convert_tick":m.convert_tick,"convert_by":m.convert_by.map(|p|p.0)});
                    if let Some(variant) = &v.map_variant {
                        saved["map_variant"] = json!(variant);
                    }
                    // v0.14: `equipment` and `drop_chances` replace `armor` and `glint`.
                    m.equipment.save_into(&mut saved, false);
                    // A zombie villager's spawn attributes (leaders' health and reinforcements).
                    if m.attributes() != super::zombie::Attrs::default() {
                        saved["attributes"] = m.attrs.save();
                    }
                    Some(saved)
                }
                _ => None,
            })
            .collect();
        json!({"trader_spawner":[self.trader_spawner.delay,self.trader_spawner.chance],"next":self.next_villager_id,"mobs":mobs,"births":self.villager_births.iter().map(|p|p.to_array()).collect::<Vec<_>>()}).to_string()
    }
    pub fn load_villagers(&mut self, text: &str) {
        let Ok(root) = serde_json::from_str::<Value>(text) else { return };
        self.trader_spawner.delay = root["trader_spawner"][0].as_f64().unwrap_or(1200.0).clamp(0.0, 1200.0) as f32;
        self.trader_spawner.chance = root["trader_spawner"][1].as_u64().unwrap_or(25).clamp(25, 75) as u8;
        self.next_villager_id = self.next_villager_id.max(root["next"].as_u64().unwrap_or(1));
        fn pos(v: &Value) -> Option<IVec3> {
            let a = v.as_array()?;
            if a.len() != 3 {
                return None;
            }
            Some(IVec3::new(
                i32::try_from(a[0].as_i64()?).ok()?,
                i32::try_from(a[1].as_i64()?).ok()?,
                i32::try_from(a[2].as_i64()?).ok()?,
            ))
        }
        if let Some(births) = root["births"].as_array() {
            for p in births {
                if let Some(p) = pos(p) {
                    self.villager_births.insert(p);
                }
            }
        }
        let Some(mobs) = root["mobs"].as_array() else { return };
        for a in mobs {
            let load = || -> Option<Mob> {
                let kind = MobKind::from_name(a["kind"].as_str().unwrap_or("villager"))?;
                if !matches!(
                    kind,
                    MobKind::Villager
                        | MobKind::ZombieVillager
                        | MobKind::IronGolem
                        | MobKind::SnowGolem
                        | MobKind::WanderingTrader
                        | MobKind::TraderLlama
                ) {
                    return None;
                }
                let p = a["p"].as_array()?;
                if p.len() != 3 {
                    return None;
                }
                let p = DVec3::new(p[0].as_f64()?, p[1].as_f64()?, p[2].as_f64()?);
                if !p.is_finite() {
                    return None;
                }
                let yaw = a["yaw"].as_f64()? as f32;
                let health = a["health"].as_f64()? as f32;
                if !yaw.is_finite() || !health.is_finite() || health <= 0.0 {
                    return None;
                }
                if matches!(kind, MobKind::IronGolem | MobKind::SnowGolem | MobKind::TraderLlama) {
                    if self.mobs.iter().any(|m| m.kind == kind && m.pos.distance_squared(p) < 0.01) {
                        return None;
                    }
                    let mut m = Mob::new(kind, p, yaw);
                    m.attrs = super::zombie::Attrs::load(&a["attributes"]);
                    if kind == MobKind::TraderLlama {
                        m.trader = Some(Box::new(super::wandering_trader::Trader::load(&a["trader"], p)?));
                    }
                    m.health = health.min(kind.max_health());
                    m.built = a["built"].as_bool().unwrap_or(false);
                    m.angry_player =
                        a["anger_player"].as_u64().and_then(|p| u32::try_from(p).ok()).map(super::PlayerId);
                    m.player_hit_left = a["anger"].as_f64().unwrap_or(0.0).clamp(0.0, 60.0) as f32;
                    m.neutral_universal = kind == MobKind::IronGolem && a["anger_all"] == true;
                    return Some(m);
                }
                let id = a["id"].as_u64()?;
                if id == 0 || self.merchant(id).is_some() {
                    return None;
                }
                let mut v = Villager::new(id, a["seed"].as_u64()?);
                v.wandering = kind == MobKind::WanderingTrader;
                v.profession = *Profession::ALL.get(a["profession"].as_u64()? as usize)?;
                v.level = u8::try_from(a["level"].as_u64()?).ok()?;
                if !(1..=5).contains(&v.level) {
                    return None;
                }
                v.xp = u16::try_from(a["xp"].as_u64()?).ok()?;
                v.home = pos(&a["home"]);
                v.map_variant = a["map_variant"].as_str().map(str::to_owned);
                v.job = pos(&a["job"]);
                for (i, o) in a["offers"].as_array()?.iter().take(10).enumerate() {
                    if !o.is_null() {
                        v.offers[i] = Some(Offer::load(o)?);
                    }
                }
                v.restock_day = a["day"].as_i64()?.max(0);
                v.restocks = (a["restocks"].as_u64()?.min(2)) as u8;
                // Saves before v0.15 Track H kept the tick of the day.
                v.last_restock = a["last_restock"]
                    .as_i64()
                    .unwrap_or_else(|| v.restock_day * 24000 + a["last"].as_i64().unwrap_or(0).clamp(0, 23999));
                v.meeting = pos(&a["meeting"]);
                if let Some(food) = a["food"].as_array() {
                    for (slot, s) in v.food.iter_mut().zip(food) {
                        *slot = s
                            .as_str()
                            .and_then(|s| stack_from_str(s).flatten())
                            .filter(|s| super::villager_breeding::food_points(s.item) > 0);
                    }
                }
                v.food_level = a["food_level"].as_u64().unwrap_or(0).min(15) as u8;
                v.bell_hide = a["bell_hide"].as_f64().unwrap_or(0.0).clamp(0.0, 15.0) as f32;
                v.last_slept = a["slept"].as_i64();
                v.golem_seen = a["golem_seen"].as_f64().unwrap_or(0.0).clamp(0.0, 30.0) as f32;
                v.gossip = super::villager_gossip::Gossip::load(&a["gossip"]);
                v.reputation = i16::try_from(a["reputation"].as_i64().unwrap_or(0)).unwrap_or(0);
                // Older saves could hold a promotion waiting on the merchant
                // timer 26.3 no longer has; it lands now.
                if a["promote_pending"] == true && a["promote"].as_f64().is_some_and(|t| t > 0.0) && v.level < 5 {
                    v.level += 1;
                    v.unlock();
                }
                let mut m = Mob::new(kind, p, yaw);
                // Before health, which a leader's max health bounds.
                m.attrs = super::zombie::Attrs::load(&a["attributes"]);
                m.health = health.min(m.max_health());
                m.age = i32::try_from(a["age"].as_i64()?).ok()?.clamp(-24000, 6000);
                m.baby = m.age < 0;
                m.built = a["built"].as_bool().unwrap_or(false);
                m.weakness_left = a["weakness"].as_f64().unwrap_or(0.0).clamp(0.0, 600.0) as f32;
                m.convert_left = a["convert"].as_f64().unwrap_or(0.0).clamp(0.0, 600.0) as f32;
                m.convert_tick = a["convert_tick"].as_f64().unwrap_or(0.0).clamp(0.0, 1.0) as f32;
                m.convert_by = a["convert_by"]
                    .as_u64()
                    .and_then(|p| u32::try_from(p).ok())
                    .map(super::PlayerId)
                    .or_else(|| (m.convert_left > 0.0).then_some(super::PlayerId::HOST));
                let fresh = std::mem::take(&mut m.equipment);
                m.equipment = super::equipment::load_mob_equipment(a, fresh, &super::equipment::Legacy::villager(a));
                if kind == MobKind::WanderingTrader {
                    m.trader = Some(Box::new(super::wandering_trader::Trader::load(&a["trader"], p)?));
                }
                m.villager = Some(Box::new(v));
                Some(m)
            };
            if let Some(mut m) = load() {
                m.effects = crate::simulation::effects::Effects::deserialize(a["effects"].as_str().unwrap_or(""));
                super::leash::load(&mut m, &a["identity"]);
                if let Some(v) = m.villager.as_ref() {
                    self.next_villager_id = self.next_villager_id.max(v.id.saturating_add(1));
                }
                self.mobs.push(m);
            }
        }
    }
}

/// Java item names the trade table uses, including deliberately unavailable
/// content; resolved with [`trades::item`]. Kept visible to the registry-gap
/// guard so offer filtering cannot hide typos.
pub fn trade_item_names() -> impl Iterator<Item = &'static str> {
    trades::TRADES
        .iter()
        .flat_map(|(_, _, t)| [Some(t.wants.0), t.also.map(|a| a.0), Some(t.gives.0)].into_iter().flatten())
}

/// Every offer `profession` can roll at `level` whose items are registered,
/// as a fresh villager of a type the trade allows would make it (seed 0).
pub fn level_offers(profession: Profession, level: u8) -> Vec<Offer> {
    trades::TRADES
        .iter()
        .filter(|(p, l, _)| *p == profession && *l == level)
        .filter_map(|(_, _, d)| d.offer(0, d.types.first().copied().unwrap_or("plains")))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::entity::{Ctx, PlayerId, Target};
    use crate::physics::BlockSource;
    use rustc_hash::FxHashMap;
    struct VillageGrid {
        blocks: FxHashMap<IVec3, Block>,
        loaded: bool,
        home: Option<(IVec3, IVec3)>,
    }
    impl BlockSource for VillageGrid {
        fn block(&self, p: IVec3) -> Option<Block> {
            self.loaded.then(|| self.blocks.get(&p).copied().unwrap_or(if p.y < 0 { Block::STONE } else { Block::AIR }))
        }
    }
    impl MobWorld for VillageGrid {
        fn loaded(&self, _: IVec3) -> bool {
            self.loaded
        }
        fn surface(&self, _: i32, _: i32) -> Option<i32> {
            Some(-1)
        }
        fn exposed(&self, _: IVec3) -> bool {
            true
        }
        fn village_pois(&self, visit: &mut dyn FnMut(IVec3, Block)) {
            if self.loaded {
                for (&p, &b) in &self.blocks {
                    if is_poi(b) {
                        visit(p, b)
                    }
                }
            }
        }
        fn village_homes(&self, visit: &mut dyn FnMut(IVec3, IVec3)) {
            if self.loaded
                && let Some((p, s)) = self.home
            {
                visit(p, s)
            }
        }
    }
    fn grid() -> VillageGrid {
        VillageGrid { blocks: FxHashMap::default(), loaded: true, home: None }
    }
    fn step(e: &mut Entities, w: &VillageGrid) {
        e.village_timer = 0.0;
        e.village_upkeep(1.0, w);
    }
    /// The table entry with Java id `id`.
    fn def(id: &str) -> &'static TradeDef {
        trades::TRADES.iter().find(|(_, _, d)| d.id == id).map(|(_, _, d)| d).unwrap()
    }
    fn fixed() -> Villager {
        let mut v = Villager::new(1, 42);
        v.profession = Profession::Farmer;
        v.offers[0] = def("farmer/1/wheat_emerald").offer(9, "plains");
        v
    }
    #[test]
    fn trades_pay_atomically_stock_and_unlock_tiers() {
        let mut v = fixed();
        let mut inv = Inventory::default();
        inv.slots[0] = Some(Stack::new(Item::WHEAT, 19));
        let before = inv.slots;
        assert_eq!(v.trade(0, &mut inv), Err("missing trade payment"));
        assert_eq!(inv.slots, before);
        assert_eq!(v.xp, 0);
        inv.slots[0] = Some(Stack::new(Item::WHEAT, 64));
        inv.slots[1] = Some(Stack::new(Item::WHEAT, 64));
        for i in 0..5 {
            let xp = v.trade(0, &mut inv).unwrap();
            assert!((3..=6).contains(&xp) || i == 4 && (8..=11).contains(&xp));
        }
        // Java 26.3 `rewardTradeXp` promotes at once; the level-2 offers
        // follow the first.
        assert_eq!((v.level, v.xp), (2, 10));
        assert_eq!(v.offers[0].unwrap().uses, 5);
        assert!(v.offers[1].is_some() && v.offers[2].is_some() && v.offers[3].is_none());
        assert_eq!(
            inv.slots.iter().flatten().filter(|s| s.item == Item::EMERALD).map(|s| s.count as u32).sum::<u32>(),
            5
        );
        v.offers[0].as_mut().unwrap().uses = 16;
        let before = inv.slots;
        assert_eq!(v.trade(0, &mut inv), Err("offer out of stock"));
        assert_eq!(inv.slots, before);
    }
    #[test]
    fn full_inventory_and_second_payment_leave_no_partial_trade() {
        let mut v = Villager::new(1, 6);
        v.offers[0] = def("librarian/1/emerald_and_book_enchanted_book").offer(6, "plains");
        let o = v.offers[0].unwrap();
        let mut inv = Inventory::default();
        inv.slots.fill(Some(Stack::new(Block::STONE, 64)));
        inv.slots[0] = Some(Stack::new(Item::EMERALD, 64));
        let before = inv.slots;
        assert_eq!(v.trade(0, &mut inv), Err("missing trade payment"));
        assert_eq!(before, inv.slots);
        inv.slots[1] = Some(Stack::new(Item::BOOK, 2));
        let before = inv.slots;
        assert_eq!(v.trade(0, &mut inv), Err("inventory full"));
        assert_eq!(before, inv.slots);
        assert_eq!(v.offers[0].unwrap(), o);
        inv.slots[1] = Some(Stack::new(Item::BOOK, 1));
        assert!(v.trade(0, &mut inv).is_ok());
        assert!(inv.slots.iter().flatten().any(|s| s.item == Item::ENCHANTED_BOOK && !s.enchants.is_empty()));
    }
    #[test]
    fn restock_follows_should_restock_and_catches_up_demand() {
        let mut v = fixed();
        v.offers[0].as_mut().unwrap().uses = 16;
        // The day's first restock needs no wait: demand 0 + 16 - 0.
        assert!(v.restock(2000, 0));
        let o = v.offers[0].unwrap();
        assert_eq!((o.uses, o.demand), (0, 16));
        // 20 + floor(20 * 16 * 0.05).
        assert_eq!(o.price().count, 36);
        v.reputation = 100;
        // Less floor(100 * 0.05) as the special price.
        assert_eq!(v.priced(o).count, 31);
        v.offers[0].as_mut().unwrap().uses = 16;
        assert!(!v.restock(2000 + 2400, 0), "more than 2400 ticks after the last");
        assert!(v.restock(2000 + 2401, 0));
        assert_eq!(v.offers[0].unwrap().demand, 32);
        v.offers[0].as_mut().unwrap().uses = 1;
        assert!(!v.restock(8000, 0), "two a day");
        // 12000 ticks after the last restock is a new day: none were missed.
        assert!(v.restock(4401 + 12001, 0));
        assert_eq!(v.offers[0].unwrap().demand, 32 + 2 - 16);
        // The next day had one restock, so one missed update lowers demand.
        assert!(!v.restock(16402 + 12001, 1), "nothing used");
        let o = v.offers[0].unwrap();
        assert_eq!((o.uses, o.demand), (0, 18 - 16));
        assert!(!v.offers.iter().flatten().any(|o| o.uses > 0));
    }
    #[test]
    fn special_prices_follow_update_special_prices() {
        let o = def("farmer/1/wheat_emerald").offer(1, "plains").unwrap();
        assert_eq!(special_price_diff(o, 0, None), 0);
        assert_eq!(special_price_diff(o, 100, None), -5);
        assert_eq!(special_price_diff(o, -100, None), 5);
        // Hero I: floor(0.3 * 20) = 6; Hero V: floor((0.3 + 0.25) * 20) = 11.
        assert_eq!(special_price_diff(o, 0, Some(0)), -6);
        assert_eq!(special_price_diff(o, 0, Some(4)), -11);
        // At least one off for cheap offers.
        let bread = def("farmer/1/emerald_bread").offer(1, "plains").unwrap();
        assert_eq!(special_price_diff(bread, 0, Some(0)), -1);
        // The clamp applies once, after demand and the special price.
        let mut hot = o;
        hot.demand = 100;
        assert_eq!(hot.price().count, 64);
        assert_eq!(hot.price_with(-10).count, 64, "20 + 100 - 10 still clamps to 64");
        assert_eq!(o.price_with(-30).count, 1);
    }
    #[test]
    fn jobs_are_exclusive_trade_locks_profession_and_babies_wait() {
        let mut w = grid();
        let p = IVec3::new(2, 0, 0);
        w.blocks.insert(p, Block::COMPOSTER);
        let mut e = Entities::new(4);
        e.spawn(MobKind::Villager, DVec3::ZERO);
        e.spawn(MobKind::Villager, DVec3::new(1.0, 0.0, 0.0));
        step(&mut e, &w);
        assert_eq!(e.mobs.iter().filter(|m| m.villager.as_ref().unwrap().job == Some(p)).count(), 1);
        assert_eq!(e.mobs[0].villager.as_ref().unwrap().profession, Profession::Farmer);
        w.blocks.remove(&p);
        step(&mut e, &w);
        assert_eq!(e.mobs[0].villager.as_ref().unwrap().profession, Profession::None);
        w.blocks.insert(p, Block::COMPOSTER);
        step(&mut e, &w);
        e.mobs[0].villager.as_mut().unwrap().xp = 2;
        w.blocks.insert(p, Block::LECTERN);
        step(&mut e, &w);
        assert_eq!(e.mobs[0].villager.as_ref().unwrap().profession, Profession::Farmer);
        assert_eq!(e.mobs[0].villager.as_ref().unwrap().job, None);
        e.mobs[1].age = -24000;
        e.mobs[1].villager.as_mut().unwrap().job = None;
        e.mobs[1].villager.as_mut().unwrap().set_profession(Profession::None);
        step(&mut e, &w);
        assert_eq!(e.mobs[1].villager.as_ref().unwrap().job, None);
    }
    #[test]
    fn homes_schedule_and_zombies_override_sleep() {
        let mut w = grid();
        let bed = IVec3::ZERO;
        w.blocks.insert(bed, Block::colored_bed(crate::color::DyeColor::Blue, true));
        let mut e = Entities::new(1);
        e.spawn(MobKind::Villager, DVec3::new(0.5, 0.6, 0.5));
        e.village_time = 0.6;
        step(&mut e, &w);
        assert_eq!(e.mobs[0].villager.as_ref().unwrap().home, Some(bed));
        assert!(e.mobs[0].villager.as_ref().unwrap().sleeping);
        e.spawn(MobKind::Zombie, DVec3::new(2.0, 0.0, 0.0));
        step(&mut e, &w);
        let v = e.mobs[0].villager.as_ref().unwrap();
        assert!(v.fleeing);
        assert!(!v.sleeping);
        assert!(v.goal.unwrap().x < 0.5);
        e.mobs.pop();
        e.village_time = 0.25;
        step(&mut e, &w);
        assert!(!e.mobs[0].villager.as_ref().unwrap().sleeping);
    }
    #[test]
    fn farmers_walk_to_ripe_crops_near_their_composter_and_harvest_them() {
        let mut w = grid();
        let job = IVec3::new(0, 1, 0);
        w.blocks.insert(job, Block::COMPOSTER);
        let ripe = IVec3::new(4, 1, 1);
        let young = IVec3::new(-3, 1, 0);
        for (cell, stage) in [(ripe, 7), (young, 3)] {
            w.blocks.insert(cell - IVec3::Y, Block::FARMLAND);
            w.blocks.insert(cell, Block::wheat(stage));
        }
        let mut e = Entities::new(1);
        e.spawn(MobKind::Villager, DVec3::new(0.5, 1.0, 0.5));
        e.village_time = 0.2;
        let events = |e: &mut Entities| {
            step(e, &w);
            std::mem::take(&mut e.pending_sounds)
        };
        assert!(events(&mut e).is_empty(), "claims the composter first; the crop is out of reach");
        let v = e.mobs[0].villager.as_ref().unwrap();
        assert_eq!(v.profession, Profession::Farmer);
        assert_eq!(v.goal, Some(ripe.as_dvec3() + DVec3::new(0.5, 0.0, 0.5)), "heads for the ripe wheat only");
        e.mobs[0].pos = DVec3::new(3.7, 1.0, 1.5);
        assert!(matches!(events(&mut e)[..], [EntityEvent::VillagerHarvest { cell }] if cell == ripe));
        // Off shift or with mobGriefing off, the crops are left alone.
        e.village_time = 0.6;
        assert!(events(&mut e).is_empty());
        e.village_time = 0.2;
        e.villager_griefing = false;
        assert!(events(&mut e).is_empty());
    }
    #[test]
    fn farmers_recognize_beetroot_age_three_but_do_not_harvest_stems() {
        let mut w = grid();
        let job = IVec3::new(0, 1, 0);
        let young = IVec3::new(1, 1, 0);
        let ripe = IVec3::new(3, 1, 0);
        w.blocks.insert(young - IVec3::Y, Block::FARMLAND);
        w.blocks.insert(young, Block::crop(crate::world::block::Crop::Beetroot, 2));
        w.blocks.insert(ripe - IVec3::Y, Block::FARMLAND);
        w.blocks.insert(ripe, Block::crop(crate::world::block::Crop::Beetroot, 3));
        assert_eq!(ripe_crop(&w, job, job.as_dvec3()), Some(ripe));
        w.blocks.insert(ripe, crate::world::crops::stem_block(crate::world::crops::Fruit::Melon, 7));
        assert_eq!(ripe_crop(&w, job, job.as_dvec3()), None);
    }
    #[test]
    fn sleeping_villagers_lie_on_the_head_half_with_their_head_on_the_pillow() {
        let color = crate::color::DyeColor::Red;
        for (foot, pillow) in [(IVec3::Z, DVec3::NEG_Z), (IVec3::NEG_X, DVec3::X), (IVec3::X, DVec3::NEG_X)] {
            let mut w = grid();
            w.blocks.insert(IVec3::ZERO, Block::colored_bed(color, true));
            w.blocks.insert(foot, Block::colored_bed(color, false));
            let mut e = Entities::new(1);
            e.spawn(MobKind::Villager, DVec3::new(0.7, 0.6, 0.4));
            e.village_time = 0.6;
            step(&mut e, &w);
            let m = &e.mobs[0];
            assert!(m.villager.as_ref().unwrap().sleeping);
            assert_eq!(m.pos, DVec3::new(0.5, Block::BED_FOOT.height(), 0.5));
            // The model's head points along (-sin yaw, cos yaw) once laid down.
            let head = DVec3::new(-m.yaw.sin() as f64, 0.0, m.yaw.cos() as f64);
            assert!((head - pillow).length() < 1e-5, "{foot:?}: {head:?}");
        }
    }
    #[test]
    fn saved_stock_claims_and_natural_birth_receipts_survive_unloading() {
        let mut w = grid();
        let bed = IVec3::ZERO;
        w.blocks.insert(bed, Block::BED_HEAD);
        w.home = Some((bed, IVec3::X));
        let mut e = Entities::new(3);
        step(&mut e, &w);
        assert_eq!(e.count(MobKind::Villager), 1);
        let v = e.mobs[0].villager.as_mut().unwrap();
        v.set_profession(Profession::Librarian);
        v.level = 3;
        v.xp = 80;
        v.unlock();
        v.offers[0].as_mut().unwrap().uses = 2;
        v.restock_day = 7;
        v.restocks = 2;
        v.last_restock = 4400;
        let saved = e.villagers_to_string();
        let mut loaded = Entities::new(3);
        loaded.load_villagers(&saved);
        loaded.load_villagers(&saved);
        assert_eq!(loaded.count(MobKind::Villager), 1);
        assert_eq!(loaded.mobs[0].villager.as_ref().unwrap().offers, e.mobs[0].villager.as_ref().unwrap().offers);
        let ctx = Ctx {
            spawn_light: crate::simulation::lighting::SpawnLight::NIGHT,
            players: vec![Target::new(PlayerId::HOST, DVec3::new(500.0, 0.0, 0.0), false)],
            daylight: 0.0,
            raining: false,
            spawning: false,
            dimension: crate::world::terrain::Dimension::Overworld,
        };
        w.loaded = false;
        loaded.update(0.05, &w, &ctx);
        assert_eq!(loaded.count(MobKind::Villager), 1);
        w.loaded = true;
        step(&mut loaded, &w);
        assert_eq!(loaded.count(MobKind::Villager), 1);
        loaded.mobs.clear();
        step(&mut loaded, &w);
        assert_eq!(loaded.count(MobKind::Villager), 0);
        loaded.spawn(MobKind::Villager, DVec3::X);
        assert!(loaded.mobs[0].villager.as_ref().unwrap().id > e.mobs[0].villager.as_ref().unwrap().id);
        loaded.load_villagers("{broken");
        loaded.load_villagers(r#"{"mobs":[{"id":42}],"births":[[1]]}"#);
        assert_eq!(loaded.count(MobKind::Villager), 1);
    }
    #[test]
    fn normal_java_tables_have_valid_seeded_offers_for_every_job() {
        for p in Profession::ALL
            .into_iter()
            .filter(|p| !matches!(p, Profession::None | Profession::Nitwit | Profession::Leatherworker))
        {
            let mut v = Villager::new(1, 999);
            v.set_profession(p);
            assert!(v.offers.iter().flatten().count() > 0, "{p:?}");
            let mut again = Villager::new(1, 999);
            again.set_profession(p);
            assert_eq!(v.offers, again.offers);
            for l in 2..=5 {
                v.level = l;
                v.unlock();
            }
            for o in v.offers.iter().flatten() {
                assert!(o.cost.count > 0 && o.cost.count <= o.cost.max());
                assert!(o.output.count > 0 && o.output.count <= o.output.max(), "{p:?} {o:?}");
                assert!(o.xp > 0);
            }
        }
        let farmer = def("farmer/1/wheat_emerald").offer(4, "plains").unwrap();
        assert_eq!((farmer.cost.count, farmer.output.item, farmer.max_uses, farmer.xp), (20, Item::EMERALD, 16, 2));
        let enchanted = def("toolsmith/5/emerald_enchanted_diamond_pickaxe").offer(4, "plains").unwrap();
        assert!(!enchanted.output.enchants.is_empty());
        assert!((18..=32).contains(&enchanted.cost.count));
        // `toItemCost` clamps to the stack size: one book and quill, not two.
        let books = def("librarian/4/writable_book_emerald").offer(4, "plains").unwrap();
        assert_eq!(books.cost, Stack::new(Item::WRITABLE_BOOK, 1));
    }
    /// Java 26.3 `villager_trade/cleric/5/emerald_experience_bottle.json`:
    /// a master cleric always offers it beside the nether wart (the level
    /// tag has two offers, and the game picks two).
    #[test]
    fn master_clerics_sell_the_bottle_o_enchanting_on_java_terms() {
        let f: serde_json::Value =
            serde_json::from_str(include_str!("../../tests/fixtures/java/26.3/thrown_items.json")).unwrap();
        let t = &f["experience_bottle"]["trade"];
        assert_eq!(t["in_level_tag"], true);
        let level: Vec<_> =
            trades::TRADES.iter().filter(|(p, l, d)| *p == Profession::Cleric && *l == 5 && d.available()).collect();
        assert_eq!(level.len() as u64, t["level_offers"].as_u64().unwrap());
        let java_item = |v: &serde_json::Value| crate::java_registry::item(v["id"].as_str().unwrap(), "").unwrap();
        for seed in 0..50 {
            let mut v = Villager::new(seed, seed * 31 + 7);
            v.set_profession(Profession::Cleric);
            for l in 2..=5 {
                v.level = l;
                v.unlock();
            }
            let offer = v
                .offers
                .iter()
                .flatten()
                .find(|o| o.output.item == Item::EXPERIENCE_BOTTLE)
                .copied()
                .expect("a master cleric sells the bottle");
            assert_eq!(offer.cost, Stack::new(java_item(&t["wants"]), t["wants"]["count"].as_u64().unwrap() as u8));
            assert_eq!(offer.second, None);
            assert_eq!(offer.output, Stack::new(java_item(&t["gives"]), t["gives"]["count"].as_u64().unwrap() as u8));
            assert_eq!(u64::from(offer.max_uses), t["max_uses"].as_u64().unwrap());
            assert_eq!(u64::from(offer.xp), t["xp"].as_u64().unwrap());
            assert_eq!(f64::from(offer.multiplier), f64::from(t["reputation_discount"].as_f64().unwrap() as f32));
        }
    }

    #[test]
    fn master_fletcher_crossbows_have_their_enchantment() {
        let def = def("fletcher/5/emerald_enchanted_crossbow");
        for seed in 0..100 {
            let offer = def.offer(seed, "plains").unwrap();
            assert_eq!(offer.output.item, Item::CROSSBOW);
            // `enchant_with_levels` over `#on_traded_equipment`, discarded unless enchanted.
            assert!(!offer.output.enchants.is_empty());
            assert!(offer.output.enchants.iter().all(|(e, _)| e.fits(Item::CROSSBOW)));
        }
    }

    #[test]
    fn round_two_mobs_save_conversion_age_and_golem_health() {
        let mut e = Entities::new(4);
        e.spawn(MobKind::Villager, DVec3::ZERO);
        e.mobs[0].kind = MobKind::ZombieVillager;
        e.mobs[0].age = -12345;
        e.mobs[0].baby = true;
        e.mobs[0].built = true;
        e.mobs[0].convert_left = 202.5;
        e.mobs[0].villager.as_mut().unwrap().set_profession(Profession::Farmer);
        e.spawn(MobKind::IronGolem, DVec3::X * 2.0);
        e.mobs[1].built = true;
        e.mobs[1].health = 42.0;
        let mut restored = Entities::new(5);
        restored.load_villagers(&e.villagers_to_string());
        assert_eq!(restored.mobs[0].kind, MobKind::ZombieVillager);
        assert_eq!(restored.mobs[0].age, -12345);
        assert!(restored.mobs[0].baby && restored.mobs[0].built);
        assert_eq!(restored.mobs[0].convert_left, 202.5);
        assert_eq!(restored.mobs[0].villager.as_ref().unwrap().offers, e.mobs[0].villager.as_ref().unwrap().offers);
        assert_eq!(restored.mobs[1].health, 42.0);
        assert!(restored.mobs[1].built);
        let mut w = grid();
        w.loaded = false;
        restored.update(
            1.0,
            &w,
            &Ctx {
                spawn_light: crate::simulation::lighting::SpawnLight::DAY,
                players: vec![Target::new(PlayerId::HOST, DVec3::ZERO, false)],
                daylight: 1.0,
                spawning: false,
                raining: false,
                dimension: crate::world::terrain::Dimension::Overworld,
            },
        );
        assert_eq!(restored.mobs.len(), 2);
        assert_eq!(restored.mobs[0].convert_left, 202.5);
    }
    #[test]
    fn breeding_consumes_twelve_food_points_claims_bed_and_saves() {
        let mut w = grid();
        for x in 0..3 {
            w.blocks.insert(IVec3::new(x * 3, 0, 0), Block::BED_HEAD);
        }
        let mut e = Entities::new(11);
        // Breeding is part of the idle activity (ticks 10 to 2000 of the day).
        e.village_time = 0.05;
        e.spawn(MobKind::Villager, DVec3::ZERO);
        e.spawn(MobKind::Villager, DVec3::X);
        e.mobs[0].villager.as_mut().unwrap().food[0] = Some(Stack::new(Item::BREAD, 3));
        e.mobs[1].villager.as_mut().unwrap().food[0] = Some(Stack::new(Item::BEETROOT, 12));
        for _ in 0..18 {
            step(&mut e, &w);
        }
        assert_eq!(e.count(MobKind::Villager), 3);
        assert_eq!(e.mobs[0].age, 6000);
        assert_eq!(e.mobs[1].age, 6000);
        assert_eq!(e.mobs[2].age, -24000);
        assert!(e.mobs[2].baby);
        assert_eq!(e.mobs[0].villager.as_ref().unwrap().food_points(), 0);
        let baby_home = e.mobs[2].villager.as_ref().unwrap().home.unwrap();
        assert!(e.mobs[..2].iter().all(|m| m.villager.as_ref().unwrap().home != Some(baby_home)));
        e.mobs[0].villager.as_mut().unwrap().food[0] = Some(Stack::new(Item::CARROT, 23));
        let mut restored = Entities::new(12);
        restored.load_villagers(&e.villagers_to_string());
        assert_eq!(restored.mobs[0].age, 6000);
        assert_eq!(restored.mobs[0].villager.as_ref().unwrap().food_points(), 23);
        assert_eq!(restored.mobs[2].age, -24000);
        assert_eq!(restored.mobs[2].villager.as_ref().unwrap().home, Some(baby_home));
        let mut rng = super::super::Rng::new(1);
        let mut events = Vec::new();
        restored.mobs[2].age = -1;
        restored.mobs[2].update(
            0.05,
            &w,
            &Ctx {
                spawn_light: crate::simulation::lighting::SpawnLight::DAY,
                players: vec![],
                daylight: 1.0,
                spawning: false,
                raining: false,
                dimension: crate::world::terrain::Dimension::Overworld,
            },
            &mut rng,
            &mut events,
        );
        assert_eq!(restored.mobs[2].age, 0);
        assert!(!restored.mobs[2].baby);
    }
    #[test]
    fn villagers_share_excess_and_obey_pickup_gamerule() {
        let w = grid();
        let mut e = Entities::new(8);
        e.spawn(MobKind::Villager, DVec3::ZERO);
        e.spawn(MobKind::Villager, DVec3::X);
        e.mobs[0].villager.as_mut().unwrap().food[0] = Some(Stack::new(Item::BREAD, 6));
        e.breed_villagers(&w);
        assert_eq!(e.mobs[1].villager.as_ref().unwrap().food_points(), 0, "small stacks are not shared");
        e.mobs[0].villager.as_mut().unwrap().food[0] = Some(Stack::new(Item::CARROT, 36));
        e.breed_villagers(&w);
        assert_eq!(e.mobs[0].villager.as_ref().unwrap().food_points(), 18);
        assert_eq!(e.mobs[1].villager.as_ref().unwrap().food_points(), 18);
        e.mobs[0].villager.as_mut().unwrap().food = [None; 8];
        e.mobs[1].villager.as_mut().unwrap().food = [None; 8];
        e.drop_from_block(Stack::new(Item::POTATO, 12), IVec3::ZERO);
        e.items[0].pickup_delay = 0.0;
        e.villager_griefing = false;
        e.breed_villagers(&w);
        assert_eq!(e.items[0].stack.count, 12);
        e.villager_griefing = true;
        e.breed_villagers(&w);
        assert!(e.items.is_empty());
        assert_eq!(e.mobs.iter().map(|m| m.villager.as_ref().unwrap().food_points()).sum::<u16>(), 12);
    }
    #[test]
    fn breeding_requires_food_free_bed_and_two_blocks_headroom() {
        for obstruction in [false, true] {
            let mut w = grid();
            for x in 0..2 {
                w.blocks.insert(IVec3::new(x * 3, 0, 0), Block::BED_HEAD);
            }
            if obstruction {
                w.blocks.insert(IVec3::new(6, 0, 0), Block::BED_HEAD);
                w.blocks.insert(IVec3::new(6, 2, 0), Block::STONE);
            }
            let mut e = Entities::new(3);
            e.village_time = 0.05;
            for x in 0..2 {
                e.spawn(MobKind::Villager, DVec3::X * x as f64);
                e.mobs[x].villager.as_mut().unwrap().food[0] = Some(Stack::new(Item::POTATO, 12));
            }
            for _ in 0..18 {
                step(&mut e, &w);
            }
            assert_eq!(e.count(MobKind::Villager), 2);
        }
        let mut v = Villager::new(1, 1);
        v.food[0] = Some(Stack::new(Item::BREAD, 2));
        v.food[1] = Some(Stack::new(Item::CARROT, 4));
        assert_eq!(v.food_points(), 12);
        v.eat_for_breeding();
        assert_eq!(v.food_points(), 0);
    }
    #[test]
    fn curing_discounts_only_the_curing_player_and_saves_identity() {
        let mut e = Entities::new(5);
        let w = grid();
        let owner = super::super::PlayerId(7);
        e.spawn(MobKind::ZombieVillager, DVec3::ZERO);
        *e.mobs[0].villager.as_mut().unwrap().as_mut() = fixed();
        e.mobs[0].weakness_left = 30.0;
        assert!(e.try_cure_for(0, owner));
        let mut restored = Entities::new(8);
        restored.load_villagers(&e.villagers_to_string());
        assert_eq!(restored.mobs[0].convert_by, Some(owner));
        restored.mobs[0].convert_left = 0.05;
        restored.update(
            0.05,
            &w,
            &Ctx {
                spawn_light: crate::simulation::lighting::SpawnLight::DAY,
                players: vec![Target::new(owner, DVec3::ZERO, false)],
                daylight: 1.0,
                spawning: false,
                raining: false,
                dimension: crate::world::terrain::Dimension::Overworld,
            },
        );
        let v = restored.mobs[0].villager.as_ref().unwrap();
        let o = v.offers[0].unwrap();
        assert_eq!(v.priced_for(o, owner).count, 14);
        assert_eq!(v.priced_for(o, super::super::PlayerId::HOST).count, 20);
        let mut saved = Entities::new(9);
        saved.load_villagers(&restored.villagers_to_string());
        assert_eq!(saved.mobs[0].villager.as_ref().unwrap().priced_for(o, owner).count, 14);
    }
}

#[cfg(test)]
#[path = "villager_fixture_tests.rs"]
mod fixture_tests;

#[cfg(test)]
mod map_tests {
    use super::*;
    use crate::world::{World, terrain::Generator};
    use std::sync::Arc;
    #[test]
    fn cartographer_explorer_offer_points_at_located_monument_and_reloads() {
        let generator = Arc::new(Generator::new(42));
        let mut world = World::new_headless(generator.clone(), Default::default(), 1);
        let mut entities = Entities::new(42);
        entities.spawn(MobKind::Villager, DVec3::new(0.0, 80.0, 0.0));
        let v = entities.mobs[0].villager.as_mut().unwrap();
        v.set_profession(Profession::Cartographer);
        v.level = 3;
        v.map_variant = Some("desert".into());
        let monument = trades::TRADES
            .iter()
            .find(|(_, _, d)| d.id == "cartographer/3/emerald_and_compass_ocean_monument_map")
            .unwrap();
        v.push_offer(monument.2.offer(1, "desert").unwrap());
        let id = v.id;
        entities.prepare_merchant_maps(&mut world, id);
        let offer = entities.mobs[0]
            .villager
            .as_ref()
            .unwrap()
            .offers
            .iter()
            .flatten()
            .find(|o| o.output.item == Item::OCEAN_MONUMENT_MAP)
            .unwrap();
        assert_eq!(offer.cost, Stack::new(Item::EMERALD, 13));
        assert_eq!(offer.second, Some(Stack::new(Item::COMPASS, 1)));
        let data = &world.maps.maps[&offer.output.map_id.unwrap()];
        let target = generator
            .temples
            .locate_rings(&generator, &[crate::world::temples::Kind::Monument], IVec3::new(0, 80, 0), 100)
            .unwrap();
        assert_eq!((data.decorations[0].x, data.decorations[0].z), (target.x as f64, target.z as f64));
        let saved = entities.villagers_to_string();
        let mut loaded = Entities::new(42);
        loaded.load_villagers(&saved);
        assert_eq!(
            loaded.mobs[0].villager.as_ref().unwrap().offers,
            entities.mobs[0].villager.as_ref().unwrap().offers
        );
        assert_eq!(loaded.mobs[0].villager.as_ref().unwrap().map_variant.as_deref(), Some("desert"));
    }
    /// v0.15 Track F: the master cartographer's
    /// `emerald_and_compass_woodland_mansion_map` offer points at a mansion.
    #[test]
    fn master_cartographer_sells_the_woodland_mansion_map() {
        let generator = Arc::new(Generator::new(42));
        let mut world = World::new_headless(generator.clone(), Default::default(), 1);
        let mut entities = Entities::new(42);
        entities.spawn(MobKind::Villager, DVec3::new(0.0, 80.0, 0.0));
        let v = entities.mobs[0].villager.as_mut().unwrap();
        v.set_profession(Profession::Cartographer);
        v.level = 5;
        v.unlock();
        let id = v.id;
        entities.prepare_merchant_maps(&mut world, id);
        let map = crate::world::explorer_maps::WOODLAND_MANSION_MAP;
        let offer =
            entities.mobs[0].villager.as_ref().unwrap().offers.iter().flatten().find(|o| o.output.item == map).unwrap();
        assert_eq!(offer.cost, Stack::new(Item::EMERALD, 14));
        assert_eq!(offer.second, Some(Stack::new(Item::COMPASS, 1)));
        assert_eq!((offer.max_uses, offer.xp, offer.multiplier), (12, 30, 0.2));
        let data = &world.maps.maps[&offer.output.map_id.unwrap()];
        let d = data.decorations.iter().find(|d| d.kind == "mansion").unwrap();
        let chunk = glam::IVec2::new(d.x as i32, d.z as i32).div_euclid(glam::IVec2::splat(16));
        assert!(crate::world::mansion::start(&generator, chunk).is_some());
    }
    /// v0.15 integration review: an explorer map offer that unlocked after
    /// the screen opened has no destination yet, so it isn't sold.
    #[test]
    fn an_unresolved_explorer_map_offer_takes_no_payment() {
        let mut entities = Entities::new(42);
        entities.spawn(MobKind::Villager, DVec3::new(0.0, 80.0, 0.0));
        let v = entities.mobs[0].villager.as_mut().unwrap();
        v.set_profession(Profession::Cartographer);
        v.level = 5;
        v.unlock();
        let id = v.id;
        let map = crate::world::explorer_maps::WOODLAND_MANSION_MAP;
        let index = v.offers.iter().position(|o| o.is_some_and(|o| o.output.item == map)).unwrap();
        let mut inv = Inventory::default();
        inv.slots[0] = Some(Stack::new(Item::EMERALD, 64));
        inv.slots[1] = Some(Stack::new(Item::COMPASS, 1));
        let before = inv.slots;
        assert_eq!(entities.trade_for(id, index, &mut inv, crate::entity::PlayerId::HOST), Err("map not ready"));
        assert_eq!(inv.slots, before, "nothing was paid");
    }
    #[test]
    fn java_26_3_cartographer_predicates_filter_biome_variants() {
        let map = |name| trades::item(name).unwrap();
        assert!(trades::map_allowed(map("jungle_pyramid_map"), "desert"));
        assert!(!trades::map_allowed(map("jungle_pyramid_map"), "plains"));
        assert!(trades::map_allowed(map("taiga_village_map"), "plains"));
        assert!(!trades::map_allowed(map("taiga_village_map"), "desert"));
        assert!(trades::map_allowed(map("ocean_monument_map"), "snow"));
        use crate::world::terrain::Biome;
        assert_eq!(map_variant(Biome::SnowyTaiga), "snow");
        assert_eq!(map_variant(Biome::Badlands), "desert");
        assert_eq!(map_variant(Biome::WindsweptHills), "taiga");
        assert_eq!(map_variant(Biome::FrozenPeaks), "snow");
    }
}

fn map_variant(b: crate::world::terrain::Biome) -> &'static str {
    use crate::world::terrain::Biome;
    if b.is_jungle() {
        "jungle"
    } else if b.is_swamp() {
        "swamp"
    } else {
        match b {
            Biome::Desert | Biome::Badlands | Biome::ErodedBadlands | Biome::WoodedBadlands => "desert",
            Biome::Savanna | Biome::SavannaPlateau | Biome::WindsweptSavanna => "savanna",
            Biome::Taiga
            | Biome::OldGrowthPineTaiga
            | Biome::OldGrowthSpruceTaiga
            | Biome::WindsweptGravellyHills
            | Biome::WindsweptHills
            | Biome::WindsweptForest => "taiga",
            Biome::DeepFrozenOcean
            | Biome::FrozenOcean
            | Biome::FrozenRiver
            | Biome::IceSpikes
            | Biome::SnowyBeach
            | Biome::SnowyTaiga
            | Biome::SnowyPlains
            | Biome::Grove
            | Biome::SnowySlopes
            | Biome::FrozenPeaks
            | Biome::JaggedPeaks => "snow",
            _ => "plains",
        }
    }
}

#[cfg(test)]
mod resident_tests {
    use super::*;
    use crate::physics::BlockSource;
    use crate::simulation::difficulty::Difficulty;
    use crate::world::village::Resident;

    /// Flat ground holding a generation 4 village's resident spawn cells.
    struct Village(Vec<(IVec3, Resident)>);

    impl BlockSource for Village {
        fn block(&self, p: IVec3) -> Option<Block> {
            Some(if p.y < 0 { Block::STONE } else { Block::AIR })
        }
    }

    impl MobWorld for Village {
        fn loaded(&self, _: IVec3) -> bool {
            true
        }
        fn surface(&self, _: i32, _: i32) -> Option<i32> {
            Some(-1)
        }
        fn exposed(&self, _: IVec3) -> bool {
            true
        }
        fn village_residents(&self, visit: &mut dyn FnMut(IVec3, Resident)) {
            for &(p, r) in &self.0 {
                visit(p, r)
            }
        }
    }

    #[test]
    fn generation_4_residents_spawn_once_with_their_java_data_and_never_again_after_reload() {
        let w = Village(vec![
            (IVec3::new(0, 0, 0), Resident::parse("villager:none:plains:baby").unwrap()),
            (IVec3::new(2, 0, 0), Resident::parse("villager:nitwit:plains").unwrap()),
            (IVec3::new(4, 0, 0), Resident::parse("iron_golem:persistent").unwrap()),
            (IVec3::new(6, 0, 0), Resident::parse("cat:calico:persistent").unwrap()),
            (IVec3::new(8, 0, 0), Resident::parse("horse:261:persistent").unwrap()),
            (IVec3::new(10, 0, 0), Resident::parse("zombie_villager:none:plains:persistent").unwrap()),
            (IVec3::new(12, 0, 0), Resident::parse("sheep:persistent").unwrap()),
        ]);
        let mut e = Entities::new(7);
        // Peaceful holds the zombie villager back without forgetting it.
        e.populate_village_residents(&w, Difficulty::Peaceful);
        assert_eq!(e.mobs.len(), 6);
        assert!(!e.mobs.iter().any(|m| m.kind == MobKind::ZombieVillager));
        e.populate_village_residents(&w, Difficulty::Normal);
        assert_eq!(e.mobs.len(), 7);
        fn find(e: &Entities, k: MobKind) -> &Mob {
            e.mobs.iter().find(|m| m.kind == k).unwrap()
        }
        let baby = e.mobs.iter().find(|m| m.kind == MobKind::Villager && m.age < 0).unwrap();
        assert_eq!(baby.age, -21359);
        let nitwit = e.mobs.iter().find(|m| m.kind == MobKind::Villager && m.age >= 0).unwrap();
        assert_eq!(nitwit.villager.as_ref().unwrap().profession, Profession::Nitwit);
        assert!(find(&e, MobKind::IronGolem).persistent);
        let cat = find(&e, MobKind::Cat);
        let calico = crate::entity::cat_spawner::VARIANTS.iter().position(|v| *v == "calico").unwrap() as u8;
        assert_eq!(cat.animal.as_ref().unwrap().variant, calico);
        // Java's `Variant` 261: coat 5 (gray), markings 1 (white).
        let horse = find(&e, MobKind::Horse);
        let s = horse.mount.as_ref().unwrap();
        assert_eq!((s.variant, s.marking), (5, 1));
        assert!(find(&e, MobKind::ZombieVillager).persistent);
        // Nothing respawns, now or after a save and reload.
        e.populate_village_residents(&w, Difficulty::Normal);
        assert_eq!(e.mobs.len(), 7);
        let saved = e.villagers_to_string();
        let mut reloaded = Entities::new(7);
        reloaded.load_villagers(&saved);
        reloaded.mobs.clear();
        reloaded.populate_village_residents(&w, Difficulty::Normal);
        assert!(reloaded.mobs.is_empty(), "a reloaded world respawned {:?}", reloaded.mobs.len());
    }

    #[test]
    fn residents_parse_every_java_spawn_piece() {
        assert_eq!(Resident::parse("villager:none:snow:baby"), Some(Resident::Villager { nitwit: false, baby: true }));
        assert_eq!(
            Resident::parse("zombie_villager:nitwit:taiga:persistent"),
            Some(Resident::ZombieVillager { nitwit: true })
        );
        assert_eq!(Resident::parse("horse:773:persistent"), Some(Resident::Horse(773)));
        assert_eq!(Resident::parse("camel"), Some(Resident::Camel));
        assert_eq!(Resident::parse("cow:persistent"), Some(Resident::Cow));
        assert_eq!(Resident::parse("pig:persistent"), Some(Resident::Pig));
        assert_eq!(Resident::parse("armor_stand"), Some(Resident::ArmorStand));
        assert_eq!(Resident::parse("parrot"), None);
    }
}
