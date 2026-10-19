# Per-chunk entity storage — 2026-10-17

[Roadmap](../roadmap.md) · [Requirement inventory](../requirements.md) · [Loaded-region activity](2026-10-14-simulation-activity-save-integrity.md)

Scope: PLY-01b gap "unloaded mobs stay in memory instead of per-chunk entity
storage", on `agent/entity-storage` after `77a6885`. Reference: Java Edition
26.3 server (`server-26.3.jar` SHA-1
`33680f5f2ac32864d6d7cf5e56a705fdb3e05f4c`; class extract
`server-26.3-classes.jar` SHA-1 `cc3964451a3b32a0488110d5e3e6ad8d75fe6a66`),
read with `javap -c -p` under ignored `target/`. Classes read:
`world.level.entity.PersistentEntitySectionManager` and its `$Callback`
(`addEntity`, `updateChunkStatus`, `storeChunkSections`, `processChunkUnload`,
`unloadEntity`, `processUnloads`, `autoSave`, `saveAll`, `onMove`,
`updateStatus`, and the `shouldBeSaved` and `getPassengersAndSelf` lambdas),
`world.level.entity.Visibility` (`fromFullChunkStatus`),
`server.level.FullChunkStatus`, `world.level.chunk.storage.EntityStorage`
(`storeEntities`, `loadEntities`), `world.entity.Entity` (`shouldBeSaved`),
`world.entity.Leashable` (`restoreLeashFromSave`), and the members of
`projectile.FishingHook`, `projectile.throwableitemprojectile.ThrownEnderpearl`
(`ticketTimer`) and `player.Player` (`shouldBeSaved` override). Tests ran
headless on the M5 MacBook Pro while other agents were building.

## Java behaviour

- A chunk's entities are in the level while the chunk is at least `FULL`
  (`Visibility.TRACKED`); they tick at `ENTITY_TICKING`. When the chunk becomes
  `HIDDEN`, `storeChunkSections` collects every entity in the chunk's sections
  that `shouldBeSaved`, writes them as one `ChunkEntities` record and unloads
  each of them with its passengers (`RemovalReason.UNLOADED_TO_CHUNK`).
  Loading the chunk again reads the record and adds the entities back.
- `Entity.shouldBeSaved` is false for passengers (they are saved inside their
  root vehicle) and for a vehicle whose only passenger is a player (it is saved
  with that player).
- An entity moving into a `HIDDEN` section stops ticking and being tracked;
  it stays in that section and is stored by the next `autoSave`/`saveAll`.
- A loaded leashed mob whose holder can't be found drops its lead after 100
  ticks (`restoreLeashFromSave`). Thrown ender pearls hold their own chunk
  tickets.

## Delivered behavior

[`entity/chunk_storage.rs`](../../src/entity/chunk_storage.rs):

- An entity is in the level exactly while its 32-block storage chunk is
  loaded. Entities whose chunk unloads, that move into a chunk that isn't
  loaded, or that load from a save into chunks that haven't streamed in yet
  move into a per-chunk store and leave the live lists (no ticking, rendering,
  targeting, collision, caps or LAN replication). `World` reports each chunk
  it installs; that chunk's stored entities rejoin the level unchanged on the
  next tick. Positions above or below the build limit clamp into the top or
  bottom chunk, so they unload with their column as Java's column-keyed status does.
- Stored kinds: mobs (including villagers, golems, trader llamas, tamed and
  leashed animals), dropped items, XP orbs, minecarts and boats, arrows,
  thrown potions, snowballs/eggs, eyes of ender, fireballs and primed TNT.
  Falling blocks are stored by `World` in the same way. Ender pearls (Java
  keeps them with their owner and gives them a chunk ticket), fishing bobbers
  (owner-bound) and explosion smoke stay live.
- A cart or boat carrying a player, a mob carrying a player and a parrot on a
  player's shoulder stay with the player. Mobs riding a cart are stored in
  the cart's chunk, wherever they stand, and come back with it.
- Stored villagers keep their bed and job claims, as Java's POI records keep
  their tickets while the villager is unloaded. The claims are checked where a
  villager picks a bed or job site, so village upkeep never walks the store; a
  bed broken meanwhile is dropped by the existing validation when the villager
  returns.
- Agents, LAN players and split-screen viewers hold chunks loaded through the
  existing streaming radii, so their entities stay live.
- Generation herds keep their saved per-column markers, so a reloaded column
  never gets a second herd; bastion and structure residents keep theirs.
- Saves: stored entities are written into the dimension's existing `items`,
  `orbs`, `minecarts`, `villagers`, `nether_mobs` and `falling` properties.
  The layout and every identifier are unchanged, so older saves load as they
  are and older builds can read new saves. Loading puts every entity in the
  level; the first tick stores those whose chunk hasn't loaded.
  `Entities::save_props`/`load_props` now carry the entity properties for the
  desktop game and for tests. The F3 entity line shows the stored count.
- Work per tick: one chunk lookup per live entity and one map removal per
  newly loaded chunk. Stored entities are never visited, and no disk I/O was added.

## Evidence

- `tests/fixtures/entity_storage/level-77a6885.txt` was written by the
  unmodified `77a6885` serializers: a named tamed wolf, a baby cow, a leashed
  sheared sheep, a chested tame donkey, a persistent piglin, a villager with
  home/job/XP and a name, a built golem, a chest cart with diamonds, a chest
  boat, a pig in a rideable cart, items, orbs and falling sand.
- `tests/entity_storage.rs`:
  - `a_save_in_the_v0_8_layout_loads_with_every_entity_and_reloads_each_chunk`:
    the fixture loads through `Storage::load_level`; nothing is lost or
    duplicated while most entities are stored; a save at that point equals the
    fixture; visiting each saved spot brings that spot's entities back with
    owner, name, health, sitting, baby age, leash knot, chest contents,
    villager claims and XP, golem flag and the pig's cart link; the final
    re-save equals the fixture.
  - `entities_saved_in_unloaded_chunks_survive_a_restart`: entities stored
    after flying 900 blocks away are saved to disk, loaded by a fresh world
    far from them, kept stored, then restored unchanged on return.
  - `flying_away_keeps_only_loaded_entities_live_and_never_regenerates_herds`:
    real generation (seed 20261017, render distance 2), eight 192-block steps
    out and back. No live mob is farther than the keep radius at any step.
    Outbound live/total mobs: 22/22, 17/35, 7/42, 3/42, 0/42, 0/42, 0/42,
    13/55. The total stays at 55 at every step of the return over populated
    columns, and the mobs first seen at the origin come back with the same uids.
  - `agents_and_split_screen_viewers_keep_their_entities_in_the_level`: mobs
    near an agent 1000 blocks away and near a split-screen viewer stay live;
    both are stored when the agent leaves and the view closes.
  - `chunk_entity_sync_cost` (ignored manual benchmark): with 500 live mobs
    and 500 items, a tick's sync with nothing to move took mean 2.41 µs, p99
    2.50 µs, max 4.50 µs over 2,000 calls. Storing all 1,000 at once took
    118.5 µs and restoring them 71.1 µs. Measured during concurrent builds;
    there is no pre-change figure because the step is new.
- Unit tests in `entity/chunk_storage.rs`: unload → reload keeps each mob's
  uid, kind, position, velocity, health, name, leash, cart link, age, baby
  flag, persistence, tame owner/sitting/collar, mount tame/owner/chest/slots,
  villager id/home/job/profession/XP, armour, effects and golem flag. Stored
  entities don't tick, despawn or explode, and a save while unloaded is
  identical. Passengers go with their vehicle; player-ridden carts and mounts
  and perched parrots stay. Entities entering an unloaded chunk are stored
  there. Entities beyond the build limit unload with their column. A homeless
  villager beside a stored villager's bed doesn't take it, the owner still has
  it on return, and it is free once given up. Saved entities wait for their
  chunk after loading. Falling blocks unload and reload with their height and
  speed; a LAN replica never stores the host's falling blocks.
- Full chain run from the worktree: `cargo fmt && cargo clippy --release
  --all-targets -- -D warnings && cargo test --release && cargo clippy
  --release --no-default-features --all-targets -- -D warnings && cargo test
  --release --no-default-features` (result in the hand-off report).

## Remaining differences and open work

- Stored entities stay in memory as structs until the game saves; Java writes
  them to `entities/` region files through its IO worker and reads them back
  asynchronously. The live list, tick work and render work are bounded, but
  the store grows with explored chunks that hold entities, just as unloaded
  player-modified chunks stay in `World` memory until a save
  (`src/world/storage.rs` writes whole-world snapshots). Region-file storage
  for chunks and entities with background I/O is open.
- The unit is the 32-block storage chunk, not Java's 16-block chunk, and an
  entity is stored as soon as its chunk unloads or it crosses into an unloaded
  chunk, rather than at the next autosave.
- Saves still use the global per-dimension lists. Non-persistent hostile
  mobs, arrows, primed TNT, thrown projectiles, eyes and fireballs survive
  unload/reload in memory but, as before this change, are not written to
  disk (Java saves them).
- A fence knot is a mob field, not an entity in the fence's chunk: while the
  leashed mob is stored the knot isn't drawn, and breaking the fence then
  drops the lead only when the mob returns. A trader llama whose trader is stored loses its
  lead line until both are loaded. Leads to an absent player are kept rather
  than dropped after 100 ticks.
- Java's `shouldBeSaved` keeps a vehicle with the player only when the player
  is its sole passenger (`hasExactlyOnePlayerPassenger`); here any cart, boat
  or mount carrying a player stays live, including a boat shared with a mob.
  The player holds that chunk loaded anyway, so this only differs if the
  player leaves the vehicle's chunk while still seated.
- Thrown ender pearls don't hold chunk tickets; they stay live and wait at the
  loaded border as before. Vehicles and shoulder parrots of players who went
  offline stay live.
- The desktop wiring (`Game::update_mobs` calls `sync_chunk_entities` each
  tick) has no headless harness; a player check of flying away and back, the
  F3 stored count and save/quit/reload on desktop, controller and LAN remains.
