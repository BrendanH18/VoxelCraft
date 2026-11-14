# VoxelCraft agent instructions

These instructions are tracked with the project. A fresh checkout needs no
external agent-notes folder or checkout hook. `CLAUDE.md` imports this file.

## Project priorities

- Read [the current roadmap](docs/roadmap.md) before choosing work, and take it
  from the earliest unreleased version in its [release plan](docs/roadmap.md#release-plan).
  Build a faithful modern Minecraft Java single-player game; multiplayer follows 1.0.
- Releases were renumbered (owner, 2026-11-13): v0.14 carries what was
  planned as v0.14, v0.15 and v0.16 plus build-5000 wave 1, and later
  versions moved down two (v0.15 villages and raids through v0.18 polish and
  performance, then 1.0). Records, commits and code comments from before that
  date use the old numbers. The 5,000-feature target was dropped; wave 1 was
  the only wave. The [release plan](docs/roadmap.md#release-plan) is current.
- Match the selected Java reference behavior, including generation and structure
  layouts. Record sources and differences; do not substitute approximate behavior
  without an explicit scope decision.
- Retain useful existing and pending work. Do not delete branches, discard
  uncommitted changes, or rewrite working features because the plan changed.
- Existing controller, CLI-agent and multiplayer paths receive maintenance and
  shared-feature integration; expansion is deferred under the roadmap.
- Update `docs/roadmap.md` with verified behavior, discovered gaps and approved
  scope changes. Passing builds or historical release notes do not prove parity.
- Follow [the restructuring plan](docs/restructuring.md) for proposed architecture
  changes. Planned tasks are not implemented features.

## Shared instructions and configuration

- This file is the single source of agent rules for every tool: Codex, Claude,
  Cursor, Grok and delegated agents. `CLAUDE.md` only imports it, and any other
  tool-specific instruction file must do the same rather than hold its own rules.
- Whenever the owner states a new rule, preference or configuration choice, or
  an agent changes one, record it in this file (or the tracked document it
  belongs in) in the same piece of work. Never keep it only in a tool-specific
  or machine-local place such as Claude memory, `~/.claude`, `~/.codex` or
  editor settings. Mirror it there only as a convenience.
- Project configuration (hooks, scripts, CI, tool settings) lives in tracked
  files too, so a fresh clone from GitHub has every rule and setting.

## Delegation and builds

- Release tags are two-part (owner, 2026-10-22): `v0.9`, `0.9` or `1.0`, naming
  Cargo version `0.9.0` / `1.0.0`. See [releases](docs/releases.md).
- Preserve unrelated changes and other worktrees. Never delete
  `refs/t3/checkpoints/*`, use `git push --mirror`, or use a bare `git stash`.
- Keep saves, build output, credentials, local backups and
  machine-specific coordination files out of git: put them under an ignored
  path such as `target/` (for example `--data-dir target/playtest`), never as
  untracked files git would pick up.
- Delegate only when requested. Give each implementation agent an isolated
  worktree, explicit scope and reserved IDs; delegated agents do not delegate further.
- Run at most four agents at once (owner, 2026-10-17): the owner's machine has
  16 GB of RAM. Queue further work until a slot frees.
- No step requires an agent to play the game through computer use (owner,
  2026-11-14). The owner's own play is the hands-on evidence, and nothing
  waits for an agent's hands-on pass.
- Build in the cloud, not on the owner's Mac (owner, 2026-11-05).
  `scripts/remote-check.sh` copies the worktree, uncommitted edits included,
  to a fresh Namespace Linux instance and runs clippy and the tests there
  (`--full` adds the `--no-default-features` checks; `-- CMD` runs any
  command), with dependencies cached on a shared Namespace volume. Every run
  has its own instance, so agents don't wait on each other. Build locally
  only to run the game: screenshots, scripted scenes and benchmarks, whose
  frame-time gate is measured on the reference M5. When `nsc` isn't
  installed or logged in, fall back to the local rules below. See
  [remote checks](docs/development.md#remote-checks).
- Agents must not each build the project (owner, 2026-10-17). Agent worktrees
  share one cargo target directory, so dependencies build once and builds take
  turns under cargo's lock: put the worktrees under one folder whose
  `.cargo/config.toml` sets `build.target-dir` (or set `CARGO_TARGET_DIR`),
  and tell agents that "Blocking waiting for file lock" means wait, not retry.
  This shares build output, not responsibility for checks: each agent still
  runs the hand-off chain below in its own worktree, taking turns under the
  lock, and the integrator runs it again on the merged result.
- A shared target directory can hand one worktree another's build: cargo
  treats the `voxelcraft` crate as fresh when none of your source files is
  newer than the last build, whichever worktree made it (found 2026-10-20).
  Before checks or benchmarks in a tree whose files didn't change since
  another tree built, run `cargo clean --release -p voxelcraft` (it keeps the
  dependencies), and keep copies of binaries you compare.
- The build script writes absolute asset paths into its output and reruns
  only when the assets change (found 2026-10-26). In a shared target, a
  build of another checkout, or of a temporary copy that has since been
  deleted, leaves that output pointing there ("couldn't read
  `/tmp/…/assets/…`"). Run `touch build.rs` before checks so it
  regenerates for your tree.

## Before handing work back

Run this exact chain so a failure stops at the first broken step:

```sh
cargo fmt && cargo clippy --release --all-targets -- -D warnings && cargo test --release
```

`scripts/remote-check.sh --full` runs the same chain (formatting here, the
rest on a Namespace instance) plus the headless-build checks, and is the
preferred way to run it.

Documentation-only changes skip this chain (owner, 2026-10-26); check their
links and rendering instead.

Run relevant headless, benchmark, visual and persistence checks as described in
[the agent guide](docs/agent-guide.md) and [development guide](docs/development.md).
Report the files changed, what was actually checked and what remains unverified.
Keep documentation and test evidence aligned with the delivered behavior.

## Engineering and acceptance

- Preserve saved identifiers and formats; block/item/state IDs are append-only,
  and enum ordering used by saves must remain compatible. Reserve new ranges
  explicitly before concurrent content work. Historical ranges are not permission
  to reuse IDs; inspect the current code and pending branches.
- Prefer compact tables and bounded work in simulation/rendering hot paths.
  Measure performance changes in representative scenes rather than assuming a
  refactor improves speed.
- A feature needs correctness, ordinary player interaction, relevant input-path
  integration, save/reload and performance evidence before it is marked Verified.
- Player evidence is the owner's own ordinary play (owner, 2026-10-22). Don't
  write formal acceptance checklists or ask for physical-controller sessions;
  controller and CLI paths are covered by the automated adapter tests, and the
  owner reports problems found in play.
- Before a version is offered to the owner (2026-11-09 audit; details in
  [roadmap §7](docs/roadmap.md#7-feature-completion-and-release-policy) and
  [§6.1](docs/roadmap.md#61-reference-configuration-and-frame-time-gate)):
  a frame-time run is recorded at that commit and the
  [requirement inventory](docs/requirements.md) is refreshed. No agent
  play-through comes first: the owner plays it (owner, 2026-11-14). At most
  one version waits for the owner's play while the next is built.
- The frame-time gate is 120 FPS on the game's own frames (owner,
  2026-11-14; it was 240 FPS windowed before): a 1% low of at least 120 FPS
  and p99 at most 8.33 ms in every scene of the offscreen suite
  (`tools/benchmark_scenes.py --offscreen`). The windowed suite still runs
  each release and fails only on a frame over 50 ms. Older records judge
  runs by the old gate ([§6.1](docs/roadmap.md#61-reference-configuration-and-frame-time-gate),
  [drawable wait](docs/verification/2026-11-14-drawable-wait.md)).
- Every Java 26.3 block, item and potion the game lacks is listed with the
  release that owns it in `tests/fixtures/java/26.3/registry_gaps.json`
  (2026-11-09 audit). `cargo test` fails when that list is out of date or
  when a trade or loot table names an item that neither resolves nor is
  listed. Registering content removes its entry; moving content to another
  release edits the entry and the roadmap together.
- Use original or appropriately licensed implementation and assets. License:
  MIT OR Apache-2.0.
- Released games trust only the update-signing keys in
  `packaging/update-keys.txt` (owner, 2026-10-22). Never commit a private
  key, never rename release assets, and never remove a key before a release
  signed by its replacement has shipped ([updates](docs/updates.md#the-signing-key)).

## Reading map

- [Roadmap and acceptance criteria](docs/roadmap.md)
- [Agent engineering guide](docs/agent-guide.md)
- [Architecture](docs/architecture.md) and [development tools](docs/development.md)
- [Contribution workflow](CONTRIBUTING.md)
- [Documentation index](docs/README.md)
- [Historical plans and briefs](docs/archive/README.md), which never override
  current instructions or establish current completion claims

## Context maintenance

- Keep project rules and durable project knowledge in tracked `AGENTS.md` and
  repository docs; do not store them in agent auto-memory or machine-local notes.
- `CLAUDE.md` contains only `@AGENTS.md`. Shared hooks, scripts and settings
  belong in tracked files. Do not ignore instruction files or shared settings;
  ignore only machine-local settings and agent worktrees.
