# ITEM-00-COMPONENTS: stack component slots for bundles, books, trims, dyes and banners

[← Campaign record](../README.md)

## What was built

The Stack side of the items-area foundation (the integrator recorded the
ID reservations in `docs/architecture.md`; this slice took no IDs).

- `src/inventory/components.rs` (new):
  - `Payload`, the variable-size components in one slot: `Bundle`
    (`bundle_contents`), `WritableBook` (`writable_book_content`),
    `WrittenBook` (`written_book_content`) and `Banner` (`banner_patterns`
    plus the shield's `base_color`). `Payload::intern` gives a
    `StackPayload`: `Copy`, pointer-sized, equal exactly when the content
    is equal (one leaked entry per distinct value; the saved field is
    encoded once at interning). An item default (empty bundle, blank
    writable book, plain banner) interns to `None`.
  - Java's codec limits as `Payload::is_valid` and constants.
  - `Trim` (`TrimPattern` x 18, `TrimMaterial` x 11), `trimmable(item)`.
  - `BANNER_PATTERNS` (43 names), `BannerPattern`, `BannerLayer`.
  - The shared dyed-colour helper lifted out of `entity/wolf_armor.rs`:
    `DYED`, `dyeable(item)`, `dyed_color`, `with_dyed_color`, `mix_dyes`,
    `apply_dyes`, `remove_dye`. The encoding is unchanged
    (`entity_data = DYED | rgb`), and it is read only on dyeable items.
    `wolf_armor::{color, dye, washed}` now call it and keep their
    wolf-armor-only behaviour.
- `src/inventory.rs`: `Stack` gains `trim: Option<Trim>` and
  `payload: Option<StackPayload>`; `stacks_with` compares both; accessors
  `with_payload`, `payload`, `bundle_contents`, `writable_pages`,
  `written_book`, `banner_patterns`. `stack_to_string` / `stack_from_str`
  append two trailing fields (`pattern.material`, then unpadded base64 of
  the payload's JSON); trailing fields are written up to the last one
  present with `~` for absent ones, which reproduces every older string
  byte for byte. A bundle's nested stacks are JSON inside the one base64
  field, so nesting does not re-encode.
- The LAN stack codec is `stack_to_string`/`stack_from_str`, so player
  state, held items, armor, containers, dropped items and decorations
  carry the new fields with no protocol change.

## Reference sources read

- `net/minecraft/world/item/ItemStack.java` (`isSameItemSameComponents`,
  `matches`)
- `net/minecraft/core/component/DataComponents.java`,
  `PatchedDataComponentMap.java` (`set` drops a value equal to the
  prototype's; `equals` compares prototype and patch)
- `net/minecraft/world/item/component/BundleContents.java` (`equals` uses
  the item list only; the selected index is not part of equality),
  `WritableBookContent.java`, `WrittenBookContent.java`, `DyedItemColor.java`
- `net/minecraft/world/item/ItemStackTemplate.java` (bundle stack counts
  1-99)
- `net/minecraft/world/item/equipment/trim/ArmorTrim.java`,
  `net/minecraft/world/level/block/entity/BannerPatternLayers.java`
- `net/minecraft/world/item/DyeColor.java`, `net/minecraft/util/ARGB.java`,
  `net/minecraft/world/item/crafting/DyeRecipe.java`,
  `net/minecraft/core/cauldron/CauldronInteractions.java`
- `net/minecraft/world/item/Items.java` (bundle, writable book, banner and
  shield component defaults)
- Server data: `trim_pattern/*`, `trim_material/*`, `banner_pattern/*`,
  `tags/item/cauldron_can_remove_dye.json`, `tags/item/trimmable_armor.json`
  and the armor tags it includes.

The brief named the dyeable tag `dyeable`; 26.3 has no such item tag. The
dyeable set is `#cauldron_can_remove_dye` (the items a cauldron washes),
which the fixture records.

## Oracle

`tools/extract_java_stack_component_fixtures.py` writes
`tests/fixtures/java/26.3/stack_components.json` from the pinned server jar
and the decompiled source (see the fixture README). `tests/stack_components.rs`:

- `modelled_components_and_registries_match_java`: trim patterns, trim
  materials and banner patterns equal Java's both ways; the seven modelled
  component ids are registered; book limits and dye colours match.
- `dyeable_and_trimmable_items_match_java_tags`: both tags against every
  registered item, both ways; unregistered tag members (chainmail armor)
  are owned gaps.
- `differing_components_never_stack_and_equal_ones_do`: 14 cases (bundle
  contents and order, writable pages, written title and generation,
  written against writable, banner layer colour and pattern, base colour,
  trim pattern and material, trimmed against plain, dyed colour) through
  `stacks_with`, `Inventory::add_stack`, `move_into`, `click_slot` and
  dropped-item merging.
- `item_defaults_leave_the_payload_slot_empty`, `payload_limits_follow_java_codecs`.
- `stack_strings_round_trip_every_component`,
  `saved_payload_fields_are_base64_json_with_names` (checked against an
  independent base64 encoder in the test), `pre_v017_stack_strings_load_unchanged`
  (literal plain, worn, enchanted and named, dyed wolf armor, lodestone,
  lost lodestone, map and hive strings, and a whole inventory).
- `component_stacks_survive_every_container`: inventory (slots, armor, off
  hand, spill), chest save, a hopper moving every sample between chests,
  dropped items, an item frame and an armor stand.
- `component_stacks_survive_a_process_restart`: a second process loads
  inventory, chest, dropped items and decorations from disk.
- `dyed_colour_helper_is_shared_and_gated_by_the_dyeable_tag`,
  `stacks_stay_small_to_copy` (Stack at most 160 bytes, payload slot one
  pointer).
- `src/inventory/components.rs` unit tests: base64 and the `applyDyes`
  arithmetic (red + blue = (173, 83, 152), worked from Java's integer
  averages and float rescale).
- `src/app/lan.rs::tests::snapshots_carry_stack_components`: a LAN state
  packet and the lite state other players get, encoded and decoded.

Java 26.3 class files are version 69 and the only JDK here is 21, so the
dye vectors were worked by hand from `DyedItemColor.applyDyes`, not run
against Java.

## IDs and orderings

No block states, item IDs or texture layers. Appended: two trailing stack
save-string fields (trim, payload), which are also the LAN stack codec's.
New enums (`TrimPattern`, `TrimMaterial`) and the `BANNER_PATTERNS` table
are new; saves store their names, never positions.

## Differences from Java

- Written-book pages are plain text, not text components; chat filtering
  is off in single player, so only raw titles and pages are kept.
- One payload slot per stack: a stack cannot carry, say, bundle contents
  and banner patterns at once. No survival path makes such a stack.
- The dyed colour shares `entity_data` with other per-item components and
  is read only on dyeable items; Java's component can sit on any item
  through commands.
- Interned payloads are never freed. Callers should intern finished
  values (a signed book, a finished bundle insertion), not each keystroke.
- A LAN frame is 256 KiB. A player carrying several full written books
  (100 pages of 1024 characters encode to about 140 KiB each) would
  overflow the player-state snapshot; the book slice should cap or ship
  payloads separately.
- `BundleContents`' selected index is not stored; it is transient UI state
  and not part of Java's equality.

## Unverified

- No item that uses the payload, trim or dye slots in survival exists yet
  (bundles, books, trims, dyed leather and pattern banners are later
  slices), so nothing here is player-visible or played.
- Old builds cannot read stacks that use the new fields (their parser
  rejects extra fields); newer builds read every older string.

## Scripted capture

Nothing new is visible; no capture applies until a slice registers an item
that uses these slots.
