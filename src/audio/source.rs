//! Java's sound categories (`net.minecraft.sounds.SoundSource`): every sound
//! plays in one, and the player sets a volume for each in Music & Sound
//! Options. Master multiplies all of them.
//!
//! Java also has a Voice category for the narrator; nothing in VoxelCraft
//! speaks, so it is left out until accessibility work needs it.

/// A sound category, in the order Java 26.3 lists them (Voice omitted).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum SoundSource {
    Master,
    Music,
    /// Jukeboxes and note blocks.
    Records,
    Weather,
    Blocks,
    Hostile,
    /// Friendly mobs (Java's `NEUTRAL`).
    Neutral,
    Players,
    Ambient,
    Ui,
}

impl SoundSource {
    pub const COUNT: usize = 10;
    pub const ALL: [SoundSource; Self::COUNT] = [
        SoundSource::Master,
        SoundSource::Music,
        SoundSource::Records,
        SoundSource::Weather,
        SoundSource::Blocks,
        SoundSource::Hostile,
        SoundSource::Neutral,
        SoundSource::Players,
        SoundSource::Ambient,
        SoundSource::Ui,
    ];

    /// Dense index in `0..COUNT`, matching [`SoundSource::ALL`].
    pub fn index(self) -> usize {
        self as usize
    }

    /// Java's `soundCategory.*` text.
    pub fn label(self) -> &'static str {
        match self {
            SoundSource::Master => "Master Volume",
            SoundSource::Music => "Music",
            SoundSource::Records => "Jukebox/Note Blocks",
            SoundSource::Weather => "Weather",
            SoundSource::Blocks => "Blocks",
            SoundSource::Hostile => "Hostile Mobs",
            SoundSource::Neutral => "Friendly Mobs",
            SoundSource::Players => "Players",
            SoundSource::Ambient => "Ambient/Environment",
            SoundSource::Ui => "UI",
        }
    }

    /// The key in `options.txt`. Master and Music keep the names they had
    /// before the other categories existed.
    pub fn option_key(self) -> &'static str {
        match self {
            SoundSource::Master => "volume",
            SoundSource::Music => "music_volume",
            SoundSource::Records => "volume_record",
            SoundSource::Weather => "volume_weather",
            SoundSource::Blocks => "volume_block",
            SoundSource::Hostile => "volume_hostile",
            SoundSource::Neutral => "volume_neutral",
            SoundSource::Players => "volume_player",
            SoundSource::Ambient => "volume_ambient",
            SoundSource::Ui => "volume_ui",
        }
    }

    pub fn from_option_key(key: &str) -> Option<SoundSource> {
        Self::ALL.into_iter().find(|s| s.option_key() == key)
    }
}
