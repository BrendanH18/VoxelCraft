> Historical record, superseded on 2026-10-12. Old task instructions, priorities,
> reference versions and completion claims are not current guidance. See
> [the archive index](../../README.md) and [the current roadmap](../../../roadmap.md).

# v0.7 task: Boats, saddles and riding
Worktree: <assigned-worktree>  Branch: agent/mounts
IDs: items 960..=999 (new append-only range; add a range const + assert like OVERWORLD_ITEM in src/item.rs), blocks 2100..=2149, texture layers 1700..=1749. MobKind: append Horse, Donkey, Mule, Llama, Camel (and SkeletonHorse/ZombieHorse if you do them) at the end.
Another agent is concurrently adding breeding/love mode, wolves/foxes/parrots, leads and name tags (ranges 1000+). Don't implement generic breeding; expose a small hook (e.g. a fn listing a mount's breeding food) and note where breeding should plug in. Horse taming is yours.

Read entity/minecart.rs (existing rideable minecarts — reuse the riding/passenger plumbing and camera/pose), entity/mob.rs, entity/nether.rs (strider), entity/player_pose.rs (riding pose gap), render/entity.rs and entity/model*.

Deliver (match Java 1.21):
1. Boats and chest boats for every wood type the game has (oak, spruce, birch, jungle, acacia, dark oak, mangrove, cherry, pale oak; bamboo raft if bamboo exists): items, crafting recipes (5 planks in a U; chest boat shapeless boat+chest), placing on water/ice, physics (paddling, momentum, ice speed incl. blue/packed, floating, slow on land, falling out of water, breaking on hard fall like Java), two seats, mobs get picked up by a boat, sneak to dismount, breaking drops the item, chest boat inventory (27 slots, opens with sneak-use or from inside), render model + paddles animation, sounds. Save/load.
2. Saddle item (dungeon/temple/fishing loot where loot tables exist), pigs: saddle, ride, steer with carrot on a stick (item + recipe + durability, boost). Striders: saddle, warped fungus on a stick, walk on lava, shivering speed.
3. Horses, donkeys, mules (and llamas, camels if time permits): spawning in their biomes (plains/savanna/meadow etc.), variants/markings, random stats (Java's health/speed/jump distributions), taming by repeated mounting with temper, feeding (wheat/sugar/apple/golden carrot/hay bale effects), saddle & horse armor (leather/iron/gold/diamond horse armor items; armor is loot-only except leather), donkey/mule chests, mount inventory screen, jumping with charge bar on the HUD, mounted HUD health instead of hunger. Llamas: carpet decor, caravans optional, spit.
4. Player riding pose and camera, mount rendering, dismount placement rules, mounts/boats saved with the world, agents and pad players can mount/dismount.
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
