# ITEM-13-DYED-LEATHER: dyed leather armor, five dye recipes, tinted drawing and cauldron washing

[← Campaign record](../README.md)

## What was built

- `src/inventory/components.rs`: `LEATHER_COLOR` (`#A06540`),
  `undyed_tint` (leather items only; undyed wolf armor is untinted),
  `dye_tint`, `dye_tint_bytes`, `dye_tint_f32`: the `minecraft:dye` tint
  every draw site multiplies an icon, worn layer or body armor by. The
  `dyed_color` encoding and `apply_dyes` from ITEM-00 are unchanged.
- `src/crafting/dyed.rs` (new): the five `leather_*_dyed` recipe-book entries
  (target plus `#dyes`, result showing the white `DyedSlotDemo` case) and
  `dynamic`, `DyeRecipe.matches`/`assemble` for every dyeable item: exactly
  one target, at least one dye, nothing else; the result keeps every other
  component. Wolf armor's dye loop moved here from `crafting/wolf_armor.rs`
  (its paired repair stays there); `#dyes` is shared.
- `src/world/utilities/cauldron.rs`: `wash_wolf_armor` became `wash_dyed`
  (any dyed `#cauldron_can_remove_dye` item; level 1 leaves an empty
  cauldron) and `washes(block, stack)`. `takes` no longer claims every
  wolf armor click. `src/content/interaction.rs::block_use_precedes_stack`
  adds the stack-dependent wash to `block_use_precedes_item`; the session's
  `block_takes_use` and `survival_items::equip_armor` (desktop, controller,
  CLI and LAN peers) use it, so dyed leather is washed and undyed leather is
  put on, as Java's `TRY_WITH_EMPTY_HAND` lets the item's own use follow.
- `src/world/utilities/interact.rs`: `Effect::Washed` (appended), silent in
  `src/app/utilities.rs` as `dyedItemIteration` plays no sound. Wolf armor
  washing used to report a cauldron exchange and play the swim sound.
- Drawing: `armor::Worn` gains `dye` (from `Worn::of`, used by
  `from_stacks` and mob `Equipment::worn`), and `armor::tint` uses it for the
  leather layer (the overlay stays untinted). Leather item icons
  (`render/item_sprites.rs`) are drawn pale grey and tinted at every icon
  site: inventory slots, the held item in first and third person, dropped
  items, item frames, shelves, brushable blocks, an allay's and a copper
  golem's held item. Armor stand shells use `decor::stand_shell_colour`.
  Leather horse armor on a horse is a sentinel recoloured from the stack in
  `entity/model/mounts.rs::colour`.
- `src/entity/mounts.rs::use_mount`: horse armor and carpets are equipped as
  the held stack (count 1) instead of a fresh `Stack::new`, which dropped the
  dye and any enchantments.
- `--dye colour[,colour...]` (`src/main.rs`, `src/app/startup.rs`,
  `docs/development.md`): dyes the first hotbar stack and dyeable worn armor
  for scripted captures.
- `tests/java_recipes/compare.rs`: the five `crafting_dye` allow-list entries
  removed; every `crafting_dye` recipe is compared as target plus dyes, and
  dyeable results ignore their colour in the grid check.

## Reference sources read

- `net/minecraft/world/item/component/DyedItemColor.java` (`LEATHER_COLOR`,
  `applyDyes`, `getOrDefault`, tooltip)
- `net/minecraft/world/item/crafting/DyeRecipe.java`,
  `crafting/display/SlotDisplay.java` (`DyedSlotDemo`)
- `net/minecraft/core/cauldron/CauldronInteractions.java`
  (`dyedItemIteration`, the `WATER` map registration),
  `net/minecraft/world/level/block/LayeredCauldronBlock.java`
  (`lowerFillLevel`), `net/minecraft/util/ARGB.java` (`color` masks)
- `net/minecraft/world/item/DyeColor.java` (`textureDiffuseColor`)
- Server data: `recipe/leather_{helmet,chestplate,leggings,boots,horse_armor}_dyed.json`,
  `recipe/wolf_armor_dyed.json`, `tags/item/cauldron_can_remove_dye.json`,
  `tags/item/dyes.json`
- Client: `items/leather_*.json`, `items/leather_horse_armor.json`,
  `items/wolf_armor.json`, `models/item/leather_helmet.json`,
  `equipment/leather.json`

The brief and the reference agree. The code differed in three places (the
reference wins): wolf armor washing played a sound, the cauldron took every
wolf armor click whether dyed or not, and horse armor lost its components
when equipped.

## Oracle

`tools/extract_java_dyed_item_fixtures.py` writes
`tests/fixtures/java/26.3/dyed_items.json` (see the fixture README):
tags, recipes, dye colours, `LEATHER_COLOR`, item-model tint defaults, the
leather equipment layers, the wash facts (method bodies matched statement
by statement) and 1216 `applyDyes` vectors from a model of Java's integer
averages and float32 rescale. Java 26.3 class files need JDK 25 and only
JDK 21 is installed here, so the vectors come from that model, not from
running Java; the extractor refuses to run if the method body changes.

## IDs and orderings

No block states, item ids or texture layers. Appended: `Effect::Washed` at
the end of `world::utilities::interact::Effect` (not saved, not sent by
index). New struct fields: `armor::Worn::dye`, `PlayerAppearance::held_tint`.
Five recipes appended to `crafting::recipes()` after wolf armor's. Saves are
unchanged: the colour is ITEM-00's `entity_data` field of the stack string.

## Tests

`tests/java_dyed_items.rs`:

- `dyeable_items_dyes_and_dye_recipes_match_java`: both tags, the dye
  colours and the dye recipes both ways.
- `every_reference_mix_crafts_on_every_dyeable_item`: 1216 vectors x 6 items
  through `Grid::result` (2x2 and 3x3, target in varying cells), components
  kept; `mix_dyes` on each vector.
- `dye_grids_need_one_target_and_only_dyes_and_consume_one_each`.
- `the_cli_dyes_and_re_dyes_leather_through_the_shared_grid`.
- `undyed_and_dyed_colours_follow_the_models_and_the_leather_equipment_asset`:
  icon tints, worn layer and overlay tints, armor stand shells, iron untinted.
- `water_cauldrons_wash_every_dyeable_item_at_every_level`: 6 items x levels
  1-3 x Survival/Creative through `PlayerSession::use_held`; lava, powder
  snow, empty and sneaking do not wash.
- `undyed_leather_is_put_on_at_a_water_cauldron_and_dyed_leather_is_washed`:
  `survival_items::equip_armor` and the CLI `place` command.
- `a_horse_wears_dyed_leather_horse_armor_through_save_and_reload`.
- `dyed_leather_survives_save_and_reload_and_old_saves_load_undyed`:
  inventory, worn armor, armor stand, stack strings.

Also `src/entity/model/mounts.rs::leather_horse_armor_is_drawn_in_its_dyed_colour`,
the updated `src/world/utilities/tests.rs` wolf-armor wash test and
`tests/java_recipes.rs` with the allow-list entries gone.

## Differences from Java

- The item icon has one tint for the whole sprite: Java tints only
  `layer0` and draws the `_overlay` layer untinted. The original icons are
  pale grey so the whole piece takes the colour.
- Tooltips show only the item name, so the "Dyed" line (and "Color: #RRGGBB"
  with advanced tooltips) is not shown.
- No statistics exist, so `Stats.CLEAN_ARMOR` is not awarded.
- The cauldron's `BLOCK_CHANGE` game event is not posted for a wash (no
  cauldron exchange posts one in VoxelCraft yet).
- The seven leatherworker offers that sell randomly dyed leather
  (`set_random_dyes`) do not exist; they belong to the villages area.

## Unverified

- Nothing was played or looked at: no build ran on this Mac and no hands-on
  run was made. The tint plumbing at the draw sites (icons, worn layers,
  armor stand, horse) is tested at the colour it computes, not on screen.
- The horse capture has no scripted path (`--spawn` cannot equip horse
  armor), and `--decor` dresses an armor stand only with undyed pieces.

## Scripted capture

```sh
# Dyed leather icon in the hotbar and dyed worn armor (front camera).
cargo run --release -- --world dyed --new --seed 42 --data-dir target/screenshots \
    --give leather_cap --wear leather_tunic --wear leather_pants --wear leather_boots \
    --dye red,blue --camera front --screenshot target/screenshots/dyed-leather.png --pose 0,190,0,45,-10
# The undyed default for comparison.
cargo run --release -- --world undyed --new --seed 42 --data-dir target/screenshots \
    --give leather_cap --wear leather_tunic --wear leather_pants --wear leather_boots \
    --camera front --screenshot target/screenshots/undyed-leather.png --pose 0,190,0,45,-10
```
