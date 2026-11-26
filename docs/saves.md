# Saves and save compatibility

This is the 1.0 save-compatibility policy (v1.0 Track 1, 2026-11-25; rows
UX-05b and UX-04a of the [requirement inventory](requirements.md); region
storage from v1.0 Track 3b, 2026-11-26). It says
which saves 1.0 opens, how an older world is brought up to date, what
happens to data the game doesn't know, how worlds are backed up, how a save
that was cut off or damaged is recovered, and what may never change once
1.0 ships. The [verification record](verification/2026-11-25-v1.0-track-01-saves.md)
lists the tests behind each part, and the
[Track 3b record](verification/2026-11-26-v1.0-track-03b-region-storage.md)
those behind region storage. Where the player data folder is and how
an old source build's saves are imported is in
[releases](releases.md#player-data-and-old-saves); the updater's backups are
in [updates](updates.md).

## What a world is on disk

Each world is a folder `saves/<name>` in the player data folder:

| File | What it holds |
| --- | --- |
| `level.txt` | `key=value` lines: the seed, the player and everything global, the containers, block entities and entities of the overworld chunks that were loaded when it was saved (`region_chunks` names those chunks), the version that saved it (`version`, from v0.11) and the world generator (`worldgen`, from v0.13) |
| `level.txt_old` | The `level.txt` the last save replaced (Java's `level.dat_old`), from 1.0 |
| `region/r.<x>.<z>.vxr` | The overworld's chunk records, one file per region of 16 x 16 columns (see [region files](#region-files)), from 1.0 |
| `chunks.bin` | Before 1.0: the overworld chunks the player changed, run-length encoded. 1.0 reads it and moves its chunks into the region files at the first save |
| `chunks.bin_old` | The `chunks.bin` that first save moved into region files, kept unchanged, from 1.0 |
| `nether/`, `end/` | The same `level.txt`, `level.txt_old`, `region/` and (before 1.0) `chunks.bin` for each other dimension |
| `icon.png` | The world's picture in Select World |
| `session.lock` | Held while a game has the world open (Java's `session.lock`) |

Beside the worlds, `saves/options.txt` holds the options and
`saves/hotbars.txt` the saved creative hotbars. Recovery can leave
`chunks.bin.unknown-<secs>`, `chunks.bin.damaged-<secs>`,
`level.txt.damaged-<secs>`, `level.txt_corrupted_<date>`,
`region/r.<x>.<z>.vxr.damaged-<secs>` and `region.unsaved-<secs>` in a world
folder; the game never deletes them.

## Region files

From 1.0 a chunk keeps everything it owns on disk once it unloads, as Java's
`RegionFile` storage does (v1.0 Track 3b, owner decision 2026-11-26): its
blocks if the player changed them, its block entities (chests, furnaces,
spawners, trial spawners and vaults, brushable blocks, beehives, campfires,
decorated pots, signs, shelves, bookshelves, banners, lecterns, jukeboxes,
creaking hearts, sculk, copper golem statue names, beacons, conduits,
crafters, potent sulfur and stored falling blocks), its scheduled fluid and
fire ticks, and its entities and dropped items. Loading the chunk reads them
back; a chunk with no record regenerates from the seed, as before. Memory
then holds only the loaded chunks' state, however far the player travels.

**The file.** A region covers 16 x 16 columns of 32-block chunks (512 x 512
blocks, the ground a Java region covers) and every chunk row of them. As
in Java's `RegionFile`, it starts with a table, then holds the records in
4096-byte sectors, so one chunk is rewritten without touching the others: a
new record goes into free sectors, then the table entry moves to it, then
the old sectors are free.

| Bytes | What |
| --- | --- |
| 0–7 | `VXREGION` |
| 8–11 | file version 1 (little-endian, as every number here) |
| 12–4095 | zero |
| 4096–36863 | 4096 entries of 8 bytes: the record's first sector (0: none) and its length in bytes. Entry `(row * 16 + z) * 16 + x` is the chunk at those coordinates modulo 16 |
| 36864 on | records, each starting a sector, padded to the next |

A record is `VXK1`, its payload's length, the payload's FNV-1a 64-bit
checksum and the payload. Payload version 1: the version byte, the chunk's
position, then the blocks (a flag, then `chunks.bin`'s `VXC4` runs), the
block entities as the level file's keys and texts, the water, lava and
bubble-column cells and the fire cells with their delays, and the entity
batches (each a list of the level file's entity keys). `world::region`
documents each field.

**Writing.** Unloading a chunk queues its record; an I/O thread (Java's
`IOWorker`) writes it, and a chunk read while its record waits gets the
queued one. At most 4096 chunks wait; past that the game waits for the
disk. Autosaves queue only the loaded chunks whose blocks changed since
their record was written, and the I/O thread writes the level files after
every record queued before them, so a level file never names a record that
isn't on disk. Quitting, changing dimension, `/save` and the LAN host's
save wait until everything is written before the world closes. A failed
write shows "Could not save the world" as a failed save always has.

**The level file and the records.** The level file still holds the state
of the chunks that were loaded when it was saved, in the keys it always
used, and `region_chunks` lists those chunks. Their records keep only their
blocks: the rest is older than the level file and is ignored when such a
chunk loads, until it unloads and its whole record is written again. The
level file also holds `fluid_ticks` (the loaded chunks' scheduled fluid
ticks, which before 1.0 were never saved), `stored_index` (the bed and job
claims of villagers, and the heart-bound creakings, stored in records) and
`entity_ids` (the next mob and cart ids, which stored entities keep).
Redstone and hopper scheduling (`redstone`, `automation`) stay in the level
file whole: they run on a world-wide clock.

A scripted run (screenshots, benchmarks) reads region files but never
writes them, and a LAN client has none.

## Which saves 1.0 opens

- **Every world saved by a tagged release from v0.8.0 to v0.18**, and by 1.0
  itself. Each of those releases has a fixture world in
  `tests/fixtures/saves/<tag>/` that its own build made, and
  `tests/save_compat.rs` opens every one of them, checks its blocks, inventory,
  armour, chest, player, agent player, mobs and dropped items, then saves it
  and reads it back.
- **Worlds from v0.7.0 and earlier** have no fixture. The readers for their
  formats stay (the one-byte `VXC1` chunk records, v0.4's bone blocks,
  pre-v0.6 terrain), and unit tests cover them, but 1.0 doesn't promise them.
- **A world saved by a newer version** asks first, as Java's `WorldOpenFlows`
  does: "Downgrading a world is not supported", with **Create Backup and
  Load**, **I Know What I'm Doing!** and **Cancel**. Downgrades stay
  unsupported, but they never destroy data the older build can't read (see
  [unknown data](#unknown-data)).
- A LAN client never writes a save; the host's world is the host's.

## How an older world is brought up to date

Loading converts in memory; nothing on disk changes until the game saves.
The first save after an older build's writes this build's format, keeps the
level file it replaced as `level.txt_old`, and records this version.

**Region files (1.0).** A world from before 1.0 has a `chunks.bin` and a
level file without `region_chunks`, which holds every chunk's state. It
opens as it always did: chunks come from `chunks.bin`, the level file's
containers and entities load. Its first save moves into region files every
`chunks.bin` chunk, every container and block entity of a chunk that isn't
loaded and every entity stored in one, writes the level file with only the
loaded chunks' state and `region_chunks`, and only then moves `chunks.bin`
to `chunks.bin_old` (kept, never deleted), and the level file it replaced to
`level.txt_old` as every save does. `tests/save_compat.rs` migrates every
release's fixture this way and reopens it from the region files. If the
game stops before that first save finished, the world opens next time as
the last save left it: a `chunks.bin` beside region files and a level file
without `region_chunks` means the region files were written by a session
that never completed a save, and they are moved aside as
`region.unsaved-<secs>`. A `chunks.bin` chunk that already has its blocks in
a record (the game stopped between writing the records and moving
`chunks.bin` aside) gives way to the record. Backups follow
[the rules below](#backups): the updater's backup holds the world from
before the update, and the replaced files are kept beside it.

**Chunks.** `chunks.bin` starts with a header naming its record format:

| Header | Block state ids | First written by |
| --- | --- | --- |
| `VXC1` | one byte, 0–255 | before v0.5 |
| `VXC2` | two bytes, below 4,096 | v0.5 |
| `VXC3` | two bytes, below 16,384 | v0.9 |
| `VXC4` | two bytes, below 32,768 (`STATE_CAPACITY`) | v0.12 |

A save writes the oldest header whose range holds every state it stores, so
an older build can still open a world that uses nothing it lacks. The
fixtures show it: v0.8.0 to v0.11 wrote `VXC2`, v0.12 on `VXC4`. Block state
ids, item ids and the orderings saves use are append-only, so a saved id
means the same thing in every later build; the fixture test checks that the
name each release gave every block it placed still names the same state.
Loading widens `VXC1` records and stands v0.4's axis-less bone blocks
upright.

**Level files.** `level.txt` is read line by line; keys the build doesn't
know are ignored, and a missing key takes its default. Records whose shape
changed are converted as they are read: for example a mob's `armor` and
`glint` became `equipment` slots in v0.14, and a villager's restock time an
absolute tick in v0.15 (both checked by `tests/entity_storage.rs`), and the
saved `time` is a fraction of a day, so a world from the ten-minute days
loads at the same time of day. A world keeps the generator that made it for
life: `worldgen` names it (worlds from before v0.13 have none and are
generation 1), because unmodified chunks are never saved and a different
generator would move the terrain around the player's builds.

**Options.** `options.txt` is read the same tolerant way: an unknown key is
ignored, a value that can't be read keeps its default, and values out of
range are clamped. Key bindings saved under older names are migrated. Every
release's options file from v0.8.0 to v0.18 loads with its values and saves
again unchanged (`every_release_options_file_loads_and_resaves_stably`).

## Unknown data

Java drops what it can't read: a block state it can't decode becomes its
default when the section loads (`SerializableChunkData`, "Recoverable errors
when loading section"), an unknown item becomes an empty stack
(`ItemStack.OPTIONAL_CODEC`), and an unknown entity is skipped ("Skipping
Entity with id", `EntityType`). 1.0 does the same, except for block
states:

- A **block state id this build doesn't register** but that fits the format
  (a newer build's block) stays in the chunk with its id. It behaves as an
  invisible, non-solid placeholder and is written back unchanged, so the
  block returns if the world goes back to a build that knows it. Java turns
  it into air instead; this is a deliberate difference.
- A state id outside the format's range makes that chunk record malformed:
  the chunk regenerates and the player is told (see
  [recovery](#interrupted-and-damaged-saves)).
- An **item id** this build doesn't register leaves its slot empty, as in
  Java, and the rest of the inventory or container loads.
- A **mob** whose kind this build doesn't know is skipped, as in Java.
- A **level file key** this build doesn't know is ignored. Renaming a world
  keeps every line, but the game's next save writes only the keys it knows,
  so a newer build's extra data is dropped by an older build's save. That is
  why downgrades ask for a backup first.
- A **`chunks.bin` in a newer format** (a `VXC` header with a version this
  build doesn't know) fails to load; the dimension's terrain regenerates and
  the next save moves the file to `chunks.bin.unknown-<secs>` instead of
  replacing it.
- A **region file of a newer version** (file version above 1) isn't
  opened: the dimension runs without region storage for the session, its
  chunks regenerate, the player is told, and the files are left alone. A
  record whose payload version is newer reads as damaged (below).

## Backups

- **Before an update**, the updater copies the whole saves folder, options
  included, to `backups/<date>_<time>-before-<version>/` while holding every
  world's session lock, and keeps the newest three such copies (owner,
  2026-10-22; [updates](updates.md)). A world open in another window stops
  the backup rather than being copied halfway through.
- **Before opening a world saved by a newer version**, **Create Backup and
  Load** copies that world to `backups/<date>_<time>_<folder>/`. Edit World's
  **Make Backup** does the same at any time. These copies are never pruned.
- **Before restoring a level file**, the recovery screen backs the world up
  the same way, as Java's `RecoverWorldDataScreen` does.
- **Every save** keeps the level file it replaces as `level.txt_old`, and a
  damaged file is copied aside before anything replaces it.
- Opening an older world in a newer build is not backed up again: the
  updater's backup already holds it, and a hand-installed build's player is
  told to copy their saves first ([releases](releases.md#player-data-and-old-saves)).

## Interrupted and damaged saves

A save writes the region records as [above](#region-files) (before 1.0,
`chunks.bin` to `chunks.bin.tmp`, renamed into place), then writes
`level.txt` the way Java's `Util.safeReplaceFile` writes `level.dat`: to
`level.txt.tmp`, then the current file becomes `level.txt_old`, then the new
one takes its place (moving the old one back if that rename fails). Neither
the game nor Java flushes the files to disk first, so a power cut can still
leave a file short; every case below is covered by
`tests/save_interrupted.rs` or the game's own tests, and none of them opens
as a fresh world.

| What happened | What the player gets |
| --- | --- |
| The game stopped while writing the temporary files | The last complete save opens; the leftovers are overwritten by the next save |
| It stopped after `chunks.bin` was replaced but before the level file | The chunks are the newer save's, the player and containers the previous one's |
| It stopped between the two level-file renames, or `level.txt` was lost | `level.txt` is missing and `level.txt_old` holds the previous save: the recovery screen below |
| `level.txt` is cut off, empty or can't be parsed | Select World marks the world "Failed to load world summary" and its button becomes **Attempt to Recover**; `--world` opens the same screen |
| Both level files are gone but `chunks.bin` is there | Listed as unreadable and refused, never opened as a new world over the chunks; restore from the backups folder |
| `chunks.bin` is cut off, or a chunk record is malformed | Every intact chunk loads; the rest regenerate, as Java's `ChunkMap` regenerates a chunk it fails to read. The damaged file is first copied to `chunks.bin.damaged-<secs>`, and chat says how many chunks were kept, with a pointer on the action bar |
| A region file is cut off (its table, or records past its end) | Opening the dimension finds it: every chunk whose record is whole loads, the rest regenerate, the file is first copied to `r.<x>.<z>.vxr.damaged-<secs>`, and chat says how many of the file's chunks were kept. The next write to the file repairs its table |
| A region record is damaged (its checksum, length or contents don't read) | Found when the chunk loads: that chunk regenerates, the file is copied aside once, and chat says so |
| The game stopped while records were being written | Each record is written into free sectors before its table entry points at it, so a chunk reads as its last record or its one before; the level file is only written after the records queued before it |
| The Nether's or End's `level.txt` is damaged | Its `level.txt_old` is used and the damaged file is kept as `level.txt.damaged-<secs>`; without a kept copy the dimension's containers and entities are lost, and chat says so |

**The recovery screen** is Java's `RecoverWorldDataScreen`: "Failed to load
world", the folder, and for the level file and its kept copy when each was
written and what is wrong with it ("No issues", "Missing file" or the error).
**Attempt to Restore** is enabled when the level file is unreadable and its
kept copy reads; otherwise its tooltip says "No state to recover from
available". Restoring backs the world up, moves the unreadable file to
`level.txt_corrupted_<date>` and the kept copy into its place, then "Recovery
done" offers **Continue** (open the world) or **Back**. Java's **Report a
Bug** button is left out.

## What may never change after 1.0

- Block state ids, item ids, and every enum ordering a save stores are
  append-only. New content takes new ids from a reserved range; a retired id
  keeps its meaning and is never reused.
- The `chunks.bin` record layout and the meaning of `VXC1` to `VXC4`. A new
  layout takes a new header, which older builds keep aside unread. 1.0 still
  reads every `chunks.bin` it promises to open.
- The region file layout of [file version 1](#region-files), its `region/`
  folder and `r.<x>.<z>.vxr` names, the record frame (`VXK1`, length,
  checksum) and payload version 1, and the meaning of the keys a record
  holds (the level file's). A new layout takes a new file or payload
  version; a newer file version is left alone, a newer payload regenerates
  its chunk.
- `region_chunks`, `fluid_ticks`, `stored_index` and `entity_ids` keep their
  meaning in the level file.
- The file names in [the table above](#what-a-world-is-on-disk) and the
  `key=value` form of `level.txt` and `options.txt`. An existing key keeps
  its meaning and value format; new data goes in new keys; a retired key's
  name is never reused.
- A world's generator: `worldgen` numbers are never reused or changed, a new
  generator takes a new number, and each old generator's output stays pinned
  by its chunk hash tests.
- The `version` key, written by every save.
- Every release from 1.0 on adds a fixture made by its own build to
  `tests/fixtures/saves/`, and checked-in fixtures are never regenerated or
  edited, so the oldest promise is tested for as long as it holds.

## Platform checks (UX-04a)

| Check | Where it runs |
| --- | --- |
| Start, build and save a world, quit by closing the window, reopen and quit again, on Linux | `tools/save_fixtures/cycle-current.sh` on Boat (Linux has no release package, so this is the `ci` source build) |
| The same cycle for every tagged release v0.8.0–v0.18 | `tools/save_fixtures/run-tag.sh` on Boat, once per tag; its results are the fixtures |
| Windows install, reinstall and uninstall keeping a save | CI's Windows installer job ([releases](releases.md#manual-validation-before-publishing)) |
| Installing the Mac app from the disk image, launching it from Finder, saving, quitting with Command-Q and with the window's close button, and reopening | The owner, on a Mac |
| Installing with the Windows installer, launching from the Start Menu, playing, saving, quitting and reopening | The owner, on Windows |
| Updating a real install to the next release, with its pre-update backup | The owner (row UX-04b) |

## How the fixtures are made

`tools/save_fixtures/` holds the scripts. For each tag a Boat sandbox named
`voxelcraft-saves-<tag>` clones the repository, builds the tag's game and
agent client with the `ci` profile (tags before v0.16 get v0.16's `ci`
profile appended to their `Cargo.toml`; their code is untouched) and runs
`make-fixture.sh` under Xvfb with Mesa's software Vulkan. The run starts a
new survival world (seed 20261125) with `--place`, `--give`, `--wear`,
`--spawn` and `--drop`, places every block from a fixed list that the tag
knows (`probe.rs` asks the build), and has an agent player fill a chest,
then closes the window, which saves the world. A copy is then opened and
closed once more. `collect.py` copies the save into the repository with a
`manifest.json` of what the run placed. The world's `chunks.bin` keeps only
the run's own chunk records (`trim_chunks.py`, byte for byte): the dozens of
terrain chunks the simulation touched near spawn would make each fixture
close to a megabyte. From 1.0 a world keeps its chunks in region files, and
`trim_regions.py` keeps the same records in them, each frame byte for byte
in its table slot (v1.0 Track 1b). Nothing else in a fixture is edited. The
game logs `region storage: N chunk records in the overworld` when it opens
a world's region files, and `saved N overworld chunk records to … written)`
when a save it waited for is on disk; `collect.py` reads both, and counts
the records in the first run's region files itself.

```sh
id=$(boat new --name voxelcraft-saves-v0-18 --type large --from voxelcraft-warm --json | jq -r 'select(.event=="ready") | .id')
boat exec "$id" "mkdir -p ~/bin"
for f in tools/save_fixtures/*; do boat scp "$f" "$id:/home/user/bin/$(basename "$f")"; done
boat exec "$id" --timeout 0 "bash ~/bin/run-tag.sh v0.18"
boat scp "$id:/home/user/out/v0.18.tar.gz" target/ && tar -C target -xzf target/v0.18.tar.gz
python3 tools/save_fixtures/collect.py target/v0.18
boat delete "$id" --yes
```
