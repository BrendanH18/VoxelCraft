//! v0.16 Track D player actions, shared by every input path through the
//! session's draw/release calls (`start_draw`, `tick_draw`, `release_draw`)
//! and its fixed tick (`tick_weapons`): charging and throwing a trident or
//! riding its Riptide spin, charging a spear (`KineticWeapon`), and the
//! spear's jab with Lunge (`PiercingWeapon`, through `attack`).

use glam::DVec3;

use super::PlayerSession;
use super::ranged::{Ranged, Shot};
use crate::{
    entity::{Entities, PlayerId},
    inventory::Stack,
    item::Item,
    knockback::TICK_RATE,
    weapons::{self, Cue, spear, trident as tr},
    world::World,
};

#[derive(Default)]
pub(super) struct State {
    /// The hotbar slot whose trident or spear is in use, and ticks used.
    using: Option<(usize, u32)>,
    /// Riptide's auto spin attack.
    spin: Option<Spin>,
    /// `recentKineticEnemies`: mob uid and the tick it was last stabbed.
    stabbed: Vec<(u32, u64)>,
    /// Fixed ticks this session has run (`level.getGameTime()` stand-in).
    clock: u64,
    /// Feet at the previous weapon tick, for the spin's swept box.
    last_pos: Option<DVec3>,
    /// `lastKineticHitFeedbackTime`.
    last_feedback: u64,
}

#[derive(Clone, Copy)]
struct Spin {
    ticks: u32,
    stack: Stack,
}

/// Which v0.16 weapon a held item is.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Held {
    Trident,
    Spear,
}

fn held_weapon(item: Option<Item>) -> Option<Held> {
    match item {
        Some(weapons::TRIDENT) => Some(Held::Trident),
        Some(i) if weapons::is_spear(i) => Some(Held::Spear),
        _ => None,
    }
}

impl PlayerSession {
    /// `Entity.isInWaterOrRain`: touching water, or rain falling at the feet
    /// or the top of the box.
    pub fn in_water_or_rain(&self, world: &World) -> bool {
        let p = &self.player;
        let top = p.pos + DVec3::Y * p.collision_shape().height;
        p.in_water || world.rains_on(p.pos.floor().as_ivec3()) || world.rains_on(top.floor().as_ivec3())
    }

    /// Whether `use` on `slot` would start charging a trident or spear
    /// (`TridentItem.use`, `Item.use` with a kinetic weapon): a trident
    /// one use from breaking fails, and so does Riptide away from water and rain.
    pub(super) fn can_charge_weapon(&self, slot: usize, world: &World) -> bool {
        let Some(stack) = self.inventory.get(slot) else { return false };
        match held_weapon(Some(stack.item)) {
            Some(Held::Trident) => {
                let max = stack.item.durability().unwrap_or(u16::MAX);
                let next_breaks = stack.damage.saturating_add(1) >= max;
                let riptide = tr::riptide_strength(stack.active_enchants()) > 0.0;
                !next_breaks && (!riptide || self.in_water_or_rain(world))
            }
            Some(Held::Spear) => true,
            None => false,
        }
    }

    /// Starts charging the weapon in `slot`; the spear plays its use sound.
    pub(super) fn start_weapon_charge(&mut self, slot: usize) -> Shot {
        let item = self.inventory.get(slot).map_or(Item::default(), |s| s.item);
        self.weapons.using = Some((slot, 0));
        self.weapons.stabbed.clear();
        let cue = weapons::is_spear(item).then(|| Cue::spear(item, true, false));
        Shot { player: self.id, ranged: Ranged::Charging { from: self.player.eye(), cue }, tool_broken: None }
    }

    /// Whether a trident or spear is being charged.
    pub fn charging_weapon(&self) -> bool {
        self.weapons.using.is_some()
    }

    /// Whether the charge is a spear's (which neither slows nor stops a sprint:
    /// `UseEffects(true, false, 1.0)`).
    pub fn charging_spear(&self) -> bool {
        self.weapons.using.is_some_and(|(slot, _)| self.inventory.get(slot).is_some_and(|s| weapons::is_spear(s.item)))
    }

    /// Ticks the current trident or spear has been charged.
    pub fn weapon_charge_ticks(&self) -> Option<u32> {
        self.weapons.using.map(|(_, t)| t)
    }

    pub(super) fn cancel_weapon_charge(&mut self) {
        self.weapons.using = None;
    }

    /// One fixed tick of holding use with a trident or spear. A spear's
    /// charge stabs what it runs into (`KineticWeapon.damageEntities`).
    pub(super) fn tick_weapon_charge(
        &mut self,
        slot: usize,
        using: bool,
        world: &World,
        entities: &mut Entities,
    ) -> Option<Shot> {
        let (charged, ticks) = self.weapons.using?;
        let stack = self.inventory.get(slot);
        if !using || charged != slot || held_weapon(stack.map(|s| s.item)).is_none() {
            self.weapons.using = None;
            return None;
        }
        let stack = stack?;
        let shot = if let Some(tier) = weapons::spear_tier(stack.item) {
            self.kinetic_tick(slot, stack, tier, ticks, world, entities)
        } else {
            None
        };
        if let Some((s, t)) = &mut self.weapons.using
            && *s == slot
        {
            *t = t.saturating_add(1);
        }
        shot
    }

    /// `KineticWeapon.damageEntities` for one tick of a player's charge.
    fn kinetic_tick(
        &mut self,
        slot: usize,
        stack: Stack,
        tier: crate::item::Tier,
        used: u32,
        world: &World,
        entities: &mut Entities,
    ) -> Option<Shot> {
        let kinetic = spear::kinetic(tier);
        used.checked_sub(kinetic.delay_ticks)?;
        let look = self.player.forward().as_dvec3();
        let own_motion = self.motion(entities);
        let along = look.dot(own_motion);
        let eye = self.player.eye();
        let range = spear::player_range(self.mode.is_creative());
        let block = self.spear_block_distance(world, eye, look, range.max + along.max(0.0) / TICK_RATE);
        let targets = entities.spear_targets(eye, look, range, along / TICK_RATE, block, None);
        let clock = self.weapons.clock;
        let mut affected = 0u8;
        let yaw = crate::knockback::java_yaw_degrees(self.player.yaw);
        for i in targets {
            let Some(m) = entities.mobs.get(i) else { continue };
            let uid = m.uid;
            if self.weapons.stabbed.iter().any(|&(u, at)| u == uid && clock - at < u64::from(kinetic.contact_cooldown))
            {
                continue;
            }
            self.weapons.stabbed.retain(|&(u, _)| u != uid);
            self.weapons.stabbed.push((uid, clock));
            let target_along = look.dot(m.vel);
            let Some(contact) = kinetic.contact(used, along, target_along, 1.0, true) else { continue };
            let stab = crate::entity::weapon_hits::Stab {
                held: stack,
                damage: contact.damage.then_some(contact.amount),
                knockback: contact.knockback,
                dismount: contact.dismount,
                origin: self.player.pos,
                yaw,
                strength: 1.0,
                owner: self.id,
            };
            let out = entities.stab_mob(i, stab);
            if out.affected {
                affected = affected.saturating_add(1);
                self.after_stab(slot, out.knocked);
            }
        }
        if affected == 0 {
            return None;
        }
        // `onKineticHit`: the hit sound at most every 10 ticks.
        let feedback = clock.saturating_sub(self.weapons.last_feedback) > 10;
        if feedback {
            self.weapons.last_feedback = clock;
        }
        let broken = self.inventory.get(slot).is_none().then_some(stack.item);
        Some(Shot {
            player: self.id,
            ranged: Ranged::WeaponCue { from: eye, cue: feedback.then(|| Cue::spear(stack.item, false, true)) },
            tool_broken: broken,
        })
    }

    /// What every landed stab costs: wear and exhaustion, and the slowed,
    /// unsprinting attacker after an extra knockback.
    fn after_stab(&mut self, slot: usize, knocked: bool) {
        if knocked {
            self.player.vel = crate::knockback::slow_attacker(self.player.vel);
            self.player.sprinting = false;
        }
        if self.mode.is_survival() {
            self.vitals.hunger.exhaust(crate::simulation::survival::EXHAUST_ATTACK);
            let _ = self.inventory.wear(slot, 1);
        }
    }

    /// The player's motion in blocks per second, or its vehicle's
    /// (`KineticWeapon.getMotion` takes a passenger's root vehicle).
    fn motion(&self, entities: &Entities) -> DVec3 {
        match self.player.vehicle {
            Some(id) => entities
                .mount(id)
                .map(|m| m.vel)
                .or_else(|| entities.cart(id).map(|c| c.vel))
                .unwrap_or(self.player.vel),
            None => self.player.vel,
        }
    }

    /// How far along `look` from `eye` the first block collision box is,
    /// within `max` (`ClipContext.Block.COLLIDER`).
    pub(super) fn spear_block_distance(&self, world: &World, eye: DVec3, look: DVec3, max: f64) -> Option<f64> {
        let step = look.normalize_or_zero() * max;
        let to = eye + step;
        let mut boxes = Vec::new();
        crate::physics::colliders(world, eye.min(to), eye.max(to), &mut boxes);
        boxes
            .iter()
            .filter_map(|c| crate::physics::ray_aabb(eye, step, c.min, c.max).filter(|t| *t <= 1.0))
            .min_by(f64::total_cmp)
            .map(|t| t * max)
    }

    /// Releasing use with a trident (`TridentItem.releaseUsing`): after at
    /// least 10 ticks it is thrown, or with Riptide in water or rain the
    /// player launches into a spin. Letting go of a spear just lowers it.
    pub(super) fn release_weapon(&mut self, slot: usize, world: &World, entities: &mut Entities) -> Option<Shot> {
        let (charged, ticks) = self.weapons.using.take()?;
        let stack = self.inventory.get(slot)?;
        if charged != slot || stack.item != weapons::TRIDENT || ticks < tr::THROW_THRESHOLD {
            return None;
        }
        let enchants = stack.active_enchants();
        let strength = tr::riptide_strength(enchants);
        let riding = self.player.vehicle.is_some();
        if strength > 0.0 && !(self.in_water_or_rain(world) && !riding) {
            return None;
        }
        let max = stack.item.durability().unwrap_or(u16::MAX);
        if stack.damage.saturating_add(1) >= max {
            return None;
        }
        let survival = self.mode.is_survival();
        // `hurtWithoutBreaking(1)`: Unbreaking may spare it; it never breaks here.
        if survival
            && let Some(s) = &mut self.inventory.slots[slot]
            && crate::enchant::wears(s.enchants, false, crate::enchant::roll())
        {
            s.damage = (s.damage + 1).min(max - 1);
        }
        let stack = self.inventory.get(slot)?;
        let eye = self.player.eye();
        if strength == 0.0 {
            let creative = self.mode.is_creative();
            if !creative {
                self.inventory.slots[slot] = None;
            }
            let (y_rot, x_rot) = self.java_rotation();
            let carry = self.motion(entities) / TICK_RATE;
            let throw = crate::entity::trident::Throw {
                owner: self.id,
                eye,
                y_rot,
                x_rot,
                carry,
                on_ground: self.player.on_ground,
                creative,
            };
            entities.throw_trident(Stack { count: 1, ..stack }, throw);
            return Some(Shot {
                player: self.id,
                ranged: Ranged::WeaponCue { from: eye, cue: Some(Cue::TridentThrow) },
                tool_broken: None,
            });
        }
        // Riptide: a push along the view, a 20-tick spin attack, and a lift off the ground.
        let level = enchants.level(crate::enchant::Enchantment::Riptide);
        let (y_rot, x_rot) = self.java_rotation();
        let push = tr::rotation_vector(y_rot, x_rot).normalize_or_zero() * f64::from(strength);
        self.player.vel += push * TICK_RATE;
        self.weapons.spin = Some(Spin { ticks: tr::SPIN_TICKS, stack });
        self.weapons.last_pos = Some(self.player.pos);
        if self.player.on_ground {
            let lifted = self.player.pos + DVec3::Y * tr::RIPTIDE_LIFT;
            if !crate::physics::overlaps_solid(world, lifted, self.player.collision_shape()) {
                self.player.pos = lifted;
                self.player.on_ground = false;
            }
        }
        Some(Shot {
            player: self.id,
            ranged: Ranged::WeaponCue { from: eye, cue: Some(Cue::riptide(level)) },
            tool_broken: None,
        })
    }

    /// Java's `(yRot, xRot)` in degrees for this player's view.
    fn java_rotation(&self) -> (f32, f32) {
        (crate::knockback::java_yaw_degrees(self.player.yaw), -self.player.pitch.to_degrees())
    }

    /// Whether Riptide's spin attack is running (`isAutoSpinAttack`).
    pub fn spinning(&self) -> bool {
        self.weapons.spin.is_some()
    }

    /// Once per fixed tick for every player: the session clock and Riptide's
    /// spin attack (`LivingEntity.checkAutoSpinAttack`), which strikes the
    /// first mob its swept box touches with 8 damage and bounces back, or
    /// stops on a wall.
    pub fn tick_weapons(&mut self, entities: &mut Entities) -> Option<Shot> {
        self.weapons.clock = self.weapons.clock.wrapping_add(1);
        let from = self.weapons.last_pos.replace(self.player.pos);
        let mut spin = self.weapons.spin?;
        if self.vitals.is_dead() {
            self.weapons.spin = None;
            return None;
        }
        spin.ticks = spin.ticks.saturating_sub(1);
        let shape = self.player.collision_shape();
        let (a0, a1) = shape.aabb(from.unwrap_or(self.player.pos));
        let (b0, b1) = shape.aabb(self.player.pos);
        let (min, max) = (a0.min(b0), a1.max(b1));
        let hit = entities.mobs.iter().position(|m| {
            let (m0, m1) = m.aabb();
            m.alive() && m0.cmplt(max).all() && m1.cmpgt(min).all()
        });
        let mut shot = None;
        if let Some(i) = hit {
            let strength = self.attack_strength_for_spin();
            let held = Some(spin.stack);
            let hit = crate::entity::PlayerMelee {
                held,
                // `autoSpinAttackDmg` replaces the attack-damage attribute.
                bonus: tr::SPIN_DAMAGE - crate::mining::attack_damage(Some(weapons::TRIDENT)),
                strength,
                critical: false,
                sprint_knockback: false,
                origin: self.player.pos,
                yaw: crate::knockback::java_yaw_degrees(self.player.yaw),
                sweep: false,
                owner: self.id,
                smash: 0.0,
                armor_effectiveness: 0.0,
            };
            let at = entities.mobs[i].pos + DVec3::Y * 0.5;
            entities.player_melee(i, hit);
            self.reset_attack_strength();
            spin.ticks = 0;
            self.player.vel *= -0.2;
            shot = Some(Shot {
                player: self.id,
                ranged: Ranged::WeaponCue { from: at, cue: Some(Cue::TridentHit) },
                tool_broken: None,
            });
        } else if self.player.horizontal_collision() {
            spin.ticks = 0;
        }
        self.weapons.spin = (spin.ticks > 0).then_some(spin);
        shot
    }

    fn attack_strength_for_spin(&self) -> f32 {
        // `Player.attack` scales by the charge of whatever is in the main hand.
        let slot = (0..crate::inventory::HOTBAR_SLOTS)
            .find(|&s| self.inventory.get(s).is_some_and(|s| s.item == weapons::TRIDENT))
            .unwrap_or(0);
        self.attack_strength(slot, 0.5)
    }

    /// Lunge after a jab (`post_piercing_attack`): when allowed, wear and
    /// exhaustion and an impulse along the horizontal look. Returns the level.
    pub(super) fn lunge(&mut self, slot: usize) -> Option<u8> {
        let stack = self.inventory.get(slot)?;
        let level = stack.active_enchants().level(crate::enchant::Enchantment::Lunge);
        if level == 0 {
            return None;
        }
        let food = self.vitals.hunger.food as u32;
        let creative = self.mode.is_creative();
        if !spear::lunge_allowed(self.player.vehicle.is_some(), false, self.player.in_water, food, creative) {
            return None;
        }
        if self.mode.is_survival() {
            let _ = self.inventory.wear(slot, 1);
            self.vitals.hunger.exhaust(spear::lunge_exhaustion(level));
        }
        let look = self.player.forward().as_dvec3();
        self.player.vel += spear::lunge_impulse(level, look) * TICK_RATE;
        Some(level)
    }

    /// For tests and adapters: the player this session belongs to.
    pub fn weapon_owner(&self) -> PlayerId {
        self.id
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use glam::{DVec3, IVec3};

    use crate::{
        enchant::Enchantment,
        entity::{Entities, MobKind, PlayerId},
        inventory::Stack,
        item::Tier,
        rules::GameMode,
        session::{PlayerSession, combat::Outcome, ranged::Ranged},
        simulation::survival::Env,
        weapons::{self, Cue},
        world::{World, block::Block, chunk::ChunkData, terrain::Generator},
    };

    const P: IVec3 = IVec3::new(8, 140, 8);

    fn world() -> World {
        let mut w = World::new_headless(Arc::new(Generator::new(7)), Default::default(), 2);
        w.insert_chunk(IVec3::new(0, 4, 0), Arc::new(ChunkData::Uniform(Block::AIR)), false);
        for x in 0..32 {
            for z in 0..32 {
                w.set_block(IVec3::new(x, P.y - 1, z), Block::STONE);
            }
        }
        w
    }

    fn session(held: Stack) -> PlayerSession {
        let mut s = PlayerSession::new(PlayerId::HOST, P.as_dvec3() + DVec3::new(0.5, 0.0, 0.5));
        s.mode = GameMode::Survival;
        s.player.yaw = 0.0;
        s.player.pitch = 0.0;
        s.inventory.slots[0] = Some(held);
        s
    }

    fn charge(s: &mut PlayerSession, ticks: u32) {
        for _ in 0..ticks {
            s.tick_attack_strength(0);
        }
    }

    #[test]
    fn a_falling_mace_smashes_bounces_and_resets_the_fall() {
        let w = world();
        let mut e = Entities::new(5);
        let mut s = session(Stack::new(weapons::MACE, 1));
        e.spawn(MobKind::Zombie, s.player.pos + DVec3::new(2.0, 0.0, 0.0));
        e.spawn(MobKind::Zombie, s.player.pos + DVec3::new(2.0, 0.0, 2.5));
        e.mobs[0].armor = Default::default();
        charge(&mut s, 40);
        let y = s.player.pos.y;
        s.player.on_ground = false;
        s.vitals.tick(0.05, &Env { y: y + 10.0, ..Default::default() }, false);
        s.vitals.tick(0.05, &Env { y, ..Default::default() }, false);
        assert!((s.vitals.fall_distance(y) - 10.0).abs() < 1e-6);
        let out = s.attack(0, &w, &mut e, false).unwrap();
        let Outcome::Hit { smash: Some(smash), critical, killed, .. } = out else { panic!("{out:?}") };
        assert!(critical);
        assert_eq!(smash.cue, Cue::SmashAir, "the zombie stood in the air");
        assert!(killed.is_some(), "(6 + 24) x 1.5 kills a zombie");
        assert!((s.player.vel.y - 0.2).abs() < 1e-3, "bounced at 0.01 blocks a tick");
        assert_eq!(s.vitals.fall_distance(y), 0.0);
        assert!(e.mobs[1].vel.y > 10.0, "the neighbour is thrown up");
        assert_eq!(s.inventory.get(0).unwrap().damage, 1);
    }

    #[test]
    fn wind_burst_launches_the_attacker_after_a_smash() {
        let w = world();
        let mut e = Entities::new(5);
        let mut mace = Stack::new(weapons::MACE, 1);
        mace.enchants = mace.enchants.with(Enchantment::WindBurst, 1);
        let mut s = session(mace);
        e.spawn(MobKind::IronGolem, s.player.pos + DVec3::new(2.0, 0.0, 0.0));
        charge(&mut s, 40);
        let y = s.player.pos.y;
        s.player.on_ground = false;
        s.vitals.tick(0.05, &Env { y: y + 3.0, ..Default::default() }, false);
        s.vitals.tick(0.05, &Env { y, ..Default::default() }, false);
        let out = s.attack(0, &w, &mut e, false).unwrap();
        let Outcome::Hit { smash: Some(smash), .. } = out else { panic!("{out:?}") };
        assert!(smash.wind_burst.is_some());
        // 0.01 + 1.2 blocks a tick upward.
        assert!(s.player.vel.y > 20.0, "{}", s.player.vel.y);
        assert_eq!(s.vitals.impulse_impact_y(), Some(y));
    }

    #[test]
    fn a_spear_jab_reaches_two_to_four_and_a_half_blocks() {
        let w = world();
        let mut e = Entities::new(5);
        let mut spear = Stack::new(weapons::spear(Tier::Iron), 1);
        spear.enchants = spear.enchants.with(Enchantment::Lunge, 1);
        let mut s = session(spear);
        e.spawn(MobKind::Zombie, s.player.pos + DVec3::new(1.5, 0.0, 0.0));
        e.spawn(MobKind::Zombie, s.player.pos + DVec3::new(4.0, 0.0, 0.0));
        e.mobs[0].armor = Default::default();
        e.mobs[1].armor = Default::default();
        // An uncharged spear refuses to attack at all.
        assert!(s.attack(0, &w, &mut e, false).is_err());
        charge(&mut s, 25);
        let out = s.attack(0, &w, &mut e, false).unwrap();
        let Outcome::Jab { hits, lunge, attack, hit, .. } = out else { panic!("{out:?}") };
        assert_eq!(hits, 1);
        assert_eq!(e.mobs[0].health, MobKind::Zombie.max_health(), "inside the minimum reach");
        assert_eq!(e.mobs[1].health, MobKind::Zombie.max_health() - 3.0);
        assert_eq!((attack, hit), (Cue::SpearAttack, Some(Cue::SpearHit)));
        assert_eq!(lunge, Some(1));
        assert!(s.player.vel.x > 8.0, "Lunge I pushes 0.458 blocks a tick forward");
        // The jab and Lunge each wore it once.
        assert_eq!(s.inventory.get(0).unwrap().damage, 2);
    }

    #[test]
    fn a_charging_spear_stabs_what_a_fast_rider_runs_into() {
        let w = world();
        let mut e = Entities::new(5);
        let mut s = session(Stack::new(weapons::spear(Tier::Wood), 1));
        e.spawn(MobKind::Zombie, s.player.pos + DVec3::new(3.0, 0.0, 0.0));
        e.mobs[0].armor = Default::default();
        let start = s.start_draw(0, &w, &mut e).unwrap();
        assert!(matches!(start.ranged, Ranged::Charging { cue: Some(Cue::SpearWoodUse), .. }));
        assert!(!s.using_item(), "a spear charge doesn't slow the player");
        s.player.vel = DVec3::X * 6.0;
        for _ in 0..14 {
            assert!(s.tick_draw(0, true, &w, &mut e).is_none(), "still in the delay");
        }
        let mut hit = false;
        for _ in 0..3 {
            hit |= s.tick_draw(0, true, &w, &mut e).is_some();
        }
        assert!(hit);
        // 1 + floor(6 * 0.7) = 5 damage.
        assert_eq!(e.mobs[0].health, MobKind::Zombie.max_health() - 5.0);
        // Only once per contact cooldown.
        s.tick_draw(0, true, &w, &mut e);
        assert_eq!(e.mobs[0].health, MobKind::Zombie.max_health() - 5.0);
        assert!(s.release_draw(0, &w, &mut e).is_none());
        assert!(!s.drawing());
    }

    #[test]
    fn tridents_are_thrown_after_ten_ticks_and_riptide_needs_water() {
        let w = world();
        let mut e = Entities::new(5);
        let mut s = session(Stack::new(weapons::TRIDENT, 1));
        let start = s.start_draw(0, &w, &mut e).unwrap();
        assert!(matches!(start.ranged, Ranged::Charging { cue: None, .. }));
        assert!(s.using_item(), "a trident charge slows like a bow");
        for _ in 0..9 {
            s.tick_draw(0, true, &w, &mut e);
        }
        assert!(s.release_draw(0, &w, &mut e).is_none(), "nine ticks is too short");
        assert!(s.inventory.get(0).is_some());
        s.start_draw(0, &w, &mut e).unwrap();
        for _ in 0..10 {
            s.tick_draw(0, true, &w, &mut e);
        }
        let shot = s.release_draw(0, &w, &mut e).unwrap();
        assert!(matches!(shot.ranged, Ranged::WeaponCue { cue: Some(Cue::TridentThrow), .. }));
        assert!(s.inventory.get(0).is_none(), "the trident left the hand");
        assert_eq!(e.tridents.len(), 1);
        assert!((e.tridents[0].vel.length() - 2.5).abs() < 0.1);
        assert_eq!(e.tridents[0].stack.damage, 1);

        // Riptide: not on dry land; in water it launches the player instead.
        let mut riptide = Stack::new(weapons::TRIDENT, 1);
        riptide.enchants = riptide.enchants.with(Enchantment::Riptide, 3);
        let mut s = session(riptide);
        assert!(s.start_draw(0, &w, &mut e).is_none());
        s.player.in_water = true;
        s.start_draw(0, &w, &mut e).unwrap();
        for _ in 0..12 {
            s.tick_draw(0, true, &w, &mut e);
        }
        let shot = s.release_draw(0, &w, &mut e).unwrap();
        assert!(matches!(shot.ranged, Ranged::WeaponCue { cue: Some(Cue::Riptide3), .. }));
        assert!(s.inventory.get(0).is_some(), "Riptide keeps the trident");
        assert!(s.spinning());
        assert!((s.player.vel.x - 60.0).abs() < 0.5, "3 blocks a tick along the view");
        assert_eq!(e.tridents.len(), 1, "nothing thrown");
    }
}
