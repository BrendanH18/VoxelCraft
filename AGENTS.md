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

- Create: `id=$(boat new --name voxelcraft-<purpose> --json | jq -r 'select(.event=="ready") | .id')`
- One-shot job, for genuine one-offs only: `boat run --push . -- npm test` (new sandbox, run, cost, delete; exits with the test exit code). Checks go through `scripts/boat-check.sh`.
- Run: `boat exec "$id" --cwd my-app "npm test"` (30 s timeout; `--timeout 0` = no limit)
- Server: `boat exec "$id" --detach "npm run dev"`, then `boat exec "$id" --status <pid>`
- Copy files: `boat push "$id" ./my-app` (git-tracked files, to /home/user/my-app) or clone the repo inside the sandbox
- Public URL for a port: `boat ssh "$id" "host 3000 --private"`
- Always pass the sandbox id. `current` does not work in a non-interactive shell.
- When done: `boat stop "$id"` (keeps the disk for at least 14 days) or `boat delete "$id" --yes` (erases it)
- Every sandbox says which project it belongs to (owner, 2026-11-21): all the
  owner's projects share one Boat account. A sandbox an agent creates by hand
  passes `--name voxelcraft-<purpose>` and is never left unnamed;
  `boat-check.sh` names its own (`voxelcraft-check-<worktree>-<time>-<pid>`,
  `voxelcraft-warm-build-<time>-<pid>`).
- Rust builds and tests run through `scripts/boat-check.sh` (see
  [Boat checks](docs/development.md#boat-checks)): `-- CMD` for the targeted
  checks while working and to reproduce a failed CI job (CI runs the full
  suite; see [Before handing work back](#before-handing-work-back)), `--done`
  when the agent's work ends (it deletes the worktree's sandbox), and
  `--save` after dependency or toolchain changes to refresh the warm
  snapshot sandboxes start from.
- One Rust toolchain (2026-11-21): `rust-toolchain.toml` pins the version CI,
  release builds and `boat-check.sh` use. Change it on purpose, and once the
  change is on origin's default branch run `scripts/boat-check.sh --save`.
  rustup installs that toolchain the first time `cargo` runs in a checkout,
  so on the Mac check `rustup toolchain list` and ask the owner before a
  local `cargo fmt` would install it.
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
- There is no fixed cap on parallel agents (owner, 2026-11-21; it replaces
  the 2026-10-17 limit of four, set when builds ran on the owner's 16 GB
  machine). Size parallel work to what Boat's daily starts and included
  hours allow (`boat limits`) and to what the owner can review.
- No step requires an agent to play the game through computer use (owner,
  2026-11-14). The owner's own play is the hands-on evidence, and nothing
  waits for an agent's hands-on pass.
- Build and test in Boat, not on this Mac (owner, 2026-11-20). This
  replaces the 2026-11-05 rule that builds ran on Namespace and the
  2026-11-20 rule that agents iterated on the Mac; Namespace now runs CI
  only. Each worktree has one long-lived sandbox, reused by every
  `boat-check.sh` run (owner, 2026-11-21; it replaces one `boat run` per
  check, which used a daily start each): created from the warm snapshot when
  one exists, it keeps cargo's target directory between checks, stops as
  soon as each check ends (owner, 2026-11-22; it replaces the 2026-11-21
  idle timeouts of 15 minutes, 5 on `xlarge`: a stopped sandbox bills
  nothing, and a resume within 10 minutes of the stop takes back the same
  machine without a start; `VC_BOAT_IDLE` to keep it up longer) and is
  deleted with `--done`. While a check runs, its auto-stop is kept 30
  minutes ahead by a loop that ends with the script (owner, 2026-11-22), so
  a killed check stops within 30 minutes. Checks use `large` sandboxes
  (owner, 2026-11-21): measured, `xlarge` was slower for this build at
  about 2.8 times the price. Never share
  a sandbox between agents: agents in one worktree each set `VC_BOAT_SLOT`.
  Each agent runs targeted checks on its own worktree, and CI runs the full
  suite on every pushed commit, the integrator's merged result included. Check `boat limits` before work that
  starts sandboxes: the account's daily starts are shared by every project. If
  `boat` isn't installed or signed in, or `boat-check.sh` can't start a
  sandbox (no starts left today, or the account can't start `large`
  sandboxes), stop and tell the owner; don't build locally.
- Local game builds share one cargo target directory (owner, 2026-10-17):
  put the worktrees under one folder whose `.cargo/config.toml` sets
  `build.target-dir` (or set `CARGO_TARGET_DIR`), and "Blocking waiting for
  file lock" means wait, not retry. A shared target can hand one worktree
  another's build (found 2026-10-20 and 2026-10-26): before running or
  benchmarking a tree that another tree built last, run `touch build.rs` and
  `cargo clean --release -p voxelcraft` (it keeps the dependencies), and keep
  copies of binaries you compare.

## Before handing work back

CI is the gate (owner, 2026-11-22; it replaces running the full chain in
Boat before every hand-off). It runs the full suite once, on the pushed
commit, and `main` is protected, so a change merges only with CI green.
Agents don't run `scripts/boat-check.sh` with no command, or with `--full`,
before handing work back. Those modes run nothing CI doesn't (compared
2026-11-22): their `cargo fmt`, clippy and tests are CI's **rustfmt** job
and its **clippy + test** job, which runs them on Linux, macOS and Windows,
and `--full`'s `--no-default-features` clippy and tests are its
**simulation without client dependencies** job. CI also runs Ruff, the
headless benchmark, the asset check, the doc-link check and the Python tool
tests it names ([ci.yml](.github/workflows/ci.yml)).

CI's Rust checks are this chain:

```sh
cargo fmt --check && cargo clippy --profile ci --all-targets --locked -- -D warnings && cargo test --profile ci --locked
```

Checks and tests use the `ci` profile (owner, 2026-11-21): release's
optimisations and assertions without its cross-crate link-time optimisation. On a
`large` sandbox the test build after a source change takes 259 s instead of
858 s with `--release`. Only shipped builds use `--release`.

1. While working, run targeted checks: `cargo fmt --check` here, and the
   rest on Boat with `scripts/boat-check.sh -- CMD`. VoxelCraft is one
   crate, so narrow by test target and filter rather than by crate. The
   cheap choices, which reuse the sandbox's warm `ci` build:
   - the unit tests of the modules you changed:
     `cargo test --profile ci --locked --lib world::` (a module path under
     `src/`);
   - the integration tests that cover the change:
     `cargo test --profile ci --locked --test copper` for `tests/copper.rs`,
     one `--test` per file;
   - clippy on what you touched:
     `cargo clippy --profile ci --locked --lib -- -D warnings`, adding
     `--test <name>` for each changed test file;
   - for code the headless build compiles differently (`client` feature
     gates), the same clippy with `--no-default-features`.

   Chain them in one command, so the sandbox resumes once:

   ```sh
   scripts/boat-check.sh -- 'cargo clippy --profile ci --locked --lib --test copper -- -D warnings && cargo test --profile ci --locked --lib world:: && cargo test --profile ci --locked --test copper'
   ```

   When a change touches Python (`tools/`, `packaging/` or `ruff.toml`), run
   `uvx ruff@0.17.5 check && uvx ruff@0.17.5 format --check` here and the
   tests of the tool you changed on Boat, for example
   `scripts/boat-check.sh -- 'python3 -m unittest tools/test_campaign_ledger.py'`.
   `uvx ruff@0.17.5 format` applies the formatting. Ruff runs through `uvx`
   at that pinned version and is not a project dependency; the settings are
   in `ruff.toml`. The scripts must keep running on Python 3.9 (macOS's
   `/usr/bin/python3`).
2. Push and let CI run everything. When a CI job fails, reproduce that job,
   or just its failing test, on Boat with the script (the headless job's
   test, for example, with
   `scripts/boat-check.sh -- 'cargo test --profile ci --locked --no-default-features --test <name>'`),
   fix it and push again. macOS and Windows failures can't be reproduced on
   Boat's Linux; work from the job's log. A full run (`scripts/boat-check.sh`
   or `--full`) is only for reproducing a CI failure a narrower command
   can't, or for checks CI doesn't run.
3. These aren't in CI, so they stay required before hand-off:
   - a new `tools/test_*.py`: CI runs only the tool test files `ci.yml`
     names (`test_benchmark_scenes.py`, `test_check_doc_links.py` and
     `test_campaign_ledger.py`), so run a new one on Boat until it's added
     there;
   - the relevant frame-time, visual and persistence checks below. CI runs
     only the headless generation and meshing benchmark; the frame-time
     suite, scripted scenes and screenshots stay on this Mac.

The hand-off report names the targeted checks that ran, with their
commands and results, and says CI is the full check (with its result on
the pushed commit, if it has finished).

Documentation-only changes skip the targeted checks (owner, 2026-10-26);
check their links and rendering instead.

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
