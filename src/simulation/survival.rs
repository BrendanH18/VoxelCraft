//! Survival health rules: fall damage, drowning, hunger, natural
//! regeneration and death. Pure logic with no world or rendering access, so it is unit
//! tested below; `Game` feeds it the player's surroundings every frame.

/// 10 hearts, in half-heart units.
pub const MAX_HEALTH: f32 = 20.0;
/// Seconds of air (Minecraft's 300 ticks).
pub const MAX_AIR: f32 = 15.0;
pub const AIR_BUBBLES: u32 = 10;
/// Seconds of air regained per second out of water.
const AIR_REFILL_RATE: f32 = 4.0;
const DROWN_DAMAGE: f32 = 2.0;
const DROWN_INTERVAL: f32 = 1.0;
/// Damage per hit while in lava; hurt immunity spaces hits 0.5 s apart.
const LAVA_DAMAGE: f32 = 4.0;
/// Full hunger bar, in half drumsticks.
pub const MAX_FOOD: f32 = 20.0;
/// Hunger and saturation of a fresh player (Minecraft's).
const START_SATURATION: f32 = 5.0;
/// Exhaustion that costs one point of saturation (or food).
const EXHAUSTION_PER_POINT: f32 = 4.0;
/// Health regenerates from this much food, and starvation hurts at zero.
const REGEN_FOOD: f32 = 18.0;
/// Seconds between natural regeneration (or starvation) ticks, and between
/// the fast heals of a full, saturated bar.
const FOOD_TICK: f32 = 4.0;
const FAST_FOOD_TICK: f32 = 0.5;
/// Sprinting needs more food than this.
pub const SPRINT_FOOD: f32 = 6.0;
/// Exhaustion per action (Minecraft's).
pub const EXHAUST_SPRINT_PER_BLOCK: f32 = 0.1;
pub const EXHAUST_SWIM_PER_BLOCK: f32 = 0.01;
pub const EXHAUST_JUMP: f32 = 0.05;
pub const EXHAUST_SPRINT_JUMP: f32 = 0.2;
pub const EXHAUST_ATTACK: f32 = 0.1;
pub const EXHAUST_DAMAGE: f32 = 0.1;
pub const EXHAUST_MINE: f32 = 0.005;
/// After a hit, further damage only applies the amount exceeding it for
/// this long (Minecraft's 10-tick hurt immunity), so mobs hitting every
/// frame do not stack.
pub const INVULNERABLE: f32 = 0.5;
/// Blocks a player can fall without taking damage.
const SAFE_FALL: f64 = 3.0;

pub const CAUSE_FALL: &str = "fell from a high place";
pub const CAUSE_DROWN: &str = "drowned";
pub const CAUSE_LAVA: &str = "tried to swim in lava";
pub const CAUSE_FIRE: &str = "burned to death";
pub const CAUSE_STARVE: &str = "starved to death";

/// Damage left after armor worth `points` (Minecraft's formula without
/// toughness): each point blocks 4%, up to 80%, though big hits punch
/// through some of it.
pub fn armor_reduce(amount: f32, points: u32) -> f32 {
    let defense = (points as f32 - amount / 2.0).max(points as f32 / 5.0).min(20.0);
    amount * (1.0 - defense / 25.0)
}

/// Damage for landing after falling `distance` blocks.
pub fn fall_damage(distance: f64) -> f32 {
    // The epsilon (above the collision skin) keeps exact block drops from
    // rounding down a point.
    (distance - SAFE_FALL + 1e-4).floor().max(0.0) as f32
}

/// What the player is touching this frame.
#[derive(Clone, Copy, Debug, Default)]
pub struct Env {
    /// Feet height.
    pub y: f64,
    pub on_ground: bool,
    pub flying: bool,
    /// Body in water: breaks falls.
    pub in_water: bool,
    /// On a ladder: also breaks falls.
    pub climbing: bool,
    /// Eyes under water: uses up air.
    pub head_in_water: bool,
    /// Body touching lava: burns.
    pub in_lava: bool,
    pub in_fire: bool,
    /// Exposed to rain: extinguishes the player like body water does.
    pub wet: bool,
    /// Horizontal distance moved this frame, and whether sprinting /
    /// jumping (hunger).
    pub moved: f64,
    pub sprinting: bool,
    pub jumped: bool,
}

/// Damage produced by one [`Vitals::tick`]; the caller applies it through
/// its single damage entry point.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Hurts {
    /// Blocks fallen by a landing this tick (0 otherwise), for trampling.
    pub landed: f64,
    pub fall: f32,
    pub drown: f32,
    pub lava: f32,
    pub fire: f32,
    /// Lingering burning damage bypasses armor; contact fire does not.
    pub burn: f32,
    pub starve: f32,
}

/// Minecraft's hunger: a food bar backed by a hidden saturation buffer that
/// drains first, both worn down by exhaustion from activity.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Hunger {
    /// 0..=MAX_FOOD, in half drumsticks.
    pub food: f32,
    /// 0..=food.
    pub saturation: f32,
    /// Accumulated effort; every 4 points cost a point of saturation or food.
    pub exhaustion: f32,
    /// Counts up to the next regeneration or starvation tick.
    timer: f32,
}

impl Default for Hunger {
    fn default() -> Self {
        Self { food: MAX_FOOD, saturation: START_SATURATION, exhaustion: 0.0, timer: 0.0 }
    }
}

impl Hunger {
    /// Restore saved hunger values within their limits and restart the regeneration timer.
    pub fn restore(food: f32, saturation: f32, exhaustion: f32) -> Self {
        let food = food.clamp(0.0, MAX_FOOD);
        Self {
            food,
            saturation: saturation.clamp(0.0, food),
            exhaustion: exhaustion.clamp(0.0, EXHAUSTION_PER_POINT),
            timer: 0.0,
        }
    }

    /// Spend accumulated effort in four-point units, draining saturation before food.
    pub fn exhaust(&mut self, amount: f32) {
        self.exhaustion += amount;
        while self.exhaustion >= EXHAUSTION_PER_POINT {
            self.exhaustion -= EXHAUSTION_PER_POINT;
            if self.saturation > 0.0 {
                self.saturation = (self.saturation - 1.0).max(0.0);
            } else {
                self.food = (self.food - 1.0).max(0.0);
            }
        }
    }

    /// Whether eating would do anything (Minecraft won't let you eat full).
    pub fn can_eat(&self) -> bool {
        self.food < MAX_FOOD
    }

    /// Add food and saturation, capping food at its maximum and saturation at current food.
    pub fn eat(&mut self, food: u8, saturation: f32) {
        self.food = (self.food + food as f32).min(MAX_FOOD);
        self.saturation = (self.saturation + saturation).min(self.food);
    }

    /// Whether food exceeds the survival sprint threshold of six points.
    pub fn can_sprint(&self) -> bool {
        self.food > SPRINT_FOOD
    }

    /// Advances regeneration and starvation; returns (health healed,
    /// starvation damage).
    fn tick(&mut self, dt: f32, health: f32) -> (f32, f32) {
        let hurt = health < MAX_HEALTH;
        if self.food >= MAX_FOOD && self.saturation > 0.0 && hurt {
            // Full and saturated: heal fast, paying in saturation.
            self.timer += dt;
            if self.timer >= FAST_FOOD_TICK {
                self.timer = 0.0;
                let spend = self.saturation.min(6.0);
                self.exhaust(spend);
                return (spend / 6.0, 0.0);
            }
        } else if self.food >= REGEN_FOOD && hurt {
            self.timer += dt;
            if self.timer >= FOOD_TICK {
                self.timer = 0.0;
                self.exhaust(6.0);
                return (1.0, 0.0);
            }
        } else if self.food <= 0.0 {
            self.timer += dt;
            if self.timer >= FOOD_TICK {
                self.timer = 0.0;
                // Normal difficulty: starving stops at half a heart.
                return (0.0, if health > 1.0 { 1.0 } else { 0.0 });
            }
        } else {
            self.timer = 0.0;
        }
        (0.0, 0.0)
    }
}

#[derive(Clone, Debug)]
pub struct Vitals {
    /// 0..=MAX_HEALTH, in half hearts.
    pub health: f32,
    /// Seconds of air left, 0..=MAX_AIR.
    pub air: f32,
    pub hunger: Hunger,
    /// Seconds since the last damage (drives the hurt flash).
    since_damage: f32,
    drown_timer: f32,
    fire_left: f32,
    fire_timer: f32,
    /// Damage of the hit that started the current immunity window.
    last_damage: f32,
    /// Highest feet Y since last standing on the ground (None while
    /// flying or swimming).
    fall_peak: Option<f64>,
    /// Set while dead: what killed the player.
    pub death: Option<String>,
}

impl Default for Vitals {
    fn default() -> Self {
        Self {
            health: MAX_HEALTH,
            air: MAX_AIR,
            hunger: Hunger::default(),
            since_damage: 1e3,
            drown_timer: 0.0,
            fire_left: 0.0,
            fire_timer: 0.0,
            last_damage: 0.0,
            fall_peak: None,
            death: None,
        }
    }
}

impl Vitals {
    /// Restores saved values; zero health means the player died.
    pub fn restore(health: f32, air: f32, death: Option<String>) -> Self {
        let mut v = Self { health: health.clamp(0.0, MAX_HEALTH), air: air.clamp(0.0, MAX_AIR), ..Self::default() };
        if v.health <= 0.0 {
            v.death = Some(death.unwrap_or_else(|| "died".into()));
        }
        v
    }

    /// Whether a death cause has been recorded; damage and survival ticks respect this state.
    pub fn is_dead(&self) -> bool {
        self.death.is_some()
    }

    /// Whether a living player has time remaining on their fire effect.
    pub fn burning(&self) -> bool {
        self.fire_left > 0.0 && !self.is_dead()
    }

    /// Seconds since the hit that started the immunity window; stronger hits within it do not reset this.
    pub fn since_damage(&self) -> f32 {
        self.since_damage
    }

    /// Air bubbles to show, 0..=AIR_BUBBLES.
    pub fn bubbles(&self) -> u32 {
        ((self.air / MAX_AIR * AIR_BUBBLES as f32).ceil() as u32).min(AIR_BUBBLES)
    }

    /// Applies damage and returns how much was actually taken (0 in
    /// creative, while dead, or when absorbed by hurt immunity).
    pub fn damage(&mut self, amount: f32, cause: &str, creative: bool) -> f32 {
        if creative || self.is_dead() || amount <= 0.0 {
            return 0.0;
        }
        let taken = if self.since_damage < INVULNERABLE {
            // Only a stronger hit gets through, and only the difference.
            if amount <= self.last_damage {
                return 0.0;
            }
            let extra = amount - self.last_damage;
            self.last_damage = amount;
            extra
        } else {
            self.last_damage = amount;
            self.since_damage = 0.0;
            amount
        };
        self.health = (self.health - taken).max(0.0);
        self.hunger.exhaust(EXHAUST_DAMAGE);
        if self.health <= 0.0 {
            self.death = Some(cause.to_string());
        }
        taken
    }

    /// Advances air, hunger, regeneration and fall tracking by `dt` seconds
    /// and returns fall/drowning/lava/starvation damage for the caller to
    /// apply.
    pub fn tick(&mut self, dt: f32, env: &Env, creative: bool) -> Hurts {
        let mut hurts = Hurts::default();
        self.since_damage = (self.since_damage + dt).min(1e3);
        if self.is_dead() {
            return hurts;
        }

        // Falling: remember the highest point since standing on the ground
        // (including the ground itself, so stepping off a ledge counts
        // the full drop).
        if env.flying || env.in_water {
            self.fall_peak = None;
        } else if env.climbing {
            self.fall_peak = Some(env.y);
        } else if env.on_ground {
            if let Some(peak) = self.fall_peak {
                hurts.landed = peak - env.y;
                hurts.fall = fall_damage(peak - env.y);
            }
            self.fall_peak = Some(env.y);
        } else {
            self.fall_peak = Some(self.fall_peak.map_or(env.y, |p| p.max(env.y)));
        }

        // Air and drowning.
        if env.head_in_water && !creative {
            self.air = (self.air - dt).max(0.0);
            if self.air <= 0.0 {
                self.drown_timer += dt;
                if self.drown_timer >= DROWN_INTERVAL {
                    self.drown_timer -= DROWN_INTERVAL;
                    hurts.drown = DROWN_DAMAGE;
                }
            }
        } else {
            self.air = (self.air + dt * AIR_REFILL_RATE).min(MAX_AIR);
            self.drown_timer = 0.0;
        }

        if env.in_lava && !creative {
            hurts.lava = LAVA_DAMAGE;
        }

        // Lava ignites for 15 s; fire for 8 s. Contact hurts immediately,
        // and after leaving it the player burns for 1 damage each second.
        if creative {
            self.fire_left = 0.0;
        } else if env.in_lava {
            self.fire_left = 15.0;
        } else if env.in_water || env.wet {
            self.fire_left = 0.0;
        } else if env.in_fire {
            self.fire_left = 8.0;
            hurts.fire = 1.0;
        } else {
            self.fire_left = (self.fire_left - dt).max(0.0);
        }
        if self.fire_left > 0.0 {
            self.fire_timer += dt;
            if self.fire_timer >= 1.0 {
                self.fire_timer -= 1.0;
                hurts.burn = 1.0;
            }
        } else {
            self.fire_timer = 0.0;
        }

        // Hunger: activity wears it down; it drives regeneration and starvation.
        if !creative {
            let h = &mut self.hunger;
            let per_block = if env.in_water {
                EXHAUST_SWIM_PER_BLOCK
            } else if env.sprinting {
                EXHAUST_SPRINT_PER_BLOCK
            } else {
                0.0
            };
            h.exhaust(env.moved as f32 * per_block);
            if env.jumped {
                h.exhaust(if env.sprinting { EXHAUST_SPRINT_JUMP } else { EXHAUST_JUMP });
            }
            let (heal, starve) = h.tick(dt, self.health);
            self.health = (self.health + heal).min(MAX_HEALTH);
            hurts.starve = starve;
        }
        hurts
    }

    /// Forgets the height a fall started from (after a teleport).
    pub fn reset_fall(&mut self) {
        self.fall_peak = None;
    }

    /// Full health and air, alive, nothing pending.
    pub fn respawn(&mut self) {
        *self = Self::default();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const DT: f32 = 1.0 / 60.0;

    #[test]
    fn fire_keeps_burning_then_water_rain_and_creative_extinguish_it() {
        let mut v = Vitals::default();
        let fire = Env { in_fire: true, ..Env::default() };
        assert_eq!(v.tick(DT, &fire, false).fire, 1.0, "contact hurts immediately");
        assert!(v.burning());
        assert_eq!(v.tick(1.0, &Env::default(), false).burn, 1.0, "burns after leaving fire");
        for _ in 0..8 {
            v.tick(1.0, &Env::default(), false);
        }
        assert!(!v.burning(), "fire expires");
        for extinguished in [Env { in_water: true, ..Env::default() }, Env { wet: true, ..Env::default() }] {
            v.tick(DT, &fire, false);
            assert!(v.burning());
            let h = v.tick(DT, &extinguished, false);
            assert_eq!((h.fire, h.burn), (0.0, 0.0));
            assert!(!v.burning());
        }
        v.tick(DT, &fire, false);
        let h = v.tick(DT, &fire, true);
        assert_eq!((h.fire, h.burn), (0.0, 0.0));
        assert!(!v.burning());
        let lava = Env { in_lava: true, ..Env::default() };
        v.tick(DT, &lava, false);
        v.tick(10.0, &Env::default(), false);
        assert!(v.burning(), "lava ignites for longer than fire");
        v.respawn();
        assert!(!v.burning());
    }

    fn ground(y: f64) -> Env {
        Env { y, on_ground: true, ..Env::default() }
    }

    fn air(y: f64) -> Env {
        Env { y, ..Env::default() }
    }

    /// Falls from `from` to `to` in `steps` frames and lands.
    fn fall(v: &mut Vitals, from: f64, to: f64, landing: Env) -> Hurts {
        v.tick(DT, &ground(from), false);
        for i in 0..=20 {
            let y = from + (to - from) * i as f64 / 20.0;
            v.tick(DT, &air(y), false);
        }
        v.tick(DT, &landing, false)
    }

    #[test]
    fn armor_blocks_most_of_small_hits() {
        assert_eq!(armor_reduce(4.0, 0), 4.0);
        assert!((armor_reduce(4.0, 20) - 4.0 * 7.0 / 25.0).abs() < 1e-5, "full diamond blocks 72%");
        assert!((armor_reduce(4.0, 7) - 4.0 * (1.0 - 5.0 / 25.0)).abs() < 1e-5);
        // A huge blast gets through more of it.
        assert!(armor_reduce(40.0, 20) / 40.0 > 0.2);
    }

    #[test]
    fn fall_damage_thresholds() {
        assert_eq!(fall_damage(0.0), 0.0);
        assert_eq!(fall_damage(3.0), 0.0);
        assert_eq!(fall_damage(3.9), 0.0);
        assert_eq!(fall_damage(74.00001 - 70.00001), 1.0);
        assert_eq!(fall_damage(4.0), 1.0);
        assert_eq!(fall_damage(10.5), 7.0);
        assert_eq!(fall_damage(-5.0), 0.0);
    }

    #[test]
    fn landing_after_fall_hurts() {
        let mut v = Vitals::default();
        // A jump peaking 1.25 blocks up does nothing.
        v.tick(DT, &ground(64.0), false);
        v.tick(DT, &air(65.25), false);
        assert_eq!(v.tick(DT, &ground(64.0), false).fall, 0.0);
        // Ten blocks: 7 damage.
        let hurts = fall(&mut v, 74.0, 64.0, ground(64.0));
        assert_eq!(hurts.fall, 7.0);
        // Stepping off a 4-block ledge counts from the ledge top.
        v.tick(DT, &ground(68.0), false);
        v.tick(DT, &air(67.99), false);
        assert_eq!(v.tick(DT, &ground(64.0), false).fall, 1.0);
        // Standing still never hurts.
        for _ in 0..10 {
            assert_eq!(v.tick(DT, &ground(64.0), false).fall, 0.0);
        }
    }

    #[test]
    fn water_and_flight_break_falls() {
        let mut v = Vitals::default();
        let splash = Env { y: 60.0, in_water: true, ..Env::default() };
        assert_eq!(fall(&mut v, 100.0, 60.0, splash).fall, 0.0);
        // Swimming to the bottom afterwards is not a fall either.
        assert_eq!(v.tick(DT, &ground(58.0), false).fall, 0.0);

        let mut v = Vitals::default();
        v.tick(DT, &Env { y: 100.0, flying: true, ..Env::default() }, false);
        // Flight stops 2 blocks up: only those 2 blocks count.
        v.tick(DT, &air(66.0), false);
        assert_eq!(v.tick(DT, &ground(64.0), false).fall, 0.0);
    }

    #[test]
    fn drowning_after_air_runs_out() {
        let mut v = Vitals::default();
        let under = Env { y: 50.0, in_water: true, head_in_water: true, ..Env::default() };
        let mut t = 0.0;
        let mut first_hurt = None;
        while t < 20.0 {
            let h = v.tick(DT, &under, false);
            t += DT;
            if h.drown > 0.0 && first_hurt.is_none() {
                first_hurt = Some(t);
                assert_eq!(h.drown, 2.0);
            }
            if t < MAX_AIR - 0.1 {
                assert!(v.air > 0.0, "air ran out early at {t}");
                assert!(v.bubbles() >= 1);
            }
        }
        let first = first_hurt.expect("never drowned");
        assert!((MAX_AIR + 0.9..MAX_AIR + 1.1).contains(&first), "first drown hit at {first}");
        assert_eq!(v.bubbles(), 0);

        // Surfacing refills air within a few seconds.
        for _ in 0..(4.0 / DT) as usize {
            v.tick(DT, &ground(50.0), false);
        }
        assert_eq!(v.air, MAX_AIR);
        assert_eq!(v.bubbles(), AIR_BUBBLES);
    }

    /// Ticks `secs` of standing still.
    fn wait(v: &mut Vitals, secs: f32) {
        for _ in 0..(secs / DT) as usize {
            v.tick(DT, &ground(0.0), false);
        }
    }

    #[test]
    fn regeneration_runs_on_food() {
        // Full and saturated: half a heart every 0.5 s, paid in saturation.
        let mut v = Vitals::default();
        v.damage(6.0, "test", false);
        wait(&mut v, 1.1);
        assert!(v.health >= 15.5, "fast regen: {}", v.health);
        // Saturation gone, food 18-19: half a heart every 4 s.
        let mut v = Vitals { hunger: Hunger::restore(18.0, 0.0, 0.0), ..Vitals::default() };
        v.damage(6.0, "test", false);
        wait(&mut v, 3.9);
        assert_eq!(v.health, 14.0);
        wait(&mut v, 0.2);
        assert_eq!(v.health, 15.0);
        // Each heal costs 6 exhaustion, so healing eats into the bar.
        wait(&mut v, 4.0 * 6.0);
        assert!(v.hunger.food < 18.0 && v.hunger.food > 15.0, "food {}", v.hunger.food);
        // Below 18 food there is no regeneration.
        let healed = v.health;
        wait(&mut v, 10.0);
        assert_eq!(v.health, healed);
    }

    #[test]
    fn exhaustion_drains_saturation_then_food() {
        let mut h = Hunger::default();
        h.exhaust(4.0 * 5.0);
        assert_eq!((h.food, h.saturation), (MAX_FOOD, 0.0));
        h.exhaust(4.0 * 3.0);
        assert_eq!(h.food, 17.0);
        // Sprinting 40 blocks costs one point (a little more, for rounding);
        // walking costs nothing.
        let mut v = Vitals { hunger: Hunger::restore(17.0, 0.0, 0.0), ..Vitals::default() };
        let sprint = Env { on_ground: true, moved: 0.1, sprinting: true, ..Env::default() };
        for _ in 0..410 {
            v.tick(DT, &sprint, false);
        }
        assert_eq!(v.hunger.food, 16.0);
        let walk = Env { sprinting: false, ..sprint };
        for _ in 0..4000 {
            v.tick(DT, &walk, false);
        }
        assert_eq!(v.hunger.food, 16.0);
        assert!(!Hunger::restore(6.0, 0.0, 0.0).can_sprint() && Hunger::restore(7.0, 0.0, 0.0).can_sprint());
    }

    #[test]
    fn eating_and_starving() {
        let mut h = Hunger::restore(10.0, 0.0, 0.0);
        h.eat(8, 12.8);
        assert_eq!((h.food, h.saturation), (18.0, 12.8));
        h.eat(8, 12.8);
        assert_eq!((h.food, h.saturation), (MAX_FOOD, MAX_FOOD), "both capped");
        assert!(!h.can_eat());

        // An empty bar hurts every 4 s, down to half a heart.
        let mut v = Vitals { hunger: Hunger::restore(0.0, 0.0, 0.0), ..Vitals::default() };
        let mut taken = 0.0;
        for _ in 0..(200.0 / DT) as usize {
            let h = v.tick(DT, &ground(0.0), false);
            taken += v.damage(h.starve, CAUSE_STARVE, false);
        }
        assert_eq!(v.health, 1.0);
        assert_eq!(taken, 19.0);
        assert!(!v.is_dead());
        // Creative players don't get hungry.
        let mut v = Vitals::default();
        let sprint = Env { on_ground: true, moved: 1.0, sprinting: true, ..Env::default() };
        for _ in 0..1000 {
            v.tick(DT, &sprint, true);
        }
        assert_eq!(v.hunger, Hunger::default());
    }

    #[test]
    fn lava_burns_every_half_second() {
        let mut v = Vitals::default();
        let lava = Env { y: 10.0, in_water: true, in_lava: true, ..Env::default() };
        let mut taken = 0.0;
        for _ in 0..60 {
            let h = v.tick(1.0 / 60.0, &lava, false);
            taken += v.damage(h.lava, CAUSE_LAVA, false);
        }
        // One second in lava: hits at 0 s and 0.5 s.
        assert_eq!(taken, 8.0);
        assert_eq!(v.tick(0.1, &lava, true).lava, 0.0, "creative is immune");
    }

    #[test]
    fn creative_is_immune() {
        let mut v = Vitals::default();
        assert_eq!(v.damage(100.0, "test", true), 0.0);
        assert_eq!(v.health, MAX_HEALTH);
        let under = Env { head_in_water: true, in_water: true, ..Env::default() };
        for _ in 0..(30.0 / DT) as usize {
            let h = v.tick(DT, &under, true);
            assert_eq!(h.drown, 0.0);
        }
        assert_eq!(v.air, MAX_AIR);
        assert!(!v.is_dead());
    }

    #[test]
    fn hurt_immunity_absorbs_repeated_hits() {
        let mut v = Vitals::default();
        assert_eq!(v.damage(3.0, "test", false), 3.0);
        assert_eq!(v.damage(3.0, "test", false), 0.0);
        assert_eq!(v.damage(5.0, "test", false), 2.0);
        assert_eq!(v.health, 15.0);
        for _ in 0..(INVULNERABLE / DT) as usize + 2 {
            v.tick(DT, &ground(0.0), false);
        }
        assert_eq!(v.damage(3.0, "test", false), 3.0);
    }

    /// Real player physics on generated terrain feeding the rules.
    #[test]
    fn falls_through_player_physics() {
        use crate::player::{MoveInput, Player};
        use crate::world::World;
        use crate::world::block::Block;
        use crate::world::terrain::Generator;
        use glam::{DVec3, IVec3};
        use std::sync::Arc;
        use std::time::{Duration, Instant};

        let generator = Arc::new(Generator::new(7));
        let spawn = generator.find_spawn();
        let mut world = World::new(generator, Default::default(), 3);
        let deadline = Instant::now() + Duration::from_secs(20);
        while !(world.loaded_chunks() > 0 && world.is_idle()) {
            world.update(spawn.as_dvec3());
            world.mesh_uploads.clear();
            assert!(Instant::now() < deadline, "world never settled");
            std::thread::sleep(Duration::from_millis(1));
        }
        let (x, z) = (spawn.x, spawn.z);
        let ground =
            (0..512).rev().find(|&y| world.get_block(IVec3::new(x, y, z)).is_some_and(|b| b.is_solid())).unwrap();
        let top = ground as f64 + 1.0;

        let drop = |world: &World, height: f64| {
            let mut player = Player::new(DVec3::new(x as f64 + 0.5, top + height, z as f64 + 0.5));
            let mut v = Vitals::default();
            v.tick(DT, &Env { y: player.pos.y, ..Env::default() }, false);
            let mut taken = 0.0;
            for _ in 0..240 {
                player.update(DT as f64, MoveInput::default(), world);
                let env = Env {
                    y: player.pos.y,
                    on_ground: player.on_ground,
                    flying: player.flying,
                    in_water: player.in_water,
                    head_in_water: player.head_in_water(world),
                    in_lava: player.in_lava(world),
                    ..Env::default()
                };
                let h = v.tick(DT, &env, false);
                taken += v.damage(h.fall, CAUSE_FALL, false);
            }
            assert!(player.on_ground, "player never landed");
            (taken, v.death)
        };
        assert_eq!(drop(&world, 3.0), (0.0, None));
        assert_eq!(drop(&world, 10.0), (7.0, None));
        assert_eq!(drop(&world, 30.0), (27.0, Some(CAUSE_FALL.to_string())));

        // Two blocks of water cushion the same fall.
        world.set_block(IVec3::new(x, ground + 1, z), Block::WATER);
        world.set_block(IVec3::new(x, ground + 2, z), Block::WATER);
        assert_eq!(drop(&world, 30.0), (0.0, None));
    }

    #[test]
    fn death_and_respawn() {
        let mut v = Vitals::default();
        v.damage(25.0, CAUSE_FALL, false);
        assert_eq!(v.health, 0.0);
        assert_eq!(v.death.as_deref(), Some(CAUSE_FALL));
        // Dead players take no more damage and do not regenerate.
        assert_eq!(v.damage(5.0, "test", false), 0.0);
        for _ in 0..(10.0 / DT) as usize {
            v.tick(DT, &ground(0.0), false);
        }
        assert_eq!(v.health, 0.0);
        v.respawn();
        assert!(!v.is_dead());
        assert_eq!((v.health, v.air), (MAX_HEALTH, MAX_AIR));
        assert_eq!(v.hunger, Hunger::default());

        let restored = Vitals::restore(0.0, 3.0, None);
        assert!(restored.is_dead());
        let restored = Vitals::restore(7.5, 99.0, None);
        assert_eq!((restored.health, restored.air), (7.5, MAX_AIR));
    }
}
