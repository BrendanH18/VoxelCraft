> Historical record, superseded on 2026-10-12. Old task instructions, priorities,
> reference versions and completion claims are not current guidance. See
> [the archive index](../README.md) and [the current roadmap](../../roadmap.md).

# Roadmap

## Current plan (owner decision, 2026-10-03) -- read this first
Match Minecraft before building extra features. Work in this order:

1. **Survival critical path to the End** (P1 first bullet; ordered steps in
   "Gap audit vs Java 26.3"): XP orbs (done) -> Endermen + pearls (done) -> blazes/spawners (done) -> fortresses +
   blazes (done) -> brewing (done, core) -> strongholds (generation done) -> Eyes of Ender + End portal (done) -> Ender Dragon (done) -> enchanting/anvils (done) -> Netherite. Done when a fresh survival
   world can be beaten.
2. **Core survival and building gaps** (rest of P1): material breadth,
   everyday items, shaped blocks, combat/armor rendering, movement poses,
   particles and music, difficulty and options.
3. **Major systems** (P2): structures and loot, mob roster and AI,
   animals/villagers, redstone, terrain/oceans and -64..319 height, food/
   farming/travel, weather rules, remaining content and commands.
4. **Multiplayer as Minecraft has it**: LAN hosting/joining (P0/P0A), then
   the dedicated server (P1 server section).
5. **Modern releases** (P3), up to the current Java release.

**Paused extras** (maintenance and bug fixes only, no new features until
the steps above land): split-screen/gamepad polish (P0B), CLI agent
features, enhanced graphics and other non-Minecraft additions. Keep them
working; when a parity feature touches them (new items, blocks, mobs),
make it work for agents and pad players too rather than leaving gaps.
Exception: the owner may still ask for specific extras directly.


## Release train to 1.0 (owner, 2026-10-07: "speed up releases, get to 1.0")
v0.3.0 shipped 2026-10-07 (survival is beatable). Ship a minor release per
wave of 2-3 parallel agent tasks; each keeps the Current plan order.
- v0.4.0 (in progress, branch t3/roadmap-release-next, pushed):
  Round 1 merged at 312f53f (gate 497 lib tests; gen 0.221 / mesh 0.649
  ms/chunk at rd 8). Villages (blocks 900-952; docs/villages.md): Java
  placement 34/8/salt 10387312 in 5 biome sets, original compact jigsaw
  pieces, loot, `/locate village`; workstations (barrel, smoker, blast
  furnace, grindstone functional; others decorative), dirt path, hay bale
  (901-903; the redstone duplicate 1469 was removed, never reuse 1469);
  villagers with 12 professions, POI claims, schedules, Java trades,
  restock, trading UI + pad tab + `/trade` + agent `trade`. Redstone
  (blocks 1100-1468, item 760 fire charge; docs/redstone.md): queued
  engine (not Java's exact update order), dust/levers/buttons/plates/
  torches/repeaters/comparators/lamps/targets/daylight, iron doors and
  trapdoors, pistons with animation, observers, dispensers/droppers/
  hoppers. Texture layers: VILLAGE band after CAKE_BOTTOM, redstone
  reserves 1100..=1138 (tex::COUNT 1139, const-asserted).
  Codex hit its usage limit mid-round; I committed both WIPs after the
  gate. Round 2 merged 2026-10-08 (09f5e48 redstone: rails/carts/notes/
  tripwire; 566442c villages: golems, zombie villagers, breeding, gossip,
  bells, composters, wandering traders). 0.4.0 prepared at eb3f096 (546 lib
  tests; gen 0.223, mesh 0.682 ms/chunk). Owner tags v0.4.0 to build the
  draft release. Gaps: raids, leatherworker, lectern/loom/cartography/
  stonecutter UIs, water rails, cart collision parity, note particles.
  Texture layers used: village pumpkins/compost VILLAGE+45..+48, redstone
  1100..=1154; next free layer 1155.
- v0.5 briefs for claude.ai/code cloud sessions in briefs/ (owner launches; Opus 5.5 high):
  agent/nether-biomes (blocks 1600-1799, items 800-839, tex layers
  1300..=1399) and agent/nether-mobs (blocks 1800-1849, items 840-899, tex
  1400..=1449). Held until v0.4 merges: breeding/taming/leads, spawning/
  pathfinding. Texture layer plan: redstone round 2 1139..=1199, villages
  round 2 should use 1200..=1299 (renumber at merge if it collided).
- v0.5.0 (2026-10-08, branch t3/prep-0-5-release, pushed; owner merges and
  tags): both cloud branches integrated, agent/* branches deleted. Done:
  Nether biomes (blocks 1600-1793, tex 1300-1399), piglins/brutes/bastion
  residents/hoglins/zoglins/striders (entity/nether.rs), crossbow (items
  840-841, recipe), crying obsidian (block 1800, tex 1400). Bone block 1581
  (v0.4) is legacy: chunks and items remap to 1631 on load. Moved to v0.6
  or later: breeding/taming/leads, light-based spawning and pathfinding,
  strider riding, soul lanterns.
  Pre-PR Codex review (2026-10-08): fixed final feature writes (delta lava),
  twisting-vine ground search + vertical seams + glass support, adult hoglin
  hunting, mobGriefing pickup, Thorns/loot attribution, carried-stack metadata,
  pocket overflow, conversion/death inventory loss, co-located corpse loot,
  baby zoglin drops, crossbow enchanting/Fletcher trades/pad FOV, Nether plank
  anvil repairs and note instruments, agent guarded-container/mining anger.
  No new IDs. Gate: 622 lib + 78 bin + 5 integration tests; headless 627 tests;
  both release clippy modes clean. Shader validation + M5 screenshot smoke pass.
  Bench rd8: Overworld 0.259 / Nether origin 0.216 / mesh 0.684 ms/chunk;
  0.18 s streaming. Cold biome medians: wastes 0.234, soul 0.275, crimson 0.417,
  warped 0.370, basalt 2.360 ms/chunk. Follow up basalt decoration performance;
  benchmark origin hides feature-heavy cost. Shared pad biome fog and converted
  piglin main-hand weapons remain documented parity gaps. Full local report:
  target/review/review.md (untracked). Fixes pushed as 887646f (world/vines), 97c7769 (mob inventories/combat), 9741675 (item/player integration).
- PR #29 review fixes (2026-10-08): melee/sweep and player arrows/potions
  retain the actual attacker ID for Nether anger/broadcasts; crossbow aim
  delay drawn once per shot; piglin arrows keep their own death message.
  Nether grid cache evicts one column at capacity and rechecks concurrent
  fills instead of clearing everything. Five regressions; required fmt,
  strict release clippy and release tests pass (710 passed, 2 existing
  ignored). Pushed 94c3021 (combat) and 63d8f0c (cache); three review
  conversations resolved. No new IDs.
- v0.5.0 original plan: Nether biomes (crimson/warped/soul sand valley/basalt deltas),
  piglins/bartering/hoglins/striders, spawning/AI fidelity (light-based
  cave spawning, pathfinding), animal breeding/taming/leads.
- v0.6.0 (done 2026-10-10, branch t3/v0-6-0-roadmap-implementation, pushed;
  owner opens the PR, merges and tags v0.6.0). Commits: 13de6d5 heights +
  blocks 1900..=2076 (tex 1450..1611, waterlogged plants via holds_water,
  Wood gains DarkOak/Mangrove/Cherry/PaleOak, world::trees, items
  900..=918); 40081f2 generator (world::climate, world::biome 53 Java
  biomes, world::caves noise caves/ravines/aquifers, true ore heights,
  height.rs removed); 81994b7 structures (world::temples, Codex);
  9f00bf4 aquatic mobs + guardians + pillagers (entity::aquatic, Codex);
  2958a76 turtles + `terrain=2` save prop (world list marks pre-0.6
  worlds); d144913 docs/README/release notes, version 0.6.0; bb0a40c
  fixed the remesh queue leak (sync edits outside mesh range kept
  is_idle false, so screenshots hung) and lush moss beside the y=-54
  lava sea burning forever; eb4085c per-column biome snow lines for
  weather; e28e538 + 16e35cd PR #30 review fixes (Peaceful-deferred
  structure residents, turtle growth and dispensed creatures saved,
  waterlogged plants flow, leaf drops, ore seams, trees off cliffs).
  Gate 670 lib + 78 bin + 5 integration tests, strict clippy.
  Bench rd8 M5: gen 0.39, mesh 0.87 ms/chunk. Docs: overworld-terrain.md,
  overworld-structures.md, aquatic-mobs.md.
  Gaps: heightmap surface (no overhangs), deep dark/glow lichen/springs,
  turtles can't lay eggs (no breeding), turtle helmet, dripleaf tilt and
  dripstone falling, buried treasure (so no heart of the sea yet),
  breeding/taming/leads and light-based spawning still open.
  Note: this Mac has no `timeout`; use `perl -e 'alarm 60; exec @ARGV' cmd`.
- v0.7.0 (in progress 2026-10-10, branch t3/continue-project-build, pushed;
  no PR): three Codex gpt-6.1-sol agents (briefs/v0.7/) hit the Codex limit
  mid-task; I finished and merged them myself (owner: "no more sub agents").
  Merged: 47e4fec/e2cebdf breeding + companions (items 1000..=1020, MobKinds
  Wolf/Fox/Parrot/Rabbit/Goat; docs/animals.md), 7259753 leads/fence knots/
  name tags (Claude), 66c48e5 boats/chest boats + 6c7cf9d saddles, pig/
  strider steering, horses/donkeys/mules (items 960..=987, MobKinds Horse/
  Donkey/Mule; docs/mounts.md; jump uses the wiki height fit), 8643cb3 LAN
  (docs/lan.md, protocol 1, no IDs). Integration: golden carrot kept once at
  987 (animal duplicate 1021 dropped, never shipped); horse/donkey breeding via
  the shared pipeline (can_mate_with, mule offspring, Java
  createOffspringAttribute in mounts::inherit); egg timer reload fix.
  After merge: 9de92d6 LAN streams min(client, host, 8) chunks + 2 rings,
  clients hold evictions until positioned (fixed never-meshing holes);
  615c338 mob looks (entity/look.rs), carts/boats and seats replicate; a raw
  LAN client mounts/dismounts a horse (target/lan_ride.py).
  7738046 llamas (strength/carpets/chests/spit, temper 30, hay breeding) and
  camels (desert villages 1 per 4 homes, two seats, dash, sitting, cactus);
  MobKinds Llama/Camel appended (54 kinds). b078cc8 0.7.0 prepared: version,
  RELEASE-NOTES, README tiles (docs/images/animals.jpg, lan.jpg), --no-hud flag,
  duplicate golden carrot recipe removed. Gate 714 lib + 81 bin + 5 + 3 tests,
  strict clippy both configs, headless example. Bench rd8 M5 same as main
  (gen ~0.44, mesh ~0.92 ms/chunk on this machine today; 0.6's 0.39/0.87 was
  a quieter run). Owner opens the PR, merges and tags v0.7.0.
  Gaps: llama caravans, undead horses, player nameplates, vehicle/mount storage over LAN, enchant/
  brew/anvil/villager screens over LAN, sounds/particles event stream to LAN
  clients, independent dimensions per player.
- v0.7.0 independent Codex review (2026-10-10, HEAD b078cc8 vs main 022ca44):
  NOT fully spec-complete; normal client/headless fmt/clippy/tests and desktop
  LAN rendering smoke pass, but 7 targeted audit checks fail. Fix fox carried
  item loss on death; desktop/pad/LAN saddled pig/strider feeding and mount
  name-tag dispatch; register Horse/Donkey/Mule/Llama/Camel for leads; notify
  wolves of owner's ranged hits (owner defense also needs integration);
  check all parrots for occupied shoulders; replicate mount max health and
  jump charge for LAN HUD. Full P0A replication/events/nameplates and the
  real multi-machine/concurrency/death/portal/reconnect/restart acceptance
  matrix remain unfinished. Portal host-follows was explicitly allowed in
  the original LAN brief; caravans/undead horses were optional. Report and
  reproducible failures: target/review-0.7/report.md and release_review.rs.
  No tracked source changed, no commit/push/tag. Gate: 714 lib + 81 desktop
  + 5 + 3 integration tests; headless 714 + 5 + 3; existing ignored tests.
- v0.7.0 audit resolution (2026-10-11/12, t3/continue-project-build, pushed):
  fdcbaed the 7 defects (+ owner defense via OwnerHurt, arrow shooter uid;
  tests/release_0_7.rs); 0129c08 llama spit projectile + wolf rivalry +
  caravans; 141a50f player name tags + discovery count/version; a33a5d2
  entity replication (entity/replica.rs, falling blocks, dragon fight);
  4b53da5 sound/particle broadcast (audio tap, Request JSON); 8679a13 all
  screens over LAN (+ --lan-cheats); 750d338 chunk Edits packets, lite player
  state, compact mobs (idle ~138 -> 27 KiB/s) + tools/lan_check.py + /save-all
  + LAN stats log; 0a9ef61 multi-crack renderer (MAX_CRACKS 16), swings,
  eating; 5b943d7 PvP Java rules + arrows (+ --lan-pvp); ddfb672 session.lock;
  fa1c0d1 host portal travel check; f5bf0d1 docs/lan.md + release notes.
  tools/lan_check.py: 13/13 on M5. Remaining P0A limits by design: shared
  host dimension (allowed), offline identities, no cross-machine pass yet.
- Back-check of earlier releases (2026-10-12): v0.5.0 planned light-based
  cave spawning + pathfinding and breeding/taming/leads; spawning/pathfinding
  were never done (breeding etc. landed in 0.7). Soul lanterns deferred from
  0.5. v0.6.0 scope delivered, but inside it the turtle helmet/Turtle Master,
  pointed dripstone falling/dripping and big dripleaf tilt are missing.
  Doing these on t3/continue-project-build before 0.7.0 ships.
- v0.8.0: dedicated server (P1 server); End cities/shulkers/elytra/
  fireworks; Wither + beacons.
- v0.9.0: advancements/statistics, maps/signs/books/banners/item frames,
  remaining mobs (illagers/raids, mansions, ancient cities/warden).
- v1.0.0: "Java 1.21 survival parity + multiplayer": every P1/P2 item
  checked or explicitly scoped out, hands-on test pass, save stability
  promise. P3 (newer drops) continues after 1.0.

## Parallel wave (branch t3/improve-project-quality, 2026-10-07)
All merged and pushed (gate: 373 lib + 71 bin + 5 integration tests):
- Procedural situational music (src/audio/music*, docs/music.md): Java
  MusicManager delays/fades, Music slider. Levels are quiet (peak ~0.1).
  Nether uses one palette (no Nether biomes yet); credits music unused.
- Mossy cobblestone (Block 224), monster rooms (src/world/dungeon.rs; gen
  0.19 -> 0.23 ms/chunk). Loot gaps: saddle, golden apple, discs, name tag,
  horse armour, redstone, seeds; dungeon books lack enchantments.
- Smithing table (223), Netherite tools/armor + upgrade template (items
  310-339); bastion loot data in place but bastions not generated.
- Difficulty, Adventure/Spectator/Hardcore, 17 gamerules, Java commands with
  tab completion (src/rules.rs, console.rs, agent.rs). Gaps: only @s/@p,
  /locate = stronghold/fortress/biomes, no lightning, no CanDestroy.
- Block u16 (4096-state table), byte/palette/u16 chunk storage, VXC2 saves
  (VXC1 loads), 11-bit texture layers (2048; 256-layer pages fallback).
  Block items >=256 use item::BLOCK_ITEM_BASE (0x8000) + state.
- Player model, F5 cameras, view bobbing/hurt tilt, armor layer + glint,
  mob armor. Gaps: no riding/elytra/bow/offhand poses, leather undyed.
- Particles: 16384 pool, one instanced draw, mailbox events + client
  ambient emitters, All/Decreased/Minimal. Smoke looked too big/black (fix
  queued). Later: hearts/angry, bubble columns, sprint dust, splash potions.
Wave 3 (all merged by 130a08e): perf fix (gen 0.39->0.22, mesh 1.36->0.70
ms; docs/performance-2026-10-07.md), bastions + blackstone/basalt/magma/
chains (blocks 800-834, items 608-610 magma cream/iron nugget/slimeball) +
credits (src/app/credits.rs; host only), dyes/16 colours (items 576-606,
blocks 561-762; docs/colors.md; brown/black dye unobtainable), everyday
items (items 512-530, blocks 520-544, shears=512; docs/survival-items.md),
mobs (cave spider, slime, magma cube, ghast, wither skeleton, witch,
husk, drowned, babies; ghast tear 640, skull item 641, splash potions
436-471; docs/mob-parity.md). Gaps: piglins/hoglins, skull block, mob
non-instant effects, Nether biomes, pad/agent credits. Generation drifted
back to ~0.28 ms/chunk after mobs (check). Next: v0.3.0 release prep
(agent/release), then villages, redstone, Nether biomes, LAN.
Original wave 3 plan (ID ranges; potions own items 400-511):
perf regression hunt (no IDs); everyday items (items 512-575, blocks
520-559); dyes/16 colours done on agent/dyes (items 576-607, blocks
560-762 of 560-799); bastions + blackstone + credits (blocks 800-879,
items 608-639); mobs (items 640-679, blocks 880-899, splash potions
436-471). Merged earlier: materials (blocks 225-499, items 361-366,
world::height::java_y), mineshafts (rails 500-509), poses.
Earlier wave (ID ranges): materials (Blocks 225-499, items 360-399),
mineshafts + smoke fix (Blocks 500-519, items 420-439), swimming/crawling
poses. Worktrees ../agent-<slug>; brief /tmp/vc_handoff.md.
Materials height map (agent/materials): Java underground Y is no longer
`+64`. `world::height::java_y` sends y ≥ 63 to y − 1 and compresses
[-64, 63] onto [0, 62] (factor 62/127). Deepslate sits at y ≈ 31.
Vein attempts scale by that factor. Nether debris is unchanged (Nether Y).
Lava stays at y = 10 because caves are not carved through bedrock.

## Ancient debris and Netherite materials (branch t3/continue-building, 2026-10-06)
Done: Block 221 ancient debris, 222 block of Netherite; items 308 scrap,
309 ingot (append-only IDs). Nether generation decorates each Java 16x16
cell with size-3 scattered ore at triangular origin y8-24 and size-2 at
uniform origin y8-119. Java's random candidate count (0..size), offsets,
24-bit float draws and rounding; candidate positions can reach y6-120.
Only netherrack is replaceable (no basalt/blackstone yet). Places after
fortress carving, rejects adjacent air (lava is allowed), handles horizontal
and vertical chunk seams. ExposureCache memoizes interpolated terrain and
32-cell neighbour columns; `Fortresses::column_at` shares Paint's structure
writes and pillar stop logic, avoiding neighbour-chunk generation. Paint
now indexes its clipped buffer dimensions (full chunks keep X/Z/Y order).
Smelting: debris -> one scrap in 10s, 2 XP. Shapeless four scrap + four
gold -> ingot; nine ingots -> block and back. Diamond pickaxe required;
hardness 30/50, self drops with Fortune/Silk Touch, no mining XP. Blocks
survive normal explosions; dropped debris/scrap/ingot/block survive fire
and lava, float in lava and still expire in five minutes. Procedural block
textures/icons, creative palette/name lookup, normal saves and generic
host/pad/agent mining/crafting/furnace paths. Cursor researched rules and
implemented initial generation; Codex review caught count/spread and
pillar-query gaps, fixed with regression coverage.
Validated: fmt, release clippy (-D warnings), 369 default tests passed
(one existing ignored sprite-sheet dump), 301 headless tests passed, rd=4
benchmark 0.930 ms/dense chunk; Nether generation over 1024 chunks averaged
0.212 ms/chunk on Apple M5. Screenshots: target/netherite-resources.png and
target/netherite-inventory.png. Sources: Minecraft Java ScatteredOreFeature
(https://raw.githubusercontent.com/mahtomedi/minecraft/main/src/main/java/net/minecraft/world/level/levelgen/feature/ScatteredOreFeature.java),
vanilla worldgen/recipe JSON from InventivetalentDev/minecraft-assets 1.21.
Known: cell seeds use VoxelCraft's deterministic terrain RNG, not vanilla's
world seed decoration stream. No bastion loot, blast furnace or Netherite
equipment yet. Next: bastions and upgrade-template acquisition, then
smithing table and Netherite tools/armor (preserve per-stack data).

## Fortresses (branch t3code/roadmap-quality-performance, 2026-10-04) -- survival path step 3
Done: nether wart crop/item (Block 184..=187, ages, soul sand, 1-in-10
random ticks, 2-4 ripe drops). world/fortress.rs: Java NetherFortressPieces
layouts (weights/limits, 112 reach, depth 30, y 48-70, region 432, every
region until bastions), cached per region, painted per chunk (~16 us);
pillars use NetherGen::column_open (exact interpolated density). Fresh
chunks register blaze spawners and nether-bridge loot chests
(`World::register_structure_features`, saved entries win). Fortress spawn
list (blaze/zombified piglin/skeleton) inside piece boxes via
`MobWorld::in_fortress`. Item icons moved to their own texture array
(layers 256+, `tex::ITEM_BASE`, u16 icon layers); block layers now
156/256, so brewing/books/lapis/Netherite icons fit.
Simplified vs Java: stalk room has no upper exit/stairs, hall fences are
approximate, entrance lava doesn't flow until disturbed.
Not yet: wither skeletons, magma cubes, bastions (then 2/5 fortress
weighting), saddles/horse armor in loot.
Brewing done (2026-10-04, same branch): status effects
(simulation/effects.rs, saved, `/effect`, `--effect`, HUD icons + inventory
panels, night vision in shaders), potions (potion.rs, 36 types, items
400+, drinking via the eat path for host/pads/agents, glass/water
bottles), ingredients (sugar, glistering melon, spider eye 1/3 drop,
spider eye poison), brewing stand block 188 + world/brewing.rs + UI + pad
tab + `--open-block`. Brewable now: awkward, mundane, thick, swiftness,
healing, poison, strength and their strong variants.
Not yet: splash/lingering potions (gunpowder; mobs have no effects yet,
so splash would need mob effects or instant-only), effects on mobs,
redstone (long variants), magma cream (fire resistance; needs slimes or
magma cubes), ghast tears, golden carrots, rabbits, pufferfish, phantoms,
fermented spider eyes (needs mushrooms), turtle master, invisibility,
milk clearing effects, effect particles, beacons.
Strongholds (2026-10-04, same branch, commit "Generate strongholds"):
blocks (stone bricks + mossy/cracked, stone brick slab 193 / stairs
189..192 as the 7th SLAB_BASES entry, iron bars 197 panes, bookshelf 198,
cobweb 199 with Java stuck movement, End portal frames 200..=207 (+4 =
eye), End portal 208), paper/book/eye of ender items + recipes,
silverfish mob, world/structure.rs (shared with fortress.rs),
world/stronghold.rs (Java rings, 11 piece kinds, portal room with
12 frames 10% eyes, silverfish spawner, loot), `Generator::strongholds`,
`Strongholds::nearest`, Feature::StrongholdChest, `--open-block`.
PR #21 review fixes (2026-10-04): Creative bottle filling keeps the empty
bottle; lethal harming drinks consume the potion before death drops;
controller brewing stands route to the pad tab; startup instant damage
uses survival damage/death handling; effect/container CLI overrides
clear after the first world; cobweb mining takes 20 s by hand / 0.4 s
with a sword; all End portal frame states emit light level 1 (Java).
PR #21 follow-up: death transitions clear active effects before saves;
--open-block rejects invalid integers instead of dropping coordinates.
End portal progression done (2026-10-05, branch t3code/continue-building,
commit "Add eyes of ender, End portal activation and End travel"):
entity/eye.rs (Java EyeOfEnder: 12-block lead +8 up, 80 ticks, 4/5 drop),
`Entities::release_eye`, frame insertion + inward ring check in
world/end_portal.rs (`World::insert_eye`), END_PORTAL travel in
`update_portal` (overworld/Nether -> EndSpawn at once, End -> Respawn),
agents via `place` (Event::EyeThrown/FrameFilled), Sounds EyeDeath,
FrameFill, PortalSpawn. Checked with target/end_portal_check.py.
Not yet: eye trail particles, credits/exit portal, mobs/items through
End portals, pad players in a different dimension.
Ender Dragon done (2026-10-06, branch t3code/continue-building-1, commit
"Add the Ender Dragon fight, End crystals and the exit portal"):
entity/dragon.rs (`Fight` in `Entities::fight`, built in restore_dimension
from `Generator::end()`; Java phases on 20 Hz ticks with Java yaw
convention; 24 path nodes; parts/hit boxes; crystal healing, detonation
next tick, `Fight::explode`; DragonFireball + BreathCloud; death XP via
EntityEvent::DragonXp), entity/dragon_model.rs (model trails 7-tick-old
yaw like Java, neck/tail from latency history, crystals, beam, breath motes,
death rays). End gen: caged pillars (rank 1/2), `podium_block` (Java
EndPodiumFeature), `World::open_exit_portal`, eternal fire on End bedrock.
Block 209 dragon egg (shaped, gravity, light 1, `World::teleport_egg` on
left/right click). Boss bar (host HUD only). Save key `dragon` =
state(0 dead,1 alive,2 dying/unopened),previously_killed,health,mask.
Sounds DragonFlap/Growl/Death. Checked with temporary spawn tweaks and
screenshots (flight, perch + breath, death rays + portal + egg).
Simplified: holding-pattern paths are ring hops (inner node when crossing)
instead of A*; strafe flies straight at the player; arrows don't know their
shooter (crystal hits by arrow don't trigger strafe); no wall torches on the
podium; boss bar not on pad split-screen views; dragon not frozen when far.
End gateways done (2026-10-06, same branch, commit "Add End gateways to
the outer islands"): Block 210 END_GATEWAY (Cutout, non-solid, light 15,
END_PORTAL texture), `EndGen::gateways` (seeded order) and
`EndGen::gateway_exit` (Java search on generator columns, no loaded chunks
needed), `end::gateway_block`, `World::build_gateway/build_end_island/
gateway_arrival`, Fight tracks gateways/unbuilt/links (save fields 5-6 and
`;a b c d e f` links), Arrival::Gateway (agents follow the host), pearls
into gateways (EntityEvent::PearlGateway, host only). Checked by
`--place 90,75,0,end_gateway` + pose inside it.
PR #23 review fixes (commit b8e96ca): crystal detonation queue remap,
portal/gateways wait for whole footprint (fight save state 3 = dead,
portal shut; state 2 health field = death ticks, death resumes over the
podium), full 5x5 cages, EntityEvent::Shove for perched wings,
`World::break_blocks` batching, agent pearls into gateways land at the
mouth. Follow-up 1197f94: player arrows ray-cast each step against mobs and the fight and take the nearest hit.
Not yet: gateway beam/particles, agent or pad pearls through gateways,
non-player entities through gateways.
**Next:** respawning the dragon with four End crystals (crystal item: glass
+ eye + ghast tear; needs ghasts first), credits on first exit, End cities/
shulkers/elytra (P2). Then enchanting/anvils (needs per-stack data). Not done in
strongholds: wall torches, infested blocks, iron doors/buttons, filler
corridors, library chandeliers.
**Order change (2026-10-04):** strongholds + Eyes of Ender + End portal
next, before enchanting/anvils: they're required to beat the game, and
enchanting first needs per-stack data (Stack is item/count/damage only),
a bigger refactor.

## Enchanting and anvils (branch t3code/continue-project-development, 2026-10-06) -- survival path step 8
Done, three commits:
1. "Add enchantments": `Stack` gained `enchants: Enchants` (u128, 3 bits per
   enchantment, append-only order) and `repair_cost: u16`; saves are
   `id:count:damage:hex:cost` (old saves load). src/enchant.rs: Java 1.21's
   27 definitions (weights, min/max costs, anvil costs, supported/primary
   item tags, exclusive sets), `JavaRandom` (java.util.Random port, tested
   against the JDK), table offer selection, anvil rules, protection math,
   `enchant::roll()` (atomic splitmix for unbreaking/mending/anvil wear).
   Effects for host, pads and agents: protection family + feather falling
   (`enchant::protect` inside `Game::damage_player` / `Agent::damage`, kind
   read from the cause string), respiration (`Env::respiration`), aqua
   affinity + Java's 5x underwater and 5x airborne dig penalties
   (`mining::dig_time` / `Digger`), depth strider (`Player::wear_boots`),
   thorns (`Target::thorns`, in mob melee), sharpness/smite/bane
   (`MobKind::creature`), knockback, fire aspect (`Mob::ignite`), looting
   (`drop_loot_with`, `drops(rng, looting)`), sweep attacks + sweeping edge
   (`Entities::melee`, now the shared player melee path), efficiency, silk
   touch + fortune (`World::spill_mined`, `mining::silk_drop`), unbreaking
   (`Inventory::wear`/`wear_armor`), power/punch/flame (`Arrow::enchants`,
   `shoot_enchanted`), infinity, mending (`Inventory::mend` in
   `orb::absorb`), binding (`can_unequip`), vanishing (`take_all`). Item
   306 enchanted book. Purple glint on icons (overlay shader: negative
   alpha = fill the texture's shape, `Ui::icon_shape`). Tooltips list
   enchantments. `/enchant`, `--enchant e[,l]` (slot 0).
2. "Add lapis lazuli and the enchanting table": blocks 211 lapis ore (y<32,
   cells 24-25 in `ore_or_stone`), 212 lapis block, 213 enchanting table
   (Shaped::EnchantingTable, light 7); item 307 lapis. app/enchanting.rs
   (Container::Enchanting, `Game::work` slots, SlotRef::EnchantItem/
   EnchantLapis/EnchantOffer), `enchant::bookshelves` (Java's ring rule),
   offers from `Experience::seed` (saved as 4th xp field). Pad tab
   Tab::Enchanting; agent `enchanting 1..3` + `target.enchanting_offers`.
3. "Add anvils and blocks of iron": 214 iron block, 215..=220 anvil /
   chipped / damaged x (top along z, along x) (Shaped::Anvil, gravity,
   broadside placement in `with_facing`). app/anvil.rs (Container::Anvil,
   SlotRef::AnvilLeft/Right/Result), `enchant::anvil` / `anvil_any_cost`,
   `enchant::wear_anvil` (12%). Pad Tab::Anvil; agent `anvil 1..9`.
PR #24 further review (2026-10-06): Mending repairs all eligible pieces
after odd-durability rounding and avoids XP arithmetic overflow. Pad work
inputs now live on Agent so saves return them (including overflow), death
drops them and Vanishing destroys cursed gear. Agent enchanting uses slots
freed by consumed lapis. Silk Touch gravel never produces flint; spawners
still drop XP with Silk Touch. Survival anvils reject plain books as the
left input. Table clues use the post-selection RNG state, and offer seeds
wrap as Java ints. Fire Aspect/Flame kills drop cooked meat and loot;
five-second player credit controls XP/player-only drops, while environmental
finishing damage does not inherit Looting. Corrected the anvil screen's
misleading "Repair & Disenchant" title. Added regression tests, including
non-vacuous saturation checks. Kept spider-eye base clamping before Looting:
Java's set_count clamps first, so the suggested raw-negative-base change
would be incorrect.
Validated: fmt, release clippy (-D warnings), 345 passing tests (one existing
ignored sprite-sheet dump), headless/no-client tests, rd=4 benchmark (~0.95
ms/dense chunk), and enchanting/anvil screen screenshots under target/.
Claude follow-up review (2026-10-06): fixed all seven confirmed Java 1.21
parity gaps. Sweeps now use base attack damage plus the secondary target's
own enchantment bonus, apply Fire Aspect, and stop at three blocks from
the player. Sheep wool ignores Looting. Thorns uses uniform 1-5 damage
and applies mainhand Looting/Fire Aspect to its kills. Depth Strider is
halved off the ground and does not speed up lava movement. Silk Touch
snowy grass returns a grass block. Fire Aspect cooks direct/sweep loot
even in water. Agent mining now accumulates normalized per-tick progress
so changed tools/water/ground conditions cannot alter past progress. Added
six regression tests and strengthened existing Silk Touch/Thorns checks.
Validated follow-up: fmt, release clippy (-D warnings), and all 351 tests
passed (one existing ignored sprite-sheet dump). Commit 3bd4364.
Attached PR #24 feedback follow-up (2026-10-06): reset accumulated mining
progress when the targeted block identity changes (agents, host and pads).
Exclude sprint attacks from sword sweeps, reading each controller agent's
current input while puppeting rather than the host's keyboard. Controller
container tabs now require their own workstation type, preserving valid
furnace-lit/anvil-damage transitions. Added regression checks for all three
issues. Older ice/Power/Vanishing/Thorns/anvil-stack findings already fixed.
Kept vanilla spider-eye base clamping; saturation and nonempty loot tests
were already addressed.
Validated: fmt, release clippy (-D warnings), all 355 tests passed (one
existing ignored sprite-sheet dump). Commit c5c0dc3.
Not yet: anvil renaming (needs per-stack names), falling-anvil damage and
landing sound, the floating book over the table and its particles, real
enchanting/anvil sounds (reuse level-up / stone place), glint on held,
worn and dropped items (icons only), enchanted books and gear in
structure loot (Java's stronghold library, fortress chests), mobs spawning
with enchanted gear, grindstone, smithing table, frost walker/soul speed/
swift sneak, thorns costing durability, sprint knockback, a random
initial enchantment seed (starts at 0).
**Next:** Netherite materials/generation done (see section above). Bastions
and Java's upgrade-template acquisition next, then the smithing table and
Netherite equipment. Then loot with enchanted books, dragon respawn (needs
ghasts).

## Experience (branch t3code/98bb2955, 2026-10-03) -- survival path step 1
Done: XP orbs (entity/orb.rs) and levels (simulation/experience.rs, held
in `Vitals::xp` so the host, agents and pad players all have it). Java
values: level curve, orb sizes, 2-tick pickup, 8-block homing, merge,
5-minute life, ore XP (coal 0-2, diamond 3-7, quartz 2-5 when harvested),
mob XP (5 hostile, 1-3 animals, via `drop_loot`), furnace per-item XP
stored in `Furnace::xp`, released on output take or block break
(`World::xp_drops`). Death drops min(7*level, 100) and keeps the total as
the death-screen score until respawn. HUD bar + level, Sound::Orb and
Sound::LevelUp, `/xp add|set|query`, `--xp`/`--orbs`, saves (`xp` prop,
agent profile `xp`, per-dimension `orbs`). Not yet: Mending, bottle o'
enchanting, breeding/fishing/trading XP (those features don't exist yet),
player-caused explosion XP, baby-mob and equipment bonuses.

Endermen + ender pearls (same branch, 2026-10-03): MobKind::Enderman with
Java's stare cone/line of sight, freeze within 16, flee stares within 4,
teleport toward far targets, arrow dodging (`Mob::teleport_pending`),
water/rain damage, daylight wandering, End packs (Ctx now has
`dimension`; End spawning on). Pearls: entity/pearl.rs, owner PlayerId
(`Agent::id`, `Game::actor` while puppeting), `Vitals::pearl_cooldown`,
hotbar veil. Not yet: endermen carrying blocks, endermites, carved
pumpkin stare immunity, looting.
**Icon budget:** item icon layers are 86/96 (tex::ITEM_COUNT +
ITEM_MORE_COUNT) and the block texture array is at 248/256 layers (layer
ids are bytes). Blaze rod/powder and eye of ender fit; brewing, books,
lapis and Netherite will not. Move item icons to their own texture array
(or widen layer ids) before that.
Blazes + spawners (same branch, 2026-10-03, commit "Add blazes,
fireballs, spawners..."): MobKind::Blaze (hover, 3-shot bursts, melee,
water damage), entity/fireball.rs, Block::SPAWNER (183, tex::SPAWNER) with
world/spawner.rs table (saved as `spawners`, `World::set_spawner`), Java
spawner rules in `Entities::run_spawners`, Block::NETHER_BRICK_FENCE (182,
recipe), blaze rod/powder items. Texture layers now 249/256, item icons
88/96. Not yet: blaze smoke particles, mob spinning inside cages.
PR #20 review: saved spawners activate only while their loaded cell holds
a cage; dead players no longer activate cages or advance their delays.
Added nether brick fences to the creative palette and removed Endermen
from the End documentation's remaining-work list. Furnace shift-click XP
finding was invalid: the existing output-count guard checks for a move,
and taking XP drains the stored tally, including on partial transfers.
Regression tests reproduced both spawner failures before the fixes.
Validated fmt, release clippy with warnings denied and release tests.
PR #20 merge preparation: extracted the existing output-count XP guard
into `Furnace::take_output_xp`, used by both click paths, with a regression
covering full inventories, partial transfers, leftover takes and new
smelts. Added API documentation for the review's coverage warning.
Headless release clippy/tests and rd=4 bench passed (0.861 ms/dense chunk).
Final commit 8d050d5: 278 default / 214 headless tests passed; all GitHub
checks green, all five review threads resolved. Latest CodeRabbit review
has no actionable findings and docstring coverage is 83.91% (passes).
Latest architecture note: verified that pre-XP builds reject seven-field
furnace records. Documented unsupported save downgrades and complete
pre-upgrade world backups/restoration in releases.md, linked from gameplay.
Final head 8749f7b: all GitHub checks green, five threads resolved, merge
state CLEAN. CodeRabbit reports no actionable findings or merge-blocking
risks and confirms the documented downgrade/recovery behavior.
**Next: Nether fortress generation** (nothing generates spawners yet; use
`--place x,y,z,spawner` in the Nether to test). Plan: region grid (432
blocks, every region until bastions exist), piece-based layout computed
per chunk from the region seed (bridges 5 wide x 19 with nether brick
fence rails and solid pillars down to y~10, 19x19 crossings, blaze
spawner thrones, enclosed 5x5 corridors through netherrack, chest
corridors, nether wart stair rooms on soul sand). Register generated
block entities (spawner kinds, loot chests seeded per position, only if
no saved entry exists) in `World::insert_chunk`. Add the nether wart crop
(4 ages, 3 texture layers, grows on soul sand without light) and item with
it; fortress-bounding-box blaze/wither skeleton spawns later.
Shaped blocks (stairs, fences, gates, ladders, doors), 0.6-block step-up,
sneaking, torch light on mobs and items, the first-person hand.
PR #13 review fixes: reject doors whose upper half exceeds the world height
and reject undefined raw block IDs in `--place`.

## Done (branch t3code/continue-minecraft-rust-port, 2026-10-02)
Fire: flint and steel lights supported empty cells, with wear only on
successful ignition; material-specific consumption/spread in the growth
random-tick dispatch plus 30-39 game-tick scheduling; eternal netherrack
fire; rain/water extinguishing; lava ignition; TNT priming with the normal
fuse; lingering burning on players/mobs, water/rain extinguishing and
zombified-piglin immunity; dropped items burn; animated emissive flames and
the first-person fire overlay. Fire ages are saved in block ids and resume
when chunks load; age changes don't remesh. Portal frames accept existing
fire. Added simulation, lighting, persistence and damage regression tests.
Validated fmt, release clippy with warnings denied, 192 passing tests (one
optional icon-sheet test ignored), and native screenshots of terrain fire,
burning mobs and the player overlay. Headless rd=8 benchmark: 0.847 ms per
dense chunk; 2344 chunks loaded and 1576 meshed in 0.16 s on 9 workers.

## P0 progress (branch t3code/continue-roadmap, 2026-10-02)

First shared-simulation foundation slice:
- [x] Reusable `voxelcraft` library for world/player/entity/inventory/crafting,
  collision, survival and weather code; the desktop executable imports it.
  `--no-default-features` excludes window/GPU/audio/UI dependencies.
- [x] Fixed 20 Hz clock in singleplayer, with <=1/120 s player physics
  substeps, bounded catch-up and offline pause/focus-loss handling.
  Camera, mobs, arrows, smoke, items, falling blocks and TNT interpolate
  positions; mouse look and the hand update every render frame. F3 shows
  completed ticks and pause state. Jump/mining taps survive between ticks.
  Paused screenshots wait for streaming without trying to drain frozen
  simulation queues. Keep the existing ten-minute day for now.
- [x] `World::new_headless` streams and edits without render-mesh jobs;
  meshing scratch buffers allocate lazily. Shared `tick_world`, player
  steps and weather state work without any devices. Added a headless smoke
  example and a Linux CI job without audio/window system headers.
- [x] Regression tests for clock cadence/pause/stalls, rendering without
  mutating simulation positions, frame-rate-independent movement/hunger,
  and headless furnaces, gravity blocks, fluids, fire, TNT, dropped items
  and edit retention after unloading.

Validation: fmt, release clippy with warnings denied for both feature
configurations; 198 passing default tests (one optional icon-sheet test
ignored), 155 passing headless tests. Headless smoke: 200 ticks / 10 game
seconds, 296 chunks, 15 mobs, zero render meshes, about 0.03 ms/tick.
Native screenshot checks of the fixed-tick world and offline pause.
rd=8 meshing benchmark: 0.850 ms/dense chunk, 2344 chunks loaded and 1576
meshed in 0.17 s on 9 workers (Apple M5).

The first P0 extraction checkbox remains open: `Game` still owns session
state, immediate input actions, damage/death, dimension travel, saves and
audio dispatch. Extract that authority and command/event boundary next;
add stable player/entity IDs, then multiple sessions/worlds. The gameplay
block-light prerequisite is now complete in the second slice below. LAN
and split-screen are not playable yet. This smoke example is not a server.

Second foundation slice — authoritative block light and torch-lit growth:
- [x] `World` owns incremental block-light removal/increase propagation;
  lit chunks allocate packed nibble arrays and dark chunks release them.
  Edits, explosions, streaming and shared simulation steps resolve light
  without render jobs. Mesh results cannot overwrite gameplay light.
- [x] Sources, blockers, overlapping lights, chunk unload/reload and world
  height bounds work headlessly. Correct light attenuation through leaves
  and water to one level per step in simulation and rendering.
- [x] Covered crops grow at block light >=9; saplings sample above their
  cell. Preserve the open-sky approximation for skylight. Authoritative
  skylight and shape-aware occlusion remain future work.
- [x] Tests cover propagation/removal, borders, saved emitters, mesh/light
  agreement, crop threshold 8 vs 9, furnace/fire emission, and explosions.
  The headless example also supports `--nether` to exercise lava lighting.

Validation: fmt, release clippy with warnings denied for both feature
configurations; 205 passing default tests (one optional icon-sheet test
ignored), 162 passing headless tests. Overworld and Nether smoke runs:
200 ticks, 296 chunks, no render meshes. Nether streaming took 0.10 s;
its 200 ticks averaged 0.03 ms/tick. Native screenshot checked a covered
farm lit by torches at night. rd=8 benchmark: 0.824 ms/dense chunk, 2344
loaded / 1576 meshed in 0.21 s on 9 workers (Apple M5).


PR #15 follow-up: verified there are no review threads and all CI checks
pass at 9f7898a. Address the review's docstring-coverage warning with
contract documentation for 42 touched functions (clock/survival/weather,
entity creation, world/light streaming, workers and client input/rendering).
No runtime changes. Validate Rustdoc links, fmt, release clippy and tests
in both feature configurations; the pre-existing bare URL in fire.rs is
outside this focused documentation fix.

PR #15 correctness follow-up (ddf5354): verified that parsed NaN food
panicked while clamping saturation, and non-finite saturation/exhaustion
could survive restoration. `Hunger::restore` now resets the entire hunger
entry to fresh-player defaults if any value is non-finite, before clamping.
Finite saved values retain existing bounds and timer reset behavior.
Regression tests reproduce the original panic and cover NaN/+inf/-inf in
each field plus finite preservation/clamping. Updated method documentation.
Validated fmt and release clippy for both feature configurations, 207
passing client tests (one optional icon-sheet test ignored), 164 passing
headless tests. Kept the runtime fix confined to survival.rs.

## Owner-requested agent CLI slice (t3code/agent-gameplay-cli, 2026-10-02)

- [x] Desktop `--agent-listen IP:PORT` embeds a bounded, versioned JSON-lines
  TCP command host; `voxelcraft-agent` builds without client features and
  supports one-shot commands or stdin commands / JSON stdout. Loopback by
  default; LAN binding requires a shared token. Explicit `--agent-cheats`.
- [x] Up to eight independently controlled agent players alongside the human:
  fixed-tick physics, look, bounded timed movement/mining, inventory, survival
  hazards/death drops, flight, placement, mob attack, crafting with alternatives,
  serialized chest transfers, pickup/drop and reconnect by named profile.
  Save profiles with the world; restore inactive. Visible animated avatars.
- [x] Host menus/focus loss continue gameplay; union chunk streaming (agent
  chunks need no render meshes), union fire/random ticking without overlap
  acceleration. Distant edits survive unloading. Timed commands wait real ticks.
- [x] In-game console on /, T or backtick, history, Tab command-name completion,
  help, give/gamemode/tp/setblock/time/weather/dimension/player commands. Offline
  console pauses; hosted console continues. Text consumes gameplay bindings.

Validation: fmt, release clippy with warnings denied in both configurations;
215 passing default tests (one optional icon-sheet test ignored), 171 passing
headless tests. Native two-client integration: simultaneous 20-tick moves in
0.997 seconds, independent inventory, shared chest put/take and no double take,
crafting, 400-block distant streaming, reconnect, structured errors and avatars.
Native console/agent screenshots inspected. Network regression caught and fixed
macOS accepted sockets inheriting nonblocking mode. docs/agents.md describes
launch commands, protocol, permissions and current limits.

This is playable CLI-controlled shared-world building, not finished P0A/P0B.
Agents share and relocate with the host's active dimension. Mob AI/spawning and
hostile arrows still target the human. Door/bed multi-cell actions, food use,
bows, buckets, furnaces, armor controls, sleep and portals need the unified
session action boundary. Desktop joining, independent simultaneous dimensions,
split-screen, discovery, PvP, authenticated internet transport and packaged
standalone authority remain open. Research: Minecraft slash-command interface.

## Inventory search (t3code/agent-gameplay-cli, 2026-10-02)
- [x] Search field in every inventory/container: case-insensitive name words,
  underscores accepted; creative catalog filters and resets scroll, survival/
  container stacks highlight/dim without rearranging slots. Click or Ctrl+F
  focuses, Ctrl+A replaces, Enter leaves text entry, Escape closes inventory.
  Creative focuses automatically when opened interactively. Shared panel
  drawing/hit testing and recipe layouts account for the search field.
- [x] CLI `catalog [query]` returns item IDs, names, underscore command names
  and stack limits using the same matching. `/catalog` works in-game.
- [x] `--inventory-search` supports deterministic screenshot queries.
Validated fmt, release clippy with warnings denied, 215 passing tests (one
optional icon-sheet test ignored); native creative/survival search screenshots.
Recipe search/categories remain open; this completes item-name inventory search.

## End terrain foundation (t3code/agent-gameplay-cli, 2026-10-02)
- [x] Third dimension (`end`), separate `end/` chunk/container/item save, root
  player dimension; /dimension end and --dimension end, return by console.
- [x] Seeded central floating end-stone island, ten ranked-height obsidian
  pillars with bedrock caps, broad void gap, deterministic outer archipelago;
  independent chunk generation, uniform air fast path, no bedrock void floor.
- [x] 5x5 obsidian arrival platform at 100,48,0 with three blocks of clearance;
  safe arrival waits for loaded columns. End stone registry/texture/pickaxe
  harvest. Static violet sky, steady ambient light, no celestial/cloud/weather
  rendering, no accidental Nether/Overworld natural mob spawning or Nether
  cave ambience. Water works in End; only Nether evaporates it. Beds explode.
- [x] New-world reset clears both old dimension folders; regression tests for
  seed repeatability, terrain/void/platform/pillars, and three isolated saves.

Validated fmt and release clippy in both feature configurations, 217 passing
client tests (one optional icon-sheet test ignored), 173 headless tests.
Headless End: 200 ticks, 296 chunks, no meshes/mobs, 0.04 ms/tick; streaming
0.02 seconds. Native central island, outer island and arrival screenshots.
Research: Mojang End Highlands overview (central/outer islands and progression).
This slice is the visiting/terrain foundation. Dragon/crystals/exit portal,
Endermen/pearls, Eyes/strongholds/End portal progression, gateways, chorus,
cities/shulkers/elytra remain open. The full End/boss checkbox stays unchecked.

## Enhanced graphics (t3code/agent-gameplay-cli, 2026-10-02)
- [x] Default Enhanced / selectable Classic graphics, persisted options and
  --graphics enhanced|classic override. Directional sun/moon skylight on
  terrain, mobs, dropped blocks and hand; warm dusk/cool moonlight, overcast
  rain lighting, preserve cave block light and steady skyless dimensions.
- [x] Water surface analytic wave normals, Fresnel sky reflections and
  sun/moon glints, world-anchored phases with camera/time wrap continuity.
  No geometry displacement/greedy-edge gaps; sides and underwater retain
  existing appearance. Sky access suppresses underground reflections.
Validation: WGSL naga validation, 217 passing release tests (one optional
icon sheet ignored), fmt and release clippy with warnings denied. Native
Classic/Enhanced dawn/day pool, night, rain, submerged and Options screenshots.
Single 1600x900 rd=8 sweep: Classic avg 1.03 ms / p50 0.96 ms; Enhanced
1.01 ms / p50 0.93 ms (sample noise; no claim of speedup). Meshing 0.814
ms/dense chunk. Geometry shadows, PBR, bloom and scene/SSR reflections
remain open; current reflection is the analytic sky only.

## HUD FPS counter (t3code/agent-gameplay-cli, 2026-10-02)
- [x] Default-on compact FPS counter at top left, half-second average from
  presented frames, readable above inventory/menu overlays. Persisted
  Options -> FPS Counter toggle. Hide with F1; F3 uses its existing full
  performance readout without duplicating the compact counter.
Validated existing settings round-trip/menu layout tests, 217 default and
173 headless release tests, both release clippy configurations with warnings
denied; native HUD, inventory, Options and F3 screenshots inspected.

## PR #16 review fixes (t3code/agent-gameplay-cli, 2026-10-02)
Verified all five inline findings and both nitpicks against the current code.
- Agent ticks pause during dimension arrival; the engine Agent also waits for
  loaded feet/support before survival, command countdown, mining and pickups,
  snapping previous_pos while suspended. Regression exercises both missing
  feet and missing support, inventory conservation, command resume and damage.
- Transport errors report the actual request/response byte limit. Invalid JSON
  responds then closes, rejecting HTTP preamble/body command injection. Accept
  errors warn/back off/retry; injected failures verify a real command after
  recovery and clean listener shutdown. Existing valid-JSON errors still retry.
- End Stone excluded from item-icon range, all icon indices round-trip.
- Finite derivative-normal fallback in block/hand and matching entity shader.
- Growth enumerates center neighbourhoods instead of all loaded chunks; sort/
  deduplicate preserves tick-once overlap and distant-agent ticking.
Four regressions reproduced before the fix. Validated fmt, release clippy with
warnings denied in both configurations, 223 client / 179 headless tests (one
optional icon-sheet test ignored), WGSL naga validation, native two-agent
integration (concurrent 20-tick moves: 1.016 s) and avatar/hand screenshot.
Baseline PR CI was already green; this patch only addresses verified findings.

## PR #16 Linux transport regression follow-up (2026-10-02)
CI headless Linux caught ConnectionReset before the malformed-HTTP error
response could be read: unread request bytes can reset TCP during close. The
security regression now accepts EOF/reset either before or after the error,
while still rejecting extra responses and checking the game queue stays empty.
A separate malformed JSON test without trailing bytes requires the structured
error followed by EOF. Runtime transport behavior is unchanged. The remaining
arrival/loading thread is already fixed in 1318f8e (Game arrival guard, engine
Agent feet/support guard and regression); no additional gameplay edit needed.
Validated the exact headless CI commands locally: clippy, 180 tests and a
200-tick headless example. Default fmt/clippy/release tests: 224 passing tests
(one optional icon sheet ignored). Remote confirmation: headless Linux and client CI on Linux, macOS and Windows
all passed at 7c757af. Remaining agent thread verified and resolved; no open
review threads remain.

## Multi-player entity targeting (t3code/continue-roadmap-work, 2026-10-02)
- [x] Stable `entity::PlayerId` (host 0; agent profiles persist `id`, legacy or
  clashing IDs reassigned) and `entity::Target`; `Ctx.players` replaces the
  single player position. `PlayerHit` names the player.
- [x] Hostile mobs chase the nearest targetable player; skeleton arrows hit any
  targetable player; Java-style per-player mob caps (mobs within 96 blocks),
  no spawns within 24 of any player, despawn only when all players are far.
- [x] Agents take melee/arrow/explosion damage via `Agent::hurt` (armor, wear,
  knockback, hurt immunity, death drops). Agents excluded during arrival.
Validation: fmt, release clippy with warnings denied in both configurations,
231 default tests (one optional icon sheet ignored), headless smoke run
0.03 ms/tick. Native: creative host + survival agent at night; the zombie
ignored the host and hit the agent (20 -> 8.3 health, knocked off platform).
Still single-player-shaped: mob melee targets the nearest player without
persistent target memory; item pickup is per-player but host pickup still
lives in Game; `Game` session/action extraction remains the next P0 slice.

## Owner priority update (2026-10-02; superseded 2026-10-03 by "Current plan")
Split-screen (P0B) and agent features are the current priority, ahead of
LAN desktop joining (P0A). Plan: (1) split-screen views of agents [done
below]; (2) gamepad local players reusing engine `Agent` as the local
session (join with Start, gilrs input: move/look/jump/sneak/sprint, mine,
place, attack, hotbar bumpers, drop); (3) controller inventory/menus and
per-player pause rules; (4) per-player audio listener mixing.

## Split-screen views (t3code/continue-roadmap-work, 2026-10-02)
- [x] Renderer draws frames as viewports: `begin_frame`/`draw_view`/`end_frame`,
  each view submitted separately so entity/weather/model buffers refill.
  `Viewport::split` (1-4 views, stacked/side for two, quarters for 3-4).
- [x] `/splitscreen <name>|off|side|stacked`, `--split-screen a,b`, `--split-layout`.
  Host keeps view 0 at the window origin (`Game::ui_size` for its HUD/menus).
- [x] Followed agent view: held-item hand, target outline, crack, name tag,
  hotbar/hearts/armor/hunger/air (`hud::HudPlayer`, `bar_ui`), death overlay;
  host and other agents drawn as avatars.
- [x] `World::set_viewers`: followed players load and mesh at full render
  distance (`horizontal_dist2` = nearest view); unwatched agents mesh-free.
- [x] Narrow views slide the hand inward (WIDE_ASPECT 16:9).
Validation: fmt, clippy both configs, 182 lib + 47 bin + 5 integration tests,
native stacked/side/distant (500 blocks) screenshots, rd=8 single-view render
bench unchanged (0.92 ms avg vs 0.92 baseline).
Open: audio follows host only; agent hand has no swing/bob; HUD isn't scaled
down for small views; host menus pause offline play for every view.

## Gamepad local players (t3code/continue-voxelcraft-development, 2026-10-03)
- [x] gilrs (client feature). Start on an unseated pad joins as the first free
  Player2..Player8 profile (saved like agents; new ones copy host mode and
  spawn beside the host) and appends it to `split.follow`. app/gamepad.rs.
- [x] Bedrock layout: sticks move/look (radial 0.15 deadzone, squared look
  curve, 4 rad/s x sensitivity), A jump/double-tap fly/respawn, RT mine+attack,
  LT place or eat, LB/RB hotbar, B drop stack, LS sprint latch, RS sneak toggle,
  hold View 1 s to leave. Unplug freezes the seat; reconnect matches by UUID.
- [x] `Agent::hold(input, mining, using)` per tick keeps mining progress;
  network agents can't claim seated names; `/splitscreen off` keeps pad views.
- [x] Agent eating: `eat` command and held use, 32-tick bites, follower HUD bar.
Validation: fmt, clippy both configs, 183 lib + 54 bin + 5 integration tests;
native launch confirmed gilrs init doesn't disturb startup. NOT tested with a
physical controller yet (no pad on the agent machine) -- owner should try it.
- [x] Controller menus (app/pad_menu.rs): Start = per-player pause (resume /
  leave; world keeps running), Y = inventory+armor, LB/RB crafting tab
  (`Agent::craftable`), LT opens doors/gates (toggle_door takes a facing),
  chests and crafting tables. A/X = left/right click via the inventory cursor,
  Y = quick move. Menus replace the follower hotbar. `--pad-player
  play|pause|inventory|crafting` seats a virtual pad player for screenshots.
- [x] Follower hands animate (Agent::swings, per-Bot HandAnim, eating lift).
- [x] Audio: mixer `Command::Others` (3 extra ears); each positional voice uses
  the loudest ear. Agent `events` (Broke/Placed/Chew) are played by the host.
- [x] Joiners stand on a free spot beside the host; avatars around a camera
  are skipped (shared spots no longer fill the view).
Native: menu/pause/play screenshots via --pad-player; still no physical pad.
- [x] `Game::puppet` swaps a pad player's player/inventory/vitals/hand and a
  per-seat `Body` (actions, cooldowns, held buttons, attack state, spawn bed,
  container) into the host fields; host input became attack_button /
  use_button / act / click_slot, so pads get full mouse parity (buckets, bows,
  beds, flint, hoes, slabs, doors, critical hits, wear). `puppet_used` routes
  containers/beds to pad screens; popups go to the pad view.
- [x] Pad furnace screen, creative palette tab (`--pad-player palette`),
  shift-craft, list scrolling. Sleep needs host + every living pad player;
  Space/A leaves bed while waiting. B drops one, hold for stack.
- [x] Other players' footsteps (Audio::other_steps); Ui::fitted_scale
  (Minecraft auto GUI scale, >=320x180) for host + follower views; world props
  `pads` (uuid=profile) and `pad_beds` restore controller profiles and beds.
Open: ambience/underwater muffling still host-only; agents (CLI) don't count
for sleep; no physical-controller test yet; pad players don't drive portals.

## Shared audio and sleep for every player (t3code/continue-voxelcraft, 2026-10-03)
- [x] Wind/cave/rain loops play at the loudest level any viewed player hears;
  drips play around whichever players are deep in caves; the mix low-pass
  scales with the share of viewed players underwater (`Command::Muffle(f32)`).
- [x] `Agent.sleeping` replaces the pad seat's: CLI `sleep` lies down in the
  targeted bed under the host's rules (`Game::bed_rules`: night/rain, no
  monsters within 8, one sleeper per bed). Acting, walking or damage gets
  up; observe/wait/still moves keep resting. The night needs the host and
  every active, living agent or connected pad player (`sleep_count`).
- [x] `Agent.spawn_bed` replaces the pad seat's bed: saved in the profile
  (`bed`), legacy `pad_beds` props migrate on load, respawn uses it in the
  Overworld and forgets broken beds; observe reports `spawn_bed`.
Validation: fmt, clippy both configs, 185 lib + 64 bin + 5 integration
tests; native two-agent check (target/sleep_check.py): sleep, occupied bed,
night held while others wake, split view fade "1/3 players sleeping".
Open: pad players can't use portals (one active World); no physical
controller test yet.

## Release 0.2.0 prep (t3code/continue-roadmap-work, 2026-10-02)
Version bumped to 0.2.0 with release notes (everything since v0.1.0, which
predates #8). A real v0.1.0-built save (blocks, chest, furnace, inventory)
loads intact; block IDs 6-8 were only renamed (log -> oak log etc.). Packages
now bundle `voxelcraft-agent` (Mac: Contents/MacOS, ad-hoc signed first;
Windows: beside the exe plus a "VoxelCraft (host agent players)" Start Menu
entry with --agent-listen 127.0.0.1:4242). Release run 37074134596 green on
all three. Windows agent exe inherits the game's winresource icon/metadata. Fixed
the clipped inventory search hint. To ship: owner merges PR #17, then push
tag v0.2.0 on main -> draft release; owner reviews and publishes.

## Priorities and research baseline (2026-10-02)

Owner preference (superseded 2026-10-02, see "Owner priority update"):
LAN play on separate computers and same-computer split-screen had **equal priority**. Build their shared foundation first;
keep both local modes ahead of a fully packaged dedicated server and
internet hosting services. This is a planning update, not implemented work.

Compare core mechanics with Java Edition, and label Bedrock-specific
features explicitly. The audit covers major systems and representative
content gaps, not every block, recipe or version-specific rule. Research
uses Mojang/Microsoft documentation and released changelogs; snapshots
are not evidence that a feature shipped. Recheck exact mechanics before
implementing each feature. The latest stable Java release verified in this
audit is [26.3, Wilderness Bound](https://www.minecraft.net/en-us/article/minecraft-java-edition-26-3).

### Existing foundation (do not rebuild as missing features)

Re-inventoried from source on 2026-10-03:
- Content: ~80 block states (stone/dirt/sand families, 5 wood types --
  oak/spruce/birch/jungle/acacia -- with logs/leaves/planks/saplings, coal/
  iron/gold/diamond/quartz ores, glass, wool, bricks, sandstone, terracotta,
  clay, ice/snow, cactus, cane, pumpkin, melon, 5 small plants, torch,
  crafting table, furnace, chest, bed, TNT, oak door/fence/gate, ladder,
  stone slab/stairs, fire, Nether blocks, End stone). 39 plain items plus
  5 tool kinds x 5 tiers (wood..diamond) and 4 armor materials (leather,
  iron, gold, diamond). About 35 crafting recipes.
- Systems: survival/creative, health/hunger/air, mining speeds and harvest
  tiers, durability, crafting + recipe browser + inventory search,
  furnaces, chests, dropped items, farming (wheat, saplings, cane, bone
  meal, torch-lit crops), fluids, fire, TNT, bows/arrows, beds/sleep/respawn
  beds, functional armor, weather (rain/snow), day/night, procedural audio.
- World: 14 Overworld biomes, height 0..255, Perlin caves; Nether terrain
  and portals; End terrain (island, pillars, outer islands) reachable only
  by command. Separate saves per dimension.
- Mobs (9): pig, cow, sheep, chicken, zombie, skeleton, creeper, spider,
  zombified piglin.
- Platform: headless engine, fixed 20 Hz clock, CLI agent players,
  split-screen with gamepads, enhanced graphics, release packaging.
Some systems approximate Minecraft rather than matching every rule.

## Gap audit vs Java 26.3 (2026-10-03)

26.3 "Wilderness Bound" (released 2026-09-15) is still the latest Java
release. Scale of the gap: Java has well over 1,000 block types and items,
about 80 mob types, and roughly 55 Overworld, 5 Nether and 5 End biomes;
VoxelCraft has ~80 block states, 9 mobs and 14 + 1 + 1 biomes. Matching
content one-for-one is a long tail, so prioritise by what survival play
actually needs.

**Survival critical path is broken (highest-value gap).** A survival
player can't finish the game, or progress past diamond gear:
- No Endermen, so no ender pearls; no Nether fortresses or blazes, so no
  blaze rods or powder; therefore no Eyes of Ender.
- No strongholds or End portal frames, so the End (whose terrain already
  exists) is reachable only by `/dimension`. No Ender Dragon, crystals,
  exit portal or credits.
- No XP, enchanting, anvils or brewing; no ancient debris, Netherite or
  smithing.
Suggested order: XP orbs -> Endermen + pearls -> fortresses + blazes ->
brewing -> enchanting/anvils -> strongholds + Eyes of Ender + End portal ->
dragon fight -> Netherite. Each step is playable on its own.

**Gaps found in this audit that the lists below didn't cover** (now added
to P1/P2):
- Materials: copper, lapis, redstone and emerald ores; raw iron/gold
  drops (iron ore currently drops itself, pre-1.17); deepslate; granite/
  diorite/andesite/tuff/calcite; stone bricks, smooth stone, mossy
  cobblestone; dark oak, mangrove, cherry, pale oak and bamboo wood sets;
  per-material slabs/stairs/fences/doors (only stone slab/stairs and oak
  door/fence/gate exist); 16 dye colours for wool/glass/terracotta/concrete.
  Partially addressed 2026-10-07 (see the P1 material-breadth bullet): ores,
  raw metals, deepslate, the stone family and dark oak/mangrove/cherry
  shapes. Dyes and the 16-colour wool/carpet/bed/glass/pane/terracotta/
  concrete/glazed set landed on agent/dyes (items 576-607, blocks through
  762). Still open there: pale oak, bamboo, geodes and the new woods' biomes.
- Everyday items: shears, fishing rod, compass, clock, map, paper/book,
  sugar, eggs, snowballs, milk, bowls/stews, carrots/potatoes, cake/pie;
  chainmail and Netherite tiers; shield, crossbow, trident, mace.
- Player feel: swimming/crawling poses and sprint-swimming, third-person
  (F5), view bobbing, block-breaking/crit/splash/torch particles, background
  music (there is no music at all), chat.
- Rules: per-world difficulty and typed gamerules persist and apply; gaps
  remain (e.g. full Adventure CanDestroy/CanPlaceOn, some multiplayer
  sleep edge cases).
- Commands: host console and agent cheats cover the listed Java staples;
  gaps: full selector/argument grammar, `/time set` sub-minute aliases,
  locate for structures beyond stronghold/fortress.

## P0 — Shared simulation for both local multiplayer modes

Minecraft supports hosted LAN worlds and separate dedicated servers.
LAN lets the host play in the world they are hosting; a standalone server
is not required for that experience. See the official
[Java LAN instructions](https://help.minecraft.net/hc/en-us/articles/4410317081741-Play-Minecraft-Java-Edition-on-a-Local-Area-Network-LAN)
and [Java server download](https://www.minecraft.net/en-us/download/server).
Native split-screen is a Bedrock console feature, supporting up to four
players; adding it to VoxelCraft desktop builds is an intentional extension
of that experience. See the official [edition comparison](https://www.minecraft.net/en-us/about-minecraft).

Current blockers verified in source:

- `src/app/mod.rs`: `Game` owns one player/inventory/vitals and still assembles
  the session in the window loop. Gameplay now advances in fixed 20 Hz steps;
  opening a menu or losing focus pauses offline play.
- `src/world/mod.rs`: streaming has one center. Growth and fire also take
  one player position; overlapping player ranges must not tick twice.
- `src/entity/mod.rs`: AI, spawning/despawning and damage events assume one
  player. There is no remote-player model or stable network entity identity.
- `src/app/dimension.rs`: portal travel replaces the active `World`.
  Players cannot currently occupy Overworld and Nether simultaneously.
- `src/world/storage.rs` and `Game::save`: one player record in `level.txt`;
  no per-player persistent identity/state. Dropped items save; mobs do not.
- Gameplay light queries borrow light computed by mesh workers. A server
  must maintain gameplay lighting without generating render meshes.
- No network transport, host/join UI, gamepad input or multiple camera/HUD
  views exists in the current executable/dependency list.

Proposed implementation, shared by singleplayer, LAN and split-screen:
- [ ] Extract a window/GPU/audio-independent simulation library. Run an
  authoritative fixed 20 Hz game clock, with render interpolation and
  suitable physics substeps. Preserve offline singleplayer pause behavior.
  Fixed ticking, interpolation and the device-free engine library landed
  in the P0 progress slice above; session/action/event extraction remains.
  Java's normal [gameplay tick rate is 20 Hz](https://www.minecraft.net/en-us/article/minecraft-java-edition-1-20-3).
- [ ] (Player IDs, mob targeting, damage assignment and per-player spawn caps
  landed in "Multi-player entity targeting" above.) Add stable player/entity IDs and per-player movement, inventory,
  health/hunger, mode, bed spawn, death state and dimension. Assign damage,
  item pickup, AI targets and action results to the correct player.
- [ ] Maintain worlds per dimension. Track the union of player simulation
  ranges, tick each region once, and track each client's chunk/entity
  subscriptions separately. Separate simulation distance from view distance.
  The [server/client responsibilities guide](https://learn.microsoft.com/en-us/minecraft/creator/documents/simulationrenderdistanceguide?view=minecraft-bedrock-stable)
  describes Minecraft's split between gameplay and rendering.
- [ ] Route local and remote actions through the same command/response
  boundary. The authority validates reach, block edits, combat, crafting,
  tool wear and container transfers. Serialize shared chest/furnace changes
  so simultaneous clicks cannot duplicate or lose items.
- [ ] Save world data once, from the authority; persist each player's state
  separately under stable profile IDs. Migrate existing singleplayer saves
  without losing inventory, beds, dimension or containers. Prevent two
  processes from opening the same save for writing.
- [ ] Give connected players shared time/weather and a multiplayer sleep
  quorum rule. Menus, inventory screens and unfocused client windows must
  not freeze a hosted game. Define host quit/disconnect and save behavior.
- [ ] Address registry evolution before freezing save/network formats:
  `Block(u8)` has only 256 states, and item IDs below 256 alias blocks.
  Introduce versioned state/registry encoding and migration; benchmark
  palettes or wider IDs to preserve compact chunks and fast meshing.

Acceptance: existing singleplayer still works; two players can interact
with the same blocks/containers, move far apart and enter different
worlds without duplicate ticks or state loss. Gameplay runs headlessly
without a window, GPU, audio device or render-mesh jobs.

## P0A — LAN hosting and joining (plan step 4, after parity work)

- [ ] First network slice: one host and one client over loopback, then two
  computers on a home LAN. Use a versioned VoxelCraft protocol with a
  compatibility handshake and bounded messages. Decide transport through
  a prototype; reliable chunk/edit delivery and timely movement updates
  are requirements, not a commitment to a specific networking crate.
- [ ] Replicate initial chunks, ordered block/container edits, player and
  mob state, projectiles, dropped items, damage/death/respawn, time/weather
  and dimension transitions. Add remote player models, names, equipment
  and animations. Interpolate remote motion and reconcile local movement.
- [ ] Add Host/Open to LAN and Multiplayer/Join screens, automatic local
  discovery and direct IP:port fallback. Show host address, player count,
  version mismatch, connection errors and disconnection reasons.
- [ ] Use local profiles for initial offline LAN identity; joining should
  not depend on a cloud service or paid hosting. Provide basic player list,
  chat, host permissions and configurable PvP.
- [ ] Support reconnect to the same profile and resynchronize chunks and
  containers. Host save-and-quit saves all players and cleanly disconnects
  clients. Host menus continue running the shared world.
- [ ] Test separate processes and real macOS/Windows/Linux machines:
  concurrent mining/pickup/container edits, distant streaming, joining
  during fire/fluid/TNT updates, death, portals, reconnect and restart.
  Measure bandwidth, join latency and server tick time under two players.

Scope: VoxelCraft clients connecting to VoxelCraft hosts. Joining vanilla
Minecraft servers, using Mojang's server binaries, Java/Bedrock cross-play
and importing Minecraft saves are separate compatibility projects, not
implied by implementing LAN networking.

## P0B — Same-computer split-screen (paused extra; see Current plan)

- [ ] Use the same authority with two local player sessions; split-screen
  can be built before LAN transport is finished. Start with two players,
  then extend to four after profiling.
- [ ] Add gamepad discovery, bindings, hot-plug/reconnect, dead zones and
  controller navigation for every menu/container. Support keyboard/mouse
  plus one gamepad and two gamepads; persist device-to-player assignment.
- [ ] Add local join/leave and profile selection, horizontal/vertical
  layouts, independent cameras/HUDs/hotbars/death screens and per-player
  inventory/menu focus. One player's menu must not capture the other's
  input; provide an explicit shared pause when no remote clients exist.
- [ ] Share chunk data and GPU meshes, but cull/sort/render for each view.
  Scale UI to each viewport and define positional audio/listener mixing.
  Include first-person hands and visible models for the other player.
- [ ] Test both players building, fighting, sleeping, dying, opening the
  same chest and occupying separate dimensions. Check controller removal,
  resize/fullscreen, small-window readability and viewport screenshots.
  Record frame time and memory for nearby and widely separated players.

## P1 — Standalone VoxelCraft server software

After both local modes are playable, package the shared authority as
`voxelcraft-server` with client graphics/window/audio dependencies excluded
from the server build. The existing headless mesh benchmark is not a
multiplayer server. Minecraft offers separate
[Java](https://www.minecraft.net/en-us/download/server) and
[Bedrock](https://www.minecraft.net/en-us/download/server/bedrock) server
software; use them as product references, not compatible backends.

- [ ] Headless binary and release artifacts for Linux, macOS and Windows;
  world/seed/bind-address/port/player-limit/game-mode/difficulty/PvP/
  simulation-distance/autosave configuration and a documented launch command.
- [ ] Console commands, graceful stop, operator roles, allowlist, kick/ban,
  world backups, save locks and tick/memory/player-count diagnostics.
  See Microsoft's [dedicated-server configuration and console guide](https://learn.microsoft.com/en-us/minecraft/creator/documents/bedrockserver/getting-started?view=minecraft-bedrock-stable).
- [ ] Clients can join, leave and rejoin while the world continues without
  its owner playing. Preserve the same saves/protocol as hosted LAN worlds.
- [ ] Later internet milestone: server browser/favorites, secure persistent
  identities and sessions, connection limits, authenticated administration
  and deployment documentation. Friend invitations/NAT traversal/relay
  hosting need separate service design and should not block local play.

Java 26.2's released notes confirm a
[friends list](https://www.minecraft.net/en-us/article/minecraft-java-edition-26-2).
Peer-to-peer world invitations appear in
[26.2 Snapshot 7](https://www.minecraft.net/en-us/article/minecraft-26-2-snapshot-7);
they were not verified in the stable 26.2/26.3 notes during this audit.
Keep released behavior and experiments distinct when revisiting online play.

## P1 — Core survival progression and building gaps (plan steps 1-2, current focus)

This is the current focus (see Current plan); it no longer waits for
local multiplayer. Keep one focused implementation/commit per feature.

- [x] Torch-lit crops: gameplay block light in `grows_here`, available
  independently of meshing; saplings sample above their cell.
- [ ] **Survival critical path to the End** (see the gap audit): Endermen
  and pearls, fortresses and blazes, Eyes of Ender, strongholds with End
  portal frames, the Ender Dragon/crystals/exit portal and credits. The
  bullets below and the P2 Nether/End items supply the pieces; track this
  as the end-to-end acceptance test (a fresh survival world can be beaten).
- [x] Experience orbs, level curve, mining/mob/smelting awards, pickup and
  death loss (2026-10-03).
- [ ] Enchanting tables/bookshelves/lapis, enchanted gear,
  anvils/repair/renaming, grindstones and smithing/Netherite upgrades.
- [ ] Brewing stands, water bottles, Nether wart/blaze ingredients, timed
  status effects, drinkable/splash/lingering potions and tipped arrows.
  Implement ingredients' acquisition alongside brewing; see Mojang's
  [brewing-stand overview](https://www.minecraft.net/ja-jp/article/taking-inventory--brewing-stand).
- [ ] Glass panes, trapdoors, walls, iron bars, top slabs, upside-down and
  corner stairs, correct door hinges/orientation, waterlogging and
  wall-mounted torches. Expand wood/material variants and recipes.
- [ ] Material breadth: copper/lapis/redstone/emerald ores and raw iron/
  gold drops; stone variants (granite, diorite, andesite, tuff, calcite,
  stone bricks, smooth stone, mossy cobblestone); dark oak, mangrove,
  cherry, pale oak and bamboo wood sets; slabs/stairs/fences/doors for each
  material; dyes and 16-colour wool/glass/terracotta/concrete.
  Progress 2026-10-07: copper, redstone and emerald ore (lapis already
  existed), raw iron/gold/copper and their blocks, copper and emerald
  blocks, deepslate with cobbled/polished forms and deepslate ores, granite/
  diorite/andesite and polished, tuff, calcite and smooth stone. Stairs,
  slabs and walls for those stones plus cobblestone, mossy cobblestone and
  stone bricks. Dark oak, mangrove and cherry logs and planks, and stairs,
  slabs, fences, gates and doors for those plus spruce, birch, jungle and
  acacia. Redstone dust is an inert item. Ore heights are Java 1.21's plus
  64, because the world is still y=0..255; deepslate starts at our y=64.
  Dyes (agent/dyes): 16 Java DyeColor items, vanilla mixing recipes,
  sheep colour/shear/dye, carpets, coloured beds (red stays default),
  stained glass/panes, remaining terracotta, concrete powder/concrete,
  glazed terracotta with Java facings. Survival sources still missing
  cocoa, squid/ink sacs, wither roses and extra flowers. Still open:
  pale oak, bamboo, calcite geodes, a mossy-cobblestone recipe (no vines
  or moss block), and leaves,
  saplings and biomes for dark oak, mangrove and cherry, so those woods are
  creative-and-crafting only. No top slabs or corner stairs. A wood door's
  upper half does not store facing; it copies the lower block.
- [ ] Everyday items: fishing rod, compass, clock, paper/book,
  sugar, eggs, snowballs, milk buckets, bowls/stews, cake and pie;
  chainmail and Netherite tiers; crossbow, trident and mace.
- [ ] Armor on player/mob models; missing armor materials, shields/offhand,
  item-dependent attack timing and remaining Java combat rules. Existing
  armor slots, defense and durability already work.
- [ ] Camera view bobbing; hide full-block faces covered by stairs/slabs;
  third-person camera/player model. Keep the existing hand animation.
- [ ] Movement poses: swimming and crawling (1-block gaps), sprint-swimming,
  and later elytra gliding, sharing one player-pose/hitbox model.
- [ ] Feedback: particles (block breaking, crits, splashes, smoke, torch
  and portal particles, rain splashes) and background music: procedural,
  in keeping with the synthesized sound bank, by dimension, biome and
  situation.
- [ ] Title-screen options, world-list seeds, seed entry/copy, key rebinding,
  difficulty (Peaceful/Easy/Normal/Hard, replacing the hard-coded Normal)
  and gamerules, accessibility/subtitles/UI scale, recipe search
  and richer creative search/categories. Options already exist in-game.

## P2 — Remaining major Minecraft systems

- [ ] **Structures and loot:** villages/dungeons first, then mineshafts,
  temples, shipwrecks/ruins, strongholds, monuments, mansions, outposts,
  ancient cities and trial chambers. Add seeded structure placement,
  structure-aware loot tables and spawners; prevent chunk-order seams.
  v0.6 progress: temples, huts, igloos, outposts, shipwrecks, ocean ruins
  and monuments now generate with loot and residents; maps,
  archaeology and exact layouts remain open (docs/overworld-structures.md).
- [ ] **Villagers and animal life:** persistent animals/mobs, babies,
  feeding/breeding, sheep grazing/wool regrowth, chicken eggs, taming,
  leads and mounts. Villager jobs, trading/restocking, golems, reputation,
  zombie conversion/cures and raids. The existing four farm animals only
  wander/panic; [villages](https://www.minecraft.net/pl-pl/article/village)
  provide a substantially larger gameplay loop.
- [ ] **Mob roster:** after Endermen and blazes (critical path), add by
  survival value: slimes, witches, drowned/husks/strays, phantoms,
  silverfish, cave spiders, bats, squid/fish/dolphins/turtles, wolves/cats/
  foxes/horses/llamas, iron golems, villagers and illagers. Java has about
  80 mob types; VoxelCraft has 9.
- [ ] **Spawning and AI fidelity:** cave/indoor spawning using actual light,
  biome/dimension spawn tables, caps across players, collision/pathfinding,
  line of sight, equipment and persistent/named mobs. Existing spawn rules
  are small per-kind caps near one player and mostly sky-exposed ground.
- [ ] **Redstone and automation:** dust/signal propagation, levers/buttons/
  pressure plates, redstone torches, repeaters/comparators, lamps, pistons/
  sticky pistons, observers, dispensers/droppers, hoppers, powered rails,
  sensors and crafters. Define Java-specific update order and piston rules
  before promising contraption parity. Microsoft's
  [redstone guide](https://learn.microsoft.com/en-us/minecraft/creator/documents/redstoneguide?view=minecraft-bedrock-stable)
  is a component overview, with Bedrock-specific behavior clearly separated.
- [ ] **Nether depth:** additional biomes, fortresses/bastions, ghasts,
  blazes/wither skeletons, piglins/bartering, hoglins/magma cubes/striders,
  Nether wart, ancient debris/Netherite, soul fire and respawn anchors.
  The port already has Nether terrain, portals and zombified piglins.
  See Mojang's [Nether overview](https://www.minecraft.net/en-us/article/around-block--nether-wastes).
- [ ] **End and bosses:** Endermen/pearls and blaze progression, Eyes of
  Ender, strongholds/End portals, End terrain, dragon/crystals/exit portal,
  gateways, End cities/ships, shulkers/boxes, elytra and fireworks; Wither
  summoning, Nether stars and beacons. See Mojang's
  [End progression overview](https://www.minecraft.net/en-us/article/around-block--end-highlands).
- [ ] **Terrain and oceans:** dimension-specific height bounds; modern
  Overworld -64 through 319 (bounds implemented in v0.6), deepslate/ore
  distribution, aquifers, lush/dripstone/deep-dark caves, additional forest/
  mountain/ocean biomes, ocean vegetation and aquatic mobs. v0.6 terrain,
  lush/dripstone caves, aquifers, new biomes and ocean vegetation are in;
  aquatic mobs are in too; deep dark remains open. Keep the
  32-cube chunk design if useful; exact Minecraft internal storage parity
  is not required. See [Caves & Cliffs Part II](https://www.minecraft.net/en-us/article/caves---cliffs--part-ii-out-today-bedrock).
- [ ] **Food, farming and travel:** carrots/potatoes/beetroot, stem crops,
  mushrooms, cocoa, berries, fishing, cooking variety, composters, bees/
  honey; boats/chest boats, minecarts/rails, saddles/riding and swimming
  posture. Existing wheat, saplings, cane and bone meal remain the base.
- [ ] **Weather and survival rules:** thunder/lightning and interactions,
  snow accumulation/melting, difficulty-dependent damage/starvation,
  poison/wither/freezing, spawn protection and multiplayer sleep rules.
- [ ] **Remaining content/UI:** decorative and utility blocks/workstations,
  double chests/barrels, maps/compasses/clocks, signs/books, dyes/banners/
  paintings/item frames, music discs/jukeboxes, advancements/statistics,
  Spectator/Adventure/Hardcore, chat, Java's command set (/kill, /effect,
  /summon, /locate, /seed, /difficulty, /gamerule, tab completion) with
  permissions, skins, resource
  packs and data-driven recipes/loot. These are separate feature groups,
  not prerequisites for the first local multiplayer release.

Mojang's [updates timeline](https://www.minecraft.net/en-us/updates/minecraft-updates-timeline-and-evolution)
is the high-level reference for the biome, aquatic, village, bee and block
families above. This backlog needs feature-by-feature behavior research.

## P3 — Modern content catch-up and optional platform services

Do not treat an older survival feature list as full current-game parity.
Missing released content includes:

- [ ] [Tricky Trials](https://www.minecraft.net/en-us/updates/tricky-trials):
  chambers, breezes/bogged, trial spawners/vaults, maces, wind charges,
  copper/tuff families and automatic crafting.
- [ ] [Garden Awakens and related drops](https://www.minecraft.net/en-us/updates/minecraft-updates-timeline-and-evolution):
  pale garden/pale oak, creaking, resin, bundles, and expanded variants.
- [ ] [Chase the Skies](https://www.minecraft.net/en-us/updates/introducing-chase-the-skies-drop):
  happy ghasts/harnesses and the multiplayer player locator bar.
- [ ] [Copper Age](https://www.minecraft.net/en-us/article/minecraft-java-edition-1-21-9):
  copper tools/armor, copper golems/chests, shelves and copper decorations.
- [ ] [Mounts of Mayhem](https://www.minecraft.net/en-us/updates/mounts-of-mayhem-drop):
  spears, additional mounts and mounted combat.
- [ ] [Chaos Cubed / Java 26.2](https://www.minecraft.net/en-us/article/minecraft-java-edition-26-2):
  sulfur caves/cubes, sulfur/cinnabar block sets and related interactions.
- [ ] [Wilderness Bound / Java 26.3](https://www.minecraft.net/en-us/article/minecraft-java-edition-26-3):
  dappled forests/poplar, abandoned camps, cushions/straw beds, new map and
  wool/concrete shaped-block content.

Optional scope decisions after core multiplayer/progression: hosted worlds
that stay online (a Realms-style service), friend accounts/invitations,
relay infrastructure, mod/plugin APIs and vanilla protocol/save imports.
Do not assume a Minecraft Marketplace/catalog, console/mobile release or
Mojang account integration is required for this Rust desktop game.

## Never tested hands-on
Clicking doors and gates, climbing ladders, sneaking at edges, swing
feel, sounds by ear, title screen clicks, portals.
Flint-and-steel clicks and playing through fire hazards.
Desktop LAN joining, split-screen and the packaged server remain
unimplemented and untested; hosted CLI players have native two-agent
integration coverage. The P0 engine library/fixed-clock slice has automated
and native
screenshot coverage. Validate local modes as their milestones land.
