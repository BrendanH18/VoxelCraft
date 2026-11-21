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
- Retain useful existing and pending work. Don't discard uncommitted changes
  or rewrite working features because the plan changed.
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

## Cloud sandboxes (Boat)

Use a Boat sandbox for builds, tests, untrusted code, Docker, and long jobs. Never run them on this computer.
Docs: https://docs.boat.dev/llms.txt. Prices and limits: run `boat limits`; never quote prices from memory.

- Create: `id=$(boat new --json | jq -r 'select(.event=="ready") | .id')`
- One-shot job: `boat run --push . -- npm test` (new sandbox, run, cost, delete; exits with the test exit code)
- Run: `boat exec "$id" --cwd my-app "npm test"` (30 s timeout; `--timeout 0` = no limit)
- Server: `boat exec "$id" --detach "npm run dev"`, then `boat exec "$id" --status <pid>`
- Copy files: `boat push "$id" ./my-app` (git-tracked files, to /home/user/my-app) or clone the repo inside the sandbox
- Public URL for a port: `boat ssh "$id" "host 3000 --private"`
- Always pass the sandbox id. `current` does not work in a non-interactive shell.
- When done: `boat stop "$id"` (keeps the disk for at least 14 days) or `boat delete "$id" --yes` (erases it)
- Rust builds and tests, while iterating and at hand-off, run through
  `scripts/boat-check.sh` (see [Boat checks](docs/development.md#boat-checks)):
  `-- 'cargo check'` or `-- 'cargo test --release <filter>'` while working,
  `--full` once the work is finished, and `--save` after dependency or
  toolchain changes to refresh the warm snapshot runs start from.
- Stay on this Mac: running the game (Metal rendering, screenshots, scripted
  scenes, hands-on runs) and benchmarks, whose frame-time gate is measured on
  the reference M5. Boat is x86_64 Linux with no GPU, so it can't check
  macOS-only code either; CI covers macOS and Windows. `cargo fmt`, Ruff and
  the doc-link check stay local too: they don't build or run tests.

## Delegation and builds

- Release tags are two-part (owner, 2026-10-22): `v0.9`, `0.9` or `1.0`, naming
  Cargo version `0.9.0` / `1.0.0`. See [releases](docs/releases.md).
- Preserve unrelated changes and other worktrees.
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
- Build and test in Boat, not on this Mac (owner, 2026-11-20). This
  replaces the 2026-11-05 rule that builds ran on Namespace and the
  2026-11-20 rule that agents iterated on the Mac; Namespace now runs CI
  only. Every `boat-check.sh` run has its own sandbox started from the warm
  snapshot, so agents don't wait on each other or on cargo's lock. Each
  agent runs the hand-off chain on its own worktree, and the integrator runs
  it again on the merged result. If `boat` isn't installed or signed in,
  stop and tell the owner; don't build locally.
- Local game builds share one cargo target directory (owner, 2026-10-17):
  put the worktrees under one folder whose `.cargo/config.toml` sets
  `build.target-dir` (or set `CARGO_TARGET_DIR`), and "Blocking waiting for
  file lock" means wait, not retry. A shared target can hand one worktree
  another's build (found 2026-10-20 and 2026-10-26): before running or
  benchmarking a tree that another tree built last, run `touch build.rs` and
  `cargo clean --release -p voxelcraft` (it keeps the dependencies), and keep
  copies of binaries you compare.

## Before handing work back

The hand-off chain, which stops at the first broken step:

```sh
cargo fmt && cargo clippy --release --all-targets -- -D warnings && cargo test --release
```

Run it with `scripts/boat-check.sh --full`: formatting here, the rest in a
Boat sandbox, plus the headless-build checks.

When a change touches Python (`tools/`, `packaging/` or `ruff.toml`), also run
the Ruff checks CI runs here and the Python tool tests in Boat
(`scripts/boat-check.sh -- "python3 -m unittest discover -s tools -p 'test_*.py'"`):

```sh
uvx ruff@0.17.5 check && uvx ruff@0.17.5 format --check && python3 -m unittest discover -s tools -p 'test_*.py'
```

`uvx ruff@0.17.5 format` applies the formatting. Ruff runs through `uvx` at
that pinned version and is not a project dependency; the settings are in
`ruff.toml`. The scripts must keep running on Python 3.9 (macOS's
`/usr/bin/python3`).

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
