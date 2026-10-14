# Cooking-fuel inventory and integration — 2026-10-12

Scope: STR-01/02, BLD-02 and AUT-03, after `c1a3680` on
`work/v0.7-assessment`. This implementation and report are committed together.
Reference: Minecraft Java Edition 26.3 official server SHA-1
`33680f5f2ac32864d6d7cf5e56a705fdb3e05f4c`.

## Reference inventory

`tools/extract_java_fuel_fixtures.py` hashes the official artifact, recursively
follows `Items` registration/property callbacks, expands colored collections
and reads their context-int fuel providers. `fuel_integrations.json` contains
all **347 extracted fuel registrations**, plus **52 reviewed non-fuel
exceptions**. The new recursive inspector also reproduces the existing
building/plant fixtures unchanged.

The engine resolves **176 reference fuel items** and **48 non-fuel exceptions**.
All those values match the independent fixture. A loaded crossbow, represented
by a separate legacy Rust item, shares the unloaded crossbow's 300 ticks.

**171 reference fuel names have no current item registration.** They remain
missing content under M0/BLD-01/02, not passing fuel integrations. The list
includes stripped overworld logs/wood blocks, unimplemented wooden switches,
signs/hanging signs, scaffolding, coal blocks, banners, shelves, poplar,
new colored wool shapes/cushions and other modern content. The machine-readable
inventory emits every unresolved name:

```sh
cargo run --release --no-default-features --example fuel_inventory > target/fuel-inventory.json
python3 tools/extract_java_fuel_fixtures.py /path/to/server.jar --output target/java-fuel-integrations.json
diff tests/fixtures/java/26.3/fuel_integrations.json target/java-fuel-integrations.json
```

The example exits nonzero on a mismatch among implemented registrations and
reports unresolved names separately. It does not substitute Rust output for
reference expectations. New item registrations automatically join the fuel
comparison as their reference names become resolvable.

## Corrections

The full inventory first failed on missing boat/raft, sapling/azalea/dead-bush,
wool/carpet, bowl, bow/crossbow/fishing-rod, bookshelf, note-block, daylight
sensor, mangrove-root and oak-switch integrations. It also caught an error in
the earlier assessment's pale-oak/bamboo trapdoor fuel: **300 ticks**, not 150.
That mistake is corrected rather than changing the fixture.

Shared typed fuel classes now define exact normal-furnace ticks. Existing
furnaces, desktop/controller GUI transfer eligibility and hopper insertion
read the same fuel function. The CLI now exposes `furnace take|put input|fuel|output` through aimed
furnace slots. Fuel eligibility uses the shared catalogue; output is take-only,
capacity failures retain items and XP, and successful output removal releases
stored XP as orbs at the player. Input changes reset cooking progress. The
regression resumes a half-finished serialized smelt through this command.
Ordinary-player acceptance still belongs to
the shared transaction work under STR-03. Boats burn for 1200; carpets burn for 67. Fast
cooking integer-divides the odd provider values before burning. Nether woods,
leaves, moss carpets, muddy roots and the selected version's beds remain
non-fuels. No saved IDs, fuel timer formats or material meanings are changed.
Reference IDs now resolve the existing bamboo-block label, and `Item::from_name`
accepts the `minecraft:` namespace without losing legacy names.

## Evidence and limits

Four integration tests compare every implemented reference fuel, the non-fuel
exceptions and loaded crossbow; run every implemented fuel through an interrupted
furnace serialize/resume job; verify 67-tick carpet rounding in all three
furnaces; and confirm non-fuels cannot mutate a cold furnace's inventory.
The existing item/building tests continue to pass. Tests print inventory
coverage and retain the unsupported list rather than silently skipping it.

The required `cargo fmt && cargo clippy --release --all-targets -- -D warnings
&& cargo test --release` chain passed: **866 passed, zero failed, three ignored**.
Strict headless release Clippy/tests passed: **781 passed, zero failed, two
ignored**. The release inventory example produced 176 implemented fuels,
171 unresolved registrations, 48 checked non-fuels and zero mismatches.
The normal headless overworld scene completed 200 ticks with 444 loaded chunks,
20 mobs and no render meshes (0.08 ms/tick in this short run); this is a smoke
check, not presented-frame or 240 FPS acceptance. Local before/after failure
logs and generated inventory live under ignored `target/`.

State: **Implemented, unverified** for fuel integration of the existing item
registrations. Ordinary player/container transfer and final representative
performance acceptance remain open. Missing registry content, recipe/acquisition
coverage for that content and complete current-version material families are
separate roadmap work; this report does not claim they exist.
