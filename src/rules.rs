//! Saved world and player rules shared by graphical and headless sessions.

use std::fmt;

/// Java Edition's four per-player game modes.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum GameMode {
    #[default]
    Survival,
    Creative,
    Adventure,
    Spectator,
}

impl GameMode {
    pub const ALL: [Self; 4] = [Self::Survival, Self::Creative, Self::Adventure, Self::Spectator];

    /// Lowercase Java name used by saves and commands.
    pub const fn name(self) -> &'static str {
        match self {
            Self::Survival => "survival",
            Self::Creative => "creative",
            Self::Adventure => "adventure",
            Self::Spectator => "spectator",
        }
    }

    pub fn from_name(name: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|mode| mode.name() == name.to_ascii_lowercase())
    }

    /// Creative inventory and instant building.
    pub const fn is_creative(self) -> bool {
        matches!(self, Self::Creative)
    }

    /// Health, hunger, drops, durability and experience behave as Survival.
    pub const fn is_survival(self) -> bool {
        matches!(self, Self::Survival | Self::Adventure)
    }

    /// Creative and Spectator players cannot be damaged.
    pub const fn invulnerable(self) -> bool {
        matches!(self, Self::Creative | Self::Spectator)
    }

    pub const fn can_fly(self) -> bool {
        matches!(self, Self::Creative | Self::Spectator)
    }

    /// Adventure needs CanDestroy/CanPlaceOn data, which stacks do not yet
    /// carry, so it cannot alter blocks.
    pub const fn can_build(self) -> bool {
        matches!(self, Self::Survival | Self::Creative)
    }

    /// Spectators cannot interact with blocks, entities or inventories.
    pub const fn can_interact(self) -> bool {
        !matches!(self, Self::Spectator)
    }

    /// Hostile mobs cannot see Creative or Spectator players.
    pub const fn targetable(self) -> bool {
        self.is_survival()
    }
}

impl fmt::Display for GameMode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Survival => "Survival",
            Self::Creative => "Creative",
            Self::Adventure => "Adventure",
            Self::Spectator => "Spectator",
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn game_mode_names_and_abilities_match_java() {
        for mode in GameMode::ALL {
            assert_eq!(GameMode::from_name(mode.name()), Some(mode));
        }
        assert!(GameMode::Creative.invulnerable() && GameMode::Creative.can_fly());
        assert!(GameMode::Spectator.invulnerable() && !GameMode::Spectator.can_interact());
        assert!(GameMode::Adventure.is_survival() && !GameMode::Adventure.can_build());
    }
}
