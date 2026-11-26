//! Java 26.3's fog, sky colours and clouds (v0.18 Track 8), as plain data
//! and functions the renderer and tests share.
//!
//! Java keeps these in environment attributes: the dimension type sets a
//! base value, the biomes around the camera override it (blended with
//! `GaussianSampler` weights), the dimension's day timeline multiplies it,
//! and the rain and thunder layers modify it (`EnvironmentAttributeSystem`).
//! `FogRenderer` then picks the first applicable fog environment (lava,
//! powder snow, blindness, darkness, water, atmospheric) for distances and
//! colour. Shaders fog each vertex by the larger of a linear environmental
//! fog on spherical distance and a linear render-distance fog on cylindrical
//! distance (`fog.glsl`). `tests/fixtures/java/26.3/atmosphere.json`, from
//! `tools/extract_java_atmosphere_fixtures.py`, holds Java's numbers and
//! `tests/atmosphere.rs` checks these tables and results against it.
//!
//! Colours here are sRGB values in 0..1, as Java's floats carry them; Java
//! blends them without linearising, and so does this module. [`linear`]
//! converts one for the sRGB render target.
//!
//! Nothing here touches gameplay light: spawning and detection read the
//! world's light levels, never these colours or distances.

use crate::world::biome::Biome;
use crate::world::nether_biome::NetherBiome;
use crate::world::terrain::Dimension;

pub mod clouds;

/// An sRGB colour, 0..1 per channel.
pub type Rgb = [f32; 3];
/// An sRGB colour with alpha, `[r, g, b, a]`.
pub type Rgba = [f32; 4];

/// `0xRRGGBB` as sRGB floats (Java's `ARGB.vector3fFromRGB24`).
pub const fn rgb(hex: u32) -> Rgb {
    [((hex >> 16) & 255) as f32 / 255.0, ((hex >> 8) & 255) as f32 / 255.0, (hex & 255) as f32 / 255.0]
}

/// `0xAARRGGBB` as sRGB floats with alpha.
pub const fn argb(hex: u32) -> Rgba {
    let [r, g, b] = rgb(hex);
    [r, g, b, (hex >> 24) as f32 / 255.0]
}

/// An sRGB colour as the linear value the sRGB target encodes back to it,
/// with the exact sRGB transfer function (the target's own), so a colour
/// drawn unchanged shows Java's value byte for byte. The shaders' fog and
/// sky blends invert it exactly (`to_srgb`/`from_srgb`).
pub fn linear(c: Rgb) -> Rgb {
    c.map(|v| {
        let v = v.max(0.0);
        if v <= 0.04045 { v / 12.92 } else { ((v + 0.055) / 1.055).powf(2.4) }
    })
}

/// What the sRGB target stores for a linear colour (its encode).
pub fn encode(c: Rgb) -> Rgb {
    c.map(|v| {
        let v = v.max(0.0);
        if v <= 0.003_130_8 { v * 12.92 } else { 1.055 * v.powf(1.0 / 2.4) - 0.055 }
    })
}

/// The bytes on screen for a pixel drawn in fog colour `c` (an sRGB value):
/// through [`linear`] to the renderer and the target's encode back.
pub fn screen_rgb8(c: Rgb) -> [u8; 3] {
    encode(linear(c)).map(|v| (v.clamp(0.0, 1.0) * 255.0).round() as u8)
}

fn lerp(t: f32, a: f32, b: f32) -> f32 {
    a + (b - a) * t
}

fn lerp3(t: f32, a: Rgb, b: Rgb) -> Rgb {
    std::array::from_fn(|i| lerp(t, a[i], b[i]))
}

fn lerp4(t: f32, a: Rgba, b: Rgba) -> Rgba {
    std::array::from_fn(|i| lerp(t, a[i], b[i]))
}

fn mul3(a: Rgb, b: Rgb) -> Rgb {
    std::array::from_fn(|i| a[i] * b[i])
}

fn mul4(a: Rgba, b: Rgba) -> Rgba {
    std::array::from_fn(|i| a[i] * b[i])
}

/// Java's `ARGB.greyscale` weights.
fn grey(c: Rgb) -> f32 {
    c[0] * 0.3 + c[1] * 0.59 + c[2] * 0.11
}

/// `ColorModifier.BLEND_TO_GRAY`: toward the colour's grey times `brightness`.
fn blend_to_gray(c: Rgb, brightness: f32, factor: f32) -> Rgb {
    lerp3(factor, c, [grey(c) * brightness; 3])
}

fn blend_to_gray4(c: Rgba, brightness: f32, factor: f32) -> Rgba {
    let [r, g, b] = blend_to_gray([c[0], c[1], c[2]], brightness, factor);
    [r, g, b, c[3]]
}

// ------------------------------------------------------------------ biomes

/// A biome's visual attributes; `None` keeps the dimension's value.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct BiomeVisuals {
    pub sky_color: Option<u32>,
    pub fog_color: Option<u32>,
    pub water_fog_color: Option<u32>,
    /// `visual/water_fog_end_distance` multiplier (swamps 0.85).
    pub water_fog_end_multiplier: f32,
    /// `has_precipitation`: rain fog is halved where it never rains.
    pub precipitation: bool,
}

/// One biome's row: id, sky, fog and water fog colours, water fog end
/// multiplier and precipitation.
pub type BiomeRow = (&'static str, Option<u32>, Option<u32>, Option<u32>, f32, bool);

/// Every Java 26.3 biome's visual attributes, by id. Generated from the
/// fixture; `tests/atmosphere.rs` checks it still matches.
pub const BIOMES: [BiomeRow; 67] = [
    ("badlands", Some(0x6EB1FF), None, None, 1.0, false),
    ("bamboo_jungle", Some(0x77A8FF), None, None, 1.0, true),
    ("basalt_deltas", None, Some(0x685F70), None, 1.0, false),
    ("beach", Some(0x78A7FF), None, None, 1.0, true),
    ("birch_forest", Some(0x7AA5FF), None, None, 1.0, true),
    ("cherry_grove", Some(0x7BA4FF), None, Some(0x5DB7EF), 1.0, true),
    ("cold_ocean", Some(0x7BA4FF), None, None, 1.0, true),
    ("crimson_forest", None, Some(0x330303), None, 1.0, false),
    ("dappled_forest", Some(0x7CA3FF), Some(0xCCD8E2), Some(0x375154), 1.0, true),
    ("dark_forest", Some(0x79A6FF), None, None, 1.0, true),
    ("deep_cold_ocean", Some(0x7BA4FF), None, None, 1.0, true),
    ("deep_dark", Some(0x78A7FF), None, None, 1.0, true),
    ("deep_frozen_ocean", Some(0x7BA4FF), None, None, 1.0, true),
    ("deep_lukewarm_ocean", Some(0x7BA4FF), None, Some(0x041633), 1.0, true),
    ("deep_ocean", Some(0x7BA4FF), None, None, 1.0, true),
    ("desert", Some(0x6EB1FF), None, None, 1.0, false),
    ("dripstone_caves", Some(0x78A7FF), None, None, 1.0, true),
    ("end_barrens", None, None, None, 1.0, false),
    ("end_highlands", None, None, None, 1.0, false),
    ("end_midlands", None, None, None, 1.0, false),
    ("eroded_badlands", Some(0x6EB1FF), None, None, 1.0, false),
    ("flower_forest", Some(0x79A6FF), None, None, 1.0, true),
    ("forest", Some(0x79A6FF), None, None, 1.0, true),
    ("frozen_ocean", Some(0x7FA1FF), None, None, 1.0, true),
    ("frozen_peaks", Some(0x859DFF), None, None, 1.0, true),
    ("frozen_river", Some(0x7FA1FF), None, None, 1.0, true),
    ("grove", Some(0x81A0FF), None, None, 1.0, true),
    ("ice_spikes", Some(0x7FA1FF), None, None, 1.0, true),
    ("jagged_peaks", Some(0x859DFF), None, None, 1.0, true),
    ("jungle", Some(0x77A8FF), None, None, 1.0, true),
    ("lukewarm_ocean", Some(0x7BA4FF), None, Some(0x041633), 1.0, true),
    ("lush_caves", Some(0x7BA4FF), None, None, 1.0, true),
    ("mangrove_swamp", Some(0x78A7FF), Some(0xC0D8FF), Some(0x4D7A60), 0.85, true),
    ("meadow", Some(0x7BA4FF), None, None, 1.0, true),
    ("mushroom_fields", Some(0x77A8FF), None, None, 1.0, true),
    ("nether_wastes", None, Some(0x330808), None, 1.0, false),
    ("ocean", Some(0x7BA4FF), None, None, 1.0, true),
    ("old_growth_birch_forest", Some(0x7AA5FF), None, None, 1.0, true),
    ("old_growth_pine_taiga", Some(0x7CA3FF), None, None, 1.0, true),
    ("old_growth_spruce_taiga", Some(0x7DA3FF), None, None, 1.0, true),
    ("pale_garden", Some(0xB9B9B9), Some(0x817770), Some(0x556980), 1.0, true),
    ("plains", Some(0x78A7FF), None, None, 1.0, true),
    ("river", Some(0x7BA4FF), None, None, 1.0, true),
    ("savanna", Some(0x6EB1FF), None, None, 1.0, false),
    ("savanna_plateau", Some(0x6EB1FF), None, None, 1.0, false),
    ("small_end_islands", None, None, None, 1.0, false),
    ("snowy_beach", Some(0x7FA1FF), None, None, 1.0, true),
    ("snowy_plains", Some(0x7FA1FF), None, None, 1.0, true),
    ("snowy_slopes", Some(0x829FFF), None, None, 1.0, true),
    ("snowy_taiga", Some(0x839EFF), None, None, 1.0, true),
    ("soul_sand_valley", None, Some(0x1B4745), None, 1.0, false),
    ("sparse_jungle", Some(0x77A8FF), None, None, 1.0, true),
    ("stony_peaks", Some(0x76A8FF), None, None, 1.0, true),
    ("stony_shore", Some(0x7DA2FF), None, None, 1.0, true),
    ("sulfur_caves", Some(0x78A7FF), Some(0x8CB831), Some(0x17543C), 1.0, true),
    ("sunflower_plains", Some(0x78A7FF), None, None, 1.0, true),
    ("swamp", Some(0x78A7FF), None, Some(0x232317), 0.85, true),
    ("taiga", Some(0x7DA3FF), None, None, 1.0, true),
    ("the_end", None, None, None, 1.0, false),
    ("the_void", Some(0x7BA4FF), None, None, 1.0, false),
    ("warm_ocean", Some(0x7BA4FF), None, Some(0x041F33), 1.0, true),
    ("warped_forest", None, Some(0x1A051A), None, 1.0, false),
    ("windswept_forest", Some(0x7DA2FF), None, None, 1.0, true),
    ("windswept_gravelly_hills", Some(0x7DA2FF), None, None, 1.0, true),
    ("windswept_hills", Some(0x7DA2FF), None, None, 1.0, true),
    ("windswept_savanna", Some(0x6EB1FF), None, None, 1.0, false),
    ("wooded_badlands", Some(0x6EB1FF), None, None, 1.0, false),
];

/// A Java biome's visual attributes by id, or none for an unknown id.
pub fn biome_visuals(id: &str) -> Option<BiomeVisuals> {
    BIOMES.iter().find(|b| b.0 == id).map(|&(_, sky, fog, water, multiplier, precipitation)| BiomeVisuals {
        sky_color: sky,
        fog_color: fog,
        water_fog_color: water,
        water_fog_end_multiplier: multiplier,
        precipitation,
    })
}

/// An Overworld biome's visual attributes.
pub fn overworld_biome(biome: Biome) -> BiomeVisuals {
    biome_visuals(biome.name()).unwrap_or(BiomeVisuals {
        sky_color: None,
        fog_color: None,
        water_fog_color: None,
        water_fog_end_multiplier: 1.0,
        precipitation: true,
    })
}

/// A Nether biome's visual attributes (its fog colour; no precipitation).
pub fn nether_biome(biome: NetherBiome) -> BiomeVisuals {
    BiomeVisuals {
        sky_color: None,
        fog_color: Some(biome.fog_rgb()),
        water_fog_color: None,
        water_fog_end_multiplier: 1.0,
        precipitation: false,
    }
}

// -------------------------------------------------------------- dimensions

/// A dimension type's visual attributes over the defaults
/// (`EnvironmentAttributes`).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct DimensionVisuals {
    pub sky_color: u32,
    pub fog_color: u32,
    /// `visual/cloud_color` (ARGB); alpha 0 means no clouds.
    pub cloud_color: u32,
    pub cloud_height: f32,
    pub fog_start: f32,
    pub fog_end: f32,
    /// Has the Overworld day timeline and weather layers.
    pub day: bool,
    /// Lowest block y (void darkness starts 32 blocks above it).
    pub min_y: f32,
}

/// Attribute defaults (`EnvironmentAttributes`).
pub const DEFAULT_FOG_START: f32 = 0.0;
pub const DEFAULT_FOG_END: f32 = 1024.0;
pub const SKY_FOG_END: f32 = 512.0;
pub const CLOUD_FOG_END: f32 = 2048.0;
pub const WATER_FOG_START: f32 = -8.0;
pub const WATER_FOG_END: f32 = 96.0;
pub const WATER_FOG_COLOR: u32 = 0x050533;
pub const CLOUD_HEIGHT: f32 = 192.33;
/// `ClientLevelData.voidDarknessOnsetRange` outside superflat worlds.
pub const VOID_DARKNESS_RANGE: f32 = 32.0;

pub const fn dimension(d: Dimension) -> DimensionVisuals {
    match d {
        Dimension::Overworld => DimensionVisuals {
            sky_color: 0x78A7FF,
            fog_color: 0xC0D8FF,
            cloud_color: 0xCCFFFFFF,
            cloud_height: CLOUD_HEIGHT,
            fog_start: DEFAULT_FOG_START,
            fog_end: DEFAULT_FOG_END,
            day: true,
            min_y: -64.0,
        },
        Dimension::Nether => DimensionVisuals {
            sky_color: 0,
            fog_color: 0,
            cloud_color: 0,
            cloud_height: CLOUD_HEIGHT,
            fog_start: 10.0,
            fog_end: 96.0,
            day: false,
            min_y: 0.0,
        },
        Dimension::End => DimensionVisuals {
            sky_color: 0,
            fog_color: 0x181318,
            cloud_color: 0,
            cloud_height: CLOUD_HEIGHT,
            fog_start: DEFAULT_FOG_START,
            fog_end: DEFAULT_FOG_END,
            day: false,
            min_y: 0.0,
        },
    }
}

// ----------------------------------------------------------- day timeline

/// The Overworld day timeline's period.
pub const DAY_TICKS: f64 = 24000.0;

/// `visual/fog_color` multipliers (`timeline/day.json`).
pub const DAY_FOG: [(u32, u32); 4] = [(133, 0xFFFFFF), (11867, 0xFFFFFF), (13670, 0x0F0F16), (22330, 0x0F0F16)];
/// `visual/sky_color` multipliers.
pub const DAY_SKY: [(u32, u32); 4] = [(133, 0xFFFFFF), (11867, 0xFFFFFF), (13670, 0x000000), (22330, 0x000000)];
/// `visual/cloud_color` multipliers (ARGB).
pub const DAY_CLOUD: [(u32, u32); 4] =
    [(133, 0xFFFFFFFF), (11867, 0xFFFFFFFF), (13670, 0xFF191926), (22330, 0xFF191926)];
/// `visual/sky_light_color` multipliers.
pub const DAY_SKY_LIGHT_COLOR: [(u32, u32); 4] =
    [(730, 0xFFFFFF), (11270, 0xFFFFFF), (13140, 0x7A7AFF), (22860, 0x7A7AFF)];
/// `visual/sky_light_factor` multipliers.
pub const DAY_SKY_LIGHT_FACTOR: [(u32, f32); 4] = [(730, 1.0), (11270, 1.0), (13140, 0.24), (22860, 0.24)];
/// `visual/sunrise_sunset_color` (ARGB, overriding the default transparent black).
pub const DAY_SUNRISE: [(u32, u32); 32] = [
    (71, 0x5FEFA333),
    (310, 0x29F5BA33),
    (565, 0x06FBD433),
    (730, 0x00FFE533),
    (11270, 0x00FFE533),
    (11397, 0x04FCD833),
    (11522, 0x0FF9CB33),
    (11690, 0x29F5BA33),
    (11929, 0x5FEFA333),
    (12243, 0xB1E78733),
    (12358, 0xCCE47E33),
    (12512, 0xE9E07233),
    (12613, 0xF6DD6B33),
    (12732, 0xFEDA6333),
    (12841, 0xFED75C33),
    (13035, 0xECD25133),
    (13252, 0xC1CC4733),
    (13775, 0x36BE3733),
    (13888, 0x1FBB3533),
    (14039, 0x09B73333),
    (14192, 0x00B33333),
    (21807, 0x00B23333),
    (21961, 0x09B73333),
    (22112, 0x1FBB3533),
    (22225, 0x36BE3733),
    (22748, 0xC1CC4733),
    (22965, 0xECD25133),
    (23159, 0xFED75C33),
    (23272, 0xFEDA6333),
    (23488, 0xE9E07233),
    (23642, 0xCCE47E33),
    (23757, 0xB1E78733),
];

/// `KeyframeTrackSampler.sample` for a looping linear track: the segment
/// containing `ticks` (wrapped into the period), lerped.
pub fn sample<T: Copy>(keys: &[(u32, T)], ticks: f64, lerp: impl Fn(f32, T, T) -> T) -> T {
    let t = ticks.rem_euclid(DAY_TICKS);
    let n = keys.len();
    let (first, last) = (keys[0], keys[n - 1]);
    // Segments: last-period -> first, each pair, last -> first+period.
    let segment = |from: (f64, T), to: (f64, T)| {
        if t <= from.0 {
            from.1
        } else if t >= to.0 {
            to.1
        } else {
            lerp(((t - from.0) / (to.0 - from.0)) as f32, from.1, to.1)
        }
    };
    if t < f64::from(first.0) {
        return segment((f64::from(last.0) - DAY_TICKS, last.1), (f64::from(first.0), first.1));
    }
    for pair in keys.windows(2) {
        if t < f64::from(pair[1].0) {
            return segment((f64::from(pair[0].0), pair[0].1), (f64::from(pair[1].0), pair[1].1));
        }
    }
    segment((f64::from(last.0), last.1), (f64::from(first.0) + DAY_TICKS, first.1))
}

fn day_rgb(keys: &[(u32, u32)], ticks: f64) -> Rgb {
    let keys: Vec<(u32, Rgb)> = keys.iter().map(|&(t, c)| (t, rgb(c))).collect();
    sample(&keys, ticks, lerp3)
}

fn day_argb(keys: &[(u32, u32)], ticks: f64) -> Rgba {
    let keys: Vec<(u32, Rgba)> = keys.iter().map(|&(t, c)| (t, argb(c))).collect();
    sample(&keys, ticks, lerp4)
}

// ----------------------------------------------------------------- weather

/// Rain and thunder levels as Java's `Level.getRainLevel` and
/// `getThunderLevel` give them (thunder is already scaled by rain).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Weather {
    pub rain: f32,
    pub thunder: f32,
}

impl Weather {
    /// `WeatherAttributes`' layer: rain at (rain - thunder), then thunder.
    fn apply<T: Copy>(
        self,
        value: T,
        rain: impl Fn(T) -> T,
        thunder: impl Fn(T) -> T,
        lerp: impl Fn(f32, T, T) -> T,
    ) -> T {
        let rain_level = self.rain - self.thunder;
        let mut v = value;
        if rain_level > 0.0 {
            v = lerp(rain_level, v, rain(v));
        }
        if self.thunder > 0.0 {
            v = lerp(self.thunder, v, thunder(v));
        }
        v
    }

    pub fn sky(self, c: Rgb) -> Rgb {
        self.apply(c, |c| blend_to_gray(c, 0.6, 0.75), |c| blend_to_gray(c, 0.24, 0.94), lerp3)
    }

    pub fn fog(self, c: Rgb) -> Rgb {
        self.apply(c, |c| mul3(c, [0.5, 0.5, 0.6]), |c| mul3(c, [0.25, 0.25, 0.3]), lerp3)
    }

    pub fn cloud(self, c: Rgba) -> Rgba {
        self.apply(c, |c| blend_to_gray4(c, 0.24, 0.5), |c| blend_to_gray4(c, 0.095, 0.94), lerp4)
    }

    pub fn sunrise(self, c: Rgba) -> Rgba {
        self.apply(c, |c| mul4(c, [0.5, 0.5, 0.6, 1.0]), |c| mul4(c, [0.25, 0.25, 0.3, 1.0]), lerp4)
    }

    /// `visual/sky_light_factor`: alpha-blended toward 0.24.
    pub fn sky_light_factor(self, f: f32) -> f32 {
        self.apply(f, |f| lerp(0.3125, f, 0.24), |f| lerp(0.527_343_75, f, 0.24), lerp)
    }

    /// `visual/sky_light_color`: alpha-blended toward the night colour.
    pub fn sky_light_color(self, c: Rgb) -> Rgb {
        let night = rgb(0x7A7AFF);
        self.apply(c, |c| lerp3(0.3125, c, night), |c| lerp3(0.527_343_75, c, night), lerp3)
    }

    /// `AtmosphericFogEnvironment.applyWeatherDarken`, applied to the sky
    /// colour again before it tints the fog.
    fn darken(self, c: Rgb) -> Rgb {
        let mut c = c;
        if self.rain > 0.0 {
            let (rg, b) = (1.0 - self.rain * 0.5, 1.0 - self.rain * 0.4);
            c = [c[0] * rg, c[1] * rg, c[2] * b];
        }
        if self.thunder > 0.0 {
            c = c.map(|v| v * (1.0 - self.thunder * 0.5));
        }
        c
    }
}

// --------------------------------------------------------------- the sky

/// The camera's blended biome values: sky, fog and water fog colours and
/// the water fog end multiplier, each already resolved against the
/// dimension (a biome without the attribute contributes the dimension's).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Local {
    pub sky: Rgb,
    pub fog: Rgb,
    pub water_fog: Rgb,
    pub water_fog_end_multiplier: f32,
}

impl Local {
    /// One biome's values over its dimension's.
    pub fn resolve(d: DimensionVisuals, b: BiomeVisuals) -> Self {
        Self {
            sky: rgb(b.sky_color.unwrap_or(d.sky_color)),
            fog: rgb(b.fog_color.unwrap_or(d.fog_color)),
            water_fog: rgb(b.water_fog_color.unwrap_or(WATER_FOG_COLOR)),
            water_fog_end_multiplier: b.water_fog_end_multiplier,
        }
    }
}

/// Java's spatial blend of biome attributes: the 6 x 6 quart columns
/// around `(camera - 2) / 4` with weights 1, 4, 6, 4, 1 (as
/// [`crate::world::nether_biome::FogSampler`] does for the Nether's fog),
/// kept until the camera crosses into another quart.
#[derive(Default)]
pub struct Sampler {
    cell: Option<(i32, i32)>,
    values: Vec<Local>,
}

impl Sampler {
    const KERNEL: [f64; 7] = [0.0, 1.0, 4.0, 6.0, 4.0, 1.0, 0.0];

    /// `local(qx, qz)` gives a quart column's resolved values.
    pub fn sample(&mut self, x: f64, z: f64, local: impl Fn(i32, i32) -> Local) -> Local {
        let (px, pz) = ((x - 2.0) * 0.25, (z - 2.0) * 0.25);
        let cell = (px.floor() as i32, pz.floor() as i32);
        if self.cell != Some(cell) || self.values.len() != 36 {
            self.cell = Some(cell);
            self.values = (0..36).map(|i| local(cell.0 - 2 + (i % 6), cell.1 - 2 + (i / 6))).collect();
        }
        let (fx, fz) = (px - f64::from(cell.0), pz - f64::from(cell.1));
        let weight = |f: f64, l: usize| Self::KERNEL[l + 1] + (Self::KERNEL[l] - Self::KERNEL[l + 1]) * f;
        let (mut total, mut sum) = (0.0f64, [0.0f64; 10]);
        for (i, v) in self.values.iter().enumerate() {
            let w = weight(fx, i % 6) * weight(fz, i / 6);
            total += w;
            let flat = [
                v.sky[0],
                v.sky[1],
                v.sky[2],
                v.fog[0],
                v.fog[1],
                v.fog[2],
                v.water_fog[0],
                v.water_fog[1],
                v.water_fog[2],
                v.water_fog_end_multiplier,
            ];
            for (s, f) in sum.iter_mut().zip(flat) {
                *s += f64::from(f) * w;
            }
        }
        let s = sum.map(|v| (v / total) as f32);
        Local {
            sky: [s[0], s[1], s[2]],
            fog: [s[3], s[4], s[5]],
            water_fog: [s[6], s[7], s[8]],
            water_fog_end_multiplier: s[9],
        }
    }
}

/// What the eye is inside (`Camera.getFluidInCamera`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Medium {
    #[default]
    Air,
    Water,
    Lava,
    PowderSnow,
}

/// Everything about the camera Java's fog reads.
#[derive(Clone, Copy, Debug)]
pub struct Eye {
    pub dimension: Dimension,
    /// Blended biome values at the camera.
    pub local: Local,
    /// The biome at the camera block has precipitation.
    pub precipitation: bool,
    /// Day timeline ticks (0 sunrise, 6000 noon).
    pub day_ticks: f64,
    pub weather: Weather,
    /// A lightning flash is lighting the sky (and Hide Sky Flashes is off).
    pub sky_flash: bool,
    /// Render distance in blocks (Java's chunks x 16).
    pub render_distance: f32,
    /// Cloud Distance in Java chunks.
    pub cloud_range: u32,
    /// Sky light at the camera block, 0..15 (rain fog needs open sky).
    pub sky_light: u8,
    pub camera_y: f32,
    /// The camera's horizontal look direction's x (east) component.
    pub forward_x: f32,
    pub medium: Medium,
    pub spectator: bool,
    pub fire_resistance: bool,
    /// Blindness ticks left (`None` without the effect; `u32::MAX` infinite).
    pub blindness: Option<u32>,
    /// Darkness's blend factor 0..1 (`None` without the effect).
    pub darkness: Option<f32>,
    /// Night vision scale 0..1 (0 without the effect, or with Darkness).
    pub night_vision: f32,
    /// `LocalPlayer.getWaterVision`, 0..1.
    pub water_vision: f32,
}

/// The fog for one eye, as `FogData` plus the sky colours the sky shader
/// needs. Colours are sRGB.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Fog {
    pub color: Rgb,
    /// The sky's colour overhead (black below the horizon in the Nether
    /// and End, which draw no sky).
    pub sky: Rgb,
    /// Sunrise/sunset glow colour and strength.
    pub sunrise: Rgba,
    pub environmental_start: f32,
    pub environmental_end: f32,
    pub render_distance_start: f32,
    pub render_distance_end: f32,
    pub sky_end: f32,
    pub cloud_end: f32,
}

impl Fog {
    /// `fog.glsl`'s `total_fog_value` for a camera-relative position.
    pub fn value(&self, rel: [f32; 3]) -> f32 {
        let linear = |d: f32, start: f32, end: f32| {
            if d <= start {
                0.0
            } else if d >= end {
                1.0
            } else {
                (d - start) / (end - start)
            }
        };
        let spherical = (rel[0] * rel[0] + rel[1] * rel[1] + rel[2] * rel[2]).sqrt();
        let cylindrical = (rel[0] * rel[0] + rel[2] * rel[2]).sqrt().max(rel[1].abs());
        linear(spherical, self.environmental_start, self.environmental_end).max(linear(
            cylindrical,
            self.render_distance_start,
            self.render_distance_end,
        ))
    }

    /// The farthest a vertex can still show through the fog: Java draws
    /// nothing past this, so culling there changes nothing.
    pub fn end(&self) -> f32 {
        self.environmental_end.min(self.render_distance_end * std::f32::consts::SQRT_2)
    }
}

/// The smoothed rain fog (`AtmosphericFogEnvironment.rainFogMultiplier`).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct RainFog(pub f32);

impl RainFog {
    /// Moves toward this eye's target over `ticks` game ticks.
    pub fn update(&mut self, eye: &Eye, ticks: f32) {
        let open = ((f32::from(eye.sky_light) - 8.0) / 7.0).clamp(0.0, 1.0);
        let target = eye.weather.rain * open * if eye.precipitation { 1.0 } else { 0.5 };
        self.0 += (target - self.0) * (ticks * 0.2).min(1.0);
    }
}

/// The sky colour overhead (`visual/sky_color` through its layers), with a
/// lightning flash lerped in.
pub fn sky_color(eye: &Eye) -> Rgb {
    if !dimension(eye.dimension).day {
        return eye.local.sky;
    }
    let sky = eye.weather.sky(mul3(eye.local.sky, day_rgb(&DAY_SKY, eye.day_ticks)));
    if eye.sky_flash { lerp3(0.22, sky, [0.8, 0.8, 1.0]) } else { sky }
}

/// `visual/sky_light_factor` and `visual/sky_light_color` for the lightmap.
pub fn sky_light(eye: &Eye) -> (f32, Rgb) {
    match eye.dimension {
        Dimension::Overworld => {
            let sample_f = |keys: &[(u32, f32)]| sample(keys, eye.day_ticks, lerp);
            let factor = eye.weather.sky_light_factor(sample_f(&DAY_SKY_LIGHT_FACTOR));
            let color = eye.weather.sky_light_color(day_rgb(&DAY_SKY_LIGHT_COLOR, eye.day_ticks));
            (if eye.sky_flash { 1.0 } else { factor }, color)
        }
        Dimension::Nether => (0.0, rgb(0x7A7AFF)),
        Dimension::End => (0.0, rgb(0xAC60CD)),
    }
}

/// The cloud colour (`visual/cloud_color` through its layers); alpha 0 means
/// Java draws no clouds.
pub fn cloud_color(eye: &Eye) -> Rgba {
    let d = dimension(eye.dimension);
    let base = argb(d.cloud_color);
    if !d.day {
        return base;
    }
    eye.weather.cloud(mul4(base, day_argb(&DAY_CLOUD, eye.day_ticks)))
}

/// `FogRenderer.setupFog` and `computeFogColor` for `eye`, with the rain
/// fog state already updated.
pub fn fog(eye: &Eye, rain_fog: RainFog) -> Fog {
    let d = dimension(eye.dimension);
    let rd = eye.render_distance;
    let chunks = (rd / 16.0).floor();
    let sky = sky_color(eye);
    let mut sunrise = [0.0; 4];

    // Colour source: the first applicable environment that provides one.
    let base = match eye.medium {
        Medium::Lava => rgb(0x991900),
        Medium::PowderSnow => rgb(0x9FBBCC),
        Medium::Water => eye.local.water_fog,
        Medium::Air => {
            let mut fog = eye.local.fog;
            if d.day {
                fog = eye.weather.fog(mul3(fog, day_rgb(&DAY_FOG, eye.day_ticks)));
                sunrise = eye.weather.sunrise(day_argb(&DAY_SUNRISE, eye.day_ticks));
            }
            if chunks >= 4.0 {
                // The sun sets in the west (-x) after noon and rises in the east.
                let sun_x = if day_sun_angle_sin(eye.day_ticks) > 0.0 { -1.0 } else { 1.0 };
                let facing = eye.forward_x * sun_x;
                if facing > 0.0 && sunrise[3] > 0.0 {
                    fog = lerp3(facing * sunrise[3], fog, [sunrise[0], sunrise[1], sunrise[2]]);
                }
            }
            let dark_sky = eye.weather.darken(sky);
            let sky_fog_end = (SKY_FOG_END / 16.0).min(chunks);
            let mix = 1.0 - lerp((sky_fog_end / 32.0).clamp(0.0, 1.0), 0.25, 1.0).powf(0.25);
            lerp3(mix, fog, dark_sky)
        }
    };
    // Void darkness, then the darkness-modifying effect (Blindness first).
    let mut darkness = ((VOID_DARKNESS_RANGE + d.min_y - eye.camera_y) / VOID_DARKNESS_RANGE).clamp(0.0, 1.0);
    if let Some(ticks) = eye.blindness {
        darkness = if ticks <= 19 { (ticks as f32 / 20.0).max(darkness) } else { 1.0 };
    } else if let Some(blend) = eye.darkness {
        darkness = blend.max(darkness);
    }
    let mut color = base;
    if darkness > 0.0 && !matches!(eye.medium, Medium::Lava | Medium::PowderSnow) {
        let b = (1.0 - darkness) * (1.0 - darkness);
        color = color.map(|c| c * b);
    }
    let brighten = match eye.medium {
        Medium::Water => eye.water_vision,
        _ => eye.night_vision,
    };
    if color.iter().all(|&c| c != 0.0) {
        let scale = 1.0 / color[0].max(color[1]).max(color[2]);
        color = color.map(|c| lerp(brighten, c, c * scale));
    }

    // Distances: the first applicable environment.
    let (start, end, sky_end, cloud_end);
    let spectator_fog = |near_end: f32| if eye.spectator { (-8.0, rd * 0.5) } else { (0.0, near_end) };
    match eye.medium {
        Medium::Lava => {
            (start, end) = if eye.spectator {
                (-8.0, rd * 0.5)
            } else if eye.fire_resistance {
                (0.0, 5.0)
            } else {
                (0.25, 1.0)
            };
            (sky_end, cloud_end) = (end, end);
        }
        Medium::PowderSnow => {
            (start, end) = spectator_fog(2.0);
            (sky_end, cloud_end) = (end, end);
        }
        _ if eye.blindness.is_some() || eye.darkness.is_some() => {
            // Blindness and Darkness take the distances whatever the fluid.
            (start, end, sky_end, cloud_end) = if let Some(ticks) = eye.blindness {
                let dist = if ticks == u32::MAX { 5.0 } else { lerp((ticks as f32 / 20.0).min(1.0), rd, 5.0) };
                (dist * 0.25, dist, dist * 0.8, dist * 0.8)
            } else {
                let dist = lerp(eye.darkness.unwrap_or(0.0), rd, 15.0);
                (dist * 0.75, dist, dist, dist)
            };
        }
        Medium::Water => {
            start = WATER_FOG_START;
            end = WATER_FOG_END * eye.local.water_fog_end_multiplier * eye.water_vision.max(0.25);
            (sky_end, cloud_end) = (end, end);
        }
        Medium::Air => {
            start = d.fog_start - 160.0 * rain_fog.0;
            end = (d.fog_end.min(96.0)).max(d.fog_end - 256.0 * rain_fog.0);
            sky_end = rd.min(SKY_FOG_END);
            cloud_end = (eye.cloud_range as f32 * 16.0).min(CLOUD_FOG_END);
        }
    }
    let span = (rd / 10.0).clamp(4.0, 64.0);
    Fog {
        color,
        sky,
        sunrise,
        environmental_start: start,
        environmental_end: end,
        render_distance_start: rd - span,
        render_distance_end: rd,
        sky_end,
        cloud_end,
    }
}

/// The sign of `sin(visual/sun_angle)`: the day timeline turns the sun from
/// 0 degrees at noon through 360, so it is positive from noon to midnight.
fn day_sun_angle_sin(ticks: f64) -> f32 {
    let t = ticks.rem_euclid(DAY_TICKS);
    if (6000.0..18000.0).contains(&t) { 1.0 } else { -1.0 }
}

/// `LocalPlayer.getWaterVision` from the ticks spent under water.
pub fn water_vision(ticks: u32) -> f32 {
    if ticks >= 600 {
        return 1.0;
    }
    let t = ticks as f32;
    let a = (t / 100.0).clamp(0.0, 1.0);
    let b = if t < 100.0 { 0.0 } else { ((t - 100.0) / 500.0).clamp(0.0, 1.0) };
    a * 0.6 + b * 0.4
}

/// One tick of `LocalPlayer.waterVisionTime`: up 1 under water (10 for a
/// spectator), down 10 out of it, within 0..600.
pub fn water_vision_tick(ticks: u32, eye_in_water: bool, spectator: bool) -> u32 {
    if eye_in_water { (ticks + if spectator { 10 } else { 1 }).min(600) } else { ticks.saturating_sub(10) }
}

// ----------------------------------------------------------------- options

/// Java's Clouds option (`CloudStatus`, saved as `renderClouds`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum Clouds {
    Off,
    Fast,
    #[default]
    Fancy,
}

impl Clouds {
    pub const ALL: [Clouds; 3] = [Clouds::Off, Clouds::Fast, Clouds::Fancy];

    /// The button's caption.
    pub fn caption(self) -> &'static str {
        match self {
            Clouds::Off => "OFF",
            Clouds::Fast => "Fast",
            Clouds::Fancy => "Fancy",
        }
    }

    /// `options.txt` value (Java's legacy names).
    pub fn saved(self) -> &'static str {
        match self {
            Clouds::Off => "false",
            Clouds::Fast => "fast",
            Clouds::Fancy => "true",
        }
    }

    pub fn parse(text: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|c| c.saved() == text)
    }

    /// The next value the button cycles to.
    pub fn next(self) -> Self {
        Self::ALL[(self as usize + 1) % 3]
    }
}

/// Cloud Distance's range and default, in Java chunks. The default is 64,
/// what a fresh Java 26.3 client shows: the option's own default is 128,
/// but the client applies its graphics preset at startup and the default
/// Fancy preset sets 64 (`GraphicsPreset.apply`; v1.0 Track 2).
pub const CLOUD_RANGE: (u32, u32) = (2, 128);
pub const DEFAULT_CLOUD_RANGE: u32 = 64;

/// How many ticks a lightning bolt flashes the sky (`setSkyFlashTime(2)`).
pub const SKY_FLASH_TICKS: u32 = 2;

#[cfg(test)]
mod tests {
    use super::*;

    fn eye() -> Eye {
        Eye {
            dimension: Dimension::Overworld,
            local: Local::resolve(dimension(Dimension::Overworld), overworld_biome(Biome::Plains)),
            precipitation: true,
            day_ticks: 6000.0,
            weather: Weather::default(),
            sky_flash: false,
            render_distance: 256.0,
            cloud_range: 128,
            sky_light: 15,
            camera_y: 100.0,
            forward_x: 0.0,
            medium: Medium::Air,
            spectator: false,
            fire_resistance: false,
            blindness: None,
            darkness: None,
            night_vision: 0.0,
            water_vision: 0.0,
        }
    }

    #[test]
    fn a_clear_noon_fogs_from_the_camera_to_1024_blocks_and_the_last_tenth_of_the_view() {
        let f = fog(&eye(), RainFog(0.0));
        assert_eq!((f.environmental_start, f.environmental_end), (0.0, 1024.0));
        assert_eq!((f.render_distance_start, f.render_distance_end), (230.4, 256.0));
        assert_eq!((f.sky_end, f.cloud_end), (256.0, 2048.0));
        assert!((f.value([100.0, 0.0, 0.0]) - 100.0 / 1024.0).abs() < 1e-6);
        assert_eq!(f.value([0.0, 0.0, 256.0]), 1.0);
    }

    #[test]
    fn the_timeline_loops_through_midnight() {
        // From 13670 to 22330 the fog multiplier holds the night value, then
        // ramps up across midnight's wrap to the day value at 133.
        assert_eq!(day_rgb(&DAY_FOG, 20000.0), rgb(0x0F0F16));
        assert_eq!(
            day_rgb(&DAY_FOG, 0.0),
            lerp3((24000.0 - 22330.0) / (24133.0 - 22330.0), rgb(0x0F0F16), rgb(0xFFFFFF))
        );
        assert_eq!(day_rgb(&DAY_FOG, 6000.0), [1.0; 3]);
    }

    #[test]
    fn rain_fog_closes_in_only_under_open_sky() {
        let mut e = eye();
        e.weather = Weather { rain: 1.0, thunder: 0.0 };
        let mut rain = RainFog::default();
        for _ in 0..200 {
            rain.update(&e, 1.0);
        }
        let f = fog(&e, rain);
        assert!((f.environmental_start + 160.0).abs() < 1e-3 && (f.environmental_end - 768.0).abs() < 1e-2);
        e.sky_light = 8;
        let mut covered = RainFog::default();
        covered.update(&e, 1000.0);
        assert_eq!(covered.0, 0.0);
    }

    #[test]
    fn clouds_cycle_off_fast_fancy() {
        assert_eq!(Clouds::default(), Clouds::Fancy);
        assert_eq!(Clouds::Fancy.next(), Clouds::Off);
        for c in Clouds::ALL {
            assert_eq!(Clouds::parse(c.saved()), Some(c));
        }
    }
}
