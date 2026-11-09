# Project audit — 2026-11-09

[Roadmap](../roadmap.md) · [Requirements](../requirements.md) · [Documentation index](../README.md)

The owner asked for an audit of the project against the roadmap and against
Minecraft Java Edition, then asked for every suggested improvement to be
carried out. This record holds the audit's evidence and findings. What was
done about each finding is in [the response](#response) at the end.

## Scope and evidence

- **Source:** `main` at `c96240b` (Cargo 0.13.0; v0.14, v0.15 and v0.16
  merged and untagged).
- **Reference:** Java Edition 26.3, released 2026-09-15. A web search on
  2026-11-09 found 26.4 still in snapshots (Snapshot 4, 2026-10-14), so
  26.3 is still the latest stable release.
- **Audits run** on a Namespace instance with
  `scripts/remote-check.sh -- 'cargo run --release --no-default-features --example <name>'`
  for `registry_audit`, `recipe_audit` and `block_loot_audit`.
- **Read:** the roadmap, the requirement inventory, the v0.12 to v0.16
  verification records, the tracked screenshots in `docs/images/`, and the
  source where a finding needed confirming.
- **Not done by the audit itself:** launching the game, a frame-time run, a
  play-through. The two defects below were found by reading code, then
  confirmed by the tests added in the response.

## What the audits measured

| Check against Java 26.3 | Result at `c96240b` |
|---|---|
| Crafting recipes compared | 1,082 of 1,083 match; 25 allow-listed; 159 need unregistered items |
| Cooking recipes | 113 of 113 match; 3 need unregistered items |
| Stonecutting recipes (data only; the screen is v0.17) | 304 of 304 match; 47 need unregistered items |
| Smithing recipes | 10 of 10 match; 20 need unregistered items |
| Block drops | 1,162 blocks, 111,507 state and tool cases, no failures; 124 blocks unregistered |
| Block names | 1,155 of 1,286 resolve (131 missing) |
| Item names | 1,340 of 1,658 resolve (318 missing, 88 of them spawn eggs) |
| Mob kinds | 75 of the 88 with a Java spawn egg (74 `MobKind`s and the dragon) |
| Brewing | Not compared by any audit |

CI was green on `main` (about 2,350 tests). The requirement inventory held
131 rows: 1 Verified (PERF-01, the benchmark baseline), 54 Implemented,
unverified, 60 Partial, 10 Missing and 6 Deferred.

## Findings

### 1. Three versions were merged and never run

`main` carried v0.14, v0.15 and v0.16, about 60,000 lines added to `src/`
and `tests/` since the `v0.13` tag, with every row waiting for the owner's
play. The [v0.16 integration record](2026-11-07-v0.16-integration.md) says
"Not run: the game, screenshots, the headless benchmark scenes, a
frame-time run, a LAN session". The release plan says to work on the
earliest unreleased version; work was two versions past it.

### 2. The frame-time gate was failing and unmeasured

The last valid windowed run was on 2026-10-29
([record](2026-10-29-v0.12-finish.md#frame-time)): `materials` at a 1% low
of 137.8 FPS and p99 4.54 ms against the gate's 240 FPS and 4.17 ms.
Offscreen comparison runs that day put `building`'s 1% low at 229, down
from 261–273 in the [2026-10-16 baseline](2026-10-16-m5-baseline.md). The
profile blamed `Entities::mesh` and `Renderer::set_entities`, which rebuilt
and re-uploaded every mob's vertices each frame, and that code was
unchanged. v0.13 to v0.16 then added density terrain, about 20 mob kinds
and sculk vibrations with no frame-time run, and every release's
performance check had been moved to v0.20.

### 3. Structures are not Java's, with no recorded exception

Roadmap §1.3 makes Java's structure layouts part of the fidelity contract.
v0.14 to v0.16 built structures from original, reduced templates: villages
have two town centres, five street pieces, two houses, a farm, a pen and a
lamp, with no profession buildings; trial chambers use 22 templates;
ancient cities are smaller than Java's. The decision was left "until the
owner decides at release". Nothing blended terrain around pieces, so they
stood on pillars and in vertical cuts, as `docs/images/v0.14-village-desert.jpg`
and `v0.14-village-taiga-street.jpg` show.

### 4. Java 26.3 content that no release owned

- **Dropped:** the v0.12 scope said poplar wood, cinnabar and sulfur would
  ship with their terrain in v0.13. v0.13's scope didn't list them and no
  later version did.
- **Mobs with no release:** happy ghast (with dried ghasts and harnesses),
  nautilus and zombie nautilus (with nautilus armor), camel husk, parched
  and sulfur cube.
- **Items and blocks with no release:** bundles, spyglass, armor trims,
  chainmail armor, mob heads, infested blocks, experience bottles, spectral
  arrows, nether gold ore, conduit, abandoned camps and their maps, copper
  and netherite horse armor, written books.
- **Not reassessed:** dungeons, mineshafts, strongholds (GEN-05c), and
  shipwrecks and monuments (GEN-05e) were targeted at v0.14 and left
  unchanged.
- **Small behaviours missing:** zombies and other mobs don't hold weapons
  or pick up equipment, and there are no jockeys or zombie reinforcements.

### 5. The requirement inventory had drifted

Rows still described the state before v0.14 to v0.16: PLY-05d listed
tridents, maces and spears as "Not registered", GEN-05j listed ancient
cities and trial chambers as "Not started", AUT-01b said sculk sensors were
missing, and PRO-03 listed eight enchantments that now exist. Rows targeted
at v0.14 and v0.15 stayed Partial with no new target.

### 6. Two defects, both in name-keyed data no audit covered

- **Brewing.** `world::brewing::brew` had no mix for Night Vision, Leaping,
  Weakness, Slowness or Harming, although the potions existed. Fermented
  spider eye and the Invisibility potions did not exist. A player could not
  brew the Weakness potion that curing a zombie villager needs.
- **Villager trades.** `entity/villager_trades.rs` named "raw cod", "raw
  salmon", "scute" and "cod bucket"; the items are "cod", "salmon", "turtle
  scute" and "bucket of cod". `TradeDef::available` skips a trade whose
  name doesn't resolve, so seven fisherman, cleric and leatherworker trades
  never appeared. Six more trades name items that aren't registered.

Both come from the same weakness: tables that refer to items by name skip
unknown names silently, so a typo and a missing item look the same.

### 7. The roadmap had become a history file

`docs/roadmap.md` was 2,227 lines, most of them delivery records, dated
follow-ups and the scopes of tagged versions, against its own rule to
"keep the active roadmap focused on present scope". v0.20 had also become
the destination for every deferred frame-time check and every row's
verification.

## What is in good shape

- Data-level fidelity is strong and reproducible: the fixtures are
  extracted from the official 26.3 jars by the tools in `tools/`, and the
  table above has no mismatches.
- Player movement, navigation, spawning and lighting are ports of the Java
  classes, with fixtures.
- Save safety is being followed: append-only IDs, per-track reservations
  and world generations pinned by golden chunk hashes.
- The reference version is current.

## Response

RESPONSE_PLACEHOLDER
