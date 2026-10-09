//! Overworld biomes and how the climate picks them.
//!
//! The list and the picking rules follow Java's `OverworldBiomeBuilder`
//! (1.18+, with 1.20 cherry groves and 1.21 pale gardens): temperature and
//! humidity choose a row of a table, continentalness and erosion choose
//! the kind of land (ocean, coast, plains, plateau, slopes, peaks), and
//! weirdness picks variants and, through peaks-and-valleys, rivers and
//! mountain tops. Lush and dripstone caves are chosen underground from
//! humidity and continentalness, like Java's cave biomes.

/// A climate sample, in Java's parameter units (roughly `-1..1`).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Climate {
    pub temperature: f32,
    pub humidity: f32,
    pub continentalness: f32,
    pub erosion: f32,
    pub weirdness: f32,
}

impl Climate {
    /// Java's peaks-and-valleys: -1 in valleys (weirdness near 0), 1 on
    /// peaks (|weirdness| near 2/3).
    pub fn peaks_valleys(&self) -> f32 {
        -3.0 * ((self.weirdness.abs() - 2.0 / 3.0).abs() - 1.0 / 3.0)
    }
}

macro_rules! biomes {
    ($($variant:ident $name:literal $temp:literal $foliage:literal),* $(,)?) => {
        #[derive(Clone, Copy, PartialEq, Eq, Debug, Hash)]
        pub enum Biome {
            $($variant),*
        }

        impl Biome {
            pub const ALL: [Biome; [$(Biome::$variant),*].len()] = [$(Biome::$variant),*];

            /// Java's biome id (without `minecraft:`).
            pub fn name(self) -> &'static str {
                match self {
                    $(Biome::$variant => $name),*
                }
            }

            /// Java's base temperature: below 0.15 rain falls as snow.
            pub fn temperature(self) -> f32 {
                match self {
                    $(Biome::$variant => $temp),*
                }
            }

            /// Which colour grass and leaves take on here (see
            /// `block::tex::tinted`): 0 temperate green, 1 murky swamp,
            /// 2 dry and yellow, 3 lush jungle, 4 cold and blue.
            pub fn foliage(self) -> u8 {
                match self {
                    $(Biome::$variant => $foliage),*
                }
            }
        }
    };
}

biomes! {
    DeepFrozenOcean "deep_frozen_ocean" 0.5 0,
    DeepColdOcean "deep_cold_ocean" 0.5 0,
    DeepOcean "deep_ocean" 0.5 0,
    DeepLukewarmOcean "deep_lukewarm_ocean" 0.5 0,
    FrozenOcean "frozen_ocean" 0.0 4,
    ColdOcean "cold_ocean" 0.5 0,
    Ocean "ocean" 0.5 0,
    LukewarmOcean "lukewarm_ocean" 0.5 0,
    WarmOcean "warm_ocean" 0.5 0,
    MushroomFields "mushroom_fields" 0.9 3,
    River "river" 0.5 0,
    FrozenRiver "frozen_river" 0.0 4,
    Beach "beach" 0.8 0,
    SnowyBeach "snowy_beach" 0.05 4,
    StonyShore "stony_shore" 0.2 4,
    Plains "plains" 0.8 0,
    SunflowerPlains "sunflower_plains" 0.8 0,
    SnowyPlains "snowy_plains" 0.0 4,
    IceSpikes "ice_spikes" 0.0 4,
    Forest "forest" 0.7 0,
    FlowerForest "flower_forest" 0.7 0,
    BirchForest "birch_forest" 0.6 0,
    OldGrowthBirchForest "old_growth_birch_forest" 0.6 0,
    DarkForest "dark_forest" 0.7 3,
    PaleGarden "pale_garden" 0.7 4,
    Taiga "taiga" 0.25 4,
    SnowyTaiga "snowy_taiga" -0.5 4,
    OldGrowthPineTaiga "old_growth_pine_taiga" 0.3 4,
    OldGrowthSpruceTaiga "old_growth_spruce_taiga" 0.25 4,
    Swamp "swamp" 0.8 1,
    MangroveSwamp "mangrove_swamp" 0.8 1,
    Desert "desert" 2.0 2,
    Savanna "savanna" 2.0 2,
    SavannaPlateau "savanna_plateau" 2.0 2,
    WindsweptSavanna "windswept_savanna" 2.0 2,
    Jungle "jungle" 0.95 3,
    SparseJungle "sparse_jungle" 0.95 3,
    BambooJungle "bamboo_jungle" 0.95 3,
    Badlands "badlands" 2.0 2,
    WoodedBadlands "wooded_badlands" 2.0 2,
    ErodedBadlands "eroded_badlands" 2.0 2,
    Meadow "meadow" 0.5 0,
    CherryGrove "cherry_grove" 0.5 0,
    Grove "grove" -0.2 4,
    SnowySlopes "snowy_slopes" -0.3 4,
    JaggedPeaks "jagged_peaks" -0.7 4,
    FrozenPeaks "frozen_peaks" -0.7 4,
    StonyPeaks "stony_peaks" 1.0 0,
    WindsweptHills "windswept_hills" 0.2 4,
    WindsweptGravellyHills "windswept_gravelly_hills" 0.2 4,
    WindsweptForest "windswept_forest" 0.2 4,
    LushCaves "lush_caves" 0.5 3,
    DripstoneCaves "dripstone_caves" 0.8 0,
}

impl Biome {
    /// Looks a biome up by Java id, with or without `minecraft:`. A few
    /// pre-1.18 names this game used still work.
    pub fn from_name(name: &str) -> Option<Self> {
        let name = name.strip_prefix("minecraft:").unwrap_or(name);
        let name = match name {
            "mountains" => "windswept_hills",
            "snowy" | "snowy_tundra" => "snowy_plains",
            other => other,
        };
        Self::ALL.into_iter().find(|biome| biome.name() == name)
    }

    pub fn is_ocean(self) -> bool {
        use Biome::*;
        matches!(
            self,
            DeepFrozenOcean
                | DeepColdOcean
                | DeepOcean
                | DeepLukewarmOcean
                | FrozenOcean
                | ColdOcean
                | Ocean
                | LukewarmOcean
                | WarmOcean
        )
    }

    pub fn is_deep_ocean(self) -> bool {
        use Biome::*;
        matches!(self, DeepFrozenOcean | DeepColdOcean | DeepOcean | DeepLukewarmOcean)
    }

    pub fn is_river(self) -> bool {
        matches!(self, Biome::River | Biome::FrozenRiver)
    }

    pub fn is_beach(self) -> bool {
        matches!(self, Biome::Beach | Biome::SnowyBeach)
    }

    /// Ocean, river or beach: water-side biomes with no land mobs or villages.
    pub fn is_watery(self) -> bool {
        self.is_ocean() || self.is_river()
    }

    pub fn is_swamp(self) -> bool {
        matches!(self, Biome::Swamp | Biome::MangroveSwamp)
    }

    pub fn is_badlands(self) -> bool {
        matches!(self, Biome::Badlands | Biome::WoodedBadlands | Biome::ErodedBadlands)
    }

    pub fn is_jungle(self) -> bool {
        matches!(self, Biome::Jungle | Biome::SparseJungle | Biome::BambooJungle)
    }

    pub fn is_savanna(self) -> bool {
        matches!(self, Biome::Savanna | Biome::SavannaPlateau | Biome::WindsweptSavanna)
    }

    pub fn is_taiga(self) -> bool {
        matches!(self, Biome::Taiga | Biome::SnowyTaiga | Biome::OldGrowthPineTaiga | Biome::OldGrowthSpruceTaiga)
    }

    pub fn is_forest(self) -> bool {
        use Biome::*;
        matches!(self, Forest | FlowerForest | BirchForest | OldGrowthBirchForest | DarkForest | PaleGarden)
    }

    /// Java's `#is_mountain` plus the windswept hills (emerald ore, goats).
    pub fn is_mountain(self) -> bool {
        use Biome::*;
        matches!(
            self,
            Meadow
                | CherryGrove
                | Grove
                | SnowySlopes
                | JaggedPeaks
                | FrozenPeaks
                | StonyPeaks
                | WindsweptHills
                | WindsweptGravellyHills
                | WindsweptForest
        )
    }

    pub fn is_peak(self) -> bool {
        matches!(self, Biome::JaggedPeaks | Biome::FrozenPeaks | Biome::StonyPeaks)
    }

    pub fn is_cave(self) -> bool {
        matches!(self, Biome::LushCaves | Biome::DripstoneCaves)
    }

    /// Snow instead of rain, and ice on still water (Java: temperature
    /// below 0.15). The deep frozen ocean is warm on paper but frozen.
    pub fn is_cold(self) -> bool {
        self.temperature() < 0.15 || self == Biome::DeepFrozenOcean
    }

    /// No rain at all (Java's `hasPrecipitation` false).
    pub fn is_dry(self) -> bool {
        self.temperature() > 1.0 && self != Biome::StonyPeaks
    }

    /// Temperature at a height: Java cools biomes above y = 80 by 0.05 per
    /// 40 blocks, so high mountains get snow.
    pub fn temperature_at(self, y: i32) -> f32 {
        let t = self.temperature();
        if y > 80 { t - (y - 80) as f32 * 0.05 / 40.0 } else { t }
    }
}

/// Java's temperature, humidity and erosion parameter bands.
const TEMPERATURES: [f32; 4] = [-0.45, -0.15, 0.2, 0.55];
const HUMIDITIES: [f32; 4] = [-0.35, -0.1, 0.1, 0.3];
const EROSIONS: [f32; 6] = [-0.78, -0.375, -0.2225, 0.05, 0.45, 0.55];

fn band(v: f32, edges: &[f32]) -> usize {
    edges.iter().take_while(|&&e| v >= e).count()
}

use Biome::*;

const MIDDLE: [[Biome; 5]; 5] = [
    [SnowyPlains, SnowyPlains, SnowyPlains, SnowyTaiga, Taiga],
    [Plains, Plains, Forest, Taiga, OldGrowthSpruceTaiga],
    [FlowerForest, Plains, Forest, BirchForest, DarkForest],
    [Savanna, Savanna, Forest, Jungle, Jungle],
    [Desert, Desert, Desert, Desert, Desert],
];
const MIDDLE_VARIANT: [[Option<Biome>; 5]; 5] = [
    [Some(IceSpikes), None, Some(SnowyTaiga), None, None],
    [None, None, None, None, Some(OldGrowthPineTaiga)],
    [Some(SunflowerPlains), None, None, Some(OldGrowthBirchForest), None],
    [None, None, Some(Plains), Some(SparseJungle), Some(BambooJungle)],
    [None, None, None, None, None],
];
const PLATEAU: [[Biome; 5]; 5] = [
    [SnowyPlains, SnowyPlains, SnowyPlains, SnowyTaiga, SnowyTaiga],
    [Meadow, Meadow, Forest, Taiga, OldGrowthSpruceTaiga],
    [Meadow, Meadow, Meadow, Meadow, DarkForest],
    [SavannaPlateau, SavannaPlateau, Forest, Forest, Jungle],
    [Badlands, Badlands, Badlands, WoodedBadlands, WoodedBadlands],
];
const PLATEAU_VARIANT: [[Option<Biome>; 5]; 5] = [
    [Some(IceSpikes), None, None, None, None],
    [Some(CherryGrove), None, Some(Meadow), Some(Meadow), Some(OldGrowthPineTaiga)],
    [Some(CherryGrove), Some(CherryGrove), Some(Forest), Some(BirchForest), Some(PaleGarden)],
    [None, None, None, None, None],
    [Some(ErodedBadlands), Some(ErodedBadlands), None, None, None],
];
const SHATTERED: [[Option<Biome>; 5]; 5] = [
    [
        Some(WindsweptGravellyHills),
        Some(WindsweptGravellyHills),
        Some(WindsweptHills),
        Some(WindsweptForest),
        Some(WindsweptForest),
    ],
    [
        Some(WindsweptGravellyHills),
        Some(WindsweptGravellyHills),
        Some(WindsweptHills),
        Some(WindsweptForest),
        Some(WindsweptForest),
    ],
    [Some(WindsweptHills), Some(WindsweptHills), Some(WindsweptHills), Some(WindsweptForest), Some(WindsweptForest)],
    [None, None, None, None, None],
    [None, None, None, None, None],
];

fn middle(t: usize, h: usize, w: f32) -> Biome {
    if w < 0.0 { MIDDLE[t][h] } else { MIDDLE_VARIANT[t][h].unwrap_or(MIDDLE[t][h]) }
}

fn middle_or_badlands(t: usize, h: usize, w: f32) -> Biome {
    if t == 4 { badlands(h, w) } else { middle(t, h, w) }
}

fn middle_or_badlands_or_slope(t: usize, h: usize, w: f32) -> Biome {
    if t == 0 { slope(t, h, w) } else { middle_or_badlands(t, h, w) }
}

fn windswept_savanna(t: usize, h: usize, w: f32, under: Biome) -> Biome {
    if t > 1 && h < 4 && w >= 0.0 { WindsweptSavanna } else { under }
}

fn beach(t: usize) -> Biome {
    match t {
        0 => SnowyBeach,
        4 => Desert,
        _ => Beach,
    }
}

fn badlands(h: usize, w: f32) -> Biome {
    match h {
        0 | 1 if w < 0.0 => Badlands,
        0 | 1 => ErodedBadlands,
        2 => Badlands,
        _ => WoodedBadlands,
    }
}

fn plateau(t: usize, h: usize, w: f32) -> Biome {
    if w < 0.0 { PLATEAU[t][h] } else { PLATEAU_VARIANT[t][h].unwrap_or(PLATEAU[t][h]) }
}

fn peak(t: usize, h: usize, w: f32) -> Biome {
    match t {
        0..=2 if w < 0.0 => JaggedPeaks,
        0..=2 => FrozenPeaks,
        3 => StonyPeaks,
        _ => badlands(h, w),
    }
}

fn slope(t: usize, h: usize, w: f32) -> Biome {
    if t >= 3 {
        plateau(t, h, w)
    } else if h <= 1 {
        SnowySlopes
    } else {
        Grove
    }
}

fn shattered(t: usize, h: usize, w: f32) -> Biome {
    SHATTERED[t][h].unwrap_or_else(|| middle(t, h, w))
}

fn shattered_coast(t: usize, h: usize, w: f32) -> Biome {
    let under = if w >= 0.0 { middle(t, h, w) } else { beach(t) };
    windswept_savanna(t, h, w, under)
}

fn swamp(t: usize, h: usize, w: f32) -> Biome {
    match t {
        0 => middle(t, h, w),
        1 | 2 => Swamp,
        _ => MangroveSwamp,
    }
}

/// Where along the coast-to-inland axis a column is.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Land {
    Coast,
    Near,
    Mid,
    Far,
}

/// Picks the surface biome for a climate (Java's `OverworldBiomeBuilder`).
pub fn pick(c: &Climate) -> Biome {
    let t = band(c.temperature, &TEMPERATURES);
    let h = band(c.humidity, &HUMIDITIES);
    let w = c.weirdness;
    let cont = c.continentalness;
    if cont < -1.05 {
        return MushroomFields;
    }
    if cont < -0.19 {
        let deep = cont < -0.455;
        return match (t, deep) {
            (0, true) => DeepFrozenOcean,
            (1, true) => DeepColdOcean,
            (2, true) => DeepOcean,
            (3, true) => DeepLukewarmOcean,
            (0, false) => FrozenOcean,
            (1, false) => ColdOcean,
            (2, false) => Ocean,
            (3, false) => LukewarmOcean,
            _ => WarmOcean,
        };
    }
    let land = if cont < -0.11 {
        Land::Coast
    } else if cont < 0.03 {
        Land::Near
    } else if cont < 0.3 {
        Land::Mid
    } else {
        Land::Far
    };
    let e = band(c.erosion, &EROSIONS);
    let aw = w.abs();
    use Land::*;
    let inland = land != Coast;
    let mid_far = matches!(land, Mid | Far);
    if aw < 0.05 {
        // Valleys: rivers wherever the ground isn't mountainous, swamps in
        // flat wet ground.
        return match (land, e) {
            (Coast, 0 | 1) => {
                if w < 0.0 {
                    StonyShore
                } else if t == 0 {
                    FrozenRiver
                } else {
                    River
                }
            }
            (Near, 0 | 1) | (_, 2..=5) | (Coast, 6) => {
                if t == 0 {
                    FrozenRiver
                } else {
                    River
                }
            }
            (_, 6) => swamp(t, h, w),
            _ => middle_or_badlands(t, h, w),
        };
    }
    let slice = if aw < 0.266_666_7 {
        0 // low
    } else if aw < 0.4 || aw >= 0.933_333_3 {
        1 // mid
    } else if aw < 0.566_666_6 || aw >= 0.766_666_7 {
        2 // high
    } else {
        3 // peaks
    };
    match slice {
        // Low: beaches and plains near the sea, swamps in flat wet land.
        0 => match (land, e) {
            (Coast, 0..=2) => StonyShore,
            (Coast, 3 | 4) => beach(t),
            (Coast, 5) => shattered_coast(t, h, w),
            (Coast, _) => beach(t),
            (_, 0 | 1) if mid_far => middle_or_badlands(t, h, w),
            (_, 0 | 1) => middle_or_badlands_or_slope(t, h, w),
            (_, 2 | 3) if mid_far => middle_or_badlands(t, h, w),
            (Near | Mid | Far, 2..=4) => middle(t, h, w),
            (_, 5) => windswept_savanna(t, h, w, middle(t, h, w)),
            _ => swamp(t, h, w),
        },
        1 => match (land, e) {
            (Coast, 0..=2) => StonyShore,
            (Far, 0) => slope(t, h, w),
            (Near | Mid, 0) => slope(t, h, w),
            (Near | Mid, 1) => middle_or_badlands_or_slope(t, h, w),
            (Far, 1) => {
                if t == 0 {
                    slope(t, h, w)
                } else {
                    plateau(t, h, w)
                }
            }
            (Near, 2) => middle(t, h, w),
            (Mid, 2) => middle_or_badlands(t, h, w),
            (Far, 2) => plateau(t, h, w),
            (Coast | Near, 3) => middle(t, h, w),
            (Mid | Far, 3) => middle_or_badlands(t, h, w),
            (Coast, 4) if w < 0.0 => beach(t),
            (_, 4) => middle(t, h, w),
            (Coast, 5) => shattered_coast(t, h, w),
            (Near, 5) => windswept_savanna(t, h, w, shattered(t, h, w)),
            (_, 5) => shattered(t, h, w),
            (Coast, _) if w < 0.0 => beach(t),
            (Coast, _) => middle(t, h, w),
            (_, _) => swamp(t, h, w),
        },
        2 => match (land, e) {
            (Coast, 0 | 1) => middle(t, h, w),
            (Near, 0) => slope(t, h, w),
            (_, 0) => peak(t, h, w),
            (Near, 1) => middle_or_badlands_or_slope(t, h, w),
            (_, 1) => slope(t, h, w),
            (Coast | Near, 2 | 3) => middle(t, h, w),
            (_, 2) => plateau(t, h, w),
            (Mid, 3) => middle_or_badlands(t, h, w),
            (Far, 3) => plateau(t, h, w),
            (_, 4) => middle(t, h, w),
            (Coast | Near, 5) => windswept_savanna(t, h, w, shattered(t, h, w)),
            (_, 5) => shattered(t, h, w),
            _ => middle(t, h, w),
        },
        _ => match (land, e) {
            (_, 0) => peak(t, h, w),
            (Coast | Near, 1) => middle_or_badlands_or_slope(t, h, w),
            (_, 1) => peak(t, h, w),
            (Coast | Near, 2 | 3) => middle(t, h, w),
            (_, 2) => plateau(t, h, w),
            (Mid, 3) => middle_or_badlands(t, h, w),
            (Far, 3) => plateau(t, h, w),
            (_, 4) => middle(t, h, w),
            (Coast | Near, 5) => windswept_savanna(t, h, w, shattered(t, h, w)),
            (_, 5) if inland => shattered(t, h, w),
            _ => middle(t, h, w),
        },
    }
}

/// The cave biome at some depth under a column with this climate, if any
/// (Java: lush caves where humidity is at least 0.7, dripstone caves where
/// continentalness is at least 0.8, both from 25 blocks under the surface).
pub fn cave(c: &Climate, depth: i32) -> Option<Biome> {
    if depth < 20 {
        return None;
    }
    if c.humidity >= 0.7 {
        Some(LushCaves)
    } else if c.continentalness >= 0.8 {
        Some(DripstoneCaves)
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn climate(t: f32, h: f32, c: f32, e: f32, w: f32) -> Climate {
        Climate { temperature: t, humidity: h, continentalness: c, erosion: e, weirdness: w }
    }

    #[test]
    fn names_round_trip_and_categories_hold() {
        for b in Biome::ALL {
            assert_eq!(Biome::from_name(b.name()), Some(b));
            assert_eq!(Biome::from_name(&format!("minecraft:{}", b.name())), Some(b));
        }
        assert_eq!(Biome::from_name("mountains"), Some(WindsweptHills));
        assert!(DeepColdOcean.is_ocean() && DeepColdOcean.is_deep_ocean());
        assert!(SnowyPlains.is_cold() && !Plains.is_cold() && DeepFrozenOcean.is_cold());
        assert!(Desert.is_dry() && !StonyPeaks.is_dry());
        assert!(Plains.temperature_at(64) > Plains.temperature_at(300));
        assert!(WindsweptHills.temperature_at(140) < 0.15, "snowy hill tops");
    }

    #[test]
    fn the_builder_follows_javas_table() {
        assert_eq!(pick(&climate(0.0, 0.0, -0.6, 0.0, 0.3)), DeepOcean);
        assert_eq!(pick(&climate(-0.6, 0.0, -0.3, 0.0, 0.3)), FrozenOcean);
        assert_eq!(pick(&climate(0.7, 0.0, -0.3, 0.0, 0.3)), WarmOcean);
        assert_eq!(pick(&climate(0.0, 0.0, -1.1, 0.0, 0.3)), MushroomFields);
        assert_eq!(pick(&climate(0.0, -0.2, 0.1, 0.2, -0.1)), Plains);
        assert_eq!(pick(&climate(0.7, 0.0, 0.1, 0.2, -0.1)), Desert);
        assert_eq!(pick(&climate(0.0, -0.2, 0.0, 0.2, 0.01)), River);
        assert_eq!(pick(&climate(-0.6, -0.2, 0.0, 0.2, 0.01)), FrozenRiver);
        assert_eq!(pick(&climate(0.0, 0.0, 0.5, -0.9, -0.65)), JaggedPeaks);
        assert_eq!(pick(&climate(0.0, 0.0, 0.5, -0.9, 0.65)), FrozenPeaks);
        assert_eq!(pick(&climate(0.3, 0.0, 0.5, -0.9, 0.65)), StonyPeaks);
        assert_eq!(pick(&climate(0.3, 0.0, 0.1, 0.6, 0.15)), MangroveSwamp);
        assert_eq!(pick(&climate(0.0, 0.0, 0.1, 0.6, 0.15)), Swamp);
        assert_eq!(pick(&climate(-0.3, -0.5, 0.5, -0.3, 0.45)), CherryGrove);
        assert_eq!(pick(&climate(0.0, 0.5, 0.5, -0.3, 0.45)), PaleGarden);
        assert_eq!(pick(&climate(0.0, 0.5, 0.5, -0.3, -0.45)), DarkForest);
        assert_eq!(pick(&climate(0.0, 0.0, -0.15, 0.2, -0.1)), Beach);
        assert_eq!(pick(&climate(0.0, 0.0, 0.1, 0.5, 0.45)), WindsweptHills);
    }

    #[test]
    fn caves_need_depth() {
        let wet = climate(0.0, 0.8, 0.2, 0.0, 0.0);
        assert_eq!(cave(&wet, 5), None);
        assert_eq!(cave(&wet, 40), Some(LushCaves));
        assert_eq!(cave(&climate(0.0, 0.0, 0.9, 0.0, 0.0), 40), Some(DripstoneCaves));
    }
}
