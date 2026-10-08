//! Persistent villagers, POI claims, daily work/rest and atomic merchant transactions.
//! Java's normal trade quantities/uses/XP are in `villager_trades`; absent items are skipped.
use crate::enchant::Enchantment;
use crate::inventory::{Inventory, SLOTS, Stack, move_into, stack_from_str, stack_to_string};
use crate::item::Item;
use crate::world::block::Block;
use glam::{DVec3, IVec3};
use serde_json::{Value, json};
fn hash(mut seed: u64) -> u64 {
    crate::world::noise::splitmix64(&mut seed)
}
use super::{Entities, Mob, MobKind, MobWorld};
#[path = "villager_trades.rs"]
mod tables;

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
    pub fn price(self) -> Stack {
        let extra = ((self.cost.count as f32 * self.demand as f32 * self.multiplier).floor() as i32).max(0);
        Stack { count: (self.cost.count as i32 + extra).clamp(1, self.cost.item.max_stack() as i32) as u8, ..self.cost }
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
#[derive(Clone, Copy)]
struct TradeDef {
    input: &'static str,
    n: u8,
    price: u8,
    output: &'static str,
    count: u8,
    uses: u8,
    xp: u16,
    mult: f32,
    ench: u8,
}
impl TradeDef {
    const fn buy(input: &'static str, n: u8, uses: u8, xp: u16) -> Self {
        Self { input, n, price: 0, output: "emerald", count: 1, uses, xp, mult: 0.05, ench: 0 }
    }
    const fn sell(output: &'static str, price: u8, count: u8, uses: u8, xp: u16, mult: f32) -> Self {
        Self { input: "emerald", n: price, price: 0, output, count, uses, xp, mult, ench: 0 }
    }
    const fn gear(output: &'static str, price: u8, uses: u8, xp: u16, mult: f32) -> Self {
        Self { ench: 1, ..Self::sell(output, price, 1, uses, xp, mult) }
    }
    const fn book(xp: u16) -> Self {
        Self { ench: 2, price: 1, ..Self::sell("enchanted book", 1, 1, 12, xp, 0.2) }
    }
    #[allow(clippy::too_many_arguments)]
    const fn exchange(
        input: &'static str,
        n: u8,
        price: u8,
        output: &'static str,
        count: u8,
        uses: u8,
        xp: u16,
    ) -> Self {
        Self { input, n, price, output, count, uses, xp, mult: 0.05, ench: 0 }
    }
    fn available(self) -> bool {
        Item::from_name(self.input).is_some() && Item::from_name(self.output).is_some()
    }
    fn offer(self, seed: u64) -> Option<Offer> {
        let mut rng = crate::enchant::JavaRandom::new(seed as i64);
        let mut cost = Stack::new(Item::from_name(self.input)?, self.n);
        let mut output = Stack::new(Item::from_name(self.output)?, self.count);
        let mut second = (self.price > 0).then(|| Stack::new(Item::EMERALD, self.price));
        if self.ench == 1 {
            let level = 5 + rng.next_bounded(15) as u32;
            cost.count = (self.n as u32 + level).min(64) as u8;
            for (e, l) in crate::enchant::offer_enchants(seed as i32, output.item, 0, level) {
                output.enchants.set(e, l);
            }
        } else if self.ench == 2 {
            let e = Enchantment::ALL[rng.next_bounded(Enchantment::COUNT as i32) as usize];
            let level = 1 + rng.next_bounded(e.def().max_level as i32) as u8;
            output.enchants.set(e, level);
            let price = 2 + rng.next_bounded(5 + 10 * level as i32) + 3 * level as i32;
            cost.count = (price * if e.def().treasure { 2 } else { 1 }).min(64) as u8;
            second = Some(Stack::new(Item::BOOK, 1));
        }
        Some(Offer {
            cost,
            second,
            output,
            uses: 0,
            max_uses: self.uses,
            xp: self.xp,
            demand: 0,
            multiplier: self.mult,
        })
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
    pub(super) food: [Option<Stack>; 8],
    pub(super) food_level: u8,
    pub(super) courtship: u16,
    pub(crate) bell_hide: f32,
    pub wandering: bool,
    pub(super) gossip: super::villager_gossip::Gossip,
    seed: u64,
    restock_day: i64,
    restocks: u8,
    last_restock: u32,
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
            food: [None; 8],
            food_level: 0,
            courtship: 0,
            bell_hide: 0.0,
            wandering: false,
            gossip: Default::default(),
            restock_day: -1,
            restocks: 0,
            last_restock: 0,
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
    /// `price` after the cure discount: `floor(reputation * multiplier)`, never below one.
    pub fn priced(&self, offer: Offer) -> Stack {
        self.priced_for(offer, super::PlayerId::HOST)
    }
    pub fn priced_for(&self, offer: Offer, owner: super::PlayerId) -> Stack {
        if self.wandering {
            return offer.price();
        }
        let base = offer.price();
        let reputation =
            self.gossip.reputation(owner) + if owner == super::PlayerId::HOST { self.reputation } else { 0 };
        let cut = (reputation as f32 * offer.multiplier).floor() as i32;
        Stack { count: (base.count as i32 - cut).clamp(1, offer.cost.item.max_stack() as i32) as u8, ..base }
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
    fn unlock(&mut self) {
        // Reservoir selection avoids an allocation and keeps Java's two distinct offers per tier.
        let mut picks: [Option<TradeDef>; 2] = [None; 2];
        let mut seen = 0u32;
        let mut seed = hash(self.seed ^ ((self.profession as u64) << 32) ^ self.level as u64);
        for &(p, l, d) in tables::TRADES {
            if p == self.profession && l == self.level && d.available() {
                seen += 1;
                seed = hash(seed);
                let i = (seed % seen as u64) as usize;
                if seen <= 2 {
                    picks[(seen - 1) as usize] = Some(d)
                } else if i < 2 {
                    picks[i] = Some(d)
                }
            }
        }
        for (i, d) in picks.into_iter().enumerate() {
            let slot = (self.level as usize - 1) * 2 + i;
            self.offers[slot] = d.and_then(|d| d.offer(hash(seed ^ slot as u64)));
        }
    }
    pub fn restock(&mut self, day: i64, tick: u32, at_work: bool) -> bool {
        if self.restock_day != day {
            self.restock_day = day;
            self.restocks = 0;
            self.last_restock = 0;
        }
        if !at_work
            || !(2000..9000).contains(&tick)
            || self.restocks >= 2
            || self.restocks > 0 && tick.saturating_sub(self.last_restock) < 2400
            || !self.offers.iter().flatten().any(|o| o.uses > 0)
        {
            return false;
        }
        for o in self.offers.iter_mut().flatten() {
            o.demand = o.demand.saturating_add(2 * o.uses as i16 - o.max_uses as i16);
            o.uses = 0;
        }
        self.restocks += 1;
        self.last_restock = tick;
        true
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
        self.gossip.add(owner, 4, 2);
        self.xp = self.xp.saturating_add(offer.xp);
        let promoted = self.level < 5 && self.xp >= [0, 10, 70, 150, 250][self.level as usize];
        if promoted {
            self.level += 1;
            self.unlock();
        }
        Ok(3 + (hash(self.seed ^ self.xp as u64 ^ self.offers[index].unwrap().uses as u64) % 4) as u32
            + if promoted { 5 } else { 0 })
    }
}

impl Entities {
    pub fn merchant(&self, id: u64) -> Option<&Mob> {
        self.mobs.iter().find(|m| m.alive() && m.villager.as_ref().is_some_and(|v| v.id == id))
    }
    pub fn merchant_in_reach(&self, id: u64, eye: DVec3) -> bool {
        self.merchant(id).is_some_and(|m| {
            matches!(m.kind, MobKind::Villager | MobKind::WanderingTrader)
                && eye.distance(m.pos + DVec3::Y) < 6.0
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
        m.villager.as_mut().unwrap().trade_for(index, inv, owner)
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
    pub(super) fn village_upkeep<W: MobWorld + ?Sized>(&mut self, dt: f32, world: &W) {
        self.village_timer -= dt;
        if self.village_timer > 0.0 {
            return;
        }
        self.village_timer = 1.0;
        world.village_homes(&mut |home, spawn| {
            if self.villager_births.insert(home) {
                self.spawn(MobKind::Villager, spawn.as_dvec3() + DVec3::new(0.5, 0.0, 0.5));
                let m = self.mobs.last_mut().unwrap();
                m.villager.as_mut().unwrap().home = Some(home);
                if hash(world.seed() ^ home.x as u64 ^ ((home.z as u64) << 32)).is_multiple_of(5) {
                    m.age = -24000;
                }
            }
        });
        self.mob_index.rebuild(&self.mobs);
        self.claimed_beds.clear();
        self.claimed_jobs.clear();
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
            }
        }
        let tick = (self.village_time.rem_euclid(1.0) * 24000.0) as u32;
        for i in 0..self.mobs.len() {
            let threat = self
                .mob_index
                .nearest(&self.mobs, self.mobs[i].pos, 8.0, |o| o.kind.is_zombie())
                .map(|j| self.mobs[j].pos);
            let m = &mut self.mobs[i];
            if !m.alive() || !world.loaded(m.pos.floor().as_ivec3()) {
                continue;
            }
            let Some(v) = &mut m.villager else { continue };
            v.gossip.decay(self.village_day);
            if m.kind != MobKind::Villager {
                continue;
            }

            if let Some(p) = v.job
                && world.loaded(p)
                && world.block(p).and_then(Profession::of) != Some(v.profession)
            {
                self.claimed_jobs.remove(&p);
                v.job = None;
                if v.xp == 0 {
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
            if v.home.is_none() || v.job.is_none() && m.age >= 0 && v.profession != Profession::Nitwit {
                let mut home = None;
                let mut job = None;
                let mut hd = 48.0f64.powi(2);
                let mut jd = hd;
                world.village_pois(&mut |p, b| {
                    let dist = m.pos.distance_squared(p.as_dvec3());
                    let prof = Profession::of(b);
                    if b.is_bed_head()
                        && (dist < hd || dist == hd && home.is_some_and(|h: IVec3| p.to_array() < h.to_array()))
                        && !self.claimed_beds.contains(&p)
                    {
                        hd = dist;
                        home = Some(p);
                    } else if prof.is_some()
                        && (v.xp == 0 || prof == Some(v.profession))
                        && (dist < jd
                            || dist == jd && job.is_some_and(|j: (IVec3, Profession)| p.to_array() < j.0.to_array()))
                        && !self.claimed_jobs.contains(&p)
                    {
                        jd = dist;
                        job = Some((p, prof.unwrap()));
                    }
                });
                if v.home.is_none() {
                    v.home = home;
                    if let Some(p) = home {
                        self.claimed_beds.insert(p);
                    }
                }
                if v.job.is_none()
                    && m.age >= 0
                    && v.profession != Profession::Nitwit
                    && let Some((p, prof)) = job
                {
                    v.job = Some(p);
                    self.claimed_jobs.insert(p);
                    v.set_profession(prof);
                }
            }
            v.fleeing = threat.is_some();
            v.sleeping = false;
            v.goal = if let Some(z) = threat {
                let d = (m.pos - z) * DVec3::new(1.0, 0.0, 1.0);
                Some(m.pos + d.normalize_or_zero() * 10.0)
            } else if v.bell_hide > 0.0 {
                v.home.map(|p| p.as_dvec3() + DVec3::new(0.5, 0.6, 0.5))
            } else if v.trading {
                None
            } else if tick >= 12000 {
                v.home.map(|p| p.as_dvec3() + DVec3::new(0.5, 0.6, 0.5))
            } else if (2000..9000).contains(&tick) && m.age >= 0 {
                v.job.map(|p| p.as_dvec3() + DVec3::new(0.5, 0.0, 0.5))
            } else {
                None
            };
            if tick >= 12000
                && !v.fleeing
                && v.bell_hide <= 0.0
                && !v.trading
                && let Some(p) = v.home
            {
                v.sleeping = m.pos.distance_squared(p.as_dvec3() + DVec3::new(0.5, 0.6, 0.5)) < 2.25;
            }
            if v.sleeping {
                v.last_slept = Some(self.village_day * 24000 + tick as i64);
            }
            v.restock(
                self.village_day,
                tick,
                !v.fleeing && !v.sleeping && v.job.is_some_and(|p| m.pos.distance_squared(p.as_dvec3() + 0.5) < 4.0),
            );
        }
        self.breed_villagers(world);
        self.life_tick(world);
    }
    pub fn villagers_to_string(&self) -> String {
        let mobs: Vec<Value> = self
            .mobs
            .iter()
            .filter(|m| m.alive())
            .filter_map(|m| match m.kind {
                MobKind::IronGolem | MobKind::SnowGolem | MobKind::TraderLlama => Some(json!({
                    "kind": m.kind.name(), "p": m.pos.to_array(), "yaw": m.yaw,
                    "health": m.health, "built": m.built, "anger": m.player_hit_left, "anger_player":m.angry_player.map(|p|p.0), "trader": m.trader.as_ref().map(|t|t.save()),
                })),
                MobKind::Villager | MobKind::ZombieVillager | MobKind::WanderingTrader => {
                    let v = m.villager.as_ref()?;
                    Some(json!({"trader":m.trader.as_ref().map(|t|t.save()),"kind":m.kind.name(),"id":v.id,"seed":v.seed,"p":m.pos.to_array(),"yaw":m.yaw,
                    "health":m.health,"age":m.age,"built":m.built,"armor":m.armor.map(|a|a.map(|a|a as u8)),"glint":m.armor_glint,"profession":v.profession as u8,"level":v.level,"xp":v.xp,
                    "job":v.job.map(|p|p.to_array()),"home":v.home.map(|p|p.to_array()),
                    "offers":v.offers.map(|o|o.map(Offer::save)),"day":v.restock_day,"restocks":v.restocks,
                    "last":v.last_restock,"slept":v.last_slept,"gossip":v.gossip.save(),"food":v.food.map(stack_to_string),"food_level":v.food_level,"bell_hide":v.bell_hide,"reputation":v.reputation,"weakness":m.weakness_left,"convert":m.convert_left,"convert_by":m.convert_by.map(|p|p.0)}))
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
                    if kind == MobKind::TraderLlama {
                        m.trader = Some(Box::new(super::wandering_trader::Trader::load(&a["trader"], p)?));
                    }
                    m.health = health.min(kind.max_health());
                    m.built = a["built"].as_bool().unwrap_or(false);
                    m.angry_player =
                        a["anger_player"].as_u64().and_then(|p| u32::try_from(p).ok()).map(super::PlayerId);
                    m.player_hit_left = a["anger"].as_f64().unwrap_or(0.0).clamp(0.0, 60.0) as f32;
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
                v.job = pos(&a["job"]);
                for (i, o) in a["offers"].as_array()?.iter().take(10).enumerate() {
                    if !o.is_null() {
                        v.offers[i] = Some(Offer::load(o)?);
                    }
                }
                v.restock_day = a["day"].as_i64()?;
                v.restocks = (a["restocks"].as_u64()?.min(2)) as u8;
                v.last_restock = a["last"].as_u64()?.min(23999) as u32;
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
                v.gossip = super::villager_gossip::Gossip::load(&a["gossip"]);
                v.reputation = i16::try_from(a["reputation"].as_i64().unwrap_or(0)).unwrap_or(0);
                let mut m = Mob::new(kind, p, yaw);
                m.health = health.min(kind.max_health());
                m.age = i32::try_from(a["age"].as_i64()?).ok()?.clamp(-24000, 6000);
                m.baby = m.age < 0;
                m.built = a["built"].as_bool().unwrap_or(false);
                m.weakness_left = a["weakness"].as_f64().unwrap_or(0.0).clamp(0.0, 600.0) as f32;
                m.convert_left = a["convert"].as_f64().unwrap_or(0.0).clamp(0.0, 600.0) as f32;
                m.convert_by = a["convert_by"]
                    .as_u64()
                    .and_then(|p| u32::try_from(p).ok())
                    .map(super::PlayerId)
                    .or_else(|| (m.convert_left > 0.0).then_some(super::PlayerId::HOST));
                if let Some(armor) = a["armor"].as_array() {
                    for (slot, value) in m.armor.iter_mut().zip(armor) {
                        *slot = value.as_u64().and_then(|i| super::armor::ArmorKind::ALL.get(i as usize).copied());
                    }
                }
                m.armor_glint = a["glint"].as_u64().unwrap_or(0).min(15) as u8;
                if kind == MobKind::WanderingTrader {
                    m.trader = Some(Box::new(super::wandering_trader::Trader::load(&a["trader"], p)?));
                }
                m.villager = Some(Box::new(v));
                Some(m)
            };
            if let Some(m) = load() {
                if let Some(v) = m.villager.as_ref() {
                    self.next_villager_id = self.next_villager_id.max(v.id.saturating_add(1));
                }
                self.mobs.push(m);
            }
        }
    }
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
    fn fixed() -> Villager {
        let mut v = Villager::new(1, 42);
        v.profession = Profession::Farmer;
        v.offers[0] = TradeDef::buy("wheat", 20, 16, 2).offer(9);
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
        assert_eq!((v.level, v.xp), (2, 10));
        assert_eq!(v.offers[0].unwrap().uses, 5);
        assert!(v.offers[2].is_some());
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
        v.offers[0] = TradeDef::book(2).offer(6);
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
    fn restock_requires_work_and_allows_two_per_day_with_demand() {
        let mut v = fixed();
        v.offers[0].as_mut().unwrap().uses = 16;
        assert!(!v.restock(0, 1500, true));
        assert!(!v.restock(0, 2000, false));
        assert!(v.restock(0, 2000, true));
        assert_eq!(v.offers[0].unwrap().price().count, 36);
        v.reputation = 100;
        assert_eq!(v.priced(v.offers[0].unwrap()).count, 31);
        v.offers[0].as_mut().unwrap().uses = 16;
        assert!(!v.restock(0, 3000, true));
        assert!(v.restock(0, 4400, true));
        v.offers[0].as_mut().unwrap().uses = 1;
        assert!(!v.restock(0, 8000, true));
        assert!(!v.restock(1, 13000, true));
        assert!(v.restock(1, 2000, true));
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
        let farmer = tables::TRADES
            .iter()
            .find(|(p, l, d)| *p == Profession::Farmer && *l == 1 && d.input == "wheat")
            .unwrap()
            .2
            .offer(4)
            .unwrap();
        assert_eq!((farmer.cost.count, farmer.output.item, farmer.max_uses, farmer.xp), (20, Item::EMERALD, 16, 2));
        let enchanted = TradeDef::gear("diamond pickaxe", 13, 3, 30, 0.2).offer(4).unwrap();
        assert!(!enchanted.output.enchants.is_empty());
        assert!((18..=32).contains(&enchanted.cost.count));
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
