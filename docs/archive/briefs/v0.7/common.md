> Historical record, superseded on 2026-10-12. Old task instructions, priorities,
> reference versions and completion claims are not current guidance. See
> [the archive index](../../README.md) and [the current roadmap](../../../roadmap.md).

## Common rules (all v0.7 agents)
- You are one of three parallel agents. Work ONLY in your assigned worktree and branch. Do not create branches, push, or open PRs; the integrator merges your branch locally. Do not spawn sub-agents.
- Historical workflow: agents read local instructions and reported roadmap notes to the integrator. Current tracked AGENTS.md and docs/roadmap.md supersede this workflow.
- Research how Java Edition 1.21 actually behaves (minecraft.wiki) and match it: numbers, timings, drops, recipes, sounds. Quality and performance matter.
- New content must also work for gamepad (pad) players and CLI agent players where sensible (see src/agent.rs, src/app/split.rs, Game::puppet).
- IDs are append-only. Use only your assigned ranges below. MobKind variants: append at the END of the enum and MobKind::ALL; saves must stay backward compatible (old saves load unchanged).
- Commit in focused commits as you go (one per feature). Before each commit run, chained with &&:
  `cargo fmt && cargo clippy --release --all-targets -- -D warnings && cargo test --release`
  (this Mac has no `timeout`; use `perl -e 'alarm 600; exec @ARGV' cmd` if needed).
- Add unit tests for rules/logic. Visually check rendering with `--screenshot` (see AGENTS.md) and look at the PNGs.
- Add or update a user doc in docs/ for your feature and the README feature list if appropriate.
- Final message: what's done, commits, test counts, known gaps vs Java, and any IDs/texture layers used.
