# EFX-ENCH-17: enchantment definitions, item tags, exclusive sets and enchantment tags from the jar

[← Campaign record](../README.md)

## What was built

- `tools/extract_java_enchantment_fixtures.py` reads the pinned server jar
  and writes `tests/fixtures/java/26.3/enchantments.json`. The fixture holds
  all 43 enchantment definitions, the 21 `#enchantable/*` item tags expanded
  to item ids, and the 22 enchantment tags expanded in `HolderSet` order.
  Names come from the pinned client's en_us. The tool fails on unknown keys,
  slots, cost forms or tag entry forms.
- `src/enchant.rs`: the hand-written `Fits` classes are replaced by
  `ItemSet`, one variant per Java `#enchantable/*` tag (21), and `Group` is
  replaced by `ExclusiveSet`, one variant per `#exclusive_set/*` tag (7).
  - `Def` now carries Java's `supported_items`, `primary_items` (optional,
    as in Java) and `exclusive_set`.
  - `compatible` is now Java's `areCompatible`: two different enchantments
    are compatible unless either one's exclusive set holds the other. The
    special cases for Frost Walker with Depth Strider and for Riptide are
    gone.
  - New public functions: `is_supported_item` and `is_primary_item`
    (`isPrimaryItem`), `cost_range` and `display_name`.
- Fixes found by comparing with the tags:
  - Efficiency goes on shears (`#enchantable/mining`), by anvil or
    `/enchant`. Silk Touch and Fortune still don't, because they use
    `#enchantable/mining_loot`.
  - Curse of Binding goes on carved pumpkins and the wither skeleton skull
    (`#enchantable/equippable`). Before, it went only on armor.
  - Curse of Vanishing goes on compasses, carved pumpkins and the wither
    skeleton skull (`#enchantable/vanishing`).
  - Tooltips list enchantments in `#tooltip_order`
    (`ItemEnchantments.addToTooltip`). Before, they followed save-bit order.
    The HUD and the CLI agent's inventory listing both read `Enchants::lines`.
  - "Luck of the Sea" was shown as "Luck Of The Sea".
- The holder sets are walked in their tag order. The table, mob spawn
  equipment and traded equipment roll from `NON_TREASURE` in
  `#non_treasure` order, and random loot rolls from `ON_RANDOM_LOOT`
  (`getAvailableEnchantmentResults` streams the options set). Before, these
  rolls followed save-bit order, so a seeded weighted pick could land on a
  different enchantment from Java's. For example, Java lists Luck of the Sea
  before Lure, the trident enchantments before the crossbow ones, and the
  curses, Frost Walker and Mending last.
  - `Enchantment::tradeable()` now returns `#tradeable` in its order.
  - The bastion `enchant_randomly` path walks `ON_RANDOM_LOOT`.

## Reference sources read

- `data/minecraft/enchantment/*.json` (43 files).
- `data/minecraft/tags/item/enchantable/*.json` (21 files) and the item tags
  they include (`#swords`, `#axes`, `#pickaxes`, `#shovels`, `#hoes`,
  `#spears`, `#skulls`, `#head_armor`, `#chest_armor`, `#leg_armor`,
  `#foot_armor`).
- `data/minecraft/tags/enchantment/**/*.json` (22 files).
- Client `assets/minecraft/lang/en_us.json`, `enchantment.minecraft.*`.
- `net.minecraft.world.item.enchantment.Enchantment`: `DIRECT_CODEC`,
  `isPrimaryItem`, `isSupportedItem`, `canEnchant`, `areCompatible` and
  `getFullname`.
- `EnchantmentHelper.selectEnchantment`, `getAvailableEnchantmentResults` and
  `filterCompatibleEnchantments`.
- `ItemEnchantments.addToTooltip`.
- `EnchantRandomlyFunction.run`.

Where the brief and the reference differ:

- The brief says 22 enchantable item tags. The jar has 21; there are 22
  enchantment tags.
- The brief doesn't mention `#enchantable/mining_loot`. Silk Touch and
  Fortune use it, so adding shears to the shared mining class would have
  been wrong.
- `#skulls` brings the wither skeleton skull into the Binding and Vanishing
  sets.

## Ids and orderings

No block states, item ids or texture layers. No enum variant was appended
and save bits are unchanged. Enchants still store levels by save bit, so
there is no save impact. New public tables: `ItemSet::ALL`,
`ExclusiveSet::ALL`, `NON_TREASURE`, `ON_RANDOM_LOOT` and `TOOLTIP_ORDER`.

## Tests

`tests/java_enchantments.rs`:

- `every_java_definition_is_registered_with_its_numbers_and_names`: name,
  levels, weight, costs per level, anvil cost, supported and primary tags
  and exclusive set. Every `Enchantment` maps to exactly one Java id, and
  Soul Speed is the one unregistered definition.
- `item_sets_match_the_enchantable_tags_for_every_registered_item`: every
  `ItemSet` against its tag over every registered item, block items
  included, both ways. Java members the game lacks must be owned in
  `registry_gaps.json`.
- `every_item_takes_exactly_the_enchantments_java_allows`: every registered
  item against every enchantment, for `is_supported_item`, `fits` and
  `is_primary_item`. Books take everything.
- `exclusive_sets_and_compatibility_match_for_every_pair`: each set's
  members in order, and `compatible` for all 42×42 ordered pairs.
- `enchantment_tags_match_and_keep_java_order`: all 22 tags are named, so a
  new tag fails the test. It checks treasure, curse, double-trade-price,
  non_treasure, the table, mob-spawn and traded-equipment sets, and the
  order of on_random_loot, tradeable and tooltip_order. The
  `prevents_*` and `smelts_loot` tags must each hold exactly the one
  enchantment their consumers name. Soul Speed must appear only in
  `#treasure` and `#tooltip_order`.
- `tooltips_list_enchantments_in_tooltip_order`.
- `anvil_books_reach_shears_pumpkins_and_compasses`: anvil results and costs
  for the three fixes; the table refuses these items; `/enchant` accepts
  them; Silk Touch and Fortune stay off shears; Binding stays off compasses.

`src/enchant.rs`, `tests::rolls_walk_the_options_in_tag_order`, checks that
the table's and random loot's available lists follow the tag order.

Checks run:

- `cargo fmt`.
- `VC_REMOTE_RETRIES=60 scripts/remote-check.sh --full` passed on the code
  in this tree. It ran clippy with `-D warnings`, 1839 lib tests and every
  integration test, plus the `--no-default-features` configuration.
- The extractor was re-run against the pinned jars, and its output is
  byte-identical to the fixture.
- `python3 tools/campaign_ledger.py` and `python3 tools/check_doc_links.py`
  pass.
- An earlier filtered run caught one wrong assertion, which was fixed:
  `#double_trade_price` includes `#treasure`, so it also lists Soul Speed.

## Differences from Java left in place

- Soul Speed isn't registered. The 128-bit enchantment encoding has no room
  for it (EFX-ENCH-18).
- Elytra and the six other `#skulls` heads aren't registered, so they're
  absent from the sets. They're owned in `registry_gaps.json`.
- The game can't yet put a carved pumpkin or the skull on a player's head
  through the armor slot. This slice changes only which enchantments those
  items accept, so Binding has no effect on them yet.
- The fixture records slots and effect component names, but this slice
  doesn't compare them (EFX-ENCH-20 owns enchantment effects).
- The game's consumers name Silk Touch and Fire Aspect directly for
  `prevents_*` and `smelts_loot`. The test pins those tags to that single
  member.
- Villager enchanted-book offers are still the game's own code, not Java
  26.3's `trade_set` data. Only their `#tradeable` order changed here.

## Unverified

- No hands-on play. The anvil fixes are proven only by function-level tests.
- No pixels of the tooltip order were checked. A scripted capture doesn't
  hover a slot, so it can't show a tooltip. The order is checked through
  `Enchants::lines`, which the HUD tooltip draws.
- Seeded table offers and loot rolls now differ from before for books,
  fishing rods and crossbows. No test compares a seeded roll against a
  running Java instance.

## Scripted capture

Nothing new is visible in a world scene. A capture of an enchanted sword in
the inventory shows the item, but not its tooltip, because no slot is
hovered:

```sh
cargo run --release -- --data-dir target/screenshots --give iron_sword --enchant unbreaking,3 --enchant vanishing_curse --enchant sharpness,5 --open-inventory --screenshot target/screenshots/ench-17-inventory.png
```

`/enchant efficiency 5` while holding shears shows the shears fix in game.
