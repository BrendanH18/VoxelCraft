//! Thrown splash potions, like Java's: they fly under gravity, shatter on a
//! block or whatever they hit, and splash everything within four blocks
//! (two vertically) with an effect scaled by distance. Players throw them
//! (10 blocks a second, aimed 20 degrees high) and witches lob them at
//! their targets.

use glam::DVec3;

use super::{Ctx, EntityEvent, Mob, MobKind, PlayerId, Rng};
use crate::potion::Potion;
use crate::simulation::effects::Effect;

/// A player's throw speed (Java's 0.5 blocks a tick).
pub const THROW_SPEED: f64 = 10.0;
/// A witch's throw speed (Java's 0.75).
pub const WITCH_SPEED: f64 = 15.0;
/// Blocks per second squared (Java's 0.05 per tick²).
const GRAVITY: f64 = 20.0;
/// How far a splash reaches horizontally and vertically.
const REACH: f64 = 4.0;
const REACH_Y: f64 = 2.0;
/// Java 26.3 splash items have no reduced duration component; the default is 1.
const DURATION_SCALE: f32 = 1.0;
/// Longest move per collision check (blocks).
const STEP: f64 = 0.2;
const MAX_FLIGHT: f32 = 30.0;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ThrownPotion {
    pub potion: Potion,
    pub lingering: bool,
    /// The throwing player; witches throw with `None`.
    pub owner: Option<PlayerId>,
    /// v1.0 gap Track 8g: the witch that threw it (`Mob::uid`), whom its
    /// harm names. v1.0 final F8: saved as `thrower` when there is one
    /// (Java's `Projectile` saves its `Owner`), so a reloaded witch's
    /// potion still names her.
    pub thrower: Option<u32>,
    pub pos: DVec3,
    /// Position at the start of the last simulation step, for rendering.
    pub previous_pos: DVec3,
    pub vel: DVec3,
    age: f32,
    /// v1.0 gap Track 8d: `remainingFireTicks` (it applies block effects,
    /// `ThrowableProjectile.tick`). Saved (v1.0 gap Track 8e), as Java's `Fire`.
    fire: u16,
    /// v1.0 final Track F2: ticks until it can use a portal again
    /// (`Entity.portalCooldown`; not saved).
    pub portal_cooldown: u16,
}

/// Aims `dir` twenty degrees higher, Java's throw offset for potions.
pub fn aim_up(dir: DVec3) -> DVec3 {
    let dir = dir.normalize_or(DVec3::X);
    let pitch = dir.y.clamp(-1.0, 1.0).asin() + 20f64.to_radians();
    let flat = DVec3::new(dir.x, 0.0, dir.z).normalize_or(DVec3::X);
    flat * pitch.cos() + DVec3::Y * pitch.sin()
}

impl ThrownPotion {
    /// v1.0 gap Track 8e: the level file's copy, with its fire.
    pub(super) fn save(&self) -> serde_json::Value {
        use super::saved_projectiles::v3;
        let mut v = serde_json::json!({
            "potion": self.potion.info().id,
            "lingering": self.lingering,
            "owner": self.owner.map(|p| p.0),
            "pos": v3(self.pos),
            "motion": v3(self.vel),
            "age": self.age,
            "fire": self.fire,
        });
        // v1.0 final F8: written only when a witch threw it.
        if let Some(uid) = self.thrower {
            v["thrower"] = uid.into();
        }
        v
    }

    pub(super) fn load(v: &serde_json::Value) -> Option<Self> {
        use super::saved_projectiles as s;
        let pos = s::read_v3(&v["pos"])?;
        Some(Self {
            portal_cooldown: 0,
            potion: Potion::from_id(v["potion"].as_str()?)?,
            lingering: v["lingering"].as_bool().unwrap_or(false),
            owner: v["owner"].as_u64().and_then(|u| u32::try_from(u).ok()).map(PlayerId),
            thrower: v["thrower"].as_u64().and_then(|u| u32::try_from(u).ok()),
            pos,
            previous_pos: pos,
            vel: s::read_v3(&v["motion"]).unwrap_or(DVec3::ZERO),
            age: s::read_f32(v, "age", 0.0).clamp(0.0, MAX_FLIGHT),
            fire: s::read_fire(v),
        })
    }

    /// v1.0 gap Track 8d: on fire (`isOnFire`).
    pub fn burning(&self) -> bool {
        self.fire > 0
    }

    /// v1.0 gap Track 8d: a LAN client's copy burns as the host says.
    pub fn set_burning(&mut self, burning: bool) {
        self.fire = if burning { self.fire.max(1) } else { 0 };
    }

    /// A LAN client's copy: drawn only, never simulated.
    pub(super) fn replica(potion: Potion, pos: DVec3, previous_pos: DVec3) -> Self {
        Self {
            portal_cooldown: 0,
            potion,
            lingering: false,
            owner: None,
            thrower: None,
            pos,
            previous_pos,
            vel: DVec3::ZERO,
            age: 0.0,
            fire: 0,
        }
    }

    pub fn new(potion: Potion, owner: Option<PlayerId>, pos: DVec3, vel: DVec3) -> Self {
        Self { portal_cooldown: 0, potion, lingering: false, owner, thrower: None, pos, previous_pos: pos, vel, age: 0.0, fire: 0 }
    }

    /// A player's throw from `eye` along their look direction `dir`, adding
    /// their own velocity `carry`.
    pub fn thrown(potion: Potion, owner: PlayerId, eye: DVec3, dir: DVec3, carry: DVec3) -> Self {
        Self::new(potion, Some(owner), eye - DVec3::Y * 0.1, aim_up(dir) * THROW_SPEED + carry)
    }

    /// Moves the potion; returns `false` once it has shattered.
    #[allow(clippy::too_many_arguments)]
    pub(super) fn update<W: super::MobWorld + ?Sized>(
        &mut self,
        dt: f64,
        world: &W,
        ctx: &Ctx,
        mobs: &mut [Mob],
        boss: &super::dragon::Boxes,
        rng: &mut Rng,
        events: &mut Vec<EntityEvent>,
    ) -> bool {
        self.age += dt as f32;
        let water = world.block(self.pos.floor().as_ivec3()).is_some_and(|b| b.holds_water());
        self.vel *= if water { 0.8f64 } else { 0.99 }.powf(dt * 20.0);
        self.vel.y -= GRAVITY * dt;
        let delta = self.vel * dt;
        super::projectile::tick_fire(&mut self.fire, world, self.pos, self.pos + delta, 0.125, 0.25);
        let steps = (delta.length() / STEP).ceil().max(1.0) as u32;
        for _ in 0..steps {
            let next = self.pos + delta / steps as f64;
            if world.block(next.floor().as_ivec3()).is_some_and(|b| b.is_solid()) {
                let cell = next.floor().as_ivec3();
                events.push(super::projectile::block_hit(
                    cell,
                    self.pos,
                    self.burning(),
                    self.owner.is_none(),
                    self.owner.map(super::trident::Owner::Player),
                ));
                self.pos = next;
                self.shatter(ctx, mobs, boss, rng, events);
                return false;
            }
            self.pos = next;
            // v1.0 gap Track 4c: an End crystal or a dragon part stops it
            // too (it has no `onHitEntity`: a crystal survives).
            if boss.hit_at(next, true).is_some() {
                self.shatter(ctx, mobs, boss, rng, events);
                return false;
            }
            let hit_player = ctx.players.iter().any(|t| t.alive && t.contains(next) && self.owner != Some(t.id));
            let hit_mob = mobs.iter().position(|m| {
                let (min, max) = m.aabb();
                m.alive() && next.cmpge(min).all() && next.cmple(max).all()
            });
            if hit_player || hit_mob.is_some() {
                if self.lingering {
                    if let Some(i) = hit_mob {
                        self.pos = mobs[i].pos;
                    } else if let Some(t) =
                        ctx.players.iter().find(|t| t.alive && t.contains(next) && self.owner != Some(t.id))
                    {
                        self.pos = t.pos;
                    }
                }
                self.shatter(ctx, mobs, boss, rng, events);
                return false;
            }
        }
        !(self.age >= MAX_FLIGHT || self.pos.y < -128.0)
    }

    fn shatter(
        &self,
        ctx: &Ctx,
        mobs: &mut [Mob],
        boss: &super::dragon::Boxes,
        rng: &mut Rng,
        events: &mut Vec<EntityEvent>,
    ) {
        events.push(EntityEvent::PotionSplashed { pos: self.pos, colour: self.potion.colour() });
        if self.lingering {
            events.push(EntityEvent::LingeringCloud { pos: self.pos, potion: self.potion, owner: self.owner });
        } else {
            let by = self.owner.map(super::trident::Owner::Player).or(self.thrower.map(super::trident::Owner::Mob));
            let cause = crate::simulation::damage::Cause::potion_magic(by, false);
            splash_by(self.potion, self.owner, cause, self.pos, self.age, ctx, mobs, rng, events);
            splash_dragons(self.potion, self.owner, self.pos, self.age, boss, events);
        }
    }
}

/// Java 26.3 measures between the potion's 0.25 × 0.25 collision box at
/// impact and the recipient's box inflated by ProjectileUtil.computeMargin.
/// The candidate query uses the uninflated recipient and a 4 × 2 × 4 expansion.
fn splash_intensity(pos: DVec3, age: f32, min: DVec3, max: DVec3) -> Option<f64> {
    let potion_min = pos - DVec3::new(0.125, 0.0, 0.125);
    let potion_max = pos + DVec3::new(0.125, 0.25, 0.125);
    let reach = DVec3::new(REACH, REACH_Y, REACH);
    if !(max.cmpgt(potion_min - reach).all() && min.cmplt(potion_max + reach).all()) {
        return None;
    }
    let margin = f64::from((((age * 20.0).round() - 2.0) / 20.0).clamp(0.0, 0.3));
    let separation = (min - margin - potion_max).max(potion_min - max - margin).max(DVec3::ZERO);
    let distance = separation.length();
    (distance < REACH).then_some(1.0 - distance / REACH)
}

/// v1.0 gap Track 4c: `ThrownSplashPotion.onHitAsPotion` on the dragons,
/// each a `LivingEntity` with its own 16 × 8 box. `EnderDragon.addEffect`
/// refuses every lasting effect, so only Harming and Healing reach it:
/// Harming through `hurtServer` (its body; only a player's potion takes
/// health), Healing through `heal`.
fn splash_dragons(
    potion: Potion,
    owner: Option<PlayerId>,
    pos: DVec3,
    age: f32,
    boss: &super::dragon::Boxes,
    events: &mut Vec<EntityEvent>,
) {
    for (effect, amplifier, _) in potion.effects() {
        for &(hit, feet) in &boss.dragons {
            let (min, max) = super::dragon::WILD_SHAPE.aabb(feet);
            let Some(scale) = splash_intensity(pos, age, min, max) else { continue };
            match effect {
                Effect::InstantDamage => {
                    let blow = super::dragon::Blow::projectile(instant_amount(6.0, amplifier, scale), owner.is_some());
                    events.push(EntityEvent::DragonStruck { hit, blow, by: owner });
                }
                Effect::InstantHealth => {
                    events.push(EntityEvent::DragonHealed { hit, amount: instant_amount(4.0, amplifier, scale) });
                }
                _ => {}
            }
        }
    }
}

/// Java's instant effect amount: `base << amplifier`, scaled and rounded.
fn instant_amount(base: f32, amplifier: u8, intensity: f64) -> f32 {
    (intensity * f64::from(base) * f64::from(1u32 << amplifier.min(20)) + 0.5).floor() as f32
}

/// Splashes `potion` at the impact position. Players get [`EntityEvent::PlayerEffect`] or
/// [`EntityEvent::PlayerMagic`]; mobs take instant damage and healing (undead
/// the other way round, witches shrug off most of the harm). Every other
/// effect is added to mobs with the attenuated duration, except Poison and
/// Regeneration on the undead. Weakness also lands on zombie villagers so a
/// golden apple can start a cure.
#[cfg(test)]
#[allow(clippy::too_many_arguments)]
pub(super) fn splash(
    potion: Potion,
    owner: Option<PlayerId>,
    pos: DVec3,
    age: f32,
    ctx: &Ctx,
    mobs: &mut [Mob],
    rng: &mut Rng,
    events: &mut Vec<EntityEvent>,
) {
    let cause = crate::simulation::damage::Cause::potion_magic(owner.map(super::trident::Owner::Player), false);
    splash_by(potion, owner, cause, pos, age, ctx, mobs, rng, events);
}

/// [`splash`], the harm to players dealt as `cause` (v1.0 gap Track 8g:
/// `indirectMagic(potion, owner)`).
#[allow(clippy::too_many_arguments)]
pub(super) fn splash_by(
    potion: Potion,
    owner: Option<PlayerId>,
    cause: crate::simulation::damage::Cause,
    pos: DVec3,
    age: f32,
    ctx: &Ctx,
    mobs: &mut [Mob],
    rng: &mut Rng,
    events: &mut Vec<EntityEvent>,
) {
    for (effect, amplifier, ticks) in potion.effects() {
        for t in ctx.players.iter().filter(|t| t.alive) {
            let (min, max) = t.shape.aabb(t.pos);
            let Some(scale) = splash_intensity(pos, age, min, max) else { continue };
            match effect {
                Effect::InstantDamage => {
                    let amount = instant_amount(6.0, amplifier, scale);
                    events.push(EntityEvent::PlayerMagic { player: t.id, amount, cause });
                }
                Effect::InstantHealth => {
                    let amount = instant_amount(4.0, amplifier, scale);
                    events.push(EntityEvent::PlayerMagic { player: t.id, amount: -amount, cause });
                }
                _ => {
                    let ticks = (scale * f64::from(ticks) * f64::from(DURATION_SCALE) + 0.5) as u32;
                    if ticks > 20 {
                        events.push(EntityEvent::PlayerEffect { player: t.id, effect, amplifier, ticks });
                    }
                }
            }
        }
        if !effect.is_instant() {
            // Java's `ThrownSplashPotion` adds every other effect to each
            // living recipient; the undead ignore Poison and Regeneration.
            for mob in mobs.iter_mut() {
                if !mob.alive() || !mob.kind.accepts_potion_effect(effect) {
                    continue;
                }
                let (min, max) = mob.aabb();
                let Some(scale) = splash_intensity(pos, age, min, max) else {
                    continue;
                };
                let duration = (scale * f64::from(ticks) * f64::from(DURATION_SCALE) + 0.5) as u32;
                if duration > 20 {
                    mob.effects.add(effect, amplifier, duration);
                }
            }
        }
        if effect == Effect::Weakness {
            for mob in mobs.iter_mut() {
                if !mob.alive() || mob.kind != MobKind::ZombieVillager {
                    continue;
                }
                let (min, max) = mob.aabb();
                let Some(scale) = splash_intensity(pos, age, min, max) else { continue };
                let ticks = (scale * f64::from(ticks) * f64::from(DURATION_SCALE) + 0.5) as u32;
                if ticks > 20 {
                    mob.weakness_left = mob.weakness_left.max(ticks as f32 / 20.0);
                }
            }
        }
        if !effect.is_instant() {
            continue;
        }
        for mob in mobs.iter_mut() {
            if !mob.alive() {
                continue;
            }
            let (min, max) = mob.aabb();
            let Some(scale) = splash_intensity(pos, age, min, max) else { continue };
            let undead = mob.kind.creature() == crate::enchant::Creature::Undead;
            // Healing hurts the undead and harming heals them.
            let harms = (effect == Effect::InstantDamage) != undead;
            let base = if effect == Effect::InstantDamage { 6.0 } else { 4.0 };
            let amount = instant_amount(base, amplifier, scale);
            if harms {
                if owner.is_some() && mob.allay.as_ref().is_some_and(|s| s.player == owner) {
                    continue;
                }
                let amount = if mob.kind == MobKind::Witch { amount * 0.15 } else { amount };
                if owner.is_some() {
                    mob.player_hit();
                }
                let (kind, burning, at) = (mob.kind, mob.burning, mob.pos);
                let killed = mob.damage_bypassing_wolf_armor(amount, None, rng);
                if owner.is_some() {
                    events.push(EntityEvent::MobShot { kind, pos: at, killed, burning, player_kill: true, owner });
                }
            } else {
                mob.health = (mob.health + amount).min(mob.max_health());
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use crate::entity::Target;
    use crate::physics::test_util::Grid;

    /// v1.0 final F8: a witch's potion saved mid-flight keeps its thrower
    /// (`Projectile`'s `Owner`); a player's writes no `thrower`.
    #[test]
    fn a_witchs_potion_keeps_its_thrower_through_a_save() {
        let (eye, dir) = (DVec3::new(0.5, 70.0, 0.5), DVec3::X);
        let mut potion = ThrownPotion::thrown(crate::potion::Potion::AWKWARD, PlayerId::HOST, eye, dir, DVec3::ZERO);
        assert!(potion.save().get("thrower").is_none(), "only when there is one");
        potion.owner = None;
        potion.thrower = Some(12);
        let saved = potion.save();
        assert_eq!(saved["thrower"], 12);
        assert_eq!(ThrownPotion::load(&saved).unwrap().thrower, Some(12));
    }

    fn ctx(players: Vec<Target>) -> Ctx {
        Ctx {
            spawn_light: crate::simulation::lighting::SpawnLight::DAY,
            players,
            daylight: 1.0,
            spawning: false,
            raining: false,
            dimension: crate::world::terrain::Dimension::Overworld,
        }
    }

    #[test]
    fn thrown_potions_arc_and_shatter_on_the_ground() {
        let world = Grid::flat(10);
        let mut p = ThrownPotion::thrown(
            Potion::from_id("harming").unwrap(),
            PlayerId::HOST,
            DVec3::new(0.5, 11.62, 0.5),
            DVec3::X,
            DVec3::ZERO,
        );
        let mut events = Vec::new();
        let c = ctx(vec![]);
        let mut n = 0;
        while p.update(0.05, &world, &c, &mut [], &Default::default(), &mut Rng::new(1), &mut events) {
            n += 1;
            assert!(n < 200);
        }
        // v1.0 gap Track 8d: `onProjectileHit` (the block hit) comes first.
        assert!(matches!(events[0], EntityEvent::ProjectileBlockHit { burning: false, .. }));
        assert!(matches!(events[1], EntityEvent::PotionSplashed { pos, .. } if (4.0..14.0).contains(&pos.x)));
    }

    #[test]
    fn splash_scales_with_distance_for_every_player() {
        let harming = Potion::from_id("harming").unwrap();
        let c = ctx(vec![
            Target::new(PlayerId::HOST, DVec3::new(0.0, 10.0, 0.0), true),
            Target::new(PlayerId(2), DVec3::new(2.0, 10.0, 0.0), true),
            Target::new(PlayerId(3), DVec3::new(9.0, 10.0, 0.0), true),
        ]);
        let mut events = Vec::new();
        splash(harming, None, DVec3::new(0.0, 10.0, 0.0), 0.0, &c, &mut [], &mut Rng::new(1), &mut events);
        let amount = |id| {
            events.iter().find_map(|e| match *e {
                EntityEvent::PlayerMagic { player, amount, .. } if player == PlayerId(id) => Some(amount),
                _ => None,
            })
        };
        assert_eq!(amount(0), Some(6.0), "standing in it hits for full strength");
        assert!(amount(2).is_some_and(|a| a < 6.0 && a > 0.0), "{:?}", amount(2));
        assert_eq!(amount(3), None, "out of reach");
    }

    #[test]
    fn turtle_master_splash_applies_both_effects_with_reference_box_distance() {
        let potion = Potion::from_id("turtle_master").unwrap();
        let origin = DVec3::new(0., 10., 0.);
        let c = ctx(vec![
            Target::new(PlayerId::HOST, origin, true),
            Target::new(PlayerId(2), origin + DVec3::X * 2., true),
            Target::new(PlayerId(3), origin + DVec3::Y * 2.3, true),
        ]);
        let mut mobs = [Mob::new(MobKind::Horse, origin, 0.)];
        let mut events = Vec::new();
        splash(potion, None, origin, 0., &c, &mut mobs, &mut Rng::new(1), &mut events);
        for (effect, amplifier) in [(Effect::Slowness, 3), (Effect::Resistance, 2)] {
            assert_eq!(mobs[0].effects.get(effect).map(|e| (e.amplifier, e.ticks)), Some((amplifier, 400)));
            assert!(events.iter().any(|e| matches!(e, EntityEvent::PlayerEffect { player: PlayerId::HOST, effect: actual, amplifier: amp, ticks: 400 } if *actual == effect && *amp == amplifier)));
            // Player half-width .3 + potion half-width .125, so separation is 1.575.
            assert!(events.iter().any(|e| matches!(e, EntityEvent::PlayerEffect { player: PlayerId(2), effect: actual, amplifier: amp, ticks: 243 } if *actual == effect && *amp == amplifier)));
        }
        assert!(!events.iter().any(|e| matches!(e, EntityEvent::PlayerEffect { player: PlayerId(3), .. })));
        // After eight flight ticks, the recipient's box has the maximum .3 margin.
        let (min, max) = c.players[1].shape.aabb(c.players[1].pos);
        assert!((splash_intensity(origin, 0.4, min, max).unwrap() - 0.68125).abs() < 1e-6);
    }

    #[test]
    fn splash_effects_keep_reference_duration_and_mobs_take_instant_damage() {
        let poison = Potion::from_id("poison").unwrap();
        let c = ctx(vec![Target::new(PlayerId(4), DVec3::new(0.0, 10.0, 0.0), true)]);
        let mut events = Vec::new();
        splash(poison, None, DVec3::new(0.0, 10.0, 0.0), 0.0, &c, &mut [], &mut Rng::new(1), &mut events);
        let EntityEvent::PlayerEffect { effect: Effect::Poison, ticks, .. } = events[0] else { panic!("{events:?}") };
        assert_eq!(ticks, 900, "Java 26.3 default splash duration scale is one");

        let mut mobs = [
            Mob::new(MobKind::Zombie, DVec3::new(1.0, 10.0, 0.0), 0.0),
            Mob::new(MobKind::Witch, DVec3::new(1.0, 10.0, 1.0), 0.0),
        ];
        let harming = Potion::from_id("harming").unwrap();
        splash(
            harming,
            Some(PlayerId::HOST),
            DVec3::new(0.0, 10.0, 0.0),
            0.0,
            &c,
            &mut mobs,
            &mut Rng::new(1),
            &mut Vec::new(),
        );
        // The undead are healed by harming; their health was already full.
        assert_eq!(mobs[0].health, MobKind::Zombie.max_health());
        assert!(mobs[1].health > MobKind::Witch.max_health() - 2.0, "witches resist magic: {}", mobs[1].health);
        let healing = Potion::from_id("healing").unwrap();
        splash(
            healing,
            Some(PlayerId::HOST),
            DVec3::new(0.0, 10.0, 0.0),
            0.0,
            &c,
            &mut mobs,
            &mut Rng::new(1),
            &mut Vec::new(),
        );
        assert!(mobs[0].health < MobKind::Zombie.max_health(), "healing hurts the undead");
    }

    #[test]
    fn panda_splash_healing_respects_the_expressed_weak_gene_health() {
        use crate::entity::panda::Gene;
        let mut mobs = [Mob::new(MobKind::Panda, DVec3::new(0.5, 10.0, 0.0), 0.0)];
        let s = mobs[0].panda.as_mut().unwrap();
        s.main = Gene::Weak;
        s.hidden = Gene::Weak;
        mobs[0].health = 9.0;
        splash(
            Potion::from_id("healing").unwrap(),
            None,
            DVec3::new(0.5, 10.0, 0.0),
            0.0,
            &ctx(vec![]),
            &mut mobs,
            &mut Rng::new(3),
            &mut Vec::new(),
        );
        assert_eq!(mobs[0].health, 10.0);
    }
    #[test]
    fn mobs_take_every_splash_effect_except_undead_poison_and_regeneration() {
        let c = ctx(vec![]);
        let mut mobs = [
            Mob::new(MobKind::Cow, DVec3::new(0.5, 10.0, 0.0), 0.0),
            Mob::new(MobKind::Zombie, DVec3::new(-0.5, 10.0, 0.0), 0.0),
        ];
        for id in ["swiftness", "poison", "regeneration", "strength", "fire_resistance"] {
            let potion = Potion::from_id(id).unwrap();
            splash(potion, None, DVec3::new(0.0, 10.0, 0.0), 0.0, &c, &mut mobs, &mut Rng::new(1), &mut Vec::new());
        }
        let has = |m: &Mob, e| m.effects.has(e);
        for e in [Effect::Speed, Effect::Poison, Effect::Regeneration, Effect::Strength, Effect::FireResistance] {
            assert!(has(&mobs[0], e), "{e:?}");
        }
        assert!(has(&mobs[1], Effect::Speed) && has(&mobs[1], Effect::Strength));
        assert!(!has(&mobs[1], Effect::Poison) && !has(&mobs[1], Effect::Regeneration));
    }
}
