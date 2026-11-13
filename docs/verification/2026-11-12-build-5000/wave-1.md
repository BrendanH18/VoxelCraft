# Build-5000 wave 1 — 2026-11-12 to 2026-11-13

[← Campaign record](README.md) · [Roadmap](../../roadmap.md)

Wave 1 built foundations from the v0.17 list of the 2026-11-09 audit (now
v0.15, after the owner's renumbering on 2026-11-13) and Java data parity.
Its fourteen slices all landed on `t3/build-5000-v017-wave-1`, and they ship
in v0.14. The owner dropped the campaign's 5,000 target on 2026-11-13, so
this is the campaign's only wave for now.

## Slices

Each slice was built by one agent and then reviewed by a second agent in a
fresh worktree. The reviewer re-ran the extractor, checked facts by hand
against the Java 26.3 source and data, showed a test failing on a mutated
constant, and trimmed the ledger. The integrator then ran
`scripts/remote-check.sh --full` on each merged batch.

| Slice | What it delivers | Review | Rows |
| --- | --- | --- | --: |
| [MOBN-01](slices/MOBN-01.md) | Mob equipment: eight Java slots, drop chances, death drops, spawn equipment, save migration | approved | 18 |
| [EFX-DMG-01](slices/EFX-DMG-01.md) | The 51 damage types and 36 tags; armor, protection, Fire Resistance and game rules read tags | repaired | 9 |
| [EFX-REG-01](slices/EFX-REG-01.md) | All 40 mob effects from the jar; the 11 missing ones registered | repaired | 17 |
| [ITEM-00-COMPONENTS](slices/ITEM-00-COMPONENTS.md) | Stack component slots for bundles, books, trims, dyes and banners | repaired | 2 |
| [ITEM-EQUIP](slices/ITEM-EQUIP.md) | Chainmail armor, copper and netherite horse armor, nautilus armor, one equipment fixture | approved | 32 |
| [ITEM-16-HARNESSES](slices/ITEM-16-HARNESSES.md) | Sixteen harnesses with their crafting and re-dye recipes | approved | 48 |
| [ITEM-02-EXPERIENCE-BOTTLE](slices/ITEM-02-EXPERIENCE-BOTTLE.md) | Bottle o' Enchanting: thrown burst, dispenser, cleric offer, chest loot | repaired | 10 |
| [EFX-ENCH-17](slices/EFX-ENCH-17.md) | All 43 enchantment definitions with item tags and exclusive sets (Soul Speed has no variant) | approved | 7 |
| [EFX-INST-02](slices/EFX-INST-02.md) | Effect instances: infinite duration, ambient and visibility flags, Java's merge rules | approved | 13 |
| [MOBN-26](slices/MOBN-26.md) | Endermites | repaired | 14 |
| [MOBN-06](slices/MOBN-06.md) | Zombie reinforcements, leaders and spawn attribute rolls | repaired | 10 |
| [EFX-RULE-21](slices/EFX-RULE-21.md) | `forgive_dead_players` and `universal_anger` for every neutral mob | repaired | 19 |
| [ITEM-08-BUNDLE-CORE](slices/ITEM-08-BUNDLE-CORE.md) | Seventeen bundles: weight rule, recipe, dye transmutes, village loot, save | repaired | 47 |
| [ITEM-13-DYED-LEATHER](slices/ITEM-13-DYED-LEATHER.md) | Dyed leather armor: dye recipes, colour mix, tinted drawing, cauldron washing | repaired | 10 |

The [ledger](ledger/) holds 256 rows (117 content, 84 behaviour, 55 fix),
checked by `python3 tools/campaign_ledger.py`. Each slice record lists the
differences from Java it left and what remains.

## Checks

- Every landed batch passed `scripts/remote-check.sh --full` (clippy with
  `-D warnings`, every test, and the `--no-default-features` checks) on a
  Namespace instance, except the last one. Its run was interrupted, so the
  merged tree, together with the renumbering and the README, was checked
  again before the pull request (see the pull request).
- Merge conflicts were almost all appended sections and list entries. Two
  needed code fixes: a duplicate `Item::is_uncommon` from ITEM-EQUIP and
  ITEM-02, and an alerted neutral mob's target choice where EFX-RULE-21 met
  MOBN-06.

## What was not checked

Nothing in this wave was played, by the owner or by an agent. The README
screenshots of 2026-11-13 ran the merged build up to EFX-INST-02, but they
show terrain, structures and older mobs, not this wave's content. No
frame-time run was made. Every row stays Implemented, unverified until the
owner has played it.
