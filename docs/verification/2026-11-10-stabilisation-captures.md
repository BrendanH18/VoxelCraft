# Stabilisation captures: the v0.14 to v0.16 content the first pass missed — 2026-11-10

[Roadmap gate](../roadmap.md#stabilisation-gate-before-v017) · [First pass](2026-11-09-stabilisation-pass.md) · [Audit](2026-11-09-project-audit.md) · [Documentation index](../README.md)

The [2026-11-09 stabilisation pass](2026-11-09-stabilisation-pass.md)
drew villages, an outpost, a ruined portal, trail ruins from above, a warm
ocean ruin, an ancient city, a trial chamber and the new mobs. This run
covers what it listed as not captured: bastions, Nether fossils, desert
wells, buried treasure, cold ocean ruins, the inside of trail ruins, the
v0.15 blocks, ocean-ruin brushing, and the v0.16 items and blocks. It is
agent evidence under [roadmap §7](../roadmap.md#7-feature-completion-and-release-policy):
the game starts and draws the content. It is not the owner's play and
moves no row to Verified.

## Builds and method

- **Source:** `t3/finish-stabilisation-gate` at `277a6355`. No game code
  changed in this run, so every capture comes from one build.
- **Build:** `cargo build --release --locked --bin voxelcraft` on the
  owner's M5 in the shared target directory after `touch build.rs`. The
  binary is kept as ignored `target/stabilisation-captures/voxelcraft-final`.
- **Locating:** `examples/locate_structures.rs` now also prints the nearest
  desert well, the buried treasure chest's depth, the trail ruin's piece
  bounds, the nearest Nether fossil and the eight nearest bastions with
  their kind. It ran on a Namespace instance for seeds 42, 7, 12345 and 9:
  `scripts/remote-check.sh -- 'cargo run -q --release --no-default-features --example locate_structures -- <seed>'`.
- **Scenes:** `--screenshot` runs at 1280 × 720, `--rd 6`, noon, clear,
  `--no-hud`, each in a new creative world under ignored
  `target/stabilisation-captures/<scene>/`. Nether scenes add
  `--dimension nether`. Poses are `x,y,z,yaw,pitch`.
- **Digging:** buried content was exposed with `--place x,y,z,air`, which
  the game already offers. Cells must be listed top down: the first
  attempts cleared bottom up, so each removed cell dropped the sand or
  gravel above it into the hole and buried the chest again. That is
  ordinary falling-block behaviour, not a defect; the
  [development guide](../development.md#screenshots-and-scripted-scenes)
  now says so. Cuts beside the sea flood, as they would in Java.
- **Checked by:** Claude Opus 5.5 looking at every image (about 45). Ten
  are tracked as `docs/images/2026-11-10-*.jpg`; the rest stayed in
  `target/stabilisation-captures/shots/`.

## What was captured and seen

| Scene | Seed and pose | Seen |
|---|---|---|
| Treasure bastion | 9, `11,55,237,-45,-53` (the v0.14 record's pose) | The same layout as the v0.14 image: gold island, magma-cube spawner, chest, lava channel, chains. Now with two sword piglins and three hoglins on the inner steps; the hoglins are natural crimson-forest spawns, which Java allows there (`bastion_remnant` has empty spawn overrides) |
| Bridge bastion | 7 (bastion at 638,33,344), `622,52,360,-45,-30` | Rampart with polished basalt pillars, a chest, gold and a piglin; huge crimson fungi grow against the tower and hang weeping vines over it without replacing its blocks |
| Stables bastion | 7 (142,33,237), `112,75,267,-45,-38` | Central blackstone hall standing in the lava lake with ramparts and pillars either side |
| Housing bastion | 42 (294,33,457), `264,75,487,-45,-38` | Open-topped housing block standing in lava, piglins on its walls, ramparts and chests below |
| Nether fossils | 42 (-444,36,774), `-456,50,786,-45,-35`; 7 (0,57,11), three poses; 9 (716,60,141), two poses | Seed 42: a rib cage resting on soul soil beside the lava sea, as Java places them. Seed 7: mostly buried in a valley slope, a few bones showing. Seed 9: a rib cage hanging off a soul-soil ledge over the lava lake, most of it in open air (defect 1) |
| Desert well | 7 (-1122,65,549), `-1129,74,556,-45,-40` and two more | Sandstone roof, four pillars, water basin, slab rim, on a terraced desert slope. Part of the rim meets a one-block step in the terrain, which Java's `DesertWellFeature` also allows |
| Buried treasure | 9, chest at -1015,57,-535; pocket carved top down, `-1009.7,57.6,-535.5,180,-25` | Chest five blocks under a beach, resting on stone with sand filled around it as `BuriedTreasurePieces` does |
| Buried treasure loot | 9, survival, `--open-block -1015,57,-535` | Heart of the sea, iron and gold ingots, emeralds, two TNT, an iron sword, cooked cod and salmon, and a water-breathing potion, all entries of Java's `chests/buried_treasure` table |
| Cold ocean ruin | 7 (344,42,136), `334,52,146,-45,-25` | Stone, mossy and cracked stone brick walls and an arch on the sea floor among seagrass and kelp. Seed 42's nearest cold ruin is hidden in a kelp forest; only one wall corner showed |
| Trail ruins, cut away | 12345 (99,74,275), 8,372 cells cleared top down in front of the ruin, `95,90,262.5,90,-30` | Cross-section of the tower and a room: terracotta, mud bricks, packed mud, bricks, coarse dirt, and gravel and dirt filling the insides. The crown's top is two blocks under the grass, matching the locator's ground height 88 against a crown top of 86 |
| Trail ruins, seed 42 | -397,48,109, cut with glass dams | The ruin's walls appear in the cut, but a lake next to it floods most of the trench; the seed 12345 cut replaces it |
| v0.15 blocks | 42, placed on a stone platform at y 150 | Bee nest, beehive, honey and honeycomb blocks, creaking heart (uprooted: bark sides, ring ends), copper golem statue, the three froglights, block of resin, resin bricks, copper chest, frogspawn on water, open and closed eyeblossoms, melon and pumpkin stems and beetroots on farmland, resin clump |
| v0.16 blocks | same platform | Sculk, sculk sensor and calibrated sensor (half height with tendrils), shrieker, catalyst, reinforced deepslate, vault, ominous vault, trial spawner, heavy core, jukebox, sculk vein |
| Ocean-ruin brushing | same platform, `--brush` at 3, 6, 8 and 9 strokes | Suspicious sand and gravel with their rewards emerging from the top face (sherds, wheat, coal, emerald); an item seen edge-on is a thin line, as Java draws it |
| v0.16 items | survival, `--give` and `--open-inventory` | Mace, trident, diamond, netherite, copper and wooden spears, jukebox, discs 5, Pigstep, Creator and Precipice, trial key, ominous trial key, ominous bottle, heavy core, wind charges, breeze rods, echo shards, recovery compass, disc fragments, and the sculk and trial blocks, all with their own icons; the held mace draws in first person |

![Bridge bastion with piglin, chest and crimson fungi](../images/2026-11-10-bastion-bridge.jpg)
![Nether fossil on soul soil, seed 42](../images/2026-11-10-nether-fossil.jpg)
![Nether fossil hanging over the lava lake, seed 9](../images/2026-11-10-nether-fossil-overhang.jpg)
![Desert well, seed 7](../images/2026-11-10-desert-well.jpg)
![Buried treasure chest in a carved pocket, seed 9](../images/2026-11-10-buried-treasure.jpg)
![Buried treasure loot](../images/2026-11-10-buried-treasure-loot.jpg)
![Cold ocean ruin, seed 7](../images/2026-11-10-cold-ocean-ruin.jpg)
![Trail ruins cut away, seed 12345](../images/2026-11-10-trail-ruins-cutaway.jpg)
![v0.16 items in the inventory](../images/2026-11-10-v016-inventory.jpg)
![v0.15 and v0.16 blocks](../images/2026-11-10-v015-v016-blocks.jpg)

No run crashed or logged a warning or error, and every `--place` applied
(up to 15,004 in one scene). No block or item drew with a missing texture.

## Problems found

| # | Problem | Disposition |
|---|---|---|
| 1 | **Nether fossils don't adapt the terrain.** Java's `nether_fossil` structure has `"terrain_adaptation": "beard_thin"`, and the Nether's `final_density` includes the `beardifier` (both checked in the 26.3 client jar's `worldgen/structure/nether_fossil.json` and `worldgen/noise_settings/nether.json`), so Java builds ground up under a fossil. VoxelCraft places the template with no adaptation, so a start on a ledge leaves the fossil in the air: seed 9's fossil at 716,60,141 hangs over the lava lake | Not fixed: it needs a beard term in generation 3 Nether density, the same kind of work as the [terrain blending](2026-11-09-structure-terrain-blending.md) change and with its own GEN-07 checks. Recorded as a gap for the roadmap |
| 2 | **Dry dirt flats in swamps.** Swamp columns whose surface is exactly y 62 (sea level − 1) get dirt with air above: `surface_block` in `src/world/terrain.rs` treats `y < SEA_LEVEL` as under water, but the sea fill stops at y 62, so these columns are dry. Java gives such ground grass, and its swamp surface rule turns noise-selected y 62 cells into water. Seen as wide bare brown plains beside the sea at seed 42 (-279,62,-136; pose `-330,150,-90,-45,-45`) and seed 9 (-1040,62,-540). A headless probe showed identical blocks in generations 2 and 3 | Not fixed: v0.13 terrain, not v0.14–v0.16 content, and generation 2's golden hashes pin it. A generation 3 fix would be small but changes terrain; recorded as a gap |
| 3 | **The F3 overlay's columns overlap.** The right column's entity line (about 95 characters) is drawn over the left column's `Biome:` line at any window width tried (1280 and 2400 pixels wide), and at 1280 × 720 the `Facing` and `Water` lines touch too | Not fixed: a debug-screen layout issue older than v0.14. Recorded for the HUD work |
| — | Bottom-up `--place air` cuts refill with falling sand | Not a defect (Java does the same); the guide now says to clear top down |

## What this did not check

- Anything that needs input: fighting piglins and the bastion's brutes,
  opening a bastion chest, brushing by hand, trading for or following an
  explorer map to the treasure, the trial keys in a vault, the jukebox
  playing, the mace and trident in use, and save and reload.
- Sound and music.
- Natural generation of the v0.15 blocks: bee nests on trees, creaking
  hearts in pale gardens and eyeblossom patches were placed by script, not
  found in the world. Earlier per-track records captured some of them
  generated.
- Suspicious sand and gravel inside generated ocean ruins (the brushing
  scene placed its own blocks), and a warm ruin's interior.
- The Nether's ruined portals, a bastion's interior rooms other than the
  treasure hall, and the fossil processors' coal and diamond overlays in
  Overworld fossils.
- Frame time.
