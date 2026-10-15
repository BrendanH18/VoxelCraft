# Gameplay skylight and cave spawning — 2026-10-13

[Roadmap](../roadmap.md) · [Restructuring](../restructuring.md) · [Reference fixtures](../../tests/fixtures/java/26.3/README.md)

Scope: ENV-05, MOB-01, STR-01/05, after `41a7290` on
`work/v0.7-assessment`. Implementation and this record accompany the same commit.
Reference: official Java Edition 26.3 server SHA-1
`33680f5f2ac32864d6d7cf5e56a705fdb3e05f4c`; tests ran on the M5 MacBook Pro.
State: **Partial**. This closes the surface-only ground-monster attempt and
torch-suppression defects for the implemented Overworld cycle; it does not
establish complete Java lighting or natural spawning parity.

## Delivered behavior

`World` owns propagated sky and block light independently of render jobs.
Complete column heightmaps imply direct level-15 sky sources. Separate packed
nibble arrays store indirect skylight below them only where lit. Removal and
increase queues resolve edits, streaming replacement, unload and reload without
waiting for rendering. Unknown sky beside incomplete/unloaded neighboring
columns returns `None`, preventing a spawn based on guessed darkness.

The dimension skylight flag is separate from the existing visual-sky/weather
flag: the Overworld and End have a skylight layer; the Nether has none. Saved
block edits rebuild light in a new process. Light arrays and queues are derived
state; saved block/item IDs and persistence formats are unchanged.

The registered Overworld ground-monster cycle now samples starts throughout a
column's height instead of only its sky-exposed surface. Reference biome lists,
pack walks, floor/occupied-cell checks and the two random light tests determine
eligible spawns. Dark caves can spawn during daytime; any positive block light
suppresses these monsters. Replacing/removing torches changes eligibility.
Swamp slime light checks also read propagated sky and block light.

Gameplay sky darkening uses the reference's 24,000-tick day timeline, linear
float interpolation and weather modifier/state-change blends. It is supplied
explicitly through `Ctx::spawn_light`, rather than reconstructed from the
renderer's visual daylight. Thunder uses the monster test's fixed darkening of
ten. Existing clock/weather transition durations remain unchanged; this is not
a claim of complete time/weather parity. Programmatic callers constructing
`Ctx` must supply `SpawnLight::DAY`, `NIGHT` or `at(...)`; in-repository callers
and both feature configurations were updated together.

## Independent evidence and recovery checks

`tools/extract_java_lighting_spawning_fixtures.py` verifies the official hash,
extracts biome/dimension/timeline/tag data and checks relevant bytecode forms
(constants and call shapes, not decompiled behavior; see the
[review fixes](2026-10-13-review-lighting-fixes.md) for the evidence levels).
Its independent fixed-point light model uses no Rust output. Six scene families
cover sealed caves, torch removal, shafts/covers/tunnels, overhangs, loaded
borders and occupied cells/floors. The fixture also records **114** clock/weather
cases including twilight boundaries, midnight wrap and partial weather levels.
Reproduction from the pinned artifact is byte-identical.

Recovery of the unfinished delegated patch exposed a real bottom-slab floor
error: it was treated as a full block. That was corrected. A night expectation
used double precision where the reference multiplies floats, and a cave test
required an unsupported absolute spawn count; their expectations were corrected
without weakening the actual cave/torch/floor rules. The proposed twilight
mapping was replaced with the independently sourced clock/weather rules.

Tests check reference light levels and spawn probabilities, real daytime cave
spawns, torch suppression/removal, Peaceful, floor exclusions, biome list values,
mesh/fresh-rebuild agreement, edit covers, partial loads and unloaded borders.
One representative unload case recorded a nonzero queue peak below 50,000;
this is scenario evidence; the per-tick time box is described below.

The new two-process storage check covers both the Overworld and End. It saves a
glass-covered shaft, tunnel and torch using the real disk format. A new process
restores their sky/block levels; subsequent torch removal, opaque covering and
reopening remove and restore the appropriate light paths.

The required formatting, strict release Clippy and release-test chain passed:
**894 passed, zero failed, three ignored**. Strict headless release Clippy/tests
passed: **812 passed, zero failed, two ignored**. These totals include the prior
shared-session/mining and retained v0.7 regressions.

The real desktop LAN regression harness subsequently passed **13/13** checks on
one Mac. The [full M5 scene baseline](2026-10-13-m5-baseline.md) measured this
commit. It has since been corrected as invalid (paused, occluded-window and
population-loss runs) and must be re-measured. Individual lighting cost
attribution and ordinary-player acceptance remain open.

The benchmark source fingerprint now includes the mining fixture embedded by
`build.rs`; changing this runtime input must change the recorded source identity.
An isolated temporary fixture check confirmed that change and repeatability.

## Remaining differences and acceptance

The existing 24–64-block ring, per-kind caps, quarter-second spawn batching,
activation/despawn policies and structure overrides still need Java correction.
(2026-10-14: Overworld monsters now use Java's 128-block spawnable chunks and
monster category caps, and every despawnable mob Java's `checkDespawn`; see the
[spawning record](2026-10-14-natural-spawning.md).)
Missing roster entries (zombie horse, stray, bogged, parched, ocelot) retain their
reference selection weight but cannot spawn; slimes and drowned now walk the same
pack cycle (see the v0.7 follow-up below). No unsupported
mob is replaced with a different species. These are open differences, not
approved fidelity exceptions or a completed MOB-01 claim.

Slabs/stairs retain the prior opaque-cell light treatment; full state/shape
occlusion is unfinished. Meshes still independently flood snapshots rather than
consume the authoritative gameplay arrays. Broader block property fidelity
remains open.

v0.7.0 defect fixes recorded here: crops, saplings, grass and berries read
gameplay light; entities and particles sample propagated skylight; the day is
24000 ticks with independent rain/thunder timers; slimes and drowned use their
own Java spawn rules; Nether and End lightmaps include Java's ambient term.

Light work is now time-boxed per tick. `World::update_block_light_within` gives
each of a tick's two light passes 2 ms (`LIGHT_TICK_BUDGET`) after at least 512
cell visits, so a streaming or large-edit backlog drains over several ticks
instead of stalling one. Pending flood entries live in `LightUpdates::flood`, so
a pause resumes mid-flood and edits made meanwhile merge correctly (a test
compares a sliced backlog against a fresh full relight). Natural overworld
spawning waits on `World::light_settled` so unresolved cells never read darker
than they are. Edits, block placement and streaming still resolve fully; only
the per-tick simulation passes are bounded. The budget is a fixed wall-clock
value, not tuned against a measured scene.

Normal keyboard/controller cave play, visual/audio review, cross-machine LAN,
representative frame-time comparisons and full M5 1080p/native acceptance remain
unverified in this record. The earlier native interaction attempt failed with
`cgWindowNotFound`; automated world and restart checks do not replace player
acceptance. No version bump, release, main merge or deletion of the original
v0.7 branch/uncommitted spawning patch is included.

## v0.7 entity and particle follow-up

`entity::sky_light`, shared by entities, held items and particles, now samples
propagated gameplay skylight. Only cells with unknown skylight use the existing
heightmap fallback. A wide roof regression checks the open edge and a centre
13 blocks from open sky; unknown-border fallback is also covered. See the
[world-fix record](2026-10-13-world-fixes.md#entity-and-particle-skylight).
Visual/player acceptance remains outstanding.

## v0.7 day and weather timing follow-up

The day is now 24000 ticks (20 minutes). Independent rain and thunder timers
use the extracted Java 26.3 ranges and levels fade by 0.01 per 50 ms tick.
Old day fractions and two/three-field weather entries still load; the optional
fourth weather field retains the independent thunder timer. See the
[world-fix record](2026-10-13-world-fixes.md#day-and-weather-timing) for tests
and remaining weather-command/gamerule limitations.

## v0.7 slime and drowned follow-up

Slimes and drowned no longer sample positions on their own legacy paths; they are
entries of the biome list like the other monsters (the legacy `slime_spot` and
`drowned_spot` are removed). Java sources: `Slime.checkSlimeSpawnRules`,
`Drowned.checkDrownedSpawnRules` and `isDeepEnoughToSpawn`, `SpawnPlacementTypes`
(`ON_GROUND`, `IN_WATER`), the `moon` timeline's `surface_slime_spawn_chance`
(0.5 at a full moon down to 0 at a new moon, equal to the old `0.5 * moonBrightness`),
and the `allows_surface_slime_spawns` (swamp, mangrove swamp) and
`more_frequent_drowned_spawns` (rivers) biome tags. See the
[world-fix record](2026-10-13-world-fixes.md#slime-and-drowned-spawn-rules).
The roster still lacks the mobs VoxelCraft does not have; adding them is new content.
