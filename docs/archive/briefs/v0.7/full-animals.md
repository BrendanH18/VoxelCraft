> Historical record, superseded on 2026-10-12. Old task instructions, priorities,
> reference versions and completion claims are not current guidance. See
> [the archive index](../../README.md) and [the current roadmap](../../../roadmap.md).

# v0.7 task: Breeding, taming, leads and name tags
Worktree: <assigned-worktree>  Branch: agent/animals
IDs: items 1000..=1039 (new append-only range; add a range const + assert like OVERWORLD_ITEM in src/item.rs), blocks 2150..=2199, texture layers 1750..=1799. MobKind: append Wolf, Fox, Parrot, Rabbit, Goat (and others you add) at the end.
Another agent is concurrently adding boats, saddles and horses/donkeys/mules/llamas/camels with horse taming (ranges 960..999). Don't implement those mounts; make your breeding API generic (e.g. MobKind -> breeding food predicate + baby spawn) so their horses plug in easily, and say how in your summary.

Read entity/mob.rs, entity/villager_breeding.rs (villagers already breed), entity/aquatic.rs (turtles: can't lay eggs yet), app/mobs.rs, entity/model*, render/entity.rs.

Deliver (match Java 1.21):
1. Love mode and breeding for all existing farm/passive animals: cow (wheat), sheep (wheat), pig (carrot/potato/beetroot), chicken (seeds), cat (raw cod/salmon), hoglin (crimson fungus), strider (warped fungus), turtle (seagrass -> lays eggs on its home beach; turtle egg block with hatching, trampling; babies imprint home), axolotl (bucket of tropical fish), plus your new mobs. Hearts particles, 5-minute adult cooldown, babies (20 min growth, feeding speeds it up 10%), 1-7 XP orb, follow-food-item tempt AI, mooshrooms if trivial. Sheep: graze grass to regrow wool, coloured-sheep inheritance. Chickens lay eggs every 5-10 min; thrown egg 1/8 chance chick.
2. Taming: wolves (bones; sit/follow/teleport to owner, attack owner's target, collar dyeing, health via tail angle, beg), cats (already exist: make tameable with raw fish, sit, gifts at morning, creeper/phantom scare), parrots (seeds; shoulder perching, dance near jukebox if any, imitate hostile sounds, cookies poison). Foxes (sweet berries, sleep by day, pick up items, trust from breeding), rabbits (carrots/dandelion; rabbit hide/foot drops if items missing), goats (wheat; ramming, horn drop if feasible). Spawning in correct biomes. Owners stored by stable player/profile ID.
3. Leads: item + recipe (4 string + 1 slimeball), attaching to mobs, 10-block leash physics, snapping beyond, fence knots on fence posts, rendering the rope, breaking knot drops lead. Name tags: item (loot only + anvil renaming already exists? integrate), naming prevents despawn, nameplate rendering, "Dinnerbone"/"jeb_" optional.
4. Everything saves/loads (age, love cooldown, owner, sitting, collar, lead attachment, name). Agents/pad players can feed, tame, leash and name.
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
