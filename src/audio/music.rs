//! Procedural background music, independent of the desktop/audio device.
//!
//! Java's client scheduler is expressed in 20 Hz ticks. Compositions are
//! original seeded scores; no Minecraft melodies, recordings or assets are
//! used. See `docs/music.md` for parity details and integration hooks.

#[path = "music/stream.rs"]
mod stream;
#[path = "music/synth.rs"]
mod synth;

pub use stream::{MusicReader, MusicStream};
pub use synth::Composition;

use crate::world::structure::Rng;
use crate::world::terrain::{Biome, Dimension};

pub const RATE: u32 = 24_000;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum Situation {
    Menu,
    Creative,
    Game,
    Underwater,
    NetherWastes,
    CrimsonForest,
    WarpedForest,
    SoulSandValley,
    BasaltDeltas,
    End,
    Dragon,
    Credits,
    Forest,
    Jungle,
    Swamp,
    Desert,
    Badlands,
    Mountains,
    Snowy,
}

impl Situation {
    pub const ALL: [Self; 19] = [
        Self::Menu,
        Self::Creative,
        Self::Game,
        Self::Underwater,
        Self::NetherWastes,
        Self::CrimsonForest,
        Self::WarpedForest,
        Self::SoulSandValley,
        Self::BasaltDeltas,
        Self::End,
        Self::Dragon,
        Self::Credits,
        Self::Forest,
        Self::Jungle,
        Self::Swamp,
        Self::Desert,
        Self::Badlands,
        Self::Mountains,
        Self::Snowy,
    ];

    pub fn name(self) -> &'static str {
        match self {
            Self::Menu => "menu",
            Self::Creative => "creative",
            Self::Game => "game",
            Self::Underwater => "underwater",
            Self::NetherWastes => "nether_wastes",
            Self::CrimsonForest => "crimson_forest",
            Self::WarpedForest => "warped_forest",
            Self::SoulSandValley => "soul_sand_valley",
            Self::BasaltDeltas => "basalt_deltas",
            Self::End => "end",
            Self::Dragon => "dragon",
            Self::Credits => "credits",
            Self::Forest => "forest",
            Self::Jungle => "jungle",
            Self::Swamp => "swamp",
            Self::Desert => "desert",
            Self::Badlands => "badlands",
            Self::Mountains => "mountains",
            Self::Snowy => "snowy",
        }
    }

    /// Java Musics/createGameMusic constants (inclusive tick ranges).
    pub fn rules(self) -> Rules {
        match self {
            Self::Menu => Rules { min: 20, max: 600, replace: true },
            Self::Credits | Self::Dragon => Rules { min: 0, max: 0, replace: true },
            Self::End => Rules { min: 6000, max: 24000, replace: true },
            _ => Rules { min: 12000, max: 24000, replace: false },
        }
    }

    fn overworld(self) -> bool {
        matches!(
            self,
            Self::Creative
                | Self::Game
                | Self::Underwater
                | Self::Forest
                | Self::Jungle
                | Self::Swamp
                | Self::Desert
                | Self::Badlands
                | Self::Mountains
                | Self::Snowy
        )
    }

    fn from_id(id: u8) -> Self {
        Self::ALL.get(id as usize).copied().unwrap_or(Self::Menu)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Rules {
    pub min: u32,
    pub max: u32,
    pub replace: bool,
}

/// Primary player's situation. Split-screen shares this selection and one
/// stream, just as it shares the device. Future Nether biome/credits callers
/// can supply those situations directly through `MusicStream::set_situation`.
#[derive(Clone, Copy, Debug)]
pub struct Context {
    pub title: bool,
    pub credits: bool,
    pub dimension: Dimension,
    pub creative: bool,
    pub underwater: bool,
    pub biome: Biome,
    pub nether: Situation,
    pub dragon: bool,
}

impl Default for Context {
    fn default() -> Self {
        Self {
            title: true,
            credits: false,
            dimension: Dimension::Overworld,
            creative: false,
            underwater: false,
            biome: Biome::Plains,
            nether: Situation::NetherWastes,
            dragon: false,
        }
    }
}

impl Context {
    pub fn select(self, current: Option<Situation>) -> Situation {
        if self.credits {
            return Situation::Credits;
        }
        if self.title {
            return Situation::Menu;
        }
        match self.dimension {
            Dimension::End => {
                if self.dragon {
                    Situation::Dragon
                } else {
                    Situation::End
                }
            }
            Dimension::Nether => self.nether,
            Dimension::Overworld => {
                if current == Some(Situation::Underwater) || (self.underwater && self.biome == Biome::Ocean) {
                    return Situation::Underwater;
                }
                if self.creative {
                    return Situation::Creative;
                }
                match self.biome {
                    Biome::Forest | Biome::BirchForest | Biome::Taiga => Situation::Forest,
                    Biome::Jungle => Situation::Jungle,
                    Biome::Swamp => Situation::Swamp,
                    Biome::Desert => Situation::Desert,
                    Biome::Badlands => Situation::Badlands,
                    Biome::Mountains => Situation::Mountains,
                    Biome::Snowy => Situation::Snowy,
                    _ => Situation::Game,
                }
            }
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Action {
    None,
    Start(Situation),
    Stop,
    Replace(Situation),
}

/// Java MusicInfo: a nullable selection and its situational volume. This is
/// independent of the Music slider. A silent biome uses `(None, 0.0)`.
#[derive(Clone, Copy, Debug)]
pub struct MusicInfo {
    pub music: Option<Situation>,
    pub volume: f32,
}

/// Java MusicManager delay/replacement state. A caller reports completion
/// when the rendered track actually ends, rather than using a wall timer.
pub struct Manager {
    pub current: Option<Situation>,
    pub delay: u32,
    pub gain: f32,
    rng: Rng,
}

impl Manager {
    pub fn new(seed: u64) -> Self {
        Self { current: None, delay: 100, gain: 1.0, rng: Rng(seed) }
    }

    pub fn tick_info(&mut self, info: MusicInfo, finished: bool) -> Action {
        let volume = info.volume.clamp(0.0, 1.0);
        if self.current.is_some() && self.gain != volume {
            self.gain = stream::fade_gain(self.gain, volume);
            if self.gain <= 1e-4 {
                self.current = None;
                self.delay = self.delay.saturating_add(100);
                return Action::Stop;
            }
        }
        let Some(desired) = info.music else {
            self.delay = self.delay.max(100);
            return Action::None;
        };
        let action = self.tick(desired, finished);
        if matches!(action, Action::Start(_) | Action::Replace(_)) {
            self.gain = volume;
        }
        action
    }

    pub fn tick(&mut self, desired: Situation, finished: bool) -> Action {
        let rules = desired.rules();
        let replaced = self.current.is_some_and(|s| s != desired) && rules.replace;
        if replaced {
            self.current = None;
            self.delay = self.rng.range(0, rules.min / 2);
        } else if finished && self.current.take().is_some() {
            self.delay = self.delay.min(self.rng.range(rules.min, rules.max));
        }
        self.delay = self.delay.min(rules.max);
        if self.current.is_none() {
            if self.delay == 0 {
                self.current = Some(desired);
                self.delay = u32::MAX;
                return if replaced { Action::Replace(desired) } else { Action::Start(desired) };
            }
            self.delay -= 1;
        }
        if replaced { Action::Stop } else { Action::None }
    }
}

#[cfg(test)]
#[path = "music/tests.rs"]
mod tests;
