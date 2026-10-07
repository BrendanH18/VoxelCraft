//! Experience levels and points, following Java Edition: the level curve,
//! how orbs split an award, the two-tick pickup rhythm, the level-up chime
//! every five levels and what a dying player drops.

/// Seconds between orbs a player absorbs (Java's 2-tick `takeXpDelay`).
pub const PICKUP_INTERVAL: f32 = 0.1;
/// Seconds between level-up chimes (Java's 100 ticks).
const CHIME_INTERVAL: f32 = 5.0;
/// Highest level points can reach; keeps the arithmetic far from overflow.
const MAX_LEVEL: u32 = 100_000;

/// Points needed to go from `level` to the next one (Java's curve).
pub fn points_to_next(level: u32) -> u32 {
    match level {
        0..=15 => 2 * level + 7,
        16..=30 => 5 * level - 38,
        _ => 9 * level - 158,
    }
}

/// Orb values Java splits an award of `points` into: the biggest of
/// 2477, 1237, 617, 307, 149, 73, 37, 17, 7, 3 and 1 that fits, repeatedly.
pub fn orb_values(mut points: u32) -> impl Iterator<Item = u32> {
    std::iter::from_fn(move || {
        let v = orb_value(points);
        (points > 0).then(|| {
            points -= v;
            v
        })
    })
}

/// The largest orb size no bigger than `points` (1 for 0).
pub fn orb_value(points: u32) -> u32 {
    const SIZES: [u32; 10] = [2477, 1237, 617, 307, 149, 73, 37, 17, 7, 3];
    SIZES.into_iter().find(|&s| points >= s).unwrap_or(1)
}

/// Which of Java's 11 orb sprites (0 = smallest) an orb worth `value` uses.
pub fn orb_icon(value: u32) -> u32 {
    const SIZES: [u32; 10] = [3, 7, 17, 37, 73, 149, 307, 617, 1237, 2477];
    SIZES.iter().filter(|&&s| value >= s).count() as u32
}

/// Rounds a fractional award (furnace smelting) the way Java does: the
/// whole part, plus one more with the fraction as its chance. `roll` is a
/// uniform number in 0..1.
pub fn round_award(amount: f32, roll: f32) -> u32 {
    let whole = amount.max(0.0).floor();
    whole as u32 + (roll < amount - whole) as u32
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Experience {
    pub level: u32,
    /// Points into the current level, below [`points_to_next`]`(level)`.
    pub points: u32,
    /// Every point collected since the last death (the death screen score).
    pub total: u32,
    /// Seconds until the next orb can be absorbed.
    pub pickup_cooldown: f32,
    /// Seconds since the last level-up chime.
    since_chime: f32,
    /// Java's enchantment seed: fixes the enchanting table's offers until
    /// the player enchants something.
    pub seed: i32,
}

impl Experience {
    /// Saved values, with the points clamped into the level.
    pub fn restore(level: u32, points: u32, total: u32) -> Self {
        let level = level.min(MAX_LEVEL);
        Self { level, points: points.min(points_to_next(level) - 1), total, ..Self::default() }
    }

    /// Fill of the experience bar, 0..1.
    pub fn progress(&self) -> f32 {
        self.points as f32 / points_to_next(self.level) as f32
    }

    /// Advances the pickup and chime timers.
    pub fn tick(&mut self, dt: f32) {
        self.pickup_cooldown = (self.pickup_cooldown - dt).max(0.0);
        self.since_chime = (self.since_chime + dt).min(1e3);
    }

    /// Adds (or with a negative `n`, removes) points, carrying across
    /// levels. Returns the chime volume if a gain reached a multiple of five
    /// levels, as Java plays its level-up sound then (louder up to level 30,
    /// at most once every five seconds).
    pub fn add_points(&mut self, n: i64) -> Option<f32> {
        self.total = (self.total as i64 + n).clamp(0, u32::MAX as i64) as u32;
        let mut points = self.points as i64 + n;
        let mut milestone = false;
        while points >= points_to_next(self.level) as i64 && self.level < MAX_LEVEL {
            points -= points_to_next(self.level) as i64;
            self.level += 1;
            milestone |= self.level.is_multiple_of(5);
        }
        while points < 0 && self.level > 0 {
            self.level -= 1;
            points += points_to_next(self.level) as i64;
        }
        self.points = points.clamp(0, points_to_next(self.level) as i64 - 1) as u32;
        self.chime(milestone)
    }

    /// Adds or removes whole levels (commands, and later enchanting),
    /// keeping the bar's fill like Java.
    pub fn add_levels(&mut self, n: i64) -> Option<f32> {
        let fill = self.progress();
        let before = self.level;
        self.level = (self.level as i64 + n).clamp(0, MAX_LEVEL as i64) as u32;
        self.points = ((fill * points_to_next(self.level) as f32) as u32).min(points_to_next(self.level) - 1);
        self.chime(self.level > before && self.level.is_multiple_of(5))
    }

    /// Returns milestone chime volume when its cooldown permits, then resets the timer.
    fn chime(&mut self, milestone: bool) -> Option<f32> {
        if !milestone || self.since_chime < CHIME_INTERVAL {
            return None;
        }
        self.since_chime = 0.0;
        Some(0.75 * (self.level as f32 / 30.0).min(1.0))
    }

    /// Points a dying player leaves behind: seven per level, at most 100.
    pub fn death_drop(&self) -> u32 {
        self.level.saturating_mul(7).min(100)
    }

    /// A death: returns the points to drop as orbs and loses every level.
    /// The total stays as the death screen's score until respawning.
    pub fn die(&mut self) -> u32 {
        let drop = self.death_drop();
        *self = Self { total: self.total, seed: self.seed, ..Self::default() };
        drop
    }

    /// `level,points,total,seed` for saves.
    pub fn serialize(&self) -> String {
        format!("{},{},{},{}", self.level, self.points, self.total, self.seed)
    }

    /// Reads what [`Experience::serialize`] wrote (older saves have no seed).
    pub fn parse(text: &str) -> Option<Self> {
        let mut fields = text.split(',').map(str::trim);
        let mut next = || fields.next().map(|v| v.parse::<u32>().ok());
        let (level, points, total) = (next()??, next()??, next()??);
        let seed = match fields.next() {
            Some(v) => v.parse::<i32>().ok()?,
            None => 0,
        };
        fields.next().is_none().then_some(Self { seed, ..Self::restore(level, points, total) })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Java's total points to reach `level` from zero.
    fn total_for(level: u32) -> u32 {
        (0..level).map(points_to_next).sum()
    }

    #[test]
    fn level_curve_matches_java() {
        assert_eq!([points_to_next(0), points_to_next(15), points_to_next(16)], [7, 37, 42]);
        assert_eq!([points_to_next(30), points_to_next(31)], [112, 121]);
        // The wiki's totals: level 16 at 352, 31 at 1507, 30 at 1395.
        assert_eq!([total_for(16), total_for(30), total_for(31)], [352, 1395, 1507]);
    }

    #[test]
    fn points_carry_across_levels_and_back() {
        let mut xp = Experience::default();
        xp.add_points(10);
        assert_eq!((xp.level, xp.points, xp.total), (1, 3, 10));
        xp.add_points(total_for(30) as i64 - 10);
        assert_eq!((xp.level, xp.points), (30, 0));
        xp.add_points(-1);
        assert_eq!((xp.level, xp.points), (29, points_to_next(29) - 1));
        xp.add_points(-100_000);
        assert_eq!((xp.level, xp.points, xp.total), (0, 0, 0));
        assert!((Experience::restore(0, 3, 3).progress() - 3.0 / 7.0).abs() < 1e-6);
    }

    #[test]
    fn levels_keep_the_bar_fill() {
        let mut xp = Experience::restore(10, points_to_next(10) / 2, 0);
        xp.add_levels(-3);
        assert_eq!(xp.level, 7);
        assert!((xp.progress() - 0.5).abs() < 0.1);
        xp.add_levels(-50);
        assert_eq!(xp.level, 0);
    }

    #[test]
    fn chime_every_five_levels_at_most_every_five_seconds() {
        let mut xp = Experience::default();
        xp.tick(10.0);
        assert!(xp.add_points(total_for(4) as i64).is_none(), "level 4");
        let quiet = xp.add_points(points_to_next(4) as i64).expect("level 5 chimes");
        assert!((quiet - 0.125).abs() < 1e-6, "quiet at low levels: {quiet}");
        assert!(xp.add_points(total_for(10) as i64).is_none(), "too soon after the last chime");
        xp.tick(5.0);
        assert_eq!(xp.add_points(total_for(40) as i64), Some(0.75), "a jump past several milestones");
        xp.tick(5.0);
        assert!(xp.add_points(-(total_for(45) as i64)).is_none(), "losing levels never chimes");
    }

    #[test]
    fn awards_split_into_java_orb_sizes() {
        assert_eq!(orb_values(0).count(), 0);
        assert_eq!(orb_values(5).collect::<Vec<_>>(), [3, 1, 1]);
        assert_eq!(orb_values(100).collect::<Vec<_>>(), [73, 17, 7, 3]);
        assert_eq!(orb_values(2600).sum::<u32>(), 2600);
        assert_eq!([orb_icon(1), orb_icon(3), orb_icon(16), orb_icon(17), orb_icon(5000)], [0, 1, 2, 3, 10]);
        assert_eq!([round_award(0.7, 0.69), round_award(0.7, 0.71), round_award(2.0, 0.0)], [1, 0, 2]);
    }

    #[test]
    fn death_drop_and_saving() {
        assert_eq!(Experience::restore(3, 0, 0).death_drop(), 21);
        assert_eq!(Experience::restore(30, 0, 0).death_drop(), 100);
        let mut xp = Experience::restore(4, 2, 50);
        assert_eq!(xp.die(), 28);
        assert_eq!((xp.level, xp.points, xp.total, xp.die()), (0, 0, 50, 0), "dying twice drops nothing");
        let xp = Experience::restore(12, 5, 300);
        assert_eq!(Experience::parse(&xp.serialize()), Some(xp));
        assert_eq!(Experience::parse("2,999,5").map(|x| x.points), Some(10), "points clamp into the level");
        assert!(Experience::parse("1,2").is_none() && Experience::parse("a,b,c").is_none());
    }
}
