# Java 26.3 brewing, trade names and registry-gap ownership

Reference: Minecraft Java Edition 26.3. Implementation worktree:
`t3/brewing-trades-registry-gaps-1`; the handoff identifies the final commit.
This record concerns PRO-02, PLY-03b, AUT-04a, village trading and the carried
BLD catalogue gaps. It establishes automated data/behavior evidence, not player
or performance acceptance.

## Changes and sources

- Extracted `tests/fixtures/java/26.3/brewing.json` from the official server
  artifact, SHA-1 `33680f5f2ac32864d6d7cf5e56a705fdb3e05f4c`, with the new
  `tools/extract_java_brewing_fixtures.py`. The argument is a caller-supplied
  `/path/to/server.jar`; no machine-local reference directory is committed.
  The extractor was read alongside the existing turtle and consumption
  extractors and fixture README. **26.3 has no `PotionBrewing` class**: the
  sources are `VanillaBrewingProvider`, `BrewingProvider.buildStartMix`, the
  279 brewing recipe JSON files, and `Potions`. Provider expansion must equal
  the JSON recipes exactly; unknown forms fail.
- Added fermented spider eye and its procedural icon and shapeless recipe
  (spider eye + brown mushroom + sugar), independently specified by the jar's
  `recipes.json` fixture. Implemented golden-carrot Night Vision, rabbit-foot
  Leaping, fermented-eye Weakness and all reference corruption mixes, plus
  missing water-to-Mundane start mixes. Removed the non-Java water + phantom
  membrane → Mundane mix, discovered by the reverse comparison. Added both Invisibility durations and
  the Invisibility effect/icon. The two-way brewing comparison covers normal,
  splash and lingering bottles, including their saved potion components.
- Allocated items 1530–1534; 1535–1539 and texture layers 3952–3967 remain
  reserved. `docs/architecture.md` records the allocation and compatibility
  boundary. Legacy potion items and effects retain their indices/IDs. Effects
  persist by Java string ID; lingering/tipped components store potion index + 1.
- Player skin and nameplates disappear under Invisibility in third-person,
  split-screen, CLI-agent and LAN-player views; armor and held items remain
  visible. Target snapshots include occupied armor slots and sneaking. Acquisition
  uses Java's visibility factor and minimum range, independently extracted from
  `LivingEntity.getVisibilityPercent/getArmorCoverPercentage` and
  `TargetingConditions.test`. Integrated with ordinary hostile targeting,
  blaze/ghast/breeze acquisition, piglin/hoglin sensing, guardians, pillagers and
  phantom acquisition. Wardens retain their nonvisual sensing.
- Corrected trade strings `raw cod`, `raw salmon`, `scute`, `cod bucket` to
  `cod`, `salmon`, `turtle scute`, `bucket of cod`; also corrected the butcher
  trade `rabbit` → `raw rabbit` (**eight rows**, five distinct names). No villager
  trade fixture exists in the reference directory; this does not verify trade
  quantities, prices or selection against Java.
- Added the authored `registry_gaps.json` ownership ledger and CI tests. The
  audit and test share `src/java_registry.rs` identity aliases. Crop blocks,
  grass, snow, wall torches, iron chain, potion containers, goat horn and the
  existing trial-chambers map resolve explicitly. `frosted_ice`, `wither_rose`,
  `nether_gold_ore` and `golden_dandelion` were inspected and have no block
  registrations: they remain real gaps. The wither-skeleton-skull **item** does
  not hide its missing placeable block.
- Fixed wandering-trader `slime ball` → `slimeball` (one more dropped offer).
  Structure loot now resolves `cooked_beef` → `steak` and `quartz` →
  `nether quartz`, restoring those previously filtered drops.
  Guarded names in villager and wandering-trader tables and all nested item
  entries of `src/world/loot_tables.json`. Existing bastion `item: None`
  placeholders carry no runtime name lookup; village legacy name tables are
  test-only, and runtime village loot uses the guarded JSON. Typed fishing,
  trial and bastion item tables do not silently filter name strings.

## Checks and counts

- Baseline remote `registry_audit`: **131 / 1,286 blocks**, **318 / 1,658 items**
  unresolved. This is the unchanged branch's actual output.
- Fixture extraction and a second independent invocation produced byte-identical
  `brewing.json`: **41 direct mixes, 11 start mixes, 2 container mixes,
  279 recipes, 46 potion types**. The normal test chain compares every registered
  mix both ways; **259 recipes** are expected to be covered and **20** explicitly
  owned by the four missing effects.

| Name inventory | Total | Before unresolved | After unresolved |
|---|---:|---:|---:|
| Java blocks | 1,286 | 131 | 122 |
| Java items | 1,658 | 318 | 309 |
| Villager trade item names | 239 | 12 | 7 |
| Wandering-trader item names | 36 | 1 | 0 |
| Structure-loot item names | 220 | 26 | 24 |

The trade audit replays the five original labels against the same item registry;
none of the newly registered brewing items occurs in these trade tables.
The loot audit captured 26 before adding the steak/quartz aliases. All remaining
unresolved trade/loot names are actual missing items with ledger entries; the
seven villager names cover chainmail (four), experience bottle, writable book
and globe banner pattern. Name resolution does not prove acquisition parity.

`VC_REMOTE_KEEP=1 scripts/remote-check.sh --full` passed on a Namespace
8-vCPU Linux instance, with the normal formatting step on the worktree and
all builds/Clippy/tests remote (Rust 1.99.1):

| Configuration | Passed | Failed | Ignored |
|---|---:|---:|---:|
| Desktop release | 2,337 | 0 | 21 |
| `--no-default-features` release | 2,132 | 0 | 20 |

Both `cargo clippy --release --all-targets -- -D warnings` configurations
passed. There were **4,469 successful test executions** across the two feature
configurations. The ignored tests remain unverified; this is not performance
or visual acceptance. `java_brewing` (3), `java_registry` (3), `java_recipes` (5)
and `process_persistence` (3) passed in both configurations. The process test
writes an in-progress stand with fermented spider eye and a player with
Invisibility to disk, starts a separate reader process, verifies the saved
ingredient/effect and finishes brewing to the new potion item. Legacy potion
ID assertions and existing persistence cases remain covered.

The final remote audit examples were run on that same retained instance after
the full chain. The instance was then destroyed. Fixture reproduction was
byte-identical, JSON parsed successfully, changed documentation links resolved,
and `git diff --check` passed. Early checks found and fixed a recipe constant
lifetime, a helper placed after a test module, the extra phantom-membrane mix,
and the unowned runtime names described above; the final chain has no failures.

## Remaining boundaries and release ownership

- Wind Charged, Weaving, Oozing and Infested remain unregistered and are assigned
  v0.17; Luck is an unregistered creative-only potion (`none`). Brewing tests
  require these omissions to stay owned, rather than silently skipping them.
- The ledger applies the audit's v0.17/v0.18/v0.19/v0.20/deferred/none rules.
  Poplar, shrubs, cinnabar, sulfur, potent sulfur, sulfur spike, shelf mushroom,
  golden dandelion and potted forms are assigned v0.18. Existing-destination
  cartographer maps (ancient city, mineshaft, desert pyramid, warm ocean ruins)
  are carried v0.17 village gaps; missing-destination maps remain v0.18.
  Three names were not explicitly covered by the audit's assignment list:
  **frosted_ice** is provisionally assigned v0.18 terrain closure, and
  **blue_egg / brown_egg** v0.17 carried farming gaps. Integrator should confirm
  these placements; none are left without an owner.
- Invisibility does not establish full AI parity: existing simplified target
  retention/provocation rules remain (the Java goal path and guardians keep
  acquired targets, while some legacy AI resenses every update). Mob disguises,
  glowing outlines, spectator/team translucency, particles and the waypoint
  transmitter attribute are outside this repair. General effect-driven hiding of
  non-player mob models is not implemented (the wandering trader retains its
  existing separate invisibility behavior). First-person bare-hand skin
  visibility has not been changed. These are explicit remaining PRO-02/PLY-03b
  presentation/status interactions.
- No windowed game, hands-on play, screenshots, M5 benchmarks, physical-device
  sessions or visual icon review were run. All builds/tests use Namespace via
  `scripts/remote-check.sh`; no software was installed or builds run on the Mac.
  Mesh tests check geometry suppression/retained equipment, not rendered pixels.
  No claim of 240 FPS, ordinary-player acceptance or complete potion parity.

## Proposed roadmap/requirement updates (integrator-owned)

Keep PRO-02 and PLY-03b partial: link this record for the 279-recipe fixture,
registered brewing parity, Invisibility slice and disk save/reload evidence;
retain the four missing effects and listed status/render interactions. Link
AUT-04a to interrupted brewing persistence and the recipe/container coverage.
Record the eight restored trade rows under village/trading evidence without
claiming reference trade-data parity. Link BLD-01a/01b and catalogue ownership
to the registry ledger, explicitly carrying poplar/cinnabar/sulfur to v0.18.
Add/confirm the three provisional placements above and existing-destination maps
in v0.17. Do not mark these rows Verified without player/performance evidence.
