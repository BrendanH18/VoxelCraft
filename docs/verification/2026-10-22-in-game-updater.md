# In-game updater — 2026-10-22

[Roadmap](../roadmap.md#v010-scope-updates) · [Requirement inventory](../requirements.md#ux--presentation-usability-and-distribution) · [Updating VoxelCraft](../updates.md)

Scope: UX-04b and the v0.10 part of UX-05a, as uncommitted work on
`t3/updater-roadmap-and-ux-polish` after `f93a9cd`. The design and its owner
decisions are in [Updating VoxelCraft](../updates.md). This is not a Java
parity feature; there is no Java reference.

## What was built

- `src/update/`: version parsing (`version.rs`), GitHub release JSON
  (`release.rs`), downloads through the system `curl` or a fixture folder
  (`fetch.rs`), signature and checksum checks (`verify.rs`), pre-update
  backups (`backup.rs`), platform install and the Mac watcher (`install.rs`),
  and the `Updater` state machine (`mod.rs`).
- Title screen: an **Update to x.y** button, the update screen (notes,
  progress, Cancel, Try Again, Release Page, Later, Skip This Version) and an
  "Updated to" notice. Options: **Check for Updates**. `options.txt` gains
  `check_updates` and `skip_update`.
- Flags: `--no-update-check`, `--update-source <folder>`, `--update-now`
  (tests only, needs `--update-source`), and the Mac watcher's internal
  `--finish-update`.
- Packaging: the installer's `/UPDATE=1` relaunch and `/WAITPID=<pid>` wait;
  `packaging/sign-checksums.sh`; `packaging/update-keys.txt` with the owner's
  release key; the release workflow's signing step.
- Safe failures: an unreadable level file returns to the title instead of
  starting the world fresh (`unreadable_level` in `src/app/mod.rs`); a
  failed save shows a message in game.
- Dependencies: `sha2` 0.11.1 and `ed25519-compact` 2.6.0 (MIT or
  Apache-2.0, and MIT), desktop build only.

## Checks run

On the owner's M5 MacBook Pro (macOS, Apple Silicon):

- `cargo fmt && cargo clippy --release --all-targets -- -D warnings && cargo
  test --release`: 1,529 tests passed. `--no-default-features` clippy and
  tests: 1,412 passed.
- Windows: `cargo clippy --release --target x86_64-pc-windows-msvc
  --all-targets -- -D warnings` passed in a scratch copy with the icon
  resource step disabled (no Windows resource compiler on the Mac). It was
  not built or run on Windows.
- 24 updater unit tests: tag parsing and ordering; release JSON and notes;
  OpenSSL-made Ed25519 signatures, tampering and key rotation; checksum
  lists; backups (locks held, `session.lock` left out, a busy world refused,
  three kept, one per version, UTC names, an unlistable folder fails); the atomic Mac swap; the offer
  rules (newer only, skipped versions, Later, checks off, failed checks
  silent); the "Updated to" and "didn't finish" notices; the full
  download, verify, back-up and prepare path against a fixture signed with
  a test key; refusal of a fixture signed by any other key; launching
  the installer only once; and a cancel that arrives with the prepared
  package. A test of unreadable level files was added too.
- `packaging/sign-checksums.sh` signed a checksum list with the real release
  key. A temporary test confirmed the game's verifier accepts that signature
  (then removed). The script refused a different key and a missing secret.

### End-to-end update on a Mac

Two packages were built with `packaging/macos/package.sh` from a scratch copy
of this tree: 0.9.0 and the same code labelled 0.9.1. A fixture folder held
the 0.9.1 disk image, `SHA256SUMS.txt` signed with the real release key and a
`latest.json`. The 0.9.0 app was installed in a writable scratch folder and
run with `--data-dir`, `--update-source` and `--update-now`, with one world
and an `options.txt`:

| Scenario | Result |
|---|---|
| 0.9.0 to 0.9.1 | Offered, downloaded, verified, backed up (world and `options.txt`), copied out of the disk image and checked, handed to the watcher. The watcher swapped the apps and opened 0.9.1 with the same data folder and fixture; 0.9.1 reported it started and the old app was deleted. About 5 seconds. Download removed; 0.9.1 finds no newer release. |
| 0.9.1 to a 0.9.2 whose program exits at once (correctly signed) | The watcher saw it exit, swapped 0.9.1 back and reopened it; 0.9.1 reported that the update didn't finish and did not retry. One backup. |

The first runs found two defects, both fixed and re-run above:

1. The game kept its "ready" state after starting the watcher, so redraws
   before the window closed started about seven watchers, which swapped the
   apps back and forth. Launching now moves to an "installing" state, and the
   watcher takes an exclusive lock (`installs_only_once`).
2. With `--update-now`, a rolled-back game reopened with the same arguments
   and retried in a loop, each attempt pruning an older backup. A failed
   update now turns `--update-now` off, and a retry replaces that version's
   earlier backup instead of an older one.

Killing the loop mid-update left a complete, runnable app in place each
time, as the atomic swap intends.

The debate review that followed found four more, fixed with tests: the
restart replayed one-shot options such as `--new` (it now passes only
`--data-dir` and `--update-source`); a folder the backup couldn't list was
left out silently (it now fails the backup); Cancel during the Mac
preparation step was ignored (`a_late_cancel_wins`); and a failed Save and
Quit hid its error (it now shows on the title screen). The Mac end-to-end
run was not repeated after these fixes.

## Not yet verified

- A real Windows update: the installer's update mode is covered by the
  release smoke test, which runs only in CI on a tag or `release/*` build.
  The game-side Windows path (curl, starting the installer, exiting) has
  only been compiled.
- A download from GitHub: the tests used fixture folders. The first real
  check needs a published release.
- The signing step in the release workflow. The `UPDATE_SIGNING_KEY` secret
  is set; the step first runs on the `v0.10` tag.
- An app in `/Applications`, the disk-image and translocation refusals, and
  the update screen as a player sees it, including Cancel.
- The owner updating a real v0.10 install to the next release on both
  platforms: the v0.10 exit check.

## Known differences and limits

- No rollback on Windows for a version that installs but won't start.
- `hdiutil` prints deprecation warnings on this macOS in favour of
  `diskutil image`; it still works.
- Check for Updates takes effect at the next launch.
