# ITEM-EQUIP: chainmail armor, copper and netherite horse armor, nautilus armor

[← Campaign](../README.md)

Catalogue entries ITEM-01-CHAINMAIL, ITEM-14-HORSE-ARMOR, ITEM-15-NAUTILUS-ARMOR
and, from MOBN-39, only which animals may wear horse armor.

## What was built

- **Chainmail armor** (`ArmorMaterial::Chainmail`, appended): helmet,
  chestplate, leggings and boots with Java's defense 2/5/4/1, durability
  15 × 11/16/15/13 (165/240/225/195), enchantability 12, no toughness or
  knockback resistance, repaired with iron ingots, `Rarity.UNCOMMON` (yellow
  tooltip name). Worn pieces use the existing chain armor sheet
  (`ArmorKind::of` maps the material; nothing else in `entity/armor.rs`
  changed); armor stands tint it grey. The four armorer offers already
  written in `villager_trades.rs` now resolve and roll (apprentice boots and
  leggings, journeyman helmet and chestplate). The pieces melt to an iron
  nugget in a furnace or blast furnace.
- **Copper and netherite horse armor**: stack 1, no durability, not
  enchantable or repairable; netherite is fire resistant. Netherite horse
  armor is a smithing upgrade of diamond horse armor; copper horse armor
  melts to a copper nugget and is now awarded by the five implemented chest
  tables that hold it. Horses draw both (new copper and netherite colours on
  the horse armor model).
- **Horse armor body attributes**: a horse's armor now comes from the
  material's `ArmorType.BODY` defense (3/4/5/7/11/19), and its toughness
  (diamond 2, netherite 3) and knockback resistance (netherite 0.1) now apply
  too; before, only the armor points did.
- **Who wears horse armor**: only horses and zombie horses
  (`#can_wear_horse_armor`). Skeleton horses no longer accept it from the hand
  or the inventory screen; a skeleton horse saved wearing armor by an older
  build still loads, and the armor drops beside it.
- **Nautilus armor** (copper, iron, golden, diamond, netherite): stack 1, no
  durability, body defense of the material, netherite fire resistant. The
  netherite one is a smithing upgrade of diamond; copper, iron and golden melt
  to their nuggets; the six ocean chest tables now award the four lootable
  ones at weights 20/10/5/2 against 148 empty. No animal in the game can wear
  them yet.
- **Icons**: original procedural icons — chainmail as armor with open
  links, horse armor as side-on barding, nautilus armor as a spiralled shell
  plate. The four older horse armors switched from the chestplate icon to
  the barding icon so horse armor no longer looks like a chestplate.

## Reference sources read

- `net.minecraft.world.item.equipment.ArmorMaterials` (`CHAINMAIL`,
  `makeDefense`), `ArmorMaterial.createAttributes`, `ArmorType`
- `net.minecraft.world.item.Item$Properties.humanoidArmor`, `.wolfArmor`,
  `.horseArmor`, `.nautilusArmor`; `net.minecraft.world.item.Items`
  (`CHAINMAIL_*`, `*_HORSE_ARMOR`, `*_NAUTILUS_ARMOR`); `net.minecraft.references.ItemIds`
- `net.minecraft.world.entity.animal.equine.AbstractHorse` (`mobInteract`,
  `canUseSlot`), `SkeletonHorse.canUseSlot`, `ZombieHorse.canUseSlot`,
  `Horse.canUseSlot`
- `net.minecraft.world.item.trading.VillagerTrade` (codec defaults: 4 uses,
  1 xp, no discount)
- `data/minecraft/villager_trade/armorer/{2,3}/emerald_chainmail_*.json`
- `data/minecraft/tags/item/repairs_*_armor.json`,
  `data/minecraft/tags/entity_type/can_wear_{horse,nautilus}_armor.json`
- `data/minecraft/recipe/{iron,gold,copper}_nugget_from_{smelting,blasting}.json`,
  `netherite_{horse,nautilus}_armor_smithing.json`
- `data/minecraft/loot_table/chests/*` through `src/world/loot_tables.json`

The new fixture `tests/fixtures/java/26.3/equipment.json` comes from
`tools/extract_java_equipment_fixtures.py`, which reads only the pinned server
jar (see the fixture README's "Armor equipment").

The brief and the code agreed with the reference except: the brief's
MOBN-39 note that skeleton horses wear armor is confirmed (`SkeletonHorse`
can use the body slot but horse armor's `allowedEntities` is the tag, which
omits it); and the existing four horse armors' points already matched Java,
but diamond horse armor's toughness 2 was missing.

## IDs and orderings

- Item ids 1550–1553 (chainmail helmet, chestplate, leggings, boots), 1561
  (copper horse armor), 1562 (netherite horse armor), 1563–1567 (copper,
  iron, golden, diamond, netherite nautilus armor). No block states or
  texture layers.
- Appended: `ArmorMaterial::Chainmail` (end of the enum and of
  `ArmorMaterial::ALL`), `Sprite::HorseArmor` and `Sprite::NautilusArmor`
  (end of `Sprite`), eleven item icons after the fermented spider eye
  (`Item::all_items`, `sprite_index` through `equipment::icon_offset`,
  `icon_count` through `equipment::ICON_COUNT`), and horse armor model colours
  4 (copper) and 5 (netherite) in `entity/model/mounts.rs`.
- Saves store items by id as before; mount saves keep their format.

## Tests

- `tests/java_equipment.rs` (new): every one of the fixture's 41 armor items
  is registered with its material and form (durability, stack size, fire
  resistance, rarity, enchantability, repair materials, humanoid
  defense/toughness/knockback, body defense, horse armor points/toughness/
  knockback) and every armor item in the game is one of them; chainmail
  equipped through the CLI agent's place command, the 12-point set, the chain
  sheet and a damaged enchanted stack's save round trip; the armorer's plain
  armor offers against the `villager_trade` JSON; the nugget recipes from
  `recipes.json` for the new armors (and none for the others); both netherite
  smithing recipes, also through the CLI agent at a smithing table; and 4000
  seeded rolls of each of the eleven chest tables against the expected count
  from the table's weights (within five standard deviations).
- `src/equipment.rs` unit tests: horse damage and knockback with each of the
  six horse armors; the wearer tag against every rideable animal (and no
  animal for nautilus armor); copper and netherite horse armor equipped from
  the hand and kept across a mob save; a skeleton horse's old armor dropping
  on load and refused from the hand; id ranges and icon offsets.
- Existing comparisons that now include the new items:
  `tests/java_recipes.rs` (cooking and smithing both ways),
  `tests/java_repair.rs` (chainmail repairs), `tests/java_registry.rs` (gaps
  removed; trade and loot names resolve), `tests/copper_tools.rs` (chainmail
  material against `combat.json`), and the test-only mirrors of the chest
  tables in `src/world/temples.rs` and `src/world/dungeon.rs`.

## Differences from Java left in place

- No armor equip sounds play for any armor (the game has none); Java plays
  `item.armor.equip_chain`, `entity.horse.armor` and
  `item.armor.equip_nautilus`.
- Shears do not take horse armor off (`Equippable.canBeSheared`); not part of
  this slice.
- Equipping horse armor on a horse that already wears some swaps it, as
  before; Java's `mobInteract` only equips an unarmored horse.
- `Rarity.UNCOMMON` is shown only for chainmail; other uncommon items (heart
  of the sea and others) keep white names. Enchanted chainmail shows the
  enchantment tooltip rather than Java's rarity bump to aqua.
- Nautilus armor cannot be worn: the nautilus and zombie nautilus are not in
  the game (MOBN-35).
- Copper horse armor in `village/village_weaponsmith` and
  `end_city_treasure` waits for those tables; chainmail from the
  hero-of-the-village gift, mansion chests and mob spawn equipment belongs to
  other slices.

## Unverified

- Nobody has looked at the new icons, the chainmail on a player, or the
  copper and netherite horse armor on a horse in the running game.
- The yellow tooltip colour is not covered by a test.
- Desktop and controller paths share the tested code (`Inventory::equip`,
  `Entities::use_mount`, `smithing::upgrade`, furnace rules) but were not
  driven directly.

## Scripted captures

```sh
# Every item icon, new ones last.
cargo test --release icon_sheet -- --ignored   # writes target/item_icons.png
# The new items in the inventory, chainmail worn on the player model.
cargo run --release -- --world screenshots --new --seed 42 --data-dir target/screenshots \
    --wear chainmail_helmet --wear chainmail_chestplate --wear chainmail_leggings --wear chainmail_boots \
    --give copper_horse_armor --give netherite_horse_armor --give copper_nautilus_armor \
    --give iron_nautilus_armor --give golden_nautilus_armor --give diamond_nautilus_armor \
    --give netherite_nautilus_armor --open-inventory --screenshot equipment.png
```

A horse wearing the new armor has no scripted capture (`--spawn` does not
equip horses); it needs a hands-on run.
