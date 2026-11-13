//! Java 26.3 item-stack data components that [`Stack`] carries beyond its
//! inline fields: the armor trim, the dyed colour, and one interned payload
//! slot for the variable-size components (bundle contents, writable and
//! written book content, banner patterns).
//!
//! Java compares whole component maps when stacks meet
//! (`ItemStack.isSameItemSameComponents`), and setting a component to the
//! item's default removes it from the patch (`PatchedDataComponentMap.set`),
//! so an empty bundle equals a fresh one. Stacks are `Copy` and move through
//! every hot inventory path, so the payload is interned once per distinct
//! value: equal payloads share one `&'static` entry, comparison is a pointer
//! check, and the saved field is encoded once. Interned values are never
//! freed; callers intern finished values (a signed book, a closed bundle
//! edit), not every keystroke.
//!
//! Saves store names (`trim_pattern`, `trim_material`, `banner_pattern` and
//! dye ids), never indices into these tables.

use super::{Stack, stack_from_str, stack_to_string};
use crate::color::DyeColor;
use crate::item::{ArmorMaterial, Item};
use serde_json::{Value, json};

/// The Java data component ids these slots model (`DataComponents`).
pub const MODELLED: [&str; 7] = [
    "banner_patterns",
    "base_color",
    "bundle_contents",
    "dyed_color",
    "trim",
    "writable_book_content",
    "written_book_content",
];

/// `WritableBookContent.MAX_PAGES` and `PAGE_EDIT_LENGTH` (UTF-16 units).
pub const WRITABLE_MAX_PAGES: usize = 100;
pub const WRITABLE_PAGE_LENGTH: usize = 1024;
/// `WrittenBookContent.PAGE_LENGTH`, `TITLE_MAX_LENGTH` and `MAX_GENERATION`.
/// The page limit applies to the page's JSON encoding ([`java_json_len`]),
/// as `ComponentSerialization.flatRestrictedCodec` measures it.
pub const WRITTEN_PAGE_LENGTH: usize = 32767;
pub const WRITTEN_TITLE_MAX_LENGTH: usize = 32;
pub const WRITTEN_MAX_GENERATION: u8 = 3;
/// `ItemStackTemplate`'s count range, for each stack in a bundle.
pub const BUNDLE_STACK_MAX: u8 = 99;
/// The longest saved payload field read back (characters).
const MAX_FIELD: usize = 8 << 20;

/// Java's trim patterns (`data/minecraft/trim_pattern`). New patterns go at
/// the end; saves store [`TrimPattern::name`].
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum TrimPattern {
    Bolt,
    Coast,
    Dune,
    Eye,
    Flow,
    Host,
    Raiser,
    Rib,
    Sentry,
    Shaper,
    Silence,
    Snout,
    Spire,
    Tide,
    Vex,
    Ward,
    Wayfinder,
    Wild,
}

impl TrimPattern {
    pub const ALL: [Self; 18] = [
        Self::Bolt,
        Self::Coast,
        Self::Dune,
        Self::Eye,
        Self::Flow,
        Self::Host,
        Self::Raiser,
        Self::Rib,
        Self::Sentry,
        Self::Shaper,
        Self::Silence,
        Self::Snout,
        Self::Spire,
        Self::Tide,
        Self::Vex,
        Self::Ward,
        Self::Wayfinder,
        Self::Wild,
    ];

    /// The registry name, without `minecraft:`.
    pub const fn name(self) -> &'static str {
        const NAMES: [&str; 18] = [
            "bolt",
            "coast",
            "dune",
            "eye",
            "flow",
            "host",
            "raiser",
            "rib",
            "sentry",
            "shaper",
            "silence",
            "snout",
            "spire",
            "tide",
            "vex",
            "ward",
            "wayfinder",
            "wild",
        ];
        NAMES[self as usize]
    }

    pub fn from_name(name: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|p| p.name() == name)
    }
}

/// Java's trim materials (`data/minecraft/trim_material`). New materials go
/// at the end; saves store [`TrimMaterial::name`].
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum TrimMaterial {
    Amethyst,
    Copper,
    Diamond,
    Emerald,
    Gold,
    Iron,
    Lapis,
    Netherite,
    Quartz,
    Redstone,
    Resin,
}

impl TrimMaterial {
    pub const ALL: [Self; 11] = [
        Self::Amethyst,
        Self::Copper,
        Self::Diamond,
        Self::Emerald,
        Self::Gold,
        Self::Iron,
        Self::Lapis,
        Self::Netherite,
        Self::Quartz,
        Self::Redstone,
        Self::Resin,
    ];

    /// The registry name, without `minecraft:`.
    pub const fn name(self) -> &'static str {
        const NAMES: [&str; 11] = [
            "amethyst",
            "copper",
            "diamond",
            "emerald",
            "gold",
            "iron",
            "lapis",
            "netherite",
            "quartz",
            "redstone",
            "resin",
        ];
        NAMES[self as usize]
    }

    pub fn from_name(name: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|m| m.name() == name)
    }
}

/// Java's `ArmorTrim` component: a pattern drawn in a material.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub struct Trim {
    pub pattern: TrimPattern,
    pub material: TrimMaterial,
}

impl Trim {
    /// `pattern.material`, as saved.
    pub fn to_field(self) -> String {
        format!("{}.{}", self.pattern.name(), self.material.name())
    }

    pub fn from_field(text: &str) -> Option<Self> {
        let (pattern, material) = text.split_once('.')?;
        Some(Self { pattern: TrimPattern::from_name(pattern)?, material: TrimMaterial::from_name(material)? })
    }
}

/// Items in Java's `#trimmable_armor` (the head, chest, leg and foot armor
/// tags) that the game registers: every armor piece.
pub fn trimmable(item: Item) -> bool {
    item.as_armor().is_some()
}

/// Java's banner patterns (`data/minecraft/banner_pattern`), by registry
/// name. New patterns go at the end; saves store the name.
pub const BANNER_PATTERNS: [&str; 43] = [
    "base",
    "border",
    "bricks",
    "circle",
    "creeper",
    "cross",
    "curly_border",
    "diagonal_left",
    "diagonal_right",
    "diagonal_up_left",
    "diagonal_up_right",
    "flow",
    "flower",
    "globe",
    "gradient",
    "gradient_up",
    "guster",
    "half_horizontal",
    "half_horizontal_bottom",
    "half_vertical",
    "half_vertical_right",
    "mojang",
    "piglin",
    "rhombus",
    "skull",
    "small_stripes",
    "square_bottom_left",
    "square_bottom_right",
    "square_top_left",
    "square_top_right",
    "straight_cross",
    "stripe_bottom",
    "stripe_center",
    "stripe_downleft",
    "stripe_downright",
    "stripe_left",
    "stripe_middle",
    "stripe_right",
    "stripe_top",
    "triangle_bottom",
    "triangle_top",
    "triangles_bottom",
    "triangles_top",
];

/// One banner pattern, an index into [`BANNER_PATTERNS`].
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub struct BannerPattern(u8);

impl BannerPattern {
    pub fn all() -> impl Iterator<Item = Self> {
        (0..BANNER_PATTERNS.len() as u8).map(Self)
    }

    pub fn name(self) -> &'static str {
        BANNER_PATTERNS[self.0 as usize]
    }

    pub fn from_name(name: &str) -> Option<Self> {
        BANNER_PATTERNS.iter().position(|&n| n == name).map(|i| Self(i as u8))
    }
}

/// `BannerPatternLayers.Layer`: a pattern in a dye colour.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub struct BannerLayer {
    pub pattern: BannerPattern,
    pub color: DyeColor,
}

/// The `banner_patterns` layers, plus the `base_color` a shield carries.
#[derive(Clone, PartialEq, Eq, Debug, Default)]
pub struct BannerPatterns {
    pub base: Option<DyeColor>,
    pub layers: Vec<BannerLayer>,
}

/// `WrittenBookContent`. Java's pages are text components; these are plain
/// text, as a signed writable book makes them. Chat filtering is off in
/// single player, so only the raw title and pages are kept.
#[derive(Clone, PartialEq, Eq, Debug, Default)]
pub struct WrittenBook {
    pub title: String,
    pub author: String,
    /// 0 original, 1 copy of original, 2 copy of copy, 3 tattered.
    pub generation: u8,
    pub pages: Vec<String>,
    pub resolved: bool,
}

/// The variable-size component a stack can carry, interned into
/// [`StackPayload`] by [`Payload::intern`].
#[derive(Clone, PartialEq, Eq, Debug)]
pub enum Payload {
    /// `bundle_contents`: the stacks inside, first in first.
    Bundle(Vec<Stack>),
    /// `writable_book_content`: raw page text.
    WritableBook(Vec<String>),
    /// `written_book_content`.
    WrittenBook(WrittenBook),
    /// `banner_patterns` and `base_color`.
    Banner(BannerPatterns),
}

/// Java's `String.length` (UTF-16 code units), which its codecs limit.
fn java_len(text: &str) -> usize {
    text.encode_utf16().count()
}

/// The UTF-16 length of a plain-text page as `flatRestrictedCodec` counts
/// it: the component collapses to a bare JSON string, which Gson's
/// `JsonWriter` writes with quotes, two-character escapes for `"`, `\` and
/// `\t \b \n \r \f`, and `\uXXXX` for other control characters and
/// U+2028/U+2029.
pub fn java_json_len(text: &str) -> usize {
    2 + text
        .encode_utf16()
        .map(|c| match c {
            0x22 | 0x5C | 0x08..=0x0A | 0x0C | 0x0D => 2,
            0x00..=0x07 | 0x0B | 0x0E..=0x1F | 0x2028 | 0x2029 => 6,
            _ => 1,
        })
        .sum::<usize>()
}

fn dye_id(color: DyeColor) -> String {
    color.adjective().replace(' ', "_")
}

fn dye_from_id(id: &str) -> Option<DyeColor> {
    DyeColor::ALL.into_iter().find(|&c| dye_id(c) == id)
}

impl Payload {
    /// Whether this is the item default Java drops from the component
    /// patch: an empty bundle, a blank writable book, a plain banner.
    pub fn is_default(&self) -> bool {
        match self {
            Payload::Bundle(items) => items.is_empty(),
            Payload::WritableBook(pages) => pages.is_empty(),
            Payload::WrittenBook(_) => false,
            Payload::Banner(b) => b.base.is_none() && b.layers.is_empty(),
        }
    }

    /// Within the limits Java's codecs accept.
    pub fn is_valid(&self) -> bool {
        match self {
            Payload::Bundle(items) => items.iter().all(|s| (1..=BUNDLE_STACK_MAX).contains(&s.count)),
            Payload::WritableBook(pages) => {
                pages.len() <= WRITABLE_MAX_PAGES && pages.iter().all(|p| java_len(p) <= WRITABLE_PAGE_LENGTH)
            }
            Payload::WrittenBook(b) => {
                java_len(&b.title) <= WRITTEN_TITLE_MAX_LENGTH
                    && b.generation <= WRITTEN_MAX_GENERATION
                    && b.pages.iter().all(|p| java_json_len(p) <= WRITTEN_PAGE_LENGTH)
            }
            Payload::Banner(_) => true,
        }
    }

    /// Interns this payload: equal payloads give the same [`StackPayload`].
    /// `None` for an item default, which Java stores as no component.
    pub fn intern(self) -> Option<StackPayload> {
        use std::collections::HashMap;
        use std::sync::{Mutex, OnceLock};
        if self.is_default() {
            return None;
        }
        static TABLE: OnceLock<Mutex<HashMap<&'static str, &'static Interned>>> = OnceLock::new();
        let field = encode64(&serde_json::to_vec(&self.to_json()).expect("payload JSON"));
        let mut table = TABLE.get_or_init(Default::default).lock().unwrap_or_else(|e| e.into_inner());
        if let Some(&found) = table.get(field.as_str()) {
            return Some(StackPayload(found));
        }
        let interned: &'static Interned = Box::leak(Box::new(Interned { payload: self, field }));
        table.insert(interned.field.as_str(), interned);
        Some(StackPayload(interned))
    }

    fn to_json(&self) -> Value {
        match self {
            Payload::Bundle(items) => json!({"bundle": items.iter().map(stack_json).collect::<Vec<_>>()}),
            Payload::WritableBook(pages) => json!({"writable_book": pages}),
            Payload::WrittenBook(b) => json!({"written_book": {
                "title": b.title, "author": b.author, "generation": b.generation,
                "pages": b.pages, "resolved": b.resolved,
            }}),
            Payload::Banner(b) => {
                let layers: Vec<Value> = b.layers.iter().map(|l| json!([l.pattern.name(), dye_id(l.color)])).collect();
                match b.base {
                    Some(base) => json!({"banner": {"base": dye_id(base), "layers": layers}}),
                    None => json!({"banner": {"layers": layers}}),
                }
            }
        }
    }

    fn from_json(v: &Value) -> Option<Self> {
        let obj = v.as_object().filter(|o| o.len() == 1)?;
        let (kind, body) = obj.iter().next()?;
        let strings = |v: &Value| -> Option<Vec<String>> {
            v.as_array()?.iter().map(|p| p.as_str().map(str::to_owned)).collect()
        };
        let payload = match kind.as_str() {
            "bundle" => Payload::Bundle(body.as_array()?.iter().map(stack_from_json).collect::<Option<_>>()?),
            "writable_book" => Payload::WritableBook(strings(body)?),
            "written_book" => Payload::WrittenBook(WrittenBook {
                title: body["title"].as_str()?.to_owned(),
                author: body["author"].as_str()?.to_owned(),
                generation: u8::try_from(body["generation"].as_u64()?).ok()?,
                pages: strings(&body["pages"])?,
                resolved: body["resolved"].as_bool()?,
            }),
            "banner" => Payload::Banner(BannerPatterns {
                base: match &body["base"] {
                    Value::Null => None,
                    v => Some(dye_from_id(v.as_str()?)?),
                },
                layers: body["layers"]
                    .as_array()?
                    .iter()
                    .map(|l| {
                        let l = l.as_array().filter(|l| l.len() == 2)?;
                        Some(BannerLayer {
                            pattern: BannerPattern::from_name(l[0].as_str()?)?,
                            color: dye_from_id(l[1].as_str()?)?,
                        })
                    })
                    .collect::<Option<_>>()?,
            }),
            _ => return None,
        };
        payload.is_valid().then_some(payload)
    }

    /// Reads a saved payload field: `Some(None)` for an item default.
    pub fn from_field(text: &str) -> Option<Option<StackPayload>> {
        if text.len() > MAX_FIELD {
            return None;
        }
        let bytes = decode64(text)?;
        Some(Self::from_json(&serde_json::from_slice(&bytes).ok()?)?.intern())
    }
}

/// A bundled stack: its string, or `[string, payload]` when it carries a
/// payload, so nested bundles nest JSON rather than re-encoding fields.
fn stack_json(s: &Stack) -> Value {
    match s.payload {
        None => Value::String(stack_to_string(Some(*s))),
        Some(p) => json!([stack_to_string(Some(Stack { payload: None, ..*s })), p.to_json()]),
    }
}

fn stack_from_json(v: &Value) -> Option<Stack> {
    match v {
        Value::String(s) => stack_from_str(s)?,
        Value::Array(a) if a.len() == 2 => {
            let stack = stack_from_str(a[0].as_str()?)??;
            if stack.payload.is_some() {
                return None;
            }
            Some(Stack { payload: Payload::from_json(&a[1])?.intern(), ..stack })
        }
        _ => None,
    }
}

struct Interned {
    payload: Payload,
    /// The saved field, encoded once.
    field: String,
}

/// A stack's interned [`Payload`]: `Copy`, pointer-sized, and equal exactly
/// when the payloads are equal.
#[derive(Clone, Copy)]
pub struct StackPayload(&'static Interned);

impl PartialEq for StackPayload {
    fn eq(&self, other: &Self) -> bool {
        std::ptr::eq(self.0, other.0)
    }
}

impl Eq for StackPayload {}

impl std::fmt::Debug for StackPayload {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.0.payload.fmt(f)
    }
}

impl std::ops::Deref for StackPayload {
    type Target = Payload;
    fn deref(&self) -> &Payload {
        &self.0.payload
    }
}

impl StackPayload {
    pub fn get(self) -> &'static Payload {
        &self.0.payload
    }

    /// The saved field (unpadded base64 of the payload's JSON).
    pub fn field(self) -> &'static str {
        &self.0.field
    }
}

const BASE64: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";

/// Unpadded standard base64: no `:`, `,`, `;`, `|`, `#` or `~`, the
/// separators of the formats stack strings sit in.
fn encode64(bytes: &[u8]) -> String {
    let mut out = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for chunk in bytes.chunks(3) {
        let n = chunk.iter().enumerate().fold(0u32, |n, (i, &b)| n | (u32::from(b) << (16 - 8 * i)));
        for i in 0..=chunk.len() {
            out.push(BASE64[((n >> (18 - 6 * i)) & 63) as usize] as char);
        }
    }
    out
}

fn decode64(text: &str) -> Option<Vec<u8>> {
    if text.len() % 4 == 1 {
        return None;
    }
    let mut out = Vec::with_capacity(text.len() / 4 * 3 + 2);
    for chunk in text.as_bytes().chunks(4) {
        let mut n = 0u32;
        for (i, &c) in chunk.iter().enumerate() {
            let v = match c {
                b'A'..=b'Z' => c - b'A',
                b'a'..=b'z' => c - b'a' + 26,
                b'0'..=b'9' => c - b'0' + 52,
                b'+' => 62,
                b'/' => 63,
                _ => return None,
            };
            n |= u32::from(v) << (18 - 6 * i);
        }
        for i in 0..chunk.len() - 1 {
            out.push((n >> (16 - 8 * i)) as u8);
        }
    }
    Some(out)
}

/// Bit set in [`Stack::entity_data`] when a dyeable stack carries Java's
/// `dyed_color`; the low 24 bits are the RGB.
pub const DYED: u32 = 1 << 24;

/// Items in Java's `#cauldron_can_remove_dye`, the dyeable items: leather
/// armor, leather horse armor and wolf armor.
pub fn dyeable(item: Item) -> bool {
    item == Item::WOLF_ARMOR
        || item == Item::LEATHER_HORSE_ARMOR
        || matches!(item.as_armor(), Some((_, ArmorMaterial::Leather)))
}

/// A dyeable stack's `dyed_color`. Other items use `entity_data` for other
/// components, so it means nothing on them.
pub fn dyed_color(stack: &Stack) -> Option<[u8; 3]> {
    (dyeable(stack.item) && stack.entity_data & DYED != 0).then_some([
        (stack.entity_data >> 16) as u8,
        (stack.entity_data >> 8) as u8,
        stack.entity_data as u8,
    ])
}

/// Sets (or with `None` removes) a dyeable stack's `dyed_color`.
pub fn with_dyed_color(mut stack: Stack, rgb: Option<[u8; 3]>) -> Stack {
    stack.entity_data = rgb.map_or(0, |c| DYED | (u32::from(c[0]) << 16) | (u32::from(c[1]) << 8) | u32::from(c[2]));
    stack
}

/// `DyedItemColor.applyDyes`: integer channel averages of the current colour
/// and each dye's `textureDiffuseColor`, rescaled in float to the average
/// brightest channel. `None` with nothing to mix.
pub fn mix_dyes(current: Option<[u8; 3]>, dyes: &[DyeColor]) -> Option<[u8; 3]> {
    let (mut sum, mut intensity, mut count) = ([0_u32; 3], 0_u32, 0_u32);
    for c in current.into_iter().chain(dyes.iter().map(|d| d.rgb())) {
        for i in 0..3 {
            sum[i] += u32::from(c[i]);
        }
        intensity += u32::from(*c.iter().max().unwrap());
        count += 1;
    }
    if count == 0 {
        return None;
    }
    let avg = sum.map(|v| v / count);
    let average_intensity = intensity as f32 / count as f32;
    let result_intensity = *avg.iter().max().unwrap() as f32;
    // Java's `(int)` of 0 * x / 0.0 (NaN) is 0.
    Some(avg.map(|v| if result_intensity == 0.0 { 0 } else { (v as f32 * average_intensity / result_intensity) as u8 }))
}

/// `DyedItemColor.applyDyes` on a stack (`DyeRecipe.assemble`): one item,
/// every other component kept. `None` for an undyeable item or no dyes.
pub fn apply_dyes(stack: Stack, dyes: &[DyeColor]) -> Option<Stack> {
    if !dyeable(stack.item) || dyes.is_empty() {
        return None;
    }
    let rgb = mix_dyes(dyed_color(&stack), dyes)?;
    Some(Stack { count: 1, ..with_dyed_color(stack, Some(rgb)) })
}

/// `CauldronInteractions.dyedItemIteration`'s result: the stack without
/// its `dyed_color`, or `None` when it has none.
pub fn remove_dye(stack: Stack) -> Option<Stack> {
    dyed_color(&stack)?;
    Some(with_dyed_color(stack, None))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn base64_round_trips_every_tail_length() {
        for n in 0..10u8 {
            let bytes: Vec<u8> = (0..n).map(|i| i.wrapping_mul(97).wrapping_add(250)).collect();
            let text = encode64(&bytes);
            assert!(!text.contains([':', ',', ';', '|', '#', '~', '=']));
            assert_eq!(decode64(&text), Some(bytes));
        }
        assert_eq!(encode64(b"Man"), "TWFu");
        assert_eq!(encode64(b"Ma"), "TWE");
        assert_eq!(decode64("TWE"), Some(b"Ma".to_vec()));
        assert_eq!(decode64("T"), None);
        assert_eq!(decode64("TW:u"), None);
    }

    #[test]
    fn mixing_follows_java_integer_averages_and_float_rescale() {
        // Red (176, 46, 38) and blue (60, 68, 170): channel averages
        // (118, 57, 104); brightness (176 + 170) / 2 = 173.0; each channel
        // `(int)(v * 173.0 / 118.0)`.
        assert_eq!(mix_dyes(None, &[DyeColor::Red, DyeColor::Blue]), Some([173, 83, 152]));
        // One dye on an undyed item keeps the dye's own colour.
        assert_eq!(mix_dyes(None, &[DyeColor::Lime]), Some(DyeColor::Lime.rgb()));
        assert_eq!(mix_dyes(None, &[]), None);
        // The current colour counts as one more colour.
        assert_eq!(mix_dyes(Some([173, 83, 152]), &[]), Some([173, 83, 152]));
    }
}
