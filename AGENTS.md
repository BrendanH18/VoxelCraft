# VoxelCraft agent instructions

These instructions are tracked with the project. A fresh checkout needs no
external agent-notes folder or checkout hook. `CLAUDE.md` imports this file.

## Project priorities

- Read [the current roadmap](docs/roadmap.md) before choosing work. Build a
  faithful modern Minecraft Java single-player game; multiplayer follows 1.0.
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

## Git workflow

- Work on the assigned branch. Create branches or open PRs only when requested
  by the owner. Push focused commits; never push directly to protected `main`.
- When asked to prepare a PR, complete and validate the requested change before
  opening it. The owner merges PRs and publishes releases unless instructed otherwise.
- Preserve unrelated changes and other worktrees. Never delete
  `refs/t3/checkpoints/*`, use `git push --mirror`, or use a bare `git stash`.
- Use the repository's configured author identity. Never commit saves, build
  output, credentials, local backups or machine-specific coordination files.
- Delegate only when requested. Give each implementation agent an isolated
  worktree, explicit scope and reserved IDs; delegated agents do not delegate further.

## Before every commit

Run this exact chain so a failure stops the commit:

```sh
cargo fmt && cargo clippy --release --all-targets -- -D warnings && cargo test --release
```

Run relevant headless, benchmark, visual and persistence checks as described in
[the agent guide](docs/agent-guide.md) and [development guide](docs/development.md).
Report what was actually checked and what remains unverified. Keep documentation
and test evidence aligned with the delivered behavior.

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
- Use original or appropriately licensed implementation and assets. License:
  MIT OR Apache-2.0.

## Reading map

- [Roadmap and acceptance criteria](docs/roadmap.md)
- [Agent engineering guide](docs/agent-guide.md)
- [Architecture](docs/architecture.md) and [development tools](docs/development.md)
- [Contribution workflow](CONTRIBUTING.md)
- [Documentation index](docs/README.md)
- [Historical plans and briefs](docs/archive/README.md), which never override
  current instructions or establish current completion claims
