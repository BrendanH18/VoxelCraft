//! Items: anything that can sit in an inventory slot.
//!
//! Ids below 256 are the block with the same id, so a `Block` converts to an
//! `Item` for free and old saves (which stored block ids) still load. Ids from
//! [`FIRST_ITEM`] up are tools, materials and food, described by a static
//! table like the block registry. Extended block items use `BLOCK_ITEM_BASE +
//! state_id`, keeping the established materials/tools/potions IDs intact.

use crate::world::block::{Block, RenderKind, tex};

#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug, Default)]
#[repr(transparent)]
pub struct Item(pub u16);

/// First id that isn't a block.
pub const FIRST_ITEM: u16 = 256;
/// Extended block items occupy a disjoint range, preserving every existing item ID.
pub const BLOCK_ITEM_BASE: u16 = 0x8000;
const _: () = assert!(BLOCK_ITEM_BASE as usize + crate::world::block::STATE_CAPACITY <= u16::MAX as usize + 1);

#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum ToolKind {
    Pickaxe,
    Shovel,
    Axe,
    Hoe,
    Sword,
}

/// Tool material, from worst to best harvest level (gold is fast but weak).
/// Netherite only comes from upgrading diamond gear at a smithing table.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum Tier {
    Wood,
    Stone,
    Iron,
    Gold,
    Diamond,
    Netherite,
}

impl Tier {
    pub const ALL: [Tier; 6] = [Tier::Wood, Tier::Stone, Tier::Iron, Tier::Gold, Tier::Diamond, Tier::Netherite];

    /// Which blocks the tier can harvest: 0 wood/gold, 1 stone, 2 iron, 3
    /// diamond, 4 Netherite (everything diamond can).
    pub fn level(self) -> u8 {
        match self {
            Tier::Wood | Tier::Gold => 0,
            Tier::Stone => 1,
            Tier::Iron => 2,
            Tier::Diamond => 3,
            Tier::Netherite => 4,
        }
    }

    /// Mining speed multiplier on blocks the tool is suited to.
    pub fn speed(self) -> f32 {
        match self {
            Tier::Wood => 2.0,
            Tier::Stone => 4.0,
            Tier::Iron => 6.0,
            Tier::Diamond => 8.0,
            Tier::Netherite => 9.0,
            Tier::Gold => 12.0,
        }
    }

    /// Uses before the tool breaks.
    pub fn durability(self) -> u16 {
        match self {
            Tier::Wood => 59,
            Tier::Stone => 131,
            Tier::Iron => 250,
            Tier::Diamond => 1561,
            Tier::Netherite => 2031,
            Tier::Gold => 32,
        }
    }
}

/// Where a piece of armor is worn; also its armor slot index.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum ArmorPiece {
    Helmet,
    Chestplate,
    Leggings,
    Boots,
}

impl ArmorPiece {
    pub const ALL: [ArmorPiece; 4] =
        [ArmorPiece::Helmet, ArmorPiece::Chestplate, ArmorPiece::Leggings, ArmorPiece::Boots];
}

#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum ArmorMaterial {
    Leather,
    Iron,
    Gold,
    Diamond,
    Netherite,
}

impl ArmorMaterial {
    pub const ALL: [ArmorMaterial; 5] = [
        ArmorMaterial::Leather,
        ArmorMaterial::Iron,
        ArmorMaterial::Gold,
        ArmorMaterial::Diamond,
        ArmorMaterial::Netherite,
    ];

    /// Armor points (half chestplates on the HUD) per piece, as in Minecraft.
    pub fn defense(self, piece: ArmorPiece) -> u8 {
        let points = match self {
            ArmorMaterial::Leather => [1, 3, 2, 1],
            ArmorMaterial::Gold => [2, 5, 3, 1],
            ArmorMaterial::Iron => [2, 6, 5, 2],
            ArmorMaterial::Diamond | ArmorMaterial::Netherite => [3, 8, 6, 3],
        };
        points[piece as usize]
    }

    /// Hits a piece takes before breaking.
    pub fn durability(self, piece: ArmorPiece) -> u16 {
        let base = match self {
            ArmorMaterial::Leather => 5,
            ArmorMaterial::Gold => 7,
            ArmorMaterial::Iron => 15,
            ArmorMaterial::Diamond => 33,
            ArmorMaterial::Netherite => 37,
        };
        base * [11, 16, 15, 13][piece as usize]
    }

    /// Armor toughness per piece: lets armor hold up against big hits (see
    /// `survival::armor_reduce`).
    pub fn toughness(self) -> f32 {
        match self {
            ArmorMaterial::Diamond => 2.0,
            ArmorMaterial::Netherite => 3.0,
            _ => 0.0,
        }
    }

    /// Share of knockback each piece shrugs off.
    pub fn knockback_resistance(self) -> f32 {
        if self == ArmorMaterial::Netherite { 0.1 } else { 0.0 }
    }
}

#[derive(Clone, Copy, PartialEq, Debug)]
pub enum ItemKind {
    /// Places this block.
    Block(Block),
    Tool(ToolKind, Tier),
    Armor(ArmorPiece, ArmorMaterial),
    /// Restores `hunger` half-drumsticks and `saturation` points when eaten.
    Food {
        hunger: u8,
        saturation: f32,
    },
    /// Draw by holding right-click and release to shoot an arrow.
    Bow,
    Shears,
    /// Lights a Nether portal frame.
    FlintAndSteel,
    /// Casts a bobber and reels it back in.
    FishingRod,
    /// Drunk like food is eaten (see `crate::potion`).
    Potion(crate::potion::Potion),
    /// Crafting ingredient or mob drop with no use of its own.
    Material,
}

/// How an item's inventory icon is drawn (see `render::item_sprites`).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Sprite {
    Stick,
    Shears,
    Bowl(Option<[u8; 3]>),
    /// A white egg.
    Egg,
    /// A compass face; the needle direction is a separate texture frame.
    Compass,
    /// A clock face; the sun's position is a separate texture frame.
    Clock,
    /// An orange carrot.
    Carrot,
    /// A brown potato. `baked` is darker; `poison` is greenish.
    Potato {
        baked: bool,
        poison: bool,
    },
    /// A pumpkin pie.
    Pie,
    /// A slice of cake, as an item icon.
    Cake,
    /// A fishing rod: a stick with a line.
    FishingRod,
    /// A cod or salmon, raw or cooked.
    Fish {
        salmon: bool,
        cooked: bool,
    },
    Lump([u8; 3]),
    Ingot([u8; 3]),
    Gem([u8; 3]),
    Apple,
    Bread,
    /// Raw or cooked cut of meat: flesh colour and fat colour.
    Meat([u8; 3], [u8; 3]),
    Drumstick([u8; 3]),
    Bone,
    String,
    Feather,
    Powder([u8; 3]),
    Leather,
    Arrow,
    Seeds,
    Wheat,
    MelonSlice,
    Bed,
    ColoredBed([u8; 3]),
    Bow,
    FlintAndSteel,
    Nugget([u8; 3]),
    /// An iron bucket, empty or holding this colour of fluid.
    Bucket(Option<[u8; 3]>),
    Door,
    /// A blaze rod: a glowing yellow stick.
    Rod([u8; 3]),
    /// A glossy sphere (ender pearl): body colour and highlight.
    Pearl([u8; 3], [u8; 3]),
    /// A knobbly red nether wart.
    Wart,
    /// A glossy red spider eye.
    SpiderEye,
    /// A melon slice with a gold rind and sparkles.
    GlisteringMelon,
    Paper,
    /// A book with this cover colour.
    Book([u8; 3]),
    /// An ender pearl turned green with a slit pupil.
    EnderEye,
    /// A smithing template: a chipped tablet with a gem set in it.
    Template,
    /// A glass bottle, empty or holding liquid of this colour.
    Bottle(Option<[u8; 3]>),
    /// A wide flask for a splash potion, with a gunpowder-grey rim.
    SplashBottle([u8; 3]),
    Tool(ToolKind, Tier),
    Armor(ArmorPiece, ArmorMaterial),
}

#[derive(Clone, Copy, Debug)]
pub struct ItemInfo {
    pub name: &'static str,
    pub kind: ItemKind,
    pub max_stack: u8,
    pub sprite: Sprite,
}

const fn item(name: &'static str, sprite: Sprite) -> ItemInfo {
    ItemInfo { name, kind: ItemKind::Material, max_stack: 64, sprite }
}

const fn food(name: &'static str, hunger: u8, saturation: f32, sprite: Sprite) -> ItemInfo {
    ItemInfo { name, kind: ItemKind::Food { hunger, saturation }, max_stack: 64, sprite }
}

const fn netherite_tool(name: &'static str, kind: ToolKind) -> ItemInfo {
    ItemInfo {
        name,
        kind: ItemKind::Tool(kind, Tier::Netherite),
        max_stack: 1,
        sprite: Sprite::Tool(kind, Tier::Netherite),
    }
}

const fn netherite_armor(name: &'static str, piece: ArmorPiece) -> ItemInfo {
    let (kind, sprite) =
        (ItemKind::Armor(piece, ArmorMaterial::Netherite), Sprite::Armor(piece, ArmorMaterial::Netherite));
    ItemInfo { name, kind, max_stack: 1, sprite }
}

const RAW_MEAT: [u8; 3] = [226, 110, 110];
const COOKED_MEAT: [u8; 3] = [150, 88, 52];
const FAT: [u8; 3] = [250, 225, 215];
const COOKED_FAT: [u8; 3] = [215, 180, 130];

/// Non-block items, in id order from [`FIRST_ITEM`]. Append only: ids are
/// stored in saves.
static ITEMS: [ItemInfo; 64] = [
    item("stick", Sprite::Stick),
    item("coal", Sprite::Lump([45, 45, 48])),
    item("charcoal", Sprite::Lump([70, 58, 44])),
    item("iron ingot", Sprite::Ingot([216, 216, 216])),
    item("gold ingot", Sprite::Ingot([250, 212, 60])),
    item("diamond", Sprite::Gem([80, 230, 220])),
    item("flint", Sprite::Gem([70, 70, 74])),
    food("apple", 4, 2.4, Sprite::Apple),
    food("bread", 5, 6.0, Sprite::Bread),
    food("raw porkchop", 3, 1.8, Sprite::Meat(RAW_MEAT, FAT)),
    food("cooked porkchop", 8, 12.8, Sprite::Meat(COOKED_MEAT, COOKED_FAT)),
    food("raw beef", 3, 1.8, Sprite::Meat([190, 50, 45], [240, 200, 200])),
    food("steak", 8, 12.8, Sprite::Meat([110, 62, 36], [170, 120, 80])),
    food("raw chicken", 2, 1.2, Sprite::Drumstick([240, 190, 170])),
    food("cooked chicken", 6, 7.2, Sprite::Drumstick([200, 130, 60])),
    food("rotten flesh", 4, 0.8, Sprite::Meat([120, 140, 70], [150, 110, 80])),
    item("bone", Sprite::Bone),
    item("string", Sprite::String),
    item("feather", Sprite::Feather),
    item("gunpowder", Sprite::Powder([120, 120, 120])),
    item("leather", Sprite::Leather),
    item("arrow", Sprite::Arrow),
    item("wheat seeds", Sprite::Seeds),
    item("wheat", Sprite::Wheat),
    item("bone meal", Sprite::Powder([238, 236, 226])),
    item("clay ball", Sprite::Lump([150, 156, 172])),
    item("brick", Sprite::Ingot([178, 92, 66])),
    food("melon slice", 2, 1.2, Sprite::MelonSlice),
    ItemInfo { name: "bed", kind: ItemKind::Material, max_stack: 1, sprite: Sprite::Bed },
    ItemInfo { name: "bow", kind: ItemKind::Bow, max_stack: 1, sprite: Sprite::Bow },
    ItemInfo { name: "flint and steel", kind: ItemKind::FlintAndSteel, max_stack: 1, sprite: Sprite::FlintAndSteel },
    item("nether quartz", Sprite::Gem([236, 230, 222])),
    item("nether brick", Sprite::Ingot([86, 40, 46])),
    item("glowstone dust", Sprite::Powder([250, 214, 110])),
    item("gold nugget", Sprite::Nugget([250, 212, 60])),
    ItemInfo { name: "bucket", kind: ItemKind::Material, max_stack: 16, sprite: Sprite::Bucket(None) },
    ItemInfo {
        name: "water bucket",
        kind: ItemKind::Material,
        max_stack: 1,
        sprite: Sprite::Bucket(Some([50, 90, 220])),
    },
    ItemInfo {
        name: "lava bucket",
        kind: ItemKind::Material,
        max_stack: 1,
        sprite: Sprite::Bucket(Some([230, 110, 20])),
    },
    item("oak door", Sprite::Door),
    ItemInfo {
        name: "ender pearl",
        kind: ItemKind::Material,
        max_stack: 16,
        sprite: Sprite::Pearl([20, 92, 80], [120, 220, 190]),
    },
    item("blaze rod", Sprite::Rod([250, 190, 40])),
    item("blaze powder", Sprite::Powder([250, 150, 30])),
    item("nether wart", Sprite::Wart),
    item("glass bottle", Sprite::Bottle(None)),
    item("sugar", Sprite::Powder([246, 246, 250])),
    item("glistering melon slice", Sprite::GlisteringMelon),
    food("spider eye", 2, 3.2, Sprite::SpiderEye),
    item("paper", Sprite::Paper),
    item("book", Sprite::Book([120, 66, 40])),
    ItemInfo { name: "eye of ender", kind: ItemKind::Material, max_stack: 64, sprite: Sprite::EnderEye },
    ItemInfo { name: "enchanted book", kind: ItemKind::Material, max_stack: 1, sprite: Sprite::Book([112, 44, 110]) },
    item("lapis lazuli", Sprite::Gem([38, 76, 190])),
    item("netherite scrap", Sprite::Lump([112, 78, 63])),
    item("netherite ingot", Sprite::Ingot([76, 67, 70])),
    item("netherite upgrade smithing template", Sprite::Template),
    // Netherite gear came after the other tiers' id blocks (see `Item::tool`).
    netherite_tool("netherite pickaxe", ToolKind::Pickaxe),
    netherite_tool("netherite shovel", ToolKind::Shovel),
    netherite_tool("netherite axe", ToolKind::Axe),
    netherite_tool("netherite hoe", ToolKind::Hoe),
    netherite_tool("netherite sword", ToolKind::Sword),
    netherite_armor("netherite helmet", ArmorPiece::Helmet),
    netherite_armor("netherite chestplate", ArmorPiece::Chestplate),
    netherite_armor("netherite leggings", ArmorPiece::Leggings),
    netherite_armor("netherite boots", ArmorPiece::Boots),
];

/// Non-block items in the materials id range, after the saved tool and
/// armor blocks. Append only within 361..=399.
static EXTRA_ITEMS: [ItemInfo; 6] = [
    item("raw iron", Sprite::Lump([216, 164, 122])),
    item("raw gold", Sprite::Lump([246, 196, 70])),
    item("raw copper", Sprite::Lump([196, 112, 72])),
    item("copper ingot", Sprite::Ingot([200, 118, 86])),
    item("redstone", Sprite::Powder([170, 24, 20])),
    item("emerald", Sprite::Gem([22, 186, 82])),
];

static MOB_ITEMS: [ItemInfo; 2] = [
    item("ghast tear", Sprite::Lump([214, 236, 220])),
    item("wither skeleton skull", Sprite::Pearl([34, 34, 38], [118, 118, 124])),
];
const MOB_ITEM: u16 = 640;
/// Splash potions: `SPLASH_POTION + potion index`.
const SPLASH_POTION: u16 = 436;
const _: () = assert!(FIRST_POTION + POTION_COUNT <= SPLASH_POTION);
const _: () = assert!(SPLASH_POTION + POTION_COUNT <= SURVIVAL_ITEM);

/// Uses before a bow breaks.
pub const BOW_DURABILITY: u16 = 384;
/// Uses before a flint and steel breaks.
pub const FLINT_AND_STEEL_DURABILITY: u16 = 64;

/// Tools of the first five tiers start at this id: `FIRST_TOOL + tier * 5
/// + kind`. Netherite tools are `NETHERITE_TOOLS + kind`.
const FIRST_TOOL: u16 = 320;
// Everyday survival items occupy a separate append-only range; 400..512 is reserved for potions.
const SURVIVAL_ITEM: u16 = 512;
const SURVIVAL_ITEMS: &[ItemInfo] = &[
    ItemInfo { name: "shears", kind: ItemKind::Shears, max_stack: 1, sprite: Sprite::Shears },
    ItemInfo {
        name: "milk bucket",
        kind: ItemKind::Material,
        max_stack: 1,
        sprite: Sprite::Bucket(Some([242, 241, 225])),
    },
    item("bowl", Sprite::Bowl(None)),
    ItemInfo {
        name: "mushroom stew",
        kind: ItemKind::Food { hunger: 6, saturation: 7.2 },
        max_stack: 1,
        sprite: Sprite::Bowl(Some([155, 113, 66])),
    },
    ItemInfo {
        name: "snowball",
        kind: ItemKind::Material,
        max_stack: 16,
        sprite: Sprite::Pearl([245, 245, 250], [255, 255, 255]),
    },
    ItemInfo { name: "egg", kind: ItemKind::Material, max_stack: 16, sprite: Sprite::Egg },
    ItemInfo { name: "compass", kind: ItemKind::Material, max_stack: 64, sprite: Sprite::Compass },
    ItemInfo { name: "clock", kind: ItemKind::Material, max_stack: 64, sprite: Sprite::Clock },
    ItemInfo {
        name: "carrot",
        kind: ItemKind::Food { hunger: 3, saturation: 3.6 },
        max_stack: 64,
        sprite: Sprite::Carrot,
    },
    ItemInfo {
        name: "potato",
        kind: ItemKind::Food { hunger: 1, saturation: 0.6 },
        max_stack: 64,
        sprite: Sprite::Potato { baked: false, poison: false },
    },
    ItemInfo {
        name: "baked potato",
        kind: ItemKind::Food { hunger: 5, saturation: 6.0 },
        max_stack: 64,
        sprite: Sprite::Potato { baked: true, poison: false },
    },
    ItemInfo {
        name: "poisonous potato",
        kind: ItemKind::Food { hunger: 2, saturation: 1.2 },
        max_stack: 64,
        sprite: Sprite::Potato { baked: false, poison: true },
    },
    ItemInfo { name: "cake", kind: ItemKind::Material, max_stack: 1, sprite: Sprite::Cake },
    ItemInfo {
        name: "pumpkin pie",
        kind: ItemKind::Food { hunger: 8, saturation: 4.8 },
        max_stack: 64,
        sprite: Sprite::Pie,
    },
    ItemInfo { name: "fishing rod", kind: ItemKind::FishingRod, max_stack: 1, sprite: Sprite::FishingRod },
    food("cod", 2, 0.4, Sprite::Fish { salmon: false, cooked: false }),
    food("cooked cod", 5, 6.0, Sprite::Fish { salmon: false, cooked: true }),
    food("salmon", 2, 0.4, Sprite::Fish { salmon: true, cooked: false }),
    food("cooked salmon", 6, 9.6, Sprite::Fish { salmon: true, cooked: true }),
];

const TOOL_KINDS: [ToolKind; 5] = [ToolKind::Pickaxe, ToolKind::Shovel, ToolKind::Axe, ToolKind::Hoe, ToolKind::Sword];
/// Tiers in the `FIRST_TOOL` block (all but Netherite).
const ID_TIERS: usize = 5;
const TOOL_COUNT: u16 = (ID_TIERS * TOOL_KINDS.len()) as u16;
/// Armor of the first four materials starts at this id: `FIRST_ARMOR +
/// material * 4 + piece`. Netherite armor is `NETHERITE_ARMOR + piece`.
const FIRST_ARMOR: u16 = FIRST_TOOL + TOOL_COUNT;
const ARMOR_COUNT: u16 = 16;
const NETHERITE_TOOLS: u16 = 311;
const NETHERITE_ARMOR: u16 = NETHERITE_TOOLS + TOOL_KINDS.len() as u16;
/// Potions start at this id: `FIRST_POTION + potion index`.
const FIRST_POTION: u16 = 400;
/// Raw metals and gems. 360 is diamond boots (`FIRST_ARMOR` ends there),
/// so these start at 361.
const EXTRA_ITEM: u16 = 361;
const POTION_COUNT: u16 = crate::potion::Potion::COUNT as u16;

impl Item {
    pub const MILK_BUCKET: Item = Item(513);
    pub const BOWL: Item = Item(514);
    pub const MUSHROOM_STEW: Item = Item(515);
    pub const SNOWBALL: Item = Item(516);
    pub const EGG: Item = Item(517);
    pub const COMPASS: Item = Item(518);
    pub const CLOCK: Item = Item(519);
    pub const CARROT: Item = Item(520);
    pub const POTATO: Item = Item(521);
    pub const BAKED_POTATO: Item = Item(522);
    pub const POISONOUS_POTATO: Item = Item(523);
    pub const CAKE: Item = Item(524);
    pub const PUMPKIN_PIE: Item = Item(525);
    pub const FISHING_ROD: Item = Item(526);
    pub const COD: Item = Item(527);
    pub const COOKED_COD: Item = Item(528);
    pub const SALMON: Item = Item(529);
    pub const COOKED_SALMON: Item = Item(530);
    pub const SHEARS: Item = Item(512);

    pub const STICK: Item = Item(256);
    pub const COAL: Item = Item(257);
    pub const CHARCOAL: Item = Item(258);
    pub const IRON_INGOT: Item = Item(259);
    pub const GOLD_INGOT: Item = Item(260);
    pub const DIAMOND: Item = Item(261);
    pub const FLINT: Item = Item(262);
    pub const APPLE: Item = Item(263);
    pub const BREAD: Item = Item(264);
    pub const RAW_PORKCHOP: Item = Item(265);
    pub const COOKED_PORKCHOP: Item = Item(266);
    pub const RAW_BEEF: Item = Item(267);
    pub const STEAK: Item = Item(268);
    pub const RAW_CHICKEN: Item = Item(269);
    pub const COOKED_CHICKEN: Item = Item(270);
    pub const ROTTEN_FLESH: Item = Item(271);
    pub const BONE: Item = Item(272);
    pub const STRING: Item = Item(273);
    pub const FEATHER: Item = Item(274);
    pub const GUNPOWDER: Item = Item(275);
    pub const LEATHER: Item = Item(276);
    pub const ARROW: Item = Item(277);
    pub const WHEAT_SEEDS: Item = Item(278);
    pub const WHEAT: Item = Item(279);
    pub const BONE_MEAL: Item = Item(280);
    pub const CLAY_BALL: Item = Item(281);
    pub const BRICK: Item = Item(282);
    pub const MELON_SLICE: Item = Item(283);
    pub const BED: Item = Item(284);
    pub const BOW: Item = Item(285);
    pub const FLINT_AND_STEEL: Item = Item(286);
    pub const NETHER_QUARTZ: Item = Item(287);
    pub const NETHER_BRICK: Item = Item(288);
    pub const GLOWSTONE_DUST: Item = Item(289);
    pub const GOLD_NUGGET: Item = Item(290);
    pub const MAGMA_CREAM: Item = Item(608);
    pub const IRON_NUGGET: Item = Item(609);
    pub const SLIME_BALL: Item = Item(610);
    pub const BUCKET: Item = Item(291);
    pub const WATER_BUCKET: Item = Item(292);
    pub const LAVA_BUCKET: Item = Item(293);
    /// Places both halves of a door (see `Block::door`).
    pub const OAK_DOOR: Item = Item(294);
    /// Thrown to teleport where it lands (see `entity::pearl`).
    pub const ENDER_PEARL: Item = Item(295);
    pub const BLAZE_ROD: Item = Item(296);
    pub const BLAZE_POWDER: Item = Item(297);
    /// Planted on soul sand (see `Block::nether_wart`).
    pub const NETHER_WART: Item = Item(298);
    /// Filled with water from a source (see `Item::potion`).
    pub const GLASS_BOTTLE: Item = Item(299);
    /// Brewing ingredients: swiftness, healing and poison.
    pub const SUGAR: Item = Item(300);
    pub const GLISTERING_MELON_SLICE: Item = Item(301);
    pub const SPIDER_EYE: Item = Item(302);
    pub const PAPER: Item = Item(303);
    pub const BOOK: Item = Item(304);
    /// Thrown, it flies toward the nearest stronghold; set in an End
    /// portal frame, it helps open the portal.
    pub const EYE_OF_ENDER: Item = Item(305);
    /// Stores enchantments for an anvil to put on gear.
    pub const ENCHANTED_BOOK: Item = Item(306);
    /// Pays for enchanting (one to three per enchantment).
    pub const LAPIS_LAZULI: Item = Item(307);
    pub const NETHERITE_SCRAP: Item = Item(308);
    pub const NETHERITE_INGOT: Item = Item(309);
    /// Upgrades diamond gear to Netherite at a smithing table.
    pub const NETHERITE_UPGRADE: Item = Item(310);
    pub const RAW_IRON: Item = Item(361);
    pub const RAW_GOLD: Item = Item(362);
    pub const RAW_COPPER: Item = Item(363);
    pub const COPPER_INGOT: Item = Item(364);
    pub const REDSTONE: Item = Item(365);
    pub const EMERALD: Item = Item(366);
    pub const GHAST_TEAR: Item = Item(640);
    pub const WITHER_SKULL: Item = Item(641);

    pub const fn tool(kind: ToolKind, tier: Tier) -> Item {
        match tier {
            Tier::Netherite => Item(NETHERITE_TOOLS + kind as u16),
            _ => Item(FIRST_TOOL + tier as u16 * 5 + kind as u16),
        }
    }

    pub const fn armor(piece: ArmorPiece, material: ArmorMaterial) -> Item {
        match material {
            ArmorMaterial::Netherite => Item(NETHERITE_ARMOR + piece as u16),
            _ => Item(FIRST_ARMOR + material as u16 * 4 + piece as u16),
        }
    }

    pub const fn potion(potion: crate::potion::Potion) -> Item {
        Item(FIRST_POTION + potion.0 as u16)
    }

    pub const fn splash_potion(potion: crate::potion::Potion) -> Item {
        Item(SPLASH_POTION + potion.0 as u16)
    }

    /// The potion of a throwable splash potion item.
    pub fn as_splash_potion(self) -> Option<crate::potion::Potion> {
        let i = self.0.checked_sub(SPLASH_POTION).filter(|&i| i < POTION_COUNT)?;
        Some(crate::potion::Potion(i as u8))
    }

    /// The potion this item is (a water bottle is one too).
    pub fn as_potion(self) -> Option<crate::potion::Potion> {
        let i = self.0.checked_sub(FIRST_POTION).filter(|&i| i < POTION_COUNT)?;
        Some(crate::potion::Potion(i as u8))
    }

    pub const fn from_block(block: Block) -> Item {
        Item(if block.0 < FIRST_ITEM { block.0 } else { BLOCK_ITEM_BASE + block.0 })
    }

    /// The block placing this item puts down: the block itself, or what a
    /// non-block item plants (seeds sow wheat).
    pub fn places(self) -> Option<Block> {
        match self {
            Item::REDSTONE => Some(crate::world::redstone_blocks::WIRE),
            Item::WHEAT_SEEDS => Some(Block::wheat(0)),
            Item::CARROT => Some(Block::crop(crate::world::block::Crop::Carrot, 0)),
            Item::POTATO => Some(Block::crop(crate::world::block::Crop::Potato, 0)),
            Item::CAKE => Some(Block::cake(0)),
            Item::NETHER_WART => Some(Block::nether_wart(0)),
            i => i.block(),
        }
    }

    /// The block this item is, if any.
    pub fn block(self) -> Option<Block> {
        let id = if self.0 < FIRST_ITEM {
            Some(self.0)
        } else {
            self.0
                .checked_sub(BLOCK_ITEM_BASE)
                .filter(|&id| id >= FIRST_ITEM && (id as usize) < crate::world::block::STATE_CAPACITY)
        };
        if let Some(id) = id {
            let b = Block(id);
            (b.kind() != RenderKind::Invisible).then_some(b)
        } else {
            None
        }
    }

    pub fn dye_color(self) -> Option<crate::color::DyeColor> {
        self.0.checked_sub(576).filter(|&i| i < 16).map(|i| crate::color::DyeColor::ALL[i as usize])
    }

    pub fn bed_color(self) -> Option<crate::color::DyeColor> {
        if self == Self::BED {
            Some(crate::color::DyeColor::Red)
        } else {
            self.0
                .checked_sub(592)
                .filter(|&i| i < 15)
                .map(|i| crate::color::DyeColor::ALL[if i == 14 { 15 } else { i } as usize])
        }
    }

    pub fn info(self) -> ItemInfo {
        match self {
            Self::MAGMA_CREAM => return item("magma cream", Sprite::Lump([242, 115, 30])),
            Self::IRON_NUGGET => return item("iron nugget", Sprite::Nugget([202, 206, 212])),
            Self::SLIME_BALL => return item("slimeball", Sprite::Lump([104, 180, 83])),
            _ => {}
        }
        if let Some(c) = self.bed_color() {
            return ItemInfo {
                name: if self == Self::BED { "bed" } else { c.bed_name() },
                kind: ItemKind::Material,
                max_stack: 1,
                sprite: Sprite::ColoredBed(c.rgb()),
            };
        }
        if let Some(c) = self.dye_color() {
            return item(c.dye_name(), Sprite::Powder(c.rgb()));
        }
        if let Some(b) = self.block() {
            return ItemInfo { name: b.name(), kind: ItemKind::Block(b), max_stack: 64, sprite: Sprite::Stick };
        }
        if let Some(info) = self.0.checked_sub(FIRST_ITEM).and_then(|i| ITEMS.get(i as usize)) {
            return *info;
        }
        if let Some(potion) = self.as_potion() {
            return ItemInfo {
                name: potion.info().name,
                kind: ItemKind::Potion(potion),
                max_stack: 1,
                sprite: Sprite::Bottle(Some(potion.colour())),
            };
        }
        if let Some(potion) = self.as_splash_potion() {
            return ItemInfo {
                name: splash_names()[potion.0 as usize],
                kind: ItemKind::Material,
                max_stack: 1,
                sprite: Sprite::SplashBottle(potion.colour()),
            };
        }
        if let Some(i) = self.0.checked_sub(FIRST_TOOL).filter(|&i| i < TOOL_COUNT) {
            let (tier, kind) = (Tier::ALL[i as usize / 5], TOOL_KINDS[i as usize % 5]);
            return ItemInfo {
                name: tool_name(kind, tier),
                kind: ItemKind::Tool(kind, tier),
                max_stack: 1,
                sprite: Sprite::Tool(kind, tier),
            };
        }
        if let Some(i) = self.0.checked_sub(FIRST_ARMOR).filter(|&i| i < ARMOR_COUNT) {
            let (material, piece) = (ArmorMaterial::ALL[i as usize / 4], ArmorPiece::ALL[i as usize % 4]);
            return ItemInfo {
                name: armor_name(piece, material),
                kind: ItemKind::Armor(piece, material),
                max_stack: 1,
                sprite: Sprite::Armor(piece, material),
            };
        }
        if let Some(info) = self.0.checked_sub(EXTRA_ITEM).and_then(|i| EXTRA_ITEMS.get(i as usize)) {
            return *info;
        }
        if let Some(info) = self.0.checked_sub(SURVIVAL_ITEM).and_then(|i| SURVIVAL_ITEMS.get(i as usize)) {
            return *info;
        }
        // Saves from the integration branch stored shears at 607.
        if self.0 == 607 {
            return SURVIVAL_ITEMS[0];
        }
        if let Some(info) = self.0.checked_sub(MOB_ITEM).and_then(|i| MOB_ITEMS.get(i as usize)) {
            return *info;
        }
        ItemInfo { name: "unknown", kind: ItemKind::Material, max_stack: 64, sprite: Sprite::Stick }
    }

    /// Whether this id names a real block or item.
    pub fn is_valid(self) -> bool {
        self.info().name != "unknown"
    }

    pub fn name(self) -> &'static str {
        self.info().name
    }

    /// Dropped Netherite materials and gear survive fire and lava (but
    /// still despawn).
    pub fn fire_resistant(self) -> bool {
        matches!(self, Self::NETHERITE_SCRAP | Self::NETHERITE_INGOT)
            || matches!(self.block(), Some(Block::ANCIENT_DEBRIS | Block::NETHERITE_BLOCK))
            || matches!(self.as_tool(), Some((_, Tier::Netherite)))
            || matches!(self.as_armor(), Some((_, ArmorMaterial::Netherite)))
    }

    pub fn max_stack(self) -> u8 {
        self.info().max_stack
    }

    pub fn as_tool(self) -> Option<(ToolKind, Tier)> {
        match self.info().kind {
            ItemKind::Tool(kind, tier) => Some((kind, tier)),
            _ => None,
        }
    }

    pub fn as_armor(self) -> Option<(ArmorPiece, ArmorMaterial)> {
        match self.info().kind {
            ItemKind::Armor(piece, material) => Some((piece, material)),
            _ => None,
        }
    }

    /// Uses before breaking, for tools and armor.
    pub fn durability(self) -> Option<u16> {
        if self == Self::SHEARS {
            return Some(238);
        }
        match self.info().kind {
            ItemKind::Tool(_, tier) => Some(tier.durability()),
            ItemKind::Armor(piece, material) => Some(material.durability(piece)),
            ItemKind::Bow => Some(BOW_DURABILITY),
            ItemKind::Shears => Some(238),
            ItemKind::FishingRod => Some(64),
            ItemKind::FlintAndSteel => Some(FLINT_AND_STEEL_DURABILITY),
            _ => None,
        }
    }

    /// The status effect eating this gives (Java's spider eye: Poison I for
    /// 5 s).
    pub fn food_effect(self) -> Option<(crate::simulation::effects::Effect, u8, u32)> {
        match self {
            Item::SPIDER_EYE | Item::POISONOUS_POTATO => Some((crate::simulation::effects::Effect::Poison, 0, 100)),
            _ => None,
        }
    }

    /// Applies the food effect when `unit` (0..1) falls under its chance.
    /// A poisonous potato poisons 60% of the time; a spider eye always does.
    pub fn food_effect_roll(self, unit: f32) -> Option<(crate::simulation::effects::Effect, u8, u32)> {
        let effect = self.food_effect()?;
        let chance = if self == Item::POISONOUS_POTATO { 0.6 } else { 1.0 };
        (unit < chance).then_some(effect)
    }

    pub fn remainder(self) -> Option<Item> {
        match self {
            Self::MILK_BUCKET => Some(Self::BUCKET),
            Self::MUSHROOM_STEW => Some(Self::BOWL),
            _ => None,
        }
    }

    pub fn is_drink(self) -> bool {
        self.as_potion().is_some() || self == Self::MILK_BUCKET
    }

    /// Hunger and saturation restored, for food.
    pub fn food(self) -> Option<(u8, f32)> {
        match self.info().kind {
            ItemKind::Food { hunger, saturation } => Some((hunger, saturation)),
            _ => None,
        }
    }

    /// Texture layer of the flat inventory icon (non-block items only), in
    /// the item icon range from `tex::ITEM_BASE`.
    pub fn icon_layer(self) -> Option<u16> {
        sprite_index(self).map(tex::item_layer)
    }

    /// Looks an item or block up by name (spaces or underscores).
    /// Case-insensitive name search: underscores act as spaces and every query word must match.
    pub fn matches_query(self, query: &str) -> bool {
        let query = query.to_lowercase().replace('_', " ");
        let name = self.name().to_lowercase();
        query.split_whitespace().all(|word| name.contains(word))
    }

    pub fn from_name(name: &str) -> Option<Item> {
        if name.replace('_', " ") == "red bed" {
            return Some(Item::BED);
        }
        if let Some(b) = Block::from_name(name) {
            // Doors and nether wart are placed by an item, not as a block.
            return Some(match b {
                b if (149..=164).contains(&b.0) => Item::OAK_DOOR,
                b if b.wart_age().is_some() => Item::NETHER_WART,
                b if b.cake_bites().is_some() => Item::CAKE,
                b if let Some(c) = b.bed_color() => c.bed(),
                b => Item::from_block(b),
            });
        }
        let name = name.replace('_', " ");
        Item::all_items().find(|i| i.name() == name)
    }

    /// Every non-block item, in id order.
    pub fn all_items() -> impl Iterator<Item = Item> {
        let materials = (0..ITEMS.len() as u16).map(|i| Item(FIRST_ITEM + i));
        let tools = (0..TOOL_COUNT + ARMOR_COUNT).map(|i| Item(FIRST_TOOL + i));
        let potions = (0..POTION_COUNT).map(|i| Item(FIRST_POTION + i));
        let extra = (0..EXTRA_ITEMS.len() as u16).map(|i| Item(EXTRA_ITEM + i));
        materials
            .chain(tools)
            .chain(potions)
            .chain(extra)
            .chain((0..SURVIVAL_ITEMS.len() as u16).map(|i| Item(SURVIVAL_ITEM + i)))
            .chain((576..607).map(Item))
            .chain((608..611).map(Item))
            .chain((0..MOB_ITEMS.len() as u16).map(|i| Item(MOB_ITEM + i)))
            .chain((0..POTION_COUNT).map(|i| Item(SPLASH_POTION + i)))
    }

    /// Everything a creative player can pick from: blocks, then items.
    pub fn creative_palette() -> impl Iterator<Item = Item> {
        Block::creative_palette().map(Item::from_block).chain(Item::all_items())
    }
}

impl From<Block> for Item {
    fn from(b: Block) -> Self {
        Item::from_block(b)
    }
}

/// "splash potion of x" for each potion, built once.
fn splash_names() -> &'static [&'static str] {
    static NAMES: std::sync::OnceLock<Vec<&'static str>> = std::sync::OnceLock::new();
    NAMES.get_or_init(|| {
        crate::potion::Potion::all()
            .map(|p| {
                let name = p.info().name;
                let name =
                    if name == "water bottle" { "splash water bottle".to_string() } else { format!("splash {name}") };
                &*Box::leak(name.into_boxed_str())
            })
            .collect()
    })
}

/// Index of an item's icon among the item texture layers.
fn sprite_index(item: Item) -> Option<u16> {
    let materials = ITEMS.len() as u16;
    match item.0 {
        i if (FIRST_ITEM..FIRST_ITEM + materials).contains(&i) => Some(i - FIRST_ITEM),
        i if item.as_tool().is_some() || item.as_armor().is_some() => Some(materials + i - FIRST_TOOL),
        i if item.as_potion().is_some() => Some(materials + TOOL_COUNT + ARMOR_COUNT + i - FIRST_POTION),
        i if (EXTRA_ITEM..EXTRA_ITEM + EXTRA_ITEMS.len() as u16).contains(&i) => {
            Some(materials + TOOL_COUNT + ARMOR_COUNT + POTION_COUNT + i - EXTRA_ITEM)
        }
        i if (SURVIVAL_ITEM..SURVIVAL_ITEM + SURVIVAL_ITEMS.len() as u16).contains(&i) => {
            Some(materials + TOOL_COUNT + ARMOR_COUNT + POTION_COUNT + EXTRA_ITEMS.len() as u16 + i - SURVIVAL_ITEM)
        }
        // The integration branch stored shears at 607. They are item 512 now;
        // a saved 607 still draws the same icon.
        607 => Some(materials + TOOL_COUNT + ARMOR_COUNT + POTION_COUNT + EXTRA_ITEMS.len() as u16),
        // Dyes and colour items, then the bastion/mob materials, after the survival items.
        576..=606 | 608..=610 => {
            let base = materials + TOOL_COUNT + ARMOR_COUNT + POTION_COUNT + EXTRA_ITEMS.len() as u16;
            let past_shears = u16::from(item.0 > 607);
            Some(base + SURVIVAL_ITEMS.len() as u16 + item.0 - 576 - past_shears)
        }
        i if (MOB_ITEM..MOB_ITEM + MOB_ITEMS.len() as u16).contains(&i) => Some(
            materials
                + TOOL_COUNT
                + ARMOR_COUNT
                + POTION_COUNT
                + EXTRA_ITEMS.len() as u16
                + SURVIVAL_ITEMS.len() as u16
                + 34
                + i
                - MOB_ITEM,
        ),
        // Splash potions come last, after every mob item.
        i if item.as_splash_potion().is_some() => Some(
            materials
                + TOOL_COUNT
                + ARMOR_COUNT
                + POTION_COUNT
                + EXTRA_ITEMS.len() as u16
                + SURVIVAL_ITEMS.len() as u16
                + 34
                + MOB_ITEMS.len() as u16
                + i
                - SPLASH_POTION,
        ),
        _ => None,
    }
}

/// How many item icons there are (layers of the item texture array).
pub const fn icon_count() -> u32 {
    ITEMS.len() as u32
        + (TOOL_COUNT + ARMOR_COUNT + POTION_COUNT) as u32
        + EXTRA_ITEMS.len() as u32
        + SURVIVAL_ITEMS.len() as u32
        + 34
        + MOB_ITEMS.len() as u32
        + POTION_COUNT as u32
}

/// Compass needle frames, after the item icons. Frame 0 points up.
pub const COMPASS_FRAMES: u16 = 32;
/// Clock frames, after the compass. Frame 0 is noon.
pub const CLOCK_FRAMES: u16 = 64;

/// Extra icon layers for the compass and clock animations.
pub const fn animated_icons() -> u32 {
    COMPASS_FRAMES as u32 + CLOCK_FRAMES as u32
}

/// Where the player is facing, where the world spawn is, and what time it is.
/// Inventory and hand icons read this when they draw a compass or clock.
#[derive(Clone, Copy, Debug)]
pub struct Dial {
    pub yaw: f32,
    pub x: f64,
    pub z: f64,
    pub spawn_x: f64,
    pub spawn_z: f64,
    pub overworld: bool,
    /// 0 sunrise, 0.25 noon, 0.5 sunset, 0.75 midnight.
    pub day_time: f32,
    /// Seconds since the view started, so a lost compass can spin.
    pub spin: f32,
}

impl Dial {
    /// Java's compass angle, as one of 32 frames. Outside the overworld it spins.
    pub fn compass_frame(self) -> u16 {
        if !self.overworld {
            return (self.spin * 8.0).rem_euclid(COMPASS_FRAMES as f32) as u16 % COMPASS_FRAMES;
        }
        let bearing = (self.spawn_z - self.z).atan2(self.spawn_x - self.x) as f32;
        let relative = (self.yaw - bearing).rem_euclid(std::f32::consts::TAU);
        ((relative / std::f32::consts::TAU) * COMPASS_FRAMES as f32).round() as u16 % COMPASS_FRAMES
    }

    /// Java's celestial angle: frame 0 is noon. Outside the overworld it spins.
    pub fn clock_frame(self) -> u16 {
        if !self.overworld {
            return (self.spin * 8.0).rem_euclid(CLOCK_FRAMES as f32) as u16 % CLOCK_FRAMES;
        }
        let celestial = (self.day_time - 0.25).rem_euclid(1.0);
        (celestial * CLOCK_FRAMES as f32).round() as u16 % CLOCK_FRAMES
    }

    /// Same world and time, from another player's eyes.
    pub fn at(self, yaw: f32, x: f64, z: f64) -> Self {
        Self { yaw, x, z, ..self }
    }
}

impl Item {
    /// The animated layer for a compass or clock. Other items keep [`icon_layer`].
    pub fn dial_layer(self, dial: Dial) -> Option<u16> {
        let base = icon_count() as u16;
        if self == Self::COMPASS {
            Some(tex::item_layer(base + dial.compass_frame()))
        } else if self == Self::CLOCK {
            Some(tex::item_layer(base + COMPASS_FRAMES + dial.clock_frame()))
        } else {
            None
        }
    }
}

/// Layer of a status effect's icon: in the item icon array, after every
/// item and the compass and clock frames.
pub fn effect_icon_layer(effect: crate::simulation::effects::Effect) -> u16 {
    tex::item_layer(icon_count() as u16 + animated_icons() as u16 + effect as u16)
}

/// The sprite drawn on item icon `index` (see `tex::item_layer`).
pub fn sprite_for_layer(index: u16) -> Option<Sprite> {
    Item::all_items().nth(index as usize).map(|i| i.info().sprite)
}

fn tool_name(kind: ToolKind, tier: Tier) -> &'static str {
    const NAMES: [[&str; 5]; 5] = [
        ["wooden pickaxe", "wooden shovel", "wooden axe", "wooden hoe", "wooden sword"],
        ["stone pickaxe", "stone shovel", "stone axe", "stone hoe", "stone sword"],
        ["iron pickaxe", "iron shovel", "iron axe", "iron hoe", "iron sword"],
        ["golden pickaxe", "golden shovel", "golden axe", "golden hoe", "golden sword"],
        ["diamond pickaxe", "diamond shovel", "diamond axe", "diamond hoe", "diamond sword"],
    ];
    NAMES[tier as usize][kind as usize]
}

fn armor_name(piece: ArmorPiece, material: ArmorMaterial) -> &'static str {
    const NAMES: [[&str; 4]; 4] = [
        ["leather cap", "leather tunic", "leather pants", "leather boots"],
        ["iron helmet", "iron chestplate", "iron leggings", "iron boots"],
        ["golden helmet", "golden chestplate", "golden leggings", "golden boots"],
        ["diamond helmet", "diamond chestplate", "diamond leggings", "diamond boots"],
    ];
    NAMES[material as usize][piece as usize]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extended_block_items_do_not_collide_with_saved_item_ids() {
        for block in [Block::NETHERITE_BLOCK, Block(4093), Block(4094), Block(4095)] {
            let item = Item::from_block(block);
            assert_eq!(item.block(), Some(block));
            assert_eq!(Item::from_name(item.name()), Some(item));
        }
        assert_eq!(Item::from_block(Block::NETHERITE_BLOCK).0, 222);
        assert_eq!(Item::from_block(Block(256)).0, BLOCK_ITEM_BASE + 256);
        assert_eq!(Item(BLOCK_ITEM_BASE + 255).block(), None);
        assert_eq!(Item(BLOCK_ITEM_BASE + 4096).block(), None);
        assert_eq!(Item::STICK.0, FIRST_ITEM);
        assert_eq!(Item::STICK.block(), None);
    }

    #[test]
    fn armor_has_names_defense_and_durability() {
        let chest = Item::armor(ArmorPiece::Chestplate, ArmorMaterial::Iron);
        assert_eq!(chest.name(), "iron chestplate");
        assert_eq!(chest.as_armor(), Some((ArmorPiece::Chestplate, ArmorMaterial::Iron)));
        assert_eq!(chest.durability(), Some(240));
        assert_eq!(chest.max_stack(), 1);
        assert_eq!(Item::tool(ToolKind::Sword, Tier::Diamond).as_armor(), None);
        let full: u8 = ArmorPiece::ALL.iter().map(|&p| ArmorMaterial::Diamond.defense(p)).sum();
        assert_eq!(full, 20);
    }

    #[test]
    fn netherite_resources_have_icons_names_and_survive_stack_saves() {
        for item in
            [Block::ANCIENT_DEBRIS.into(), Block::NETHERITE_BLOCK.into(), Item::NETHERITE_SCRAP, Item::NETHERITE_INGOT]
        {
            assert_eq!(Item::from_name(&item.name().replace(' ', "_")), Some(item));
            assert!(Item::creative_palette().any(|i| i == item));
            let stack = Some(crate::inventory::Stack::new(item, 64));
            let saved = crate::inventory::stack_to_string(stack);
            assert_eq!(crate::inventory::stack_from_str(&saved), Some(stack));
            if item.block().is_none() {
                assert!(item.icon_layer().is_some());
            }
        }
    }

    #[test]
    fn netherite_gear_has_java_ids_stats_and_fire_resistance() {
        use crate::inventory::{Stack, stack_from_str, stack_to_string};
        assert_eq!(Item::NETHERITE_UPGRADE, Item(310));
        assert_eq!(Item::NETHERITE_UPGRADE.name(), "netherite upgrade smithing template");
        assert!(!Item::NETHERITE_UPGRADE.fire_resistant(), "Java's templates burn");
        assert_eq!(Item::tool(ToolKind::Pickaxe, Tier::Netherite), Item(311));
        assert_eq!(Item::tool(ToolKind::Sword, Tier::Netherite), Item(315));
        assert_eq!(Item::armor(ArmorPiece::Helmet, ArmorMaterial::Netherite), Item(316));
        assert_eq!(Item::armor(ArmorPiece::Boots, ArmorMaterial::Netherite), Item(319));
        // Every tier and material maps to its own id and back.
        let mut ids = std::collections::HashSet::new();
        for tier in Tier::ALL {
            for kind in TOOL_KINDS {
                let item = Item::tool(kind, tier);
                assert_eq!(item.as_tool(), Some((kind, tier)), "{item:?}");
                assert!(ids.insert(item));
            }
        }
        for material in ArmorMaterial::ALL {
            for piece in ArmorPiece::ALL {
                let item = Item::armor(piece, material);
                assert_eq!(item.as_armor(), Some((piece, material)), "{item:?}");
                assert!(ids.insert(item));
            }
        }
        let pick = Item::tool(ToolKind::Pickaxe, Tier::Netherite);
        assert_eq!((pick.name(), pick.durability(), pick.max_stack()), ("netherite pickaxe", Some(2031), 1));
        assert_eq!((Tier::Netherite.speed(), Tier::Netherite.level()), (9.0, 4));
        let durability: Vec<_> =
            ArmorPiece::ALL.map(|p| Item::armor(p, ArmorMaterial::Netherite).durability().unwrap()).into();
        assert_eq!(durability, [407, 592, 555, 481]);
        let full: u8 = ArmorPiece::ALL.iter().map(|&p| ArmorMaterial::Netherite.defense(p)).sum();
        assert_eq!(full, 20);
        assert_eq!((ArmorMaterial::Netherite.toughness(), ArmorMaterial::Diamond.toughness()), (3.0, 2.0));
        assert_eq!(ArmorMaterial::Netherite.knockback_resistance(), 0.1);
        assert_eq!(ArmorMaterial::Diamond.knockback_resistance(), 0.0);
        for item in Item::all_items().filter(|i| i.name().starts_with("netherite ") && *i != Item::NETHERITE_UPGRADE) {
            assert!(item.fire_resistant(), "{}", item.name());
        }
        assert!(!Item::tool(ToolKind::Sword, Tier::Diamond).fire_resistant());
        // Damage and enchantments survive a save.
        let enchants = crate::enchant::Enchants::NONE.with(crate::enchant::Enchantment::Sharpness, 5);
        let sword = Some(Stack {
            damage: 77,
            enchants,
            repair_cost: 7,
            ..Stack::new(Item::tool(ToolKind::Sword, Tier::Netherite), 1)
        });
        assert_eq!(stack_from_str(&stack_to_string(sword)), Some(sword));
    }

    #[test]
    fn blocks_are_items_with_the_same_id() {
        let i = Item::from(Block::COBBLESTONE);
        assert_eq!(i.block(), Some(Block::COBBLESTONE));
        assert_eq!(i.name(), "cobblestone");
        assert_eq!(Item::from(Block::AIR).block(), None);
        assert_eq!(Item::COAL.block(), None);
    }

    #[test]
    fn tools_have_names_tiers_and_durability() {
        let pick = Item::tool(ToolKind::Pickaxe, Tier::Iron);
        assert_eq!(pick.name(), "iron pickaxe");
        assert_eq!(pick.as_tool(), Some((ToolKind::Pickaxe, Tier::Iron)));
        assert_eq!(pick.durability(), Some(250));
        assert_eq!(pick.max_stack(), 1);
        assert_eq!(Item::tool(ToolKind::Sword, Tier::Gold).name(), "golden sword");
        assert_eq!(Item::STICK.as_tool(), None);
    }

    #[test]
    fn names_round_trip_and_ids_are_unique() {
        let all: Vec<Item> = Item::creative_palette().collect();
        for &i in &all {
            assert!(i.is_valid(), "{i:?}");
            assert_eq!(Item::from_name(i.name()), Some(i), "{}", i.name());
        }
        let names: std::collections::HashSet<_> = all.iter().map(|i| i.name()).collect();
        assert_eq!(names.len(), all.len());
        assert!(!Item(9999).is_valid());
    }

    #[test]
    fn icon_layers_fit_the_texture_array() {
        for i in Item::all_items() {
            let index = tex::item_index(i.icon_layer().unwrap()).unwrap();
            assert!((index as u32) < icon_count());
            assert_eq!(Item::all_items().count() as u32, icon_count());
            assert_eq!(sprite_for_layer(index), Some(i.info().sprite));
        }
        // Icons stay addressable by the UI's and models' 16-bit layers.
        assert!(tex::ITEM_BASE as u32 + icon_count() + animated_icons() + 32 <= u16::MAX as u32);
        assert_eq!(Item::from(Block::STONE).icon_layer(), None);
    }

    #[test]
    fn food_values() {
        assert_eq!(Item::COOKED_PORKCHOP.food(), Some((8, 12.8)));
        assert_eq!(Item::STICK.food(), None);
    }

    #[test]
    fn compass_points_at_spawn_and_clock_reads_noon() {
        let looking =
            Dial { yaw: 0.0, x: 0.0, z: 0.0, spawn_x: 10.0, spawn_z: 0.0, overworld: true, day_time: 0.25, spin: 0.0 };
        assert_eq!(looking.compass_frame(), 0, "yaw 0 looks +X, where the spawn is");
        assert_eq!(Item::COMPASS.dial_layer(looking), Some(tex::item_layer(icon_count() as u16)));
        let right = Dial { spawn_z: -10.0, spawn_x: 0.0, ..looking };
        assert_eq!(right.compass_frame(), 8, "spawn on the right is a quarter turn");
        let lost = Dial { overworld: false, spin: 1.0, ..looking };
        assert_ne!(lost.compass_frame(), looking.compass_frame());
        assert_eq!(looking.clock_frame(), 0, "noon is frame 0");
        let sunrise = Dial { day_time: 0.0, ..looking };
        assert_eq!(sunrise.clock_frame(), 48);
        assert_eq!(Item::from_name("compass"), Some(Item::COMPASS));
        assert_eq!(Item::from_name("clock"), Some(Item::CLOCK));
        assert_eq!(Item::COMPASS.max_stack(), 64);
    }
}
