# VoxelCraft agent instructions

## Project priorities

- Read [the current roadmap](docs/roadmap.md) before choosing work, and take it
  from the earliest unreleased version in its [release plan](docs/roadmap.md#release-plan).
  Build a faithful modern Minecraft Java single-player game; multiplayer follows 1.0.
- Releases were renumbered on 2026-11-13, so older records use the old
  numbers; the [current plan](docs/roadmap.md#current-plan--updated-2026-11-13) explains them.
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
  Cursor, Grok and delegated agents. A fresh checkout needs nothing else.
  `CLAUDE.md` contains only `@AGENTS.md`, and any other tool-specific
  instruction file only imports this one too.
- Whenever the owner states a new rule, preference or configuration choice, or
  an agent changes one, record it in this file (or the tracked document it
  belongs in) in the same piece of work. Never keep rules or project
  knowledge in agent memory or a machine-local place such as `~/.claude`,
  `~/.codex` or editor settings.
- Project configuration (hooks, scripts, CI, tool settings) lives in tracked
  files too, so a fresh clone from GitHub has every rule and setting. Never
  ignore instruction files or shared settings; ignore only machine-local
  settings and agent worktrees.

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
- There is no fixed cap on parallel agents (owner, 2026-11-21). Size
  parallel work to what Boat's daily starts and included hours allow
  (`boat limits`) and to what the owner can review.
- No step requires an agent to play the game through computer use (owner,
  2026-11-14). The owner's own play is the hands-on evidence, and nothing
  waits for an agent's hands-on pass.
- Build and test in Boat, not on this Mac (owner, 2026-11-20); Namespace
  runs CI only. Each worktree has one long-lived `large` sandbox, reused by
  every `boat-check.sh` run: it starts from the warm snapshot when one
  exists, keeps cargo's target directory between checks, stops as soon as
  each check ends (`VC_BOAT_IDLE` keeps it up longer) and is deleted with
  `--done`. [Boat checks](docs/development.md#boat-checks) explains how it
  works and the measurements and earlier rules behind it. Never share a
  sandbox between agents: agents in one worktree each set `VC_BOAT_SLOT`.
  Each agent runs targeted checks on its own worktree, and CI runs the full
  suite on every pushed commit, the integrator's merged result included.
  Check `boat limits` before work that starts sandboxes: the account's daily
  starts are shared by every project. If `boat` isn't installed or signed
  in, or `boat-check.sh` can't start a sandbox (no starts left today, or the
  account can't start `large` sandboxes), stop and tell the owner; don't
  build locally.
- Each worktree builds into its own `target/` (owner, 2026-11-22). For a
  before/after benchmark on this Mac, copy each binary out of `target/`
  before building the other commit, since the second build replaces the
  first.

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
optimisations and assertions without its cross-crate link-time optimisation
([timings](docs/development.md#checks)). Only shipped builds use `--release`.

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
2. CI runs the full suite on every commit of a pull request and every push
   to `main` (not on a branch with no pull request). When a CI job fails,
   reproduce that job, or just its failing test, on Boat with the script
   (the headless job's test, for example, with
   `scripts/boat-check.sh -- 'cargo test --profile ci --locked --no-default-features --test <name>'`)
   and fix it. macOS and Windows failures can't be reproduced on
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
