//! Java 26.3 fireworks (v0.16 Track B): firework stars
//! (`FireworkExplosion`, item 1677), firework rockets (`Fireworks`,
//! `FireworkRocketItem`, item 1676), their special crafting recipes
//! (`FireworkStarRecipe`, `FireworkStarFadeRecipe`, `FireworkRocketRecipe`)
//! and their tooltips (`Fireworks.addToTooltip`,
//! `FireworkExplosion.addAdditionalTooltip`).
//!
//! A star carries its explosion as the stack's [`Payload::Explosion`]; a
//! rocket its flight duration and stars as [`Payload::Fireworks`], whose
//! item default (`Fireworks(1, [])`) is no payload at all. The rocket
//! entity is `entity::firework`. Numbers are checked against
//! `tests/fixtures/java/26.3/flight.json`.

use crate::color::DyeColor;
use crate::inventory::Stack;
use crate::inventory::components::Payload;
use crate::item::{Item, ItemInfo, ItemKind, Sprite};
use serde_json::{Value, json};

/// `Items.FIREWORK_ROCKET`.
pub const FIREWORK_ROCKET: Item = Item(1676);
/// `Items.FIREWORK_STAR`.
pub const FIREWORK_STAR: Item = Item(1677);

/// v0.16 Track B's sound events (shulker boxes, fireworks and elytra),
/// played with original procedural sounds (`audio::flight`). Append only.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum Cue {
    /// `block.shulker_box.open` and `block.shulker_box.close`.
    ShulkerOpen,
    ShulkerClose,
    /// `entity.firework_rocket.launch`.
    Launch,
    /// `entity.firework_rocket.blast`, `.large_blast` and their `_far`
    /// forms (16 blocks or more from the camera).
    Blast,
    BlastFar,
    LargeBlast,
    LargeBlastFar,
    /// `entity.firework_rocket.twinkle` and `.twinkle_far`.
    Twinkle,
    TwinkleFar,
    /// `entity.firework_rocket.shoot` (a dispenser's rocket, level event 1004).
    Shoot,
    /// `item.armor.equip_elytra`.
    EquipElytra,
    /// `item.elytra.flying`, a stretch of the looping glide wind.
    ElytraFlying,
}

impl Cue {
    pub const ALL: [Cue; 12] = [
        Cue::ShulkerOpen,
        Cue::ShulkerClose,
        Cue::Launch,
        Cue::Blast,
        Cue::BlastFar,
        Cue::LargeBlast,
        Cue::LargeBlastFar,
        Cue::Twinkle,
        Cue::TwinkleFar,
        Cue::Shoot,
        Cue::EquipElytra,
        Cue::ElytraFlying,
    ];

    /// Java's sound event name.
    pub fn key(self) -> &'static str {
        match self {
            Cue::ShulkerOpen => "block.shulker_box.open",
            Cue::ShulkerClose => "block.shulker_box.close",
            Cue::Launch => "entity.firework_rocket.launch",
            Cue::Blast => "entity.firework_rocket.blast",
            Cue::BlastFar => "entity.firework_rocket.blast_far",
            Cue::LargeBlast => "entity.firework_rocket.large_blast",
            Cue::LargeBlastFar => "entity.firework_rocket.large_blast_far",
            Cue::Twinkle => "entity.firework_rocket.twinkle",
            Cue::TwinkleFar => "entity.firework_rocket.twinkle_far",
            Cue::Shoot => "entity.firework_rocket.shoot",
            Cue::EquipElytra => "item.armor.equip_elytra",
            Cue::ElytraFlying => "item.elytra.flying",
        }
    }
}

/// `Fireworks.MAX_EXPLOSIONS`.
pub const MAX_EXPLOSIONS: usize = 256;
/// `FireworkRocketItem.CRAFTABLE_DURATIONS`: one to three gunpowder.
pub const CRAFTABLE_DURATIONS: [u8; 3] = [1, 2, 3];
/// `firework_rocket.json` and `firework_rocket_simple.json`: three rockets.
pub const ROCKETS_PER_CRAFT: u8 = 3;
/// The `Firework` item tint's default (-7697782): a star with no colours.
pub const DEFAULT_STAR_TINT: [u8; 3] = [0x8A, 0x8A, 0x8A];

/// `FireworkExplosion.Shape`, in id order (saves store the name).
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum Shape {
    SmallBall,
    LargeBall,
    Star,
    Creeper,
    Burst,
}

impl Shape {
    pub const ALL: [Shape; 5] = [Shape::SmallBall, Shape::LargeBall, Shape::Star, Shape::Creeper, Shape::Burst];

    /// The serialized name.
    pub const fn name(self) -> &'static str {
        ["small_ball", "large_ball", "star", "creeper", "burst"][self as usize]
    }

    pub fn from_name(name: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|s| s.name() == name)
    }

    /// `item.minecraft.firework_star.shape.*` in en_us.
    pub const fn title(self) -> &'static str {
        ["Small Ball", "Large Ball", "Star-shaped", "Creeper-shaped", "Burst"][self as usize]
    }
}

/// `FireworkExplosion`: a shape, colours (RGB ints), fade colours, trail
/// and twinkle.
#[derive(Clone, PartialEq, Eq, Hash, Debug)]
pub struct Explosion {
    pub shape: Shape,
    pub colors: Vec<u32>,
    pub fade_colors: Vec<u32>,
    pub trail: bool,
    pub twinkle: bool,
}

impl Explosion {
    /// `FireworkExplosion.DEFAULT`.
    pub fn default_small_ball() -> Self {
        Self { shape: Shape::SmallBall, colors: Vec::new(), fade_colors: Vec::new(), trail: false, twinkle: false }
    }

    pub fn is_valid(&self) -> bool {
        self.colors.iter().chain(&self.fade_colors).all(|&c| c <= 0xFF_FFFF)
    }

    pub fn to_json(&self) -> Value {
        json!({
            "shape": self.shape.name(),
            "colors": self.colors,
            "fade_colors": self.fade_colors,
            "has_trail": self.trail,
            "has_twinkle": self.twinkle,
        })
    }

    pub fn from_json(v: &Value) -> Option<Self> {
        let colours = |v: &Value| -> Option<Vec<u32>> {
            match v {
                Value::Null => Some(Vec::new()),
                v => v.as_array()?.iter().map(|c| c.as_u64().and_then(|c| u32::try_from(c).ok())).collect(),
            }
        };
        let flag = |v: &Value| match v {
            Value::Null => Some(false),
            v => v.as_bool(),
        };
        Some(Self {
            shape: Shape::from_name(v["shape"].as_str()?)?,
            colors: colours(&v["colors"])?,
            fade_colors: colours(&v["fade_colors"])?,
            trail: flag(&v["has_trail"])?,
            twinkle: flag(&v["has_twinkle"])?,
        })
    }

    /// `FireworkExplosion.addAdditionalTooltip`: colours, fade colours,
    /// trail and twinkle.
    pub fn detail_lines(&self) -> Vec<String> {
        let mut out = Vec::new();
        if !self.colors.is_empty() {
            out.push(colour_names(&self.colors));
        }
        if !self.fade_colors.is_empty() {
            out.push(format!("Fade to {}", colour_names(&self.fade_colors)));
        }
        if self.trail {
            out.push("Trail".into());
        }
        if self.twinkle {
            out.push("Twinkle".into());
        }
        out
    }
}

/// `Fireworks`: the flight duration and the stars a rocket bursts into.
#[derive(Clone, PartialEq, Eq, Hash, Debug)]
pub struct Fireworks {
    pub flight: u8,
    pub explosions: Vec<Explosion>,
}

impl Default for Fireworks {
    /// The rocket item's default component, `Fireworks(1, List.of())`.
    fn default() -> Self {
        Self { flight: 1, explosions: Vec::new() }
    }
}

impl Fireworks {
    pub fn is_default(&self) -> bool {
        self.flight == 1 && self.explosions.is_empty()
    }

    pub fn is_valid(&self) -> bool {
        self.explosions.len() <= MAX_EXPLOSIONS && self.explosions.iter().all(Explosion::is_valid)
    }

    pub fn to_json(&self) -> Value {
        json!({
            "flight_duration": self.flight,
            "explosions": self.explosions.iter().map(Explosion::to_json).collect::<Vec<_>>(),
        })
    }

    pub fn from_json(v: &Value) -> Option<Self> {
        let flight = match &v["flight_duration"] {
            Value::Null => 0,
            v => u8::try_from(v.as_u64()?).ok()?,
        };
        let explosions = match &v["explosions"] {
            Value::Null => Vec::new(),
            v => v.as_array()?.iter().map(Explosion::from_json).collect::<Option<_>>()?,
        };
        Some(Self { flight, explosions })
    }

    /// `Fireworks.addToTooltip`: the flight duration, then each run of
    /// equal stars as one line (`N x Shape` for a run of several) with its
    /// details indented.
    pub fn tooltip_lines(&self) -> Vec<String> {
        let mut out = Vec::new();
        if self.flight > 0 {
            out.push(format!("Flight Duration: {}", self.flight));
        }
        let mut i = 0;
        while i < self.explosions.len() {
            let e = &self.explosions[i];
            let run = self.explosions[i..].iter().take_while(|o| *o == e).count();
            out.push(if run == 1 { e.shape.title().to_string() } else { format!("{run} x {}", e.shape.title()) });
            out.extend(e.detail_lines().into_iter().map(|l| format!("  {l}")));
            i += run;
        }
        out
    }
}

/// `DyeColor.fireworkColor`, in `DyeColor` order.
pub const FIREWORK_COLORS: [u32; 16] = [
    0xF0F0F0, 0xEB8844, 0xC354CD, 0x6689D3, 0xDECF2A, 0x41CD34, 0xD88198, 0x434343, 0xABABAB, 0x287697, 0x7B2FBE,
    0x253192, 0x51301A, 0x3B511A, 0xB3312C, 0x1E1B1B,
];

pub const fn firework_color(c: DyeColor) -> u32 {
    FIREWORK_COLORS[c as usize]
}

/// `DyeColor.byFireworkColor`.
pub fn dye_of_firework_color(rgb: u32) -> Option<DyeColor> {
    DyeColor::ALL.into_iter().find(|&c| firework_color(c) == rgb)
}

/// `item.minecraft.firework_star.<colour>`, "Custom" for any other colour.
fn colour_name(rgb: u32) -> String {
    match dye_of_firework_color(rgb) {
        Some(c) => c.adjective().split(' ').map(capitalise).collect::<Vec<_>>().join(" "),
        None => "Custom".into(),
    }
}

fn capitalise(word: &str) -> String {
    let mut c = word.chars();
    c.next().map_or_else(String::new, |f| f.to_uppercase().chain(c).collect())
}

fn colour_names(colours: &[u32]) -> String {
    colours.iter().map(|&c| colour_name(c)).collect::<Vec<_>>().join(", ")
}

pub(crate) fn item_info(item: Item) -> Option<ItemInfo> {
    match item {
        FIREWORK_ROCKET => Some(ItemInfo {
            name: "firework rocket",
            kind: ItemKind::Material,
            max_stack: 64,
            sprite: Sprite::FireworkRocket,
        }),
        FIREWORK_STAR => Some(ItemInfo {
            name: "firework star",
            kind: ItemKind::Material,
            max_stack: 64,
            sprite: Sprite::FireworkStar,
        }),
        _ => None,
    }
}

/// A rocket's `fireworks` component (the item default when it carries none).
pub fn fireworks(stack: &Stack) -> Fireworks {
    match stack.payload() {
        Some(Payload::Fireworks(f)) => f.clone(),
        _ => Fireworks::default(),
    }
}

/// A star's `firework_explosion`, if it has one.
pub fn explosion(stack: &Stack) -> Option<&'static Explosion> {
    match stack.payload() {
        Some(Payload::Explosion(e)) if stack.item == FIREWORK_STAR => Some(e),
        _ => None,
    }
}

/// A rocket of `fireworks`, `count` of them.
pub fn rocket(fireworks: Fireworks, count: u8) -> Stack {
    Stack::new(FIREWORK_ROCKET, count).with_payload(Payload::Fireworks(fireworks))
}

/// A star of `explosion`.
pub fn star(explosion: Explosion) -> Stack {
    Stack::new(FIREWORK_STAR, 1).with_payload(Payload::Explosion(explosion))
}

/// The `Firework` item tint of a star's icon: one colour as it is, several
/// averaged channel by channel (integer division), none the grey default.
pub fn star_tint(stack: &Stack) -> Option<[u8; 3]> {
    if stack.item != FIREWORK_STAR {
        return None;
    }
    let colours = explosion(stack).map_or(&[][..], |e| e.colors.as_slice());
    if colours.is_empty() {
        return Some(DEFAULT_STAR_TINT);
    }
    let n = colours.len() as u32;
    let sum = colours
        .iter()
        .fold([0u32; 3], |acc, &c| [acc[0] + (c >> 16 & 255), acc[1] + (c >> 8 & 255), acc[2] + (c & 255)]);
    Some(sum.map(|v| (v / n) as u8))
}

/// The `#minecraft:skulls` items the game registers (the creeper shape's
/// ingredient). Java's tag lists player, creeper, zombie, skeleton, wither
/// skeleton, dragon and piglin heads; v0.16 Track C registers the heads, so
/// any of these names that resolves counts.
pub fn is_skull(item: Item) -> bool {
    static SKULLS: std::sync::OnceLock<Vec<Item>> = std::sync::OnceLock::new();
    SKULLS
        .get_or_init(|| {
            [
                "player_head",
                "creeper_head",
                "zombie_head",
                "skeleton_skull",
                "wither_skeleton_skull",
                "dragon_head",
                "piglin_head",
            ]
            .into_iter()
            .filter_map(Item::from_name)
            .chain([Item::WITHER_SKULL])
            .collect()
        })
        .contains(&item)
}

/// `FireworkStarRecipe.shapes`' ingredient for each shape but the small ball.
pub fn shape_of(item: Item) -> Option<Shape> {
    match item {
        Item::FEATHER => Some(Shape::Burst),
        Item::FIRE_CHARGE => Some(Shape::LargeBall),
        Item::GOLD_NUGGET => Some(Shape::Star),
        i if is_skull(i) => Some(Shape::Creeper),
        _ => None,
    }
}

/// `FireworkStarRecipe`: gunpowder, at least one dye, and at most one shape
/// ingredient, one diamond (trail) and one glowstone dust (twinkle).
pub fn star_recipe(occupied: &[Stack]) -> Option<Stack> {
    if occupied.len() < 2 {
        return None;
    }
    let (mut fuel, mut dye, mut shape, mut trail, mut twinkle) = (false, false, None, false, false);
    let mut colors = Vec::new();
    for s in occupied {
        match s.item {
            Item::GLOWSTONE_DUST if !twinkle => twinkle = true,
            Item::GLOWSTONE_DUST => return None,
            Item::DIAMOND if !trail => trail = true,
            Item::DIAMOND => return None,
            Item::GUNPOWDER if !fuel => fuel = true,
            Item::GUNPOWDER => return None,
            i if i.dye_color().is_some() => {
                dye = true;
                colors.push(firework_color(i.dye_color()?));
            }
            i => match shape_of(i) {
                Some(found) if shape.is_none() => shape = Some(found),
                _ => return None,
            },
        }
    }
    (fuel && dye).then(|| {
        star(Explosion { shape: shape.unwrap_or(Shape::SmallBall), colors, fade_colors: Vec::new(), trail, twinkle })
    })
}

/// `FireworkStarFadeRecipe`: one star and at least one dye; the star keeps
/// its components and its fade colours become the dyes' (an explosion-less
/// star gets `FireworkExplosion.DEFAULT` with them).
pub fn fade_recipe(occupied: &[Stack]) -> Option<Stack> {
    if occupied.len() < 2 {
        return None;
    }
    let mut target = None;
    let mut colors = Vec::new();
    for s in occupied {
        if let Some(c) = s.item.dye_color() {
            colors.push(firework_color(c));
        } else if s.item == FIREWORK_STAR && target.is_none() {
            target = Some(*s);
        } else {
            return None;
        }
    }
    let target = target?;
    if colors.is_empty() {
        return None;
    }
    let mut e = explosion(&target).cloned().unwrap_or_else(Explosion::default_small_ball);
    e.fade_colors = colors;
    Some(Stack { count: 1, ..target }.with_payload(Payload::Explosion(e)))
}

/// `FireworkRocketRecipe`: one paper, one to three gunpowder (the flight
/// duration) and any number of stars, whose explosions go in grid order.
/// Three rockets.
pub fn rocket_recipe(occupied: &[Stack]) -> Option<Stack> {
    if occupied.len() < 2 {
        return None;
    }
    let (mut shell, mut fuel) = (false, 0u8);
    let mut explosions = Vec::new();
    for s in occupied {
        match s.item {
            Item::PAPER if !shell => shell = true,
            Item::PAPER => return None,
            Item::GUNPOWDER => {
                fuel += 1;
                if fuel > 3 {
                    return None;
                }
            }
            FIREWORK_STAR => explosions.extend(explosion(s).cloned()),
            _ => return None,
        }
    }
    (shell && fuel >= 1).then(|| rocket(Fireworks { flight: fuel, explosions }, ROCKETS_PER_CRAFT))
}

/// The firework recipes the crafting grid checks before its table.
pub fn grid_special(occupied: &[Stack]) -> Option<Stack> {
    rocket_recipe(occupied).or_else(|| star_recipe(occupied)).or_else(|| fade_recipe(occupied))
}

/// The lines under a firework rocket's or star's name.
pub fn tooltip_lines(stack: &Stack) -> Vec<String> {
    match stack.item {
        FIREWORK_ROCKET => fireworks(stack).tooltip_lines(),
        FIREWORK_STAR => match explosion(stack) {
            Some(e) => std::iter::once(e.shape.title().to_string()).chain(e.detail_lines()).collect(),
            None => Vec::new(),
        },
        _ => Vec::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn s(item: Item) -> Stack {
        Stack::new(item, 1)
    }

    #[test]
    fn stars_take_one_of_each_modifier_and_any_dyes() {
        let red = DyeColor::Red.dye();
        let blue = DyeColor::Blue.dye();
        let star = star_recipe(&[s(Item::GUNPOWDER), s(red), s(blue), s(Item::DIAMOND), s(Item::GLOWSTONE_DUST)])
            .expect("a star");
        let e = explosion(&star).unwrap();
        assert_eq!(e.shape, Shape::SmallBall);
        assert_eq!(e.colors, [0xB3312C, 0x253192]);
        assert!(e.trail && e.twinkle);
        assert_eq!(
            explosion(&star_recipe(&[s(Item::GUNPOWDER), s(red), s(Item::FIRE_CHARGE)]).unwrap()).unwrap().shape,
            Shape::LargeBall
        );
        // No dye, two gunpowder, two shapes or an unrelated item: no star.
        assert!(star_recipe(&[s(Item::GUNPOWDER), s(Item::DIAMOND)]).is_none());
        assert!(star_recipe(&[s(Item::GUNPOWDER), s(Item::GUNPOWDER), s(red)]).is_none());
        assert!(star_recipe(&[s(Item::GUNPOWDER), s(red), s(Item::FEATHER), s(Item::GOLD_NUGGET)]).is_none());
        assert!(star_recipe(&[s(Item::GUNPOWDER), s(red), s(Item::STICK)]).is_none());
    }

    #[test]
    fn rockets_count_gunpowder_and_collect_stars_in_order() {
        let red = star_recipe(&[s(Item::GUNPOWDER), s(DyeColor::Red.dye())]).unwrap();
        let rocket = rocket_recipe(&[s(Item::PAPER), s(Item::GUNPOWDER), s(Item::GUNPOWDER), red, red]).unwrap();
        assert_eq!(rocket.count, 3);
        let f = fireworks(&rocket);
        assert_eq!((f.flight, f.explosions.len()), (2, 2));
        assert_eq!(f.tooltip_lines(), ["Flight Duration: 2", "2 x Small Ball", "  Red"]);
        // The plain rocket is the item default: no payload.
        let plain = rocket_recipe(&[s(Item::PAPER), s(Item::GUNPOWDER)]).unwrap();
        assert_eq!(plain, Stack::new(FIREWORK_ROCKET, 3));
        assert!(
            rocket_recipe(&[
                s(Item::PAPER),
                s(Item::GUNPOWDER),
                s(Item::GUNPOWDER),
                s(Item::GUNPOWDER),
                s(Item::GUNPOWDER)
            ])
            .is_none()
        );
        assert!(rocket_recipe(&[s(Item::PAPER), s(Item::PAPER), s(Item::GUNPOWDER)]).is_none());
        assert!(rocket_recipe(&[s(Item::PAPER), red]).is_none());
    }

    #[test]
    fn fading_replaces_fade_colours_and_keeps_the_rest() {
        let star = star_recipe(&[s(Item::GUNPOWDER), s(DyeColor::Lime.dye()), s(Item::DIAMOND)]).unwrap();
        let faded = fade_recipe(&[star, s(DyeColor::White.dye()), s(DyeColor::Cyan.dye())]).unwrap();
        let e = explosion(&faded).unwrap();
        assert_eq!(
            (e.colors.as_slice(), e.fade_colors.as_slice(), e.trail),
            (&[0x41CD34][..], &[0xF0F0F0, 0x287697][..], true)
        );
        assert_eq!(tooltip_lines(&faded), ["Small Ball", "Lime", "Fade to White, Cyan", "Trail"]);
        assert!(fade_recipe(&[star]).is_none());
        assert!(fade_recipe(&[star, star, s(DyeColor::Red.dye())]).is_none());
    }

    #[test]
    fn payloads_round_trip_and_custom_colours_name_custom() {
        let e = Explosion {
            shape: Shape::Creeper,
            colors: vec![0x123456],
            fade_colors: vec![],
            trail: false,
            twinkle: true,
        };
        let rocket = rocket(Fireworks { flight: 3, explosions: vec![e.clone()] }, 1);
        let text = crate::inventory::stack_to_string(Some(rocket));
        assert_eq!(crate::inventory::stack_from_str(&text), Some(Some(rocket)));
        assert_eq!(e.detail_lines(), ["Custom", "Twinkle"]);
        assert_eq!(star_tint(&star(e)), Some([0x12, 0x34, 0x56]));
        assert_eq!(star_tint(&Stack::new(FIREWORK_STAR, 1)), Some(DEFAULT_STAR_TINT));
    }
}
