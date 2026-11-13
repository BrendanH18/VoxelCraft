# Build-5000 campaign — started 2026-11-12

[← Roadmap](../../roadmap.md) · [Requirements](../../requirements.md) · [Documentation index](../../README.md)

On 2026-11-12 the owner asked for building to continue: "Build 5000 new
features or improvements", split across agents, finished and merged without
waiting for review. This folder is the campaign's record. It says how the
work is cut up, how each piece is checked and how the total is counted, and
it holds one record and one ledger fragment per slice of work.

Nothing here is Verified in the roadmap's sense. Every row this campaign
touches stays Implemented, unverified until the owner has played it
([roadmap §7](../../roadmap.md#7-feature-completion-and-release-policy)).

## What counts

The number reported to the owner is the number of rows in
[`ledger/`](ledger/), checked by `python3 tools/campaign_ledger.py` in CI.
A row is one of:

- **content**: one block (by registry name, however many states it has),
  item, mob, status effect, potion type, enchantment, recipe, villager
  trade, loot table, structure piece, screen, command, option or caption;
- **behaviour**: one Java behaviour implemented or corrected, with its own
  test case;
- **fix**: one defect corrected.

Every row names the test or fixture that proves it, and a name appears
once in the whole ledger. A fact that already matched Java is not a row: a
new fixture that proves a whole table counts once, plus one row for each
difference it found and fixed.

## How a slice is built

1. **Scouted.** Read-only agents compared one area of the game with the
   Java 26.3 reference (the official jars' data files and decompiled
   source, kept outside the repository) and wrote slices: work one agent
   can finish in a sitting, with the items it delivers and how each is
   checked.
2. **Implemented** by one agent in its own worktree, with ID ranges
   reserved for it. The oracle comes first: a fixture extracted from the
   jar by a tool in `tools/`, or constants cited from the decompiled
   source, and tests that compare the game with it in both directions.
3. **Reviewed and repaired** by a second agent in a fresh worktree, which
   re-runs the extractor, checks facts by hand, proves the tests fail when
   the code is wrong, and removes ledger rows that aren't real.
4. **Landed** by the integrator, a few slices at a time, with clippy and
   every test run on the merged tree on a Namespace instance
   (`scripts/remote-check.sh --full`).

At most four agents run at once (owner, 2026-10-17). Builds run in the
cloud; the Mac builds only to run the game.

## What is not checked

Agents can't supply the owner's play, and scripted captures show
appearance, not gameplay. Each wave's record lists what was looked at in
the running game, what wasn't, and the frame-time run for its merged
commit where one could be made.

## Waves

| Wave | Scope | Slices landed | Ledger rows | Record |
| --- | --- | --- | --- | --- |
| 1 | v0.17 foundations and Java data parity | in progress | — | — |
