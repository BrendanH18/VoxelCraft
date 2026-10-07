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
