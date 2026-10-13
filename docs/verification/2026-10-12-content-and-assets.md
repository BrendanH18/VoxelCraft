# Audited content integrations and original resource pipeline

This report extends the [initial assessment](2026-10-12-v0.7-assessment.md).
The retained `f5bf0d1` work was merged at `22ad25c`; command/entity module
extraction followed at `48a095e`. The content/resource changes accompanying
this report are on `work/v0.7-assessment`. They do not establish a 0.7 release
or completion of the whole modern Java catalogue.

## Reference and delivered behavior

Reference: official Minecraft Java 26.3 server artifact, SHA-1
`33680f5f2ac32864d6d7cf5e56a705fdb3e05f4c`.

- `tests/building_integrations.rs` checks 29 independently extracted Java
  recipes for pale oak/bamboo building members, bamboo mosaic and its stairs/
  slab, mud brick/prismarine shapes. It verifies consumption, drops, material
  tools, fuel and orientation, and rejects nonexistent Java family members.
- `tests/plant_integrations.rs` checks the reviewed compost catalogue against
  Items' provider registrations, including kelp, berries, moss, flowers and
  version-specific exceptions. Ripe cocoa consistently gives three beans;
  budding amethyst drops nothing with ordinary tools, Fortune or Silk Touch.
  Fishing includes the reference 60/25/2/13 cod/salmon/tropical/puffer weights;
  the retained v0.7 treasure pool already includes nautilus shells.
- Dry sponges absorb connected source/flowing water within six steps, with a
  64-cell bound, become wet, and react when neighboring water arrives later.
  Solid barriers stop traversal. Wet sponges dry on placement in the Nether;
  the existing furnace drying recipe is retained. The current representation
  of permanently waterlogged coral/pickles does not support draining their
  water while retaining a dry block; that broader waterlogging gap remains.
- Dark oak growth requires a 2 x 2 sapling group. Mangrove leaf bone meal
  creates a hanging propagule; four age steps produce a harvestable item.
  Unsupported mature propagules drop correctly. Existing approximate tree
  layouts and static propagule appearance remain under GEN/UX requirements.
- Shared static content metadata now owns plank eligibility, fuel,
  composting classes and fishing fish weights. Lookups do not parse names or
  allocate in their gameplay calls. Specialized growth/placement remains in
  the world systems. Recipe and registry consolidation continues under STR-02.

Reproduce the fixtures without using Rust output as expected behavior:

```sh
python3 tools/extract_java_audit_fixtures.py /path/to/server.jar --output-dir target/reference-audit
diff tests/fixtures/java/26.3/building_integrations.json target/reference-audit/building_integrations.json
diff tests/fixtures/java/26.3/plant_integrations.json target/reference-audit/plant_integrations.json
```

The extractor verifies the official artifact hash and requires JDK `javap`
for compost registration associations. Both reproduced files matched exactly.
Game binaries and inspection dumps remain under ignored `target/`.

## Disk and process restart

`tests/process_persistence.rs` starts separate writer and reader processes.
It exercises the actual level/chunk files and save lock, interrupted fuel and
pufferfish brewing, scheduled composter readiness, new building IDs and hanging
propagule age, inventory, saddled donkey ownership/storage, a fox's carried
berries and baby turtle age. The reader completes the interrupted work without
re-consuming the inputs. While the writer holds the save lock it starts a
third, separate process that must fail to lock the same world with
`WouldBlock` (corrected 2026-10-13: the original check locked twice inside
one process). This complements the retained v0.7 lifecycle/LAN tests;
it does not establish all old-version migration or cross-machine acceptance.

## Original resources

`assets/manifest.json` records stable keys, provenance, licenses and runtime
mappings. The build embeds every declared file; required missing files and
duplicate keys fail with the asset key. The original stone texture is used in
world/held/dropped/inventory rendering, its box model supplies item geometry,
and the original pickup cue replaces the mapped audio-bank sample. Unmapped
resources retain their procedural generation. Static model format limits are
explicit; entity animations/music still use their original Rust implementations.

`tools/validate_assets.py` checks formats, references and provenance. A release
executable successfully validated all three embedded resources from a temporary
working directory outside the checkout: 9,993 embedded bytes, approximately
0.05 ms validation on this M5. That tiny CLI check is not startup/gameplay or
perceptual-quality acceptance. Screenshots, ordinary controls, listening and
representative performance still need their own evidence.

## Checks and remaining work

At the checkpoint preceding this report, formatting and strict release Clippy
passed. Desktop release tests passed **854 tests**, three ignored; the separate
process persistence tests passed. Headless release checks also passed. Dated
final evidence will identify the complete delivered source and subsequent tests.

The initial five-scene benchmark runner has completed a smoke run. Its short
samples are deliberately ineligible for the performance gate. Full-length,
repeated baselines and native-resolution measurements remain separate work.
The project has no new claim of 240 FPS acceptance.

Full shared actions/session ownership, remaining audit mechanics (including
light/spawning/navigation, geodes and cave interactions), broader registry/
recipe metadata and player acceptance are still open. No entire roadmap
requirement is promoted to Verified by these tests alone.
