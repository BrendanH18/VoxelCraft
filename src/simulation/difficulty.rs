//! Per-world difficulty and the Java rules which depend directly on it.

use std::fmt;

/// Java Edition's four world difficulties, in menu order.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Difficulty {
    Peaceful,
    Easy,
    #[default]
    Normal,
    Hard,
}

impl Difficulty {
    pub const ALL: [Self; 4] = [Self::Peaceful, Self::Easy, Self::Normal, Self::Hard];

    /// Lowercase name used by saves and commands.
    pub const fn name(self) -> &'static str {
        match self {
            Self::Peaceful => "peaceful",
            Self::Easy => "easy",
            Self::Normal => "normal",
            Self::Hard => "hard",
        }
    }

    pub fn from_name(name: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|d| d.name() == name.to_ascii_lowercase())
    }

    /// Next menu value, wrapping after Hard.
    pub const fn next(self) -> Self {
        match self {
            Self::Peaceful => Self::Easy,
            Self::Easy => Self::Normal,
            Self::Normal => Self::Hard,
            Self::Hard => Self::Peaceful,
        }
    }

    /// Java's `Difficulty.getId`.
    pub const fn id(self) -> u8 {
        self as u8
    }

    /// Java's `DifficultyInstance.calculateDifficulty`: the effective
    /// (regional) difficulty from the overworld clock time, the chunk's
    /// inhabited time and the moon's brightness.
    pub fn effective(self, clock_time: i64, inhabited: i64, moon: f32) -> f32 {
        if self == Self::Peaceful {
            return 0.0;
        }
        let time = ((clock_time as f32 + -72000.0) / 1_440_000.0).clamp(0.0, 1.0) * 0.25;
        let mut extra = (inhabited as f32 / 3_600_000.0).clamp(0.0, 1.0) * if self == Self::Hard { 1.0 } else { 0.75 };
        extra += (moon * 0.25).clamp(0.0, time);
        if self == Self::Easy {
            extra *= 0.5;
        }
        f32::from(self.id()) * (0.75 + time + extra)
    }

    /// Scales a hostile mob's Normal damage to a player.
    pub fn mob_damage(self, damage: f32) -> f32 {
        match self {
            Self::Peaceful => 0.0,
            Self::Easy => (damage * 0.5 + 1.0).min(damage),
            Self::Normal => damage,
            Self::Hard => damage * 1.5,
        }
    }

    /// Lowest health starvation can reach: 10 on Easy, 1 on Normal, and
    /// zero on Hard. Peaceful never starves.
    pub const fn starvation_floor(self) -> f32 {
        match self {
            Self::Peaceful => 20.0,
            Self::Easy => 10.0,
            Self::Normal => 1.0,
            Self::Hard => 0.0,
        }
    }
}

impl fmt::Display for Difficulty {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Peaceful => "Peaceful",
            Self::Easy => "Easy",
            Self::Normal => "Normal",
            Self::Hard => "Hard",
        })
    }
}

/// Java's `DifficultyInstance.getSpecialMultiplier`: zero below an effective
/// difficulty of 2, rising to one at 4.
pub fn special_multiplier(effective: f32) -> f32 {
    if effective < 2.0 {
        0.0
    } else if effective > 4.0 {
        1.0
    } else {
        (effective - 2.0) / 2.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_round_trip_and_menu_wraps() {
        for d in Difficulty::ALL {
            assert_eq!(Difficulty::from_name(d.name()), Some(d));
        }
        assert_eq!(Difficulty::Hard.next(), Difficulty::Peaceful);
    }

    #[test]
    fn java_mob_damage_scaling() {
        assert_eq!(Difficulty::Peaceful.mob_damage(6.0), 0.0);
        assert_eq!(Difficulty::Easy.mob_damage(6.0), 4.0);
        assert_eq!(Difficulty::Easy.mob_damage(1.0), 1.0);
        assert_eq!(Difficulty::Normal.mob_damage(6.0), 6.0);
        assert_eq!(Difficulty::Hard.mob_damage(6.0), 9.0);
    }
}

#[cfg(test)]
mod local_tests {
    use super::*;

    #[test]
    fn effective_difficulty_follows_the_reference_formula() {
        // A new Normal world: 2 * 0.75, below the threshold, so no spawn gear.
        assert_eq!(Difficulty::Normal.effective(0, 0, 1.0), 1.5);
        assert_eq!(special_multiplier(1.5), 0.0);
        // Full moon only counts as much as the elapsed-time term allows.
        let t = (1_000_000.0f32 - 72000.0) / 1_440_000.0 * 0.25;
        assert_eq!(Difficulty::Normal.effective(1_000_000, 0, 1.0), 2.0 * (0.75 + t + (0.25f32).min(t)));
        // Fully aged world and chunk on Hard saturate at 3 * 2.0 + 0.75 extra.
        assert_eq!(Difficulty::Hard.effective(i64::MAX / 2, 3_600_000, 1.0), 3.0 * (0.75 + 0.25 + 1.0 + 0.25));
        assert_eq!(Difficulty::Normal.effective(i64::MAX / 2, 3_600_000, 1.0), 2.0 * (0.75 + 0.25 + 0.75 + 0.25));
        assert_eq!(Difficulty::Peaceful.effective(10_000_000, 10_000_000, 1.0), 0.0);
        // Easy halves the inhabited/moon term.
        assert_eq!(Difficulty::Easy.effective(10_000_000, 3_600_000, 1.0), 0.75 + 0.25 + (0.75 + 0.25) * 0.5);
        assert_eq!((special_multiplier(3.0), special_multiplier(4.5)), (0.5, 1.0));
    }
}
