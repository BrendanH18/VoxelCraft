//! Saddleable animals and Java horse attributes, temper, equipment and rider controls.
use super::{Entities, Mob, MobKind, PlayerId, Rng};
use crate::inventory::{Inventory, Stack, stack_from_str, stack_to_string};
use crate::item::Item;
use crate::physics::{self, BlockSource};
use crate::player::{MoveInput, Player};
use glam::DVec3;
use serde_json::{Value, json};

/// Mob vehicle ids occupy the upper half; carts keep their existing ids.
pub const MOB_VEHICLE: u32 = 1 << 31;

/// Java's `AbstractHorse` family: temper taming, mount health and inventory, horse fall damage.
pub fn equine(kind: MobKind) -> bool {
    matches!(kind, MobKind::Horse | MobKind::Donkey | MobKind::Mule | MobKind::Llama | MobKind::Camel)
}
pub fn rideable(kind: MobKind) -> bool {
    equine(kind) || matches!(kind, MobKind::Pig | MobKind::Strider)
}
/// Mounts with a charged jump (a camel's charge is its dash).
pub fn jumps(kind: MobKind) -> bool {
    matches!(kind, MobKind::Horse | MobKind::Donkey | MobKind::Mule | MobKind::Camel)
}
/// Donkeys, mules and llamas can carry a chest.
pub fn chested(kind: MobKind) -> bool {
    matches!(kind, MobKind::Donkey | MobKind::Mule | MobKind::Llama)
}
/// Java's `getMaxTemper`: llamas tame within 30 temper, other horses within 100.
pub fn max_temper(kind: MobKind) -> u8 {
    if kind == MobKind::Llama { 30 } else { 100 }
}
/// Carpets are llama decor.
pub fn is_carpet(item: Item) -> bool {
    item.block().is_some_and(|b| b.carpet_color().is_some())
}
/// Ticks between camel dashes (Java's 55).
pub const DASH_COOLDOWN: u16 = 55;

/// Java's `AbstractHorse.createOffspringAttribute`: the parents' mean, spread by a
/// triangular random draw, reflected back inside the attribute's natural range.
pub fn offspring_attribute(a: f64, b: f64, min: f64, max: f64, rng: &mut Rng) -> f64 {
    let (a, b) = (a.clamp(min, max), b.clamp(min, max));
    let spread = (a - b).abs() + 0.15 * (max - min) * 2.;
    let draw = (rng.next_f32() as f64 + rng.next_f32() as f64 + rng.next_f32() as f64) / 3. - 0.5;
    let v = (a + b) / 2. + spread * draw;
    if v > max {
        max - (v - max)
    } else if v < min {
        min + (min - v)
    } else {
        v
    }
}

/// Foal stats and coat from its parents (no-op for animals without mount state).
pub(super) fn inherit(child: &mut Mob, a: &Mob, b: &Mob, rng: &mut Rng) {
    let (Some(c), Some(x), Some(y)) = (child.mount.as_mut(), a.mount.as_deref(), b.mount.as_deref()) else { return };
    if !equine(child.kind) {
        return;
    }
    if child.kind == MobKind::Camel {
        return;
    }
    c.max_health = offspring_attribute(x.max_health as f64, y.max_health as f64, 15., 30., rng) as f32;
    if child.kind == MobKind::Llama {
        // Java: a random strength up to the stronger parent's, rarely one more; a parent's coat.
        let top = x.strength.max(y.strength) as u32 + u32::from(rng.chance(0.03));
        c.strength = (1 + rng.next_int(top.max(1))).min(5) as u8;
        c.variant = if rng.chance(0.5) { x.variant } else { y.variant };
        child.health = c.max_health;
        return;
    }
    c.speed = offspring_attribute(x.speed, y.speed, 0.1125, 0.3375, rng);
    c.jump = offspring_attribute(x.jump, y.jump, 0.4, 1., rng);
    if child.kind == MobKind::Horse {
        // Java: each parent's coat 4/9 of the time, otherwise a random one; markings likewise.
        let mut pick = |p: u8, q: u8, n: u32| match rng.next_int(9) {
            0..=3 => p,
            4..=7 => q,
            _ => rng.next_int(n) as u8,
        };
        c.variant = pick(x.variant, y.variant, 7);
        c.marking = pick(x.marking, y.marking, 5);
    }
    child.health = c.max_health;
}

pub struct State {
    pub tame: bool,
    pub temper: u8,
    pub owner: Option<PlayerId>,
    pub rider: Option<PlayerId>,
    pub max_health: f32,
    /// Java movement_speed attribute, converted to blocks/sec by control().
    pub speed: f64,
    pub jump: f64,
    pub variant: u8,
    pub marking: u8,
    pub chest: bool,
    /// Llama strength 1..=5: three chest slots per point.
    pub strength: u8,
    /// A camel's second (rear) seat.
    pub passenger: Option<PlayerId>,
    /// Ticks until a camel can dash again.
    pub dash_cooldown: u16,
    /// Camel sitting: ticks left sitting down, and ticks left standing up after a rider mounts.
    pub sit_ticks: u32,
    pub stand_ticks: u16,
    /// Saddle (llamas: unused), horse armor or llama carpet, then fifteen storage slots.
    pub slots: [Option<Stack>; 17],
    pub jump_charge: f32,
    jump_ticks: u32,
    jump_held: bool,
    pending_jump: Option<f32>,
    pub boost_ticks: u16,
    pub boost_total: u16,
    wish: DVec3,
    yaw: f32,
    steering: bool,
    sprinting: bool,
    pub fall_distance: f64,
    pub(super) rider_fall: f32,
    regen_ticks: f32,
}

impl State {
    pub fn new(kind: MobKind, rng: &mut Rng) -> Self {
        let horse = kind == MobKind::Horse;
        let camel = kind == MobKind::Camel;
        let max_health =
            if equine(kind) && !camel { (15 + rng.next_int(8) + rng.next_int(9)) as f32 } else { kind.max_health() };
        let speed = if horse {
            (0.45 + rng.next_f32() as f64 * 0.3 + rng.next_f32() as f64 * 0.3 + rng.next_f32() as f64 * 0.3) * 0.25
        } else if camel {
            0.09
        } else {
            0.175
        };
        // Java: strength 1..=3, or 1..=5 one time in 25.
        let strength = if kind == MobKind::Llama {
            let top = if rng.chance(0.04) { 5 } else { 3 };
            1 + rng.next_int(top) as u8
        } else {
            3
        };
        let jump = if horse {
            0.4 + rng.next_f32() as f64 * 0.2 + rng.next_f32() as f64 * 0.2 + rng.next_f32() as f64 * 0.2
        } else {
            0.5
        };
        Self {
            // Camels need no taming, only a saddle.
            tame: !equine(kind) || camel,
            temper: 0,
            owner: None,
            rider: None,
            max_health,
            speed,
            jump,
            variant: match kind {
                MobKind::Horse => rng.next_int(7) as u8,
                MobKind::Llama => rng.next_int(4) as u8,
                _ => 0,
            },
            marking: if horse { rng.next_int(5) as u8 } else { 0 },
            chest: false,
            strength,
            passenger: None,
            dash_cooldown: 0,
            sit_ticks: 0,
            stand_ticks: 0,
            slots: [None; 17],
            jump_charge: 0.,
            jump_ticks: 0,
            jump_held: false,
            pending_jump: None,
            boost_ticks: 0,
            boost_total: 0,
            wish: DVec3::ZERO,
            yaw: 0.,
            steering: false,
            sprinting: false,
            fall_distance: 0.,
            rider_fall: 0.,
            regen_ticks: 0.,
        }
    }
    pub fn controlled(&self) -> bool {
        self.rider.is_some() && self.steering
    }
    pub fn saddled(&self) -> bool {
        self.slots[0].is_some_and(|s| s.item == Item::SADDLE)
    }
    pub fn slot_count(&self, kind: MobKind) -> usize {
        match kind {
            MobKind::Llama if self.chest => 2 + 3 * self.strength as usize,
            _ if self.chest => 17,
            MobKind::Horse | MobKind::Llama => 2,
            _ => 1,
        }
    }
    pub fn accepts(&self, kind: MobKind, slot: usize, item: Item) -> bool {
        if !self.tame {
            return false;
        }
        match slot {
            0 => item == Item::SADDLE && kind != MobKind::Llama,
            1 => kind == MobKind::Horse && armor_points(item) > 0 || kind == MobKind::Llama && is_carpet(item),
            2..=16 => self.chest && slot < self.slot_count(kind),
            _ => false,
        }
    }
    pub fn input(
        &mut self,
        kind: MobKind,
        player: &Player,
        input: MoveInput,
        held: Option<Item>,
        offhand: Option<Item>,
    ) {
        self.wish = player.ride_push(input) / 0.22;
        self.yaw = player.yaw;
        self.sprinting = input.sprint;
        let steering_item = match kind {
            MobKind::Pig => Some(Item::CARROT_ON_A_STICK),
            MobKind::Strider => Some(Item::WARPED_FUNGUS_ON_A_STICK),
            _ => None,
        };
        self.steering =
            self.saddled() && self.tame && steering_item.is_none_or(|i| held == Some(i) || offhand == Some(i));
        if equine(kind) && self.steering {
            if input.jump {
                self.jump_ticks = self.jump_ticks.saturating_add(1);
                self.jump_charge = jump_charge(self.jump_ticks);
            } else if self.jump_held {
                self.pending_jump = Some(self.jump_charge);
                self.jump_ticks = 0;
                self.jump_charge = 0.;
            }
        }
        self.jump_held = input.jump;
    }
    pub fn save(&self) -> Value {
        json!({"tame":self.tame,"temper":self.temper,"owner":self.owner.map(|p|p.0),"rider":self.rider.map(|p|p.0),
            "health":self.max_health,"speed":self.speed,"jump":self.jump,"variant":self.variant,"marking":self.marking,
            "chest":self.chest,"strength":self.strength,"passenger":self.passenger.map(|p|p.0),
            "slots":self.slots.iter().map(|s|stack_to_string(*s)).collect::<Vec<_>>()})
    }
    pub fn load(kind: MobKind, v: &Value) -> Option<Self> {
        let mut s = Self::new(kind, &mut Rng::new(0));
        s.tame = v["tame"].as_bool()?;
        s.temper = u8::try_from(v["temper"].as_u64()?).ok()?.min(100);
        s.owner = v["owner"].as_u64().and_then(|p| u32::try_from(p).ok()).map(PlayerId);
        s.rider = v["rider"].as_u64().and_then(|p| u32::try_from(p).ok()).map(PlayerId);
        s.max_health = v["health"].as_f64()? as f32;
        s.speed = v["speed"].as_f64()?;
        s.jump = v["jump"].as_f64()?;
        if !s.max_health.is_finite()
            || !(1. ..=40.).contains(&s.max_health)
            || !(0.05..=0.4).contains(&s.speed)
            || !(0.4..=1.).contains(&s.jump)
        {
            return None;
        }
        s.variant = v["variant"].as_u64()?.min(6) as u8;
        s.marking = v["marking"].as_u64()?.min(4) as u8;
        s.chest = v["chest"].as_bool().unwrap_or(false) && chested(kind);
        s.strength = v["strength"].as_u64().map_or(s.strength, |n| n.clamp(1, 5) as u8);
        s.passenger = v["passenger"].as_u64().and_then(|p| u32::try_from(p).ok()).map(PlayerId);
        if kind == MobKind::Llama {
            s.variant = s.variant.min(3);
        }
        for (i, piece) in v["slots"].as_array()?.iter().take(17).enumerate() {
            let stack = stack_from_str(piece.as_str()?)?;
            if let Some(stack) = stack
                && !s.accepts(kind, i, stack.item)
            {
                return None;
            }
            s.slots[i] = stack;
        }
        Some(s)
    }
}

pub fn armor_points(item: Item) -> u32 {
    match item {
        Item::LEATHER_HORSE_ARMOR => 3,
        Item::IRON_HORSE_ARMOR => 5,
        Item::GOLDEN_HORSE_ARMOR => 7,
        Item::DIAMOND_HORSE_ARMOR => 11,
        _ => 0,
    }
}
/// Java charge peaks after ten ticks then drops toward 0.8 for a long hold.
pub fn jump_charge(ticks: u32) -> f32 {
    if ticks < 10 { ticks as f32 * 0.1 } else { 0.8 + 2. / (ticks - 9) as f32 * 0.1 }
}

/// Java's horse jump height in blocks for a jump strength `j` (the Minecraft Wiki's fit of
/// vanilla's per-tick motion: 1.1 blocks at 0.4 up to 5.3 at 1.0).
fn java_peak(j: f64) -> f64 {
    ((-0.181_758_495_2 * j + 3.689_713_992) * j + 2.128_599_134) * j - 0.343_930_367
}

/// Launch speed (blocks/s) that reaches Java's peak for jump strength `v` under
/// this crate's continuous equine physics: dv/dt = -g - c·v, with g = 32 and c = -20·ln 0.98.
pub fn launch_speed(v: f64) -> f64 {
    const G: f64 = 32.;
    let c = -20. * 0.98f64.ln();
    let peak = |v0: f64| v0 / c - G / (c * c) * (1. + c * v0 / G).ln();
    let target = java_peak(v);
    let (mut lo, mut hi) = (0., 80.);
    for _ in 0..50 {
        let mid = (lo + hi) / 2.;
        if peak(mid) < target { lo = mid } else { hi = mid }
    }
    lo
}

/// Java `handleEating`: health healed, temper gained and baby growth in ticks.
pub fn food(kind: MobKind, item: Item) -> Option<(f32, u8, i32)> {
    let hay = Item::from_block(crate::world::block::Block::HAY_BALE);
    if kind == MobKind::Llama {
        return match item {
            Item::WHEAT => Some((2., 3, 200)),
            i if i == hay => Some((10., 6, 1800)),
            _ => None,
        };
    }
    if kind == MobKind::Camel {
        return (item == Item::from_block(crate::world::block::Block::CACTUS)).then_some((2., 0, 200));
    }
    Some(match item {
        Item::SUGAR => (1., 3, 600),
        Item::WHEAT => (2., 3, 400),
        Item::APPLE => (3., 3, 1200),
        Item::GOLDEN_CARROT => (4., 5, 1200),
        Item::GOLDEN_APPLE => (10., 10, 4800),
        i if i == Item::from_block(crate::world::block::Block::HAY_BALE) => (20., 0, 3600),
        _ => return None,
    })
}

/// Run before ordinary AI movement. Untamed mounts run and occasionally buck/tame.
/// Tamed unsaddled animals and pigs without a steering item keep their normal AI.
pub(super) fn control(mob: &mut Mob, dt: f64, rng: &mut Rng) -> Option<(Option<DVec3>, f64)> {
    let s = mob.mount.as_mut()?;
    if mob.health <= 0.0 {
        s.rider = None;
        s.passenger = None;
        return None;
    }
    let ticks = (dt * 20.).round() as u32;
    s.regen_ticks += dt as f32 * 20.;
    if equine(mob.kind) && s.regen_ticks >= 900. {
        mob.health = (mob.health + 1.).min(s.max_health);
        s.regen_ticks = 0.;
    }
    s.dash_cooldown = s.dash_cooldown.saturating_sub(ticks as u16);
    if mob.kind == MobKind::Camel {
        // Idle camels sit down now and then; a rider makes one stand, which takes 2.6 s.
        if s.rider.is_some() && s.sit_ticks > 0 {
            s.sit_ticks = 0;
            s.stand_ticks = 52;
        }
        if s.rider.is_none() && s.sit_ticks == 0 && !mob.baby && mob.on_ground && rng.chance(ticks as f32 / 2400.) {
            s.sit_ticks = 600 + rng.next_int(600);
        }
        if s.sit_ticks > 0 {
            s.sit_ticks = s.sit_ticks.saturating_sub(ticks);
            return Some((None, 0.));
        }
        if s.stand_ticks > 0 {
            s.stand_ticks = s.stand_ticks.saturating_sub(ticks as u16);
            return Some((None, 0.));
        }
    }
    let rider = s.rider?;
    if !s.tame {
        if rng.chance((dt as f32 * 20. / 50.).min(1.)) {
            if rng.next_int(max_temper(mob.kind) as u32) < s.temper as u32 {
                s.tame = true;
                s.owner = Some(rider);
            } else {
                s.temper = (s.temper + 5).min(max_temper(mob.kind));
                s.rider = None;
            }
        }
        return Some((Some(DVec3::new(mob.yaw.cos() as f64, 0., mob.yaw.sin() as f64)), 2.4));
    }
    if !s.steering {
        return None;
    }
    mob.yaw = s.yaw;
    let forward = DVec3::new(mob.yaw.cos() as f64, 0., mob.yaw.sin() as f64);
    if mob.kind == MobKind::Camel {
        // Java dash: forward 22.2 x charge x speed and up 1.43 x charge x 0.42 blocks/tick.
        if let Some(charge) = s.pending_jump.take()
            && mob.on_ground
            && s.dash_cooldown == 0
            && charge > 0.
        {
            let charge = charge as f64;
            mob.vel += forward * 22.2222 * charge * s.speed * 20. * 0.45;
            mob.vel.y = 1.4285 * charge * 0.42 * 20.;
            s.dash_cooldown = DASH_COOLDOWN;
        }
    } else if mob.on_ground
        && let Some(charge) = s.pending_jump.take()
    {
        mob.vel.y = launch_speed(s.jump * if charge >= 0.9 { 1. } else { 0.4 + 0.4 * charge as f64 / 0.9 });
    }
    let mut speed = if mob.kind == MobKind::Camel {
        // Java's ridden camel: +0.1 speed while sprinting with the dash ready.
        (s.speed + if s.sprinting && s.dash_cooldown == 0 { 0.1 } else { 0. }) * 43.17
    } else if equine(mob.kind) {
        s.speed * 43.17
    } else if mob.kind == MobKind::Pig {
        2.42
    } else if mob.nether.as_ref().is_some_and(|n| n.cold) {
        1.74
    } else {
        4.14
    };
    if s.boost_total > 0 {
        s.boost_ticks = s.boost_ticks.saturating_add(1);
        if s.boost_ticks > s.boost_total {
            s.boost_total = 0;
            s.boost_ticks = 0;
        } else {
            speed *= 1. + 1.15 * (std::f64::consts::PI * s.boost_ticks as f64 / s.boost_total as f64).sin();
        }
    }
    Some((Some(if equine(mob.kind) { s.wish } else { forward }), speed))
}

impl Entities {
    /// Java llamas spit (1 damage) at whoever hurt them, every two seconds while provoked.
    pub(super) fn llama_spit(&mut self, ctx: &super::Ctx, events: &mut Vec<super::EntityEvent>) {
        for m in &mut self.mobs {
            if m.kind != MobKind::Llama || !m.alive() || m.attack_cooldown > 0.0 {
                continue;
            }
            let Some(target) = m.angry_player.and_then(|id| ctx.players.iter().find(|t| t.id == id && t.targetable))
            else {
                continue;
            };
            let mouth = m.pos + DVec3::Y * (m.shape().height * 0.9);
            let aim = target.pos + DVec3::Y * 1.4 - mouth;
            if aim.length_squared() > 16.0 * 16.0 {
                continue;
            }
            m.look_at(target.pos, 1.0);
            m.attack_cooldown = 2.0;
            let mut b = crate::particles::Burst::new(crate::particles::Kind::Poof, mouth, 6);
            b.velocity_spread = DVec3::splat(0.01);
            b.color = Some([0.95, 0.95, 0.9, 1.0]);
            self.particles.push(crate::particles::Request::Burst(b));
            let push = (aim.with_y(0.0).normalize_or_zero() * 0.2).as_vec3();
            events.push(super::EntityEvent::PlayerHit {
                player: target.id,
                damage: 1.0,
                knockback: push,
                cause: "was spat on by a llama",
            });
        }
    }
}

impl Entities {
    /// Assign identities before scripted or interactive riding, including newly spawned mobs.
    pub fn ensure_mob_ids(&mut self) {
        for m in &mut self.mobs {
            if m.uid == 0 {
                self.next_uid = self.next_uid.max(1);
                m.uid = self.next_uid;
                self.next_uid = self.next_uid.wrapping_add(1).max(1);
            }
        }
    }
    /// Keep the physical player and camera on their seat, or clear a lost vehicle.
    pub fn seat_player(&self, player: &mut Player, id: PlayerId) {
        let Some(vehicle) = player.vehicle else { return };
        let seat = self.mount_seat(vehicle, id).or_else(|| self.cart(vehicle)?.seat_for(id));
        if let Some(pos) = seat {
            player.pos = pos;
            player.vel = DVec3::ZERO;
            player.on_ground = true;
        } else {
            if let Some(cart) = self.cart(vehicle) {
                player.pos = cart.pos + DVec3::X;
            }
            player.vehicle = None;
            player.vel = DVec3::ZERO;
        }
    }
    pub fn mount(&self, vehicle: u32) -> Option<&Mob> {
        (vehicle & MOB_VEHICLE != 0)
            .then(|| self.mobs.iter().find(|m| m.uid == vehicle & !MOB_VEHICLE && m.alive()))
            .flatten()
    }
    pub fn mount_mut(&mut self, vehicle: u32) -> Option<&mut Mob> {
        (vehicle & MOB_VEHICLE != 0)
            .then(|| self.mobs.iter_mut().find(|m| m.uid == vehicle & !MOB_VEHICLE && m.alive()))
            .flatten()
    }
    pub fn mount_seat(&self, vehicle: u32, player: PlayerId) -> Option<DVec3> {
        let m = self.mount(vehicle)?;
        let s = m.mount.as_ref()?;
        let seat = m.pos + DVec3::Y * (m.shape().height * 0.75 - 0.35);
        if m.kind != MobKind::Camel {
            return (s.rider == Some(player)).then_some(seat);
        }
        // Java camel seats: 0.5 blocks either side of centre when both are taken.
        let forward = DVec3::new(m.yaw.cos() as f64, 0., m.yaw.sin() as f64) * 0.5;
        let seat = seat - DVec3::Y * if s.sit_ticks > 0 { 0.9 } else { 0. };
        if s.rider == Some(player) {
            Some(if s.passenger.is_some() { seat + forward } else { seat })
        } else {
            (s.passenger == Some(player)).then_some(seat - forward)
        }
    }
    pub fn mount_for_player(&self, player: PlayerId) -> Option<u32> {
        self.mobs
            .iter()
            .find(|m| {
                m.alive() && m.mount.as_ref().is_some_and(|s| s.rider == Some(player) || s.passenger == Some(player))
            })
            .map(|m| MOB_VEHICLE | m.uid)
    }
    pub fn mount_input(
        &mut self,
        id: PlayerId,
        player: &Player,
        input: MoveInput,
        inventory: &Inventory,
        selected: usize,
    ) {
        if let Some(vehicle) = player.vehicle
            && let Some(m) = self.mount_mut(vehicle)
            && let Some(s) = m.mount.as_mut()
            && s.rider == Some(id)
        {
            s.input(m.kind, player, input, inventory.get(selected).map(|s| s.item), None);
        }
    }
    /// Use saddle/armor/chest/food or mount an adult under the crosshair.
    /// Returns true when the interaction was handled; callers attach via mount_for_player.
    #[allow(clippy::too_many_arguments)]
    pub fn use_mount(
        &mut self,
        eye: DVec3,
        dir: DVec3,
        reach: f64,
        player: PlayerId,
        inventory: &mut Inventory,
        selected: usize,
        creative: bool,
    ) -> bool {
        let Some((i, _)) = self.raycast(eye, dir, reach) else { return false };
        self.ensure_mob_ids();
        let m = &mut self.mobs[i];
        if !rideable(m.kind) || m.riding.is_some() {
            return false;
        }
        let s = m.mount.as_mut().unwrap();
        let kind = m.kind;
        let held = inventory.get(selected).map(|s| s.item);
        let mut consume = false;
        if held == Some(Item::SADDLE) && s.tame && !s.saddled() && !m.baby && kind != MobKind::Llama {
            s.slots[0] = Some(Stack::new(Item::SADDLE, 1));
            consume = true;
        } else if let Some(item) = held
            && matches!(kind, MobKind::Horse | MobKind::Llama)
            && !m.baby
            && s.accepts(kind, 1, item)
        {
            // Horse armor or llama carpet: equip, returning what was worn.
            let previous = s.slots[1].replace(Stack::new(item, 1));
            if !creative {
                inventory.take_one(selected);
            }
            m.persistent = true;
            let pos = m.pos;
            if let Some(stack) = previous {
                let count = inventory.add_stack(stack);
                if count > 0 {
                    self.scatter(Stack { count, ..stack }, pos);
                }
            }
            return true;
        } else if held == Some(Item::from_block(crate::world::block::Block::CHEST))
            && s.tame
            && !m.baby
            && !s.chest
            && chested(kind)
        {
            s.chest = true;
            consume = true;
        } else if let Some((heal, temper, growth)) = held.and_then(|i| food(kind, i))
            && equine(kind)
        {
            if m.baby {
                m.age = (m.age + growth).min(0);
                m.baby = m.age < 0;
                consume = true;
            }
            let max = max_temper(kind);
            if m.health < s.max_health || !s.tame && s.temper < max {
                m.health = (m.health + heal).min(s.max_health);
                s.temper = s.temper.saturating_add(temper).min(max);
                consume = true;
            } else if !consume {
                return false;
            }
        } else if !m.baby
            && s.rider.is_none()
            && (equine(kind) && (s.tame || held.is_none()) || !equine(kind) && s.saddled())
        {
            s.rider = Some(player);
        } else if !m.baby
            && kind == MobKind::Camel
            && s.rider.is_some_and(|r| r != player)
            && s.passenger.is_none()
            && held.is_none_or(|i| food(kind, i).is_none())
        {
            // A camel's rear seat.
            s.passenger = Some(player);
        } else {
            return false;
        }
        if consume && !creative {
            inventory.take_one(selected);
        }
        m.persistent = true;
        true
    }
    pub fn boost_mount(
        &mut self,
        player: PlayerId,
        inventory: &mut Inventory,
        selected: usize,
        creative: bool,
    ) -> bool {
        let Some(vehicle) = self.mount_for_player(player) else { return false };
        let m = self.mount_mut(vehicle).unwrap();
        let (item, wear) = match m.kind {
            MobKind::Pig => (Item::CARROT_ON_A_STICK, 7),
            MobKind::Strider => (Item::WARPED_FUNGUS_ON_A_STICK, 1),
            _ => return false,
        };
        let Some(stack) = inventory.get(selected) else { return false };
        if stack.item != item || stack.damage >= item.durability().unwrap() {
            return false;
        }
        let s = m.mount.as_mut().unwrap();
        if !s.saddled() || s.boost_total > 0 {
            return false;
        }
        // Java nextInt(841) + 140: 7–49 seconds, smooth sine speed boost.
        let total = self.rng.next_int(841) as u16 + 140;
        let s = self.mount_mut(vehicle).unwrap().mount.as_mut().unwrap();
        s.boost_total = total;
        s.boost_ticks = 0;
        if !creative && inventory.wear(selected, wear) {
            inventory.slots[selected] = Some(Stack { enchants: stack.enchants, ..Stack::new(Item::FISHING_ROD, 1) });
        }
        true
    }
    pub fn dismount_animal<W: BlockSource + ?Sized>(&mut self, world: &W, player: PlayerId) -> Option<DVec3> {
        let id = self.mount_for_player(player)?;
        let m = self.mount_mut(id)?;
        let s = m.mount.as_mut()?;
        if s.rider == Some(player) {
            // The rear passenger slides forward, as in Java.
            s.rider = s.passenger.take();
        } else {
            s.passenger = None;
        }
        let side = DVec3::new(-(m.yaw.sin() as f64), 0., m.yaw.cos() as f64) * (m.shape().half_width + 0.4);
        for offset in [side, -side, DVec3::X * 1.5, DVec3::NEG_X * 1.5, DVec3::Z * 1.5, DVec3::NEG_Z * 1.5] {
            for dy in [0., 1., -1.] {
                let mut pos = m.pos + offset + DVec3::Y * dy;
                if !physics::overlaps_solid(world, pos, crate::player::SHAPE)
                    && !physics::touches_block(world, pos, crate::player::SHAPE, |b| b.is_lava())
                {
                    let mut velocity = DVec3::NEG_Y;
                    let floor =
                        physics::move_box(world, &mut pos, &mut velocity, DVec3::NEG_Y * 2., crate::player::SHAPE);
                    if floor.on_ground && !physics::touches_block(world, pos, crate::player::SHAPE, |b| b.is_lava()) {
                        return Some(pos);
                    }
                }
            }
        }
        Some(m.pos + DVec3::Y * m.shape().height)
    }
}

impl Entities {
    /// Shared container access: cart ids stay unchanged, equines use tagged mob ids.
    pub fn vehicle_slots(&self, id: u32) -> Option<&[Option<Stack>]> {
        if id & MOB_VEHICLE == 0 {
            let c = self.cart(id)?;
            return Some(&c.slots[..c.slot_count()]);
        }
        let m = self.mount(id)?;
        let s = m.mount.as_ref()?;
        (equine(m.kind) && s.tame && !m.baby).then(|| &s.slots[..s.slot_count(m.kind)])
    }
    pub fn vehicle_slots_mut(&mut self, id: u32) -> Option<&mut [Option<Stack>]> {
        if id & MOB_VEHICLE == 0 {
            let c = self.cart_mut(id)?;
            let n = c.slot_count();
            return Some(&mut c.slots[..n]);
        }
        let m = self.mount_mut(id)?;
        let s = m.mount.as_mut()?;
        let n = s.slot_count(m.kind);
        (equine(m.kind) && s.tame && !m.baby).then(|| &mut s.slots[..n])
    }
    pub fn vehicle_accepts(&self, id: u32, slot: usize, item: Item) -> bool {
        if id & MOB_VEHICLE == 0 {
            return self.vehicle_slots(id).is_some_and(|s| slot < s.len());
        }
        self.mount(id).is_some_and(|m| {
            m.mount.as_ref().is_some_and(|s| slot < s.slot_count(m.kind) && s.accepts(m.kind, slot, item))
        })
    }
    pub fn insert_vehicle(&mut self, id: u32, mut stack: Stack) -> Option<Stack> {
        let n = self.vehicle_slots(id)?.len();
        for i in 0..n {
            if !self.vehicle_accepts(id, i, stack.item) {
                continue;
            }
            let slots = self.vehicle_slots_mut(id)?;
            let cell = &mut slots[i];
            let room = match cell {
                None => stack.max(),
                Some(s) if s.stacks_with(&stack) => s.max() - s.count,
                _ => 0,
            };
            let count = room.min(stack.count);
            if count > 0 {
                *cell = Some(Stack { count: cell.map_or(0, |s| s.count) + count, ..stack });
                stack.count -= count;
            }
            if stack.count == 0 {
                return None;
            }
        }
        Some(stack)
    }
    pub fn vehicle_title(&self, id: u32) -> &'static str {
        if let Some(m) = self.mount(id) {
            return match m.kind {
                MobKind::Horse => "Horse (saddle, armor)",
                MobKind::Donkey => "Donkey (saddle, storage)",
                MobKind::Mule => "Mule (saddle, storage)",
                _ => "Mount",
            };
        }
        if let Some(c) = self.cart(id) {
            return if c.kind.boat().is_some() {
                "Boat with Chest"
            } else if c.slot_count() == 5 {
                "Minecart with Hopper"
            } else {
                "Minecart with Chest"
            };
        }
        "Storage"
    }
    pub fn vehicle_container(&self, eye: DVec3, dir: DVec3, reach: f64) -> Option<u32> {
        let cart = self.cart_container(eye, dir, reach).map(|(id, _)| id);
        let animal = self.raycast(eye, dir, reach).and_then(|(i, _)| {
            let m = &self.mobs[i];
            (equine(m.kind) && !m.baby && m.mount.as_ref().is_some_and(|s| s.tame)).then_some(MOB_VEHICLE | m.uid)
        });
        match (cart, animal) {
            (Some(c), Some(m)) => {
                if self.cart(c)?.pos.distance_squared(eye) < self.mount(m)?.pos.distance_squared(eye) {
                    Some(c)
                } else {
                    Some(m)
                }
            }
            (c, m) => c.or(m),
        }
    }
    pub fn vehicle_in_reach(&self, id: u32, eye: DVec3) -> bool {
        let pos = self.cart(id).map(|c| c.pos).or_else(|| self.mount(id).map(|m| m.pos));
        self.vehicle_slots(id).is_some() && pos.is_some_and(|p| p.distance(eye) <= 7.)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::world::block::Block;
    use crate::world::{World, chunk::ChunkData, terrain::Generator};
    use glam::IVec3;
    use std::sync::Arc;

    fn world() -> World {
        let mut w = World::new_headless(Arc::new(Generator::new(1)), Default::default(), 2);
        w.insert_chunk(IVec3::new(0, 4, 0), Arc::new(ChunkData::Uniform(Block::AIR)), false);
        for x in 0..16 {
            for z in 0..16 {
                w.set_block(IVec3::new(x, 139, z), Block::STONE);
            }
        }
        w
    }
    #[test]
    fn java_horse_distributions_and_fixed_pack_animal_attributes() {
        let mut rng = Rng::new(17);
        let mut totals = [0.; 3];
        for _ in 0..10000 {
            let s = State::new(MobKind::Horse, &mut rng);
            assert!((15. ..=30.).contains(&s.max_health));
            assert!((0.1125..0.3375).contains(&s.speed));
            assert!((0.4..1.).contains(&s.jump));
            assert!(s.variant < 7 && s.marking < 5);
            totals[0] += s.max_health as f64;
            totals[1] += s.speed;
            totals[2] += s.jump;
        }
        assert!((totals[0] / 10000. - 22.5).abs() < 0.15);
        assert!((totals[1] / 10000. - 0.225).abs() < 0.002);
        assert!((totals[2] / 10000. - 0.7).abs() < 0.005);
        for kind in [MobKind::Donkey, MobKind::Mule] {
            let s = State::new(kind, &mut rng);
            assert_eq!((s.speed, s.jump), (0.175, 0.5));
        }
        assert!(!MobKind::Mule.is_breedable());
    }
    #[test]
    fn bucking_raises_temper_and_a_full_temper_mount_tames_to_its_rider() {
        let mut m = Mob::new(MobKind::Horse, DVec3::ZERO, 0.);
        let mut rng = Rng::new(5);
        m.mount.as_mut().unwrap().rider = Some(PlayerId(7));
        control(&mut m, 2.5, &mut rng);
        let s = m.mount.as_ref().unwrap();
        assert_eq!((s.temper, s.rider, s.tame), (5, None, false));
        let s = m.mount.as_mut().unwrap();
        s.temper = 100;
        s.rider = Some(PlayerId(7));
        control(&mut m, 2.5, &mut rng);
        let s = m.mount.as_ref().unwrap();
        assert!(s.tame);
        assert_eq!(s.owner, Some(PlayerId(7)));
    }
    #[test]
    fn pig_needs_a_saddle_and_matching_stick_and_boost_uses_durability_once() {
        let mut e = Entities::new(1);
        e.spawn(MobKind::Pig, DVec3::new(5., 140., 5.));
        let mut inv = Inventory::default();
        inv.slots[0] = Some(Stack::new(Item::SADDLE, 1));
        let eye = DVec3::new(5., 140.5, 2.);
        assert!(e.use_mount(eye, DVec3::Z, 5., PlayerId::HOST, &mut inv, 0, false));
        assert!(inv.slots[0].is_none());
        assert!(e.use_mount(eye, DVec3::Z, 5., PlayerId::HOST, &mut inv, 0, false));
        let id = e.mount_for_player(PlayerId::HOST).unwrap();
        let mut p = Player::new(eye);
        p.vehicle = Some(id);
        e.mount_input(PlayerId::HOST, &p, MoveInput::default(), &inv, 0);
        assert!(!e.mount(id).unwrap().mount.as_ref().unwrap().controlled());
        inv.slots[0] = Some(Stack::new(Item::CARROT_ON_A_STICK, 1));
        e.mount_input(PlayerId::HOST, &p, MoveInput::default(), &inv, 0);
        assert!(e.mount(id).unwrap().mount.as_ref().unwrap().controlled());
        assert!(e.boost_mount(PlayerId::HOST, &mut inv, 0, false));
        assert_eq!(inv.get(0).unwrap().damage, 7);
        assert!(!e.boost_mount(PlayerId::HOST, &mut inv, 0, false));
        assert_eq!(inv.get(0).unwrap().damage, 7);
        assert!((140..=980).contains(&e.mount(id).unwrap().mount.as_ref().unwrap().boost_total));
    }
    #[test]
    fn equine_storage_validates_gear_and_saves_owner_stats_chest_and_rider() {
        let mut e = Entities::new(3);
        e.spawn(MobKind::Donkey, DVec3::new(5., 140., 5.));
        let m = &mut e.mobs[0];
        m.uid = 7;
        let s = m.mount.as_mut().unwrap();
        s.tame = true;
        s.owner = Some(PlayerId(3));
        s.rider = Some(PlayerId(4));
        s.chest = true;
        let id = MOB_VEHICLE | 7;
        assert_eq!(e.vehicle_slots(id).unwrap().len(), 17);
        assert!(!e.vehicle_accepts(id, 1, Item::DIAMOND_HORSE_ARMOR));
        assert!(e.insert_vehicle(id, Stack::new(Item::SADDLE, 1)).is_none());
        assert!(e.insert_vehicle(id, Stack::new(Item::DIAMOND, 64)).is_none());
        let saved = e.nether_mobs_to_string();
        let mut restored = Entities::new(3);
        restored.load_nether_mobs(&saved);
        assert_eq!(restored.nether_mobs_to_string(), saved);
        assert_eq!(restored.mount_for_player(PlayerId(4)), Some(id));
        let pos = restored.dismount_animal(&world(), PlayerId(4)).unwrap();
        assert!(!physics::overlaps_solid(&world(), pos, crate::player::SHAPE));
    }
    #[test]
    fn charged_jump_uses_mount_attribute_and_armor_reduces_damage_without_wear() {
        assert_eq!(jump_charge(10), 1.);
        assert_eq!(jump_charge(0), 0.);
        assert!(jump_charge(100) > 0.8 && jump_charge(100) < 0.81);
        let mut m = Mob::new(MobKind::Horse, DVec3::ZERO, 0.);
        m.on_ground = true;
        let s = m.mount.as_mut().unwrap();
        s.tame = true;
        s.rider = Some(PlayerId::HOST);
        s.slots[0] = Some(Stack::new(Item::SADDLE, 1));
        s.slots[1] = Some(Stack::new(Item::DIAMOND_HORSE_ARMOR, 1));
        s.jump = 0.7;
        let p = Player::new(DVec3::ZERO);
        for _ in 0..10 {
            m.mount.as_mut().unwrap().input(
                MobKind::Horse,
                &p,
                MoveInput { jump: true, ..Default::default() },
                None,
                None,
            );
        }
        m.mount.as_mut().unwrap().input(MobKind::Horse, &p, MoveInput::default(), None, None);
        control(&mut m, 0.05, &mut Rng::new(1));
        assert!((m.vel.y - launch_speed(0.7)).abs() < 1e-6);
        let before = m.health;
        m.damage(5., None, &mut Rng::new(1));
        assert!(m.health > before - 5.);
        assert_eq!(m.mount.as_ref().unwrap().slots[1].unwrap().damage, 0);
    }
    #[test]
    fn feeding_heals_raises_temper_and_grows_foals_without_mounting_them() {
        let mut e = Entities::new(5);
        e.spawn(MobKind::Horse, DVec3::new(5., 140., 5.));
        e.mobs[0].baby = true;
        e.mobs[0].age = -24000;
        e.mobs[0].health = 5.;
        let mut inv = Inventory::default();
        inv.slots[0] = Some(Stack::new(Item::GOLDEN_CARROT, 2));
        assert!(e.use_mount(DVec3::new(5., 140.5, 3.), DVec3::Z, 5., PlayerId::HOST, &mut inv, 0, false));
        assert_eq!(e.mobs[0].health, 9.);
        assert_eq!(e.mobs[0].age, -22800);
        assert_eq!(e.mobs[0].mount.as_ref().unwrap().temper, 5);
        assert_eq!(e.mount_for_player(PlayerId::HOST), None);
        assert_eq!(inv.get(0).unwrap().count, 1);
    }
    #[test]
    fn old_persistent_pig_save_loads_and_boat_passenger_identity_survives() {
        let mut e = Entities::new(1);
        e.spawn(MobKind::Pig, DVec3::new(5., 140., 5.));
        e.mobs[0].persistent = true;
        e.mobs[0].riding = Some(13);
        let text = e.nether_mobs_to_string();
        let mut old: Value = serde_json::from_str(&text).unwrap();
        old["mobs"][0].as_object_mut().unwrap().remove("mount");
        old["mobs"][0].as_object_mut().unwrap().remove("uid");
        let mut loaded = Entities::new(1);
        loaded.load_nether_mobs(&old.to_string());
        assert_eq!(loaded.mobs.len(), 1);
        assert_eq!(loaded.mobs[0].riding, Some(13));
        assert!(loaded.mobs[0].mount.is_some());
        assert!(!loaded.mobs[0].mount.as_ref().unwrap().saddled());
    }
    #[test]
    fn strider_shivering_changes_riding_speed_and_pig_uses_java_speed() {
        for (kind, cold, speed) in
            [(MobKind::Pig, false, 2.42), (MobKind::Strider, false, 4.14), (MobKind::Strider, true, 1.74)]
        {
            let mut m = Mob::new(kind, DVec3::ZERO, 0.);
            if let Some(n) = &mut m.nether {
                n.cold = cold;
            }
            let s = m.mount.as_mut().unwrap();
            s.rider = Some(PlayerId::HOST);
            s.slots[0] = Some(Stack::new(Item::SADDLE, 1));
            let item = if kind == MobKind::Pig { Item::CARROT_ON_A_STICK } else { Item::WARPED_FUNGUS_ON_A_STICK };
            s.input(kind, &Player::new(DVec3::ZERO), MoveInput::default(), Some(item), None);
            assert_eq!(control(&mut m, 0.05, &mut Rng::new(1)).unwrap().1, speed);
        }
    }
    #[test]
    fn equine_steps_a_full_block_and_rounds_half_fall_damage() {
        let mut w = world();
        for z in 0..16 {
            w.set_block(IVec3::new(7, 140, z), Block::STONE);
        }
        let mut m = Mob::new(MobKind::Horse, DVec3::new(5., 140., 5.), 0.);
        m.on_ground = true;
        let mut top = m.pos.y;
        for _ in 0..30 {
            m.physics_step(0.05, &w, Some(DVec3::X), 5.);
            top = top.max(m.pos.y);
        }
        // Climbs onto the one-block wall at x = 7 without jumping, then walks off it.
        assert!(m.pos.x > 8. && top >= 141., "{:?} peak {top}", m.pos);
        m.pos = DVec3::new(4., 147., 5.);
        m.vel = DVec3::ZERO;
        m.on_ground = false;
        let before = m.health;
        for _ in 0..120 {
            m.physics_step(1. / 120., &w, None, 0.);
            if m.on_ground {
                break;
            }
        }
        assert!(m.on_ground);
        assert_eq!(before - m.health, 1.);
    }
    #[test]
    fn horse_jump_height_matches_java_attribute_range() {
        let w = world();
        for (jump, expected) in [(0.4, 1.1093), (0.7, 2.8933), (1., 5.29997)] {
            let mut m = Mob::new(MobKind::Horse, DVec3::new(5., 140., 5.), 0.);
            m.vel.y = launch_speed(jump);
            let mut peak = m.pos.y;
            for _ in 0..240 {
                m.physics_step(1. / 120., &w, None, 0.);
                peak = peak.max(m.pos.y);
                if m.on_ground {
                    break;
                }
            }
            assert!((peak - 140. - expected).abs() < 0.2, "jump {jump}: {} vs {expected}", peak - 140.);
        }
    }
    #[test]
    fn offspring_attributes_stay_in_range_and_centre_on_the_parents() {
        let mut rng = Rng::new(3);
        let mut sum = 0.;
        for _ in 0..20000 {
            let v = offspring_attribute(0.2, 0.3, 0.1125, 0.3375, &mut rng);
            assert!((0.1125..=0.3375).contains(&v));
            sum += v;
        }
        assert!((sum / 20000. - 0.25).abs() < 0.005);
        // Reflection keeps a draw past the top inside the range.
        for _ in 0..1000 {
            assert!(offspring_attribute(1., 1., 0.4, 1., &mut rng) <= 1.);
        }
    }
    #[test]
    fn tamed_horse_and_donkey_breed_a_mule_but_untamed_horses_do_not_love() {
        let mut e = Entities::new(4);
        e.spawn(MobKind::Horse, DVec3::new(0., 140., 0.));
        e.spawn(MobKind::Donkey, DVec3::new(2., 140., 0.));
        assert_eq!(e.use_animal(0, Some(Stack::new(Item::GOLDEN_CARROT, 1)), PlayerId::HOST), None);
        for i in 0..2 {
            e.mobs[i].uid = i as u32 + 1;
            e.mobs[i].mount.as_mut().unwrap().tame = true;
            assert_eq!(e.use_animal(i, Some(Stack::new(Item::GOLDEN_CARROT, 1)), PlayerId::HOST), Some(true));
        }
        let w = world();
        let ctx = super::super::Ctx {
            players: vec![super::super::Target::new(PlayerId::HOST, DVec3::new(0., 140., 0.), false)],
            daylight: 1.,
            spawning: false,
            raining: false,
            dimension: crate::world::terrain::Dimension::Overworld,
        };
        for _ in 0..61 {
            e.tick_animals(0.05, &w, &ctx, &mut Vec::new());
        }
        assert_eq!(e.mobs.len(), 3);
        let foal = &e.mobs[2];
        assert_eq!(foal.kind, MobKind::Mule);
        assert!(foal.baby);
        let s = foal.mount.as_ref().unwrap();
        assert!((15. ..=30.).contains(&s.max_health) && (0.4..=1.).contains(&s.jump));
        assert_eq!(foal.health, s.max_health);
    }
    #[test]
    fn llamas_have_java_strength_coats_and_chest_slots() {
        let mut rng = Rng::new(11);
        let mut seen = [0u32; 6];
        for _ in 0..4000 {
            let s = State::new(MobKind::Llama, &mut rng);
            seen[s.strength as usize] += 1;
            assert!(s.variant < 4 && !s.tame);
        }
        assert_eq!(seen[0], 0);
        // Strength 4 and 5 only come from the 1-in-25 roll.
        assert!(seen[4] + seen[5] > 0 && seen[4] + seen[5] < 200);
        let mut s = State::new(MobKind::Llama, &mut rng);
        s.tame = true;
        s.chest = true;
        s.strength = 2;
        assert_eq!(s.slot_count(MobKind::Llama), 8);
        assert!(!s.accepts(MobKind::Llama, 0, Item::SADDLE));
        let carpet = Item::from_block(crate::world::block::Block::carpet(crate::color::DyeColor::Blue));
        assert!(s.accepts(MobKind::Llama, 1, carpet));
        assert!(s.accepts(MobKind::Llama, 7, Item::WHEAT) && !s.accepts(MobKind::Llama, 8, Item::WHEAT));
        let saved = State::load(MobKind::Llama, &s.save()).unwrap();
        assert_eq!((saved.strength, saved.chest), (2, true));
    }
    #[test]
    fn camels_need_no_taming_seat_two_and_dash_on_cooldown() {
        let mut e = Entities::new(3);
        e.spawn(MobKind::Camel, DVec3::new(5., 140., 5.));
        e.mobs[0].on_ground = true;
        let mut inv = Inventory::default();
        inv.slots[0] = Some(Stack::new(Item::SADDLE, 1));
        let eye = DVec3::new(5., 141.5, 2.);
        assert!(e.use_mount(eye, DVec3::Z, 5., PlayerId::HOST, &mut inv, 0, false));
        assert!(e.mobs[0].mount.as_ref().unwrap().saddled());
        assert!(e.use_mount(eye, DVec3::Z, 5., PlayerId::HOST, &mut inv, 1, false));
        assert!(e.use_mount(eye, DVec3::Z, 5., PlayerId(7), &mut inv, 1, false));
        let id = e.mount_for_player(PlayerId(7)).unwrap();
        let (front, back) = (e.mount_seat(id, PlayerId::HOST).unwrap(), e.mount_seat(id, PlayerId(7)).unwrap());
        assert!((front - back).length() > 0.9);
        let m = &mut e.mobs[0];
        let s = m.mount.as_mut().unwrap();
        let mut p = Player::new(DVec3::ZERO);
        p.yaw = 0.;
        for jump in [true, true, true, false] {
            s.input(MobKind::Camel, &p, MoveInput { jump, ..Default::default() }, None, None);
        }
        control(m, 0.05, &mut Rng::new(1));
        assert!(m.vel.x > 5. && m.vel.y > 0.);
        assert_eq!(m.mount.as_ref().unwrap().dash_cooldown, DASH_COOLDOWN);
        assert!(e.dismount_animal(&world(), PlayerId::HOST).is_some());
        assert_eq!(e.mobs[0].mount.as_ref().unwrap().rider, Some(PlayerId(7)));
    }
}
