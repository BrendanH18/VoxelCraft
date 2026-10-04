//! Items: anything that can sit in an inventory slot.
//!
//! Ids below 256 are the block with the same id, so a `Block` converts to an
//! `Item` for free and old saves (which stored block ids) still load. Ids from
//! [`FIRST_ITEM`] up are tools, materials and food, described by a static
//! table like the block registry.

use crate::world::block::{Block, RenderKind, tex};

#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug, Default)]
#[repr(transparent)]
pub struct Item(pub u16);

/// First id that isn't a block.
pub const FIRST_ITEM: u16 = 256;

#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum ToolKind {
    Pickaxe,
    Shovel,
    Axe,
    Hoe,
    Sword,
}

/// Tool material, from worst to best harvest level (gold is fast but weak).
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum Tier {
    Wood,
    Stone,
    Iron,
    Gold,
    Diamond,
}

impl Tier {
    pub const ALL: [Tier; 5] = [Tier::Wood, Tier::Stone, Tier::Iron, Tier::Gold, Tier::Diamond];

    /// Which blocks the tier can harvest: 0 wood/gold, 1 stone, 2 iron, 3 diamond.
    pub fn level(self) -> u8 {
        match self {
            Tier::Wood | Tier::Gold => 0,
            Tier::Stone => 1,
            Tier::Iron => 2,
            Tier::Diamond => 3,
        }
    }

    /// Mining speed multiplier on blocks the tool is suited to.
    pub fn speed(self) -> f32 {
        match self {
            Tier::Wood => 2.0,
            Tier::Stone => 4.0,
            Tier::Iron => 6.0,
            Tier::Diamond => 8.0,
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
}

impl ArmorMaterial {
    pub const ALL: [ArmorMaterial; 4] =
        [ArmorMaterial::Leather, ArmorMaterial::Iron, ArmorMaterial::Gold, ArmorMaterial::Diamond];

    /// Armor points (half chestplates on the HUD) per piece, as in Minecraft.
    pub fn defense(self, piece: ArmorPiece) -> u8 {
        let points = match self {
            ArmorMaterial::Leather => [1, 3, 2, 1],
            ArmorMaterial::Gold => [2, 5, 3, 1],
            ArmorMaterial::Iron => [2, 6, 5, 2],
            ArmorMaterial::Diamond => [3, 8, 6, 3],
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
        };
        base * [11, 16, 15, 13][piece as usize]
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
    /// Lights a Nether portal frame.
    FlintAndSteel,
    /// Crafting ingredient or mob drop with no use of its own.
    Material,
}

/// How an item's inventory icon is drawn (see `render::item_sprites`).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Sprite {
    Stick,
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

const RAW_MEAT: [u8; 3] = [226, 110, 110];
const COOKED_MEAT: [u8; 3] = [150, 88, 52];
const FAT: [u8; 3] = [250, 225, 215];
const COOKED_FAT: [u8; 3] = [215, 180, 130];

/// Non-block items, in id order from [`FIRST_ITEM`]. Append only: ids are
/// stored in saves.
static ITEMS: [ItemInfo; 43] = [
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
];

/// Uses before a bow breaks.
pub const BOW_DURABILITY: u16 = 384;
/// Uses before a flint and steel breaks.
pub const FLINT_AND_STEEL_DURABILITY: u16 = 64;

/// Tools start at this id: `FIRST_TOOL + tier * 5 + kind`.
const FIRST_TOOL: u16 = 320;
const TOOL_KINDS: [ToolKind; 5] = [ToolKind::Pickaxe, ToolKind::Shovel, ToolKind::Axe, ToolKind::Hoe, ToolKind::Sword];
const TOOL_COUNT: u16 = (Tier::ALL.len() * TOOL_KINDS.len()) as u16;
/// Armor starts at this id: `FIRST_ARMOR + material * 4 + piece`.
const FIRST_ARMOR: u16 = FIRST_TOOL + TOOL_COUNT;
const ARMOR_COUNT: u16 = 16;

impl Item {
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

    pub const fn tool(kind: ToolKind, tier: Tier) -> Item {
        Item(FIRST_TOOL + tier as u16 * 5 + kind as u16)
    }

    pub const fn armor(piece: ArmorPiece, material: ArmorMaterial) -> Item {
        Item(FIRST_ARMOR + material as u16 * 4 + piece as u16)
    }

    pub const fn from_block(block: Block) -> Item {
        Item(block.0 as u16)
    }

    /// The block placing this item puts down: the block itself, or what a
    /// non-block item plants (seeds sow wheat).
    pub fn places(self) -> Option<Block> {
        match self {
            Item::WHEAT_SEEDS => Some(Block::wheat(0)),
            Item::NETHER_WART => Some(Block::nether_wart(0)),
            i => i.block(),
        }
    }

    /// The block this item is, if any.
    pub fn block(self) -> Option<Block> {
        if self.0 < FIRST_ITEM {
            let b = Block(self.0 as u8);
            (b.kind() != RenderKind::Invisible).then_some(b)
        } else {
            None
        }
    }

    pub fn info(self) -> ItemInfo {
        if let Some(b) = self.block() {
            return ItemInfo { name: b.name(), kind: ItemKind::Block(b), max_stack: 64, sprite: Sprite::Stick };
        }
        if let Some(info) = self.0.checked_sub(FIRST_ITEM).and_then(|i| ITEMS.get(i as usize)) {
            return *info;
        }
        if let Some(i) = self.0.checked_sub(FIRST_TOOL)
            && let (Some(&tier), Some(&kind)) = (Tier::ALL.get(i as usize / 5), TOOL_KINDS.get(i as usize % 5))
        {
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
        ItemInfo { name: "unknown", kind: ItemKind::Material, max_stack: 64, sprite: Sprite::Stick }
    }

    /// Whether this id names a real block or item.
    pub fn is_valid(self) -> bool {
        self.info().name != "unknown"
    }

    pub fn name(self) -> &'static str {
        self.info().name
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
        match self.info().kind {
            ItemKind::Tool(_, tier) => Some(tier.durability()),
            ItemKind::Armor(piece, material) => Some(material.durability(piece)),
            ItemKind::Bow => Some(BOW_DURABILITY),
            ItemKind::FlintAndSteel => Some(FLINT_AND_STEEL_DURABILITY),
            _ => None,
        }
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
        if let Some(b) = Block::from_name(name) {
            // Doors and nether wart are placed by an item, not as a block.
            return Some(match b {
                b if b.is_door() => Item::OAK_DOOR,
                b if b.wart_age().is_some() => Item::NETHER_WART,
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
        materials.chain(tools)
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

/// Index of an item's icon among the item texture layers.
fn sprite_index(item: Item) -> Option<u16> {
    let materials = ITEMS.len() as u16;
    match item.0 {
        i if (FIRST_ITEM..FIRST_ITEM + materials).contains(&i) => Some(i - FIRST_ITEM),
        i if item.as_tool().is_some() || item.as_armor().is_some() => Some(materials + i - FIRST_TOOL),
        _ => None,
    }
}

/// How many item icons there are (layers of the item texture array).
pub fn icon_count() -> u32 {
    Item::all_items().count() as u32
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
            assert_eq!(sprite_for_layer(index), Some(i.info().sprite));
        }
        // Icons stay addressable by the UI's and models' 16-bit layers.
        assert!(tex::ITEM_BASE as u32 + icon_count() <= u16::MAX as u32);
        assert_eq!(Item::from(Block::STONE).icon_layer(), None);
    }

    #[test]
    fn food_values() {
        assert_eq!(Item::COOKED_PORKCHOP.food(), Some((8, 12.8)));
        assert_eq!(Item::STICK.food(), None);
    }
}
