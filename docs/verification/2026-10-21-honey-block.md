# Honey block and honey bottle — 2026-10-21

[Roadmap](../roadmap.md#release-plan) · [Requirement inventory](../requirements.md) · [ID allocations](../architecture.md#release-id-reservations-v09v017)

Scope: the honey slice of PLY-02c, on detached `d17fb37` with the
integrator's uncommitted Java player movement port retained. This report
covers the uncommitted working tree; it does not establish full PLY-02c parity.
Roadmap and requirement inventory edits are reserved to the integrator.

## Reference

Minecraft Java Edition **26.3**, local official reference artifacts:

- `server-26.3-classes.jar`, SHA-256
  `a362163eec5d1612d520772bc16e5b39c09e3b234fdc045f56bf544284ee8ae6`;
- `client/client-26.3.jar`, SHA-256
  `4508d006323f24fa02876310c192d739af56516eb259000ac50f0909a68c9a2d`.

Read `HoneyBlock`, `Blocks`, `Entity.restituteMovementAfterCollisions` and
`LivingEntity.calculateFallDamage` in the reference source. Decompiled
`Foods`, `Consumables`, `Items`, `HalfTransparentBlock` and `SoundType` with
Vineflower into ignored `target/honey-reference/`. There is no
`HoneyBottleItem.class` in this reference: `Items.HONEY_BOTTLE` combines
food, consumable, craft remainder and use-conversion components.

Read client JAR data:
`recipe/honey_block.json`, `recipe/honey_bottle.json`,
`recipe/sugar_from_honey_bottle.json`, `loot_table/blocks/honey_block.json`
and `tags/block/suppresses_bounce.json` under `data/minecraft/`.

## Delivered behavior

- Honey is a translucent full visual/selection cube with collision
  `[1,0,1]..[15,15,15] / 16`. Matching faces self-cull; light passes through
  (opacity zero). Hardness is zero, no preferred/required tool, and mining
  yields itself.
- `speedFactor=0.4f`, `jumpFactor=0.5f`, default friction `0.6f` and the
  `suppresses_bounce` tag are wired into the Java movement port. The speed
  factor multiplies retained horizontal velocity after movement: steady
  walking displacement is **not** simply normal walking multiplied by 0.4.
- Side contact uses `HoneyBlock.isSlidingDown` and `doSlideMovement`:
  ground, top-height, recovered pre-gravity velocity and width checks;
  horizontal reduction for old Y below `-0.13`; and post-gravity velocity
  `(-0.05 - 0.08) * (double)0.98f` blocks/tick. Sliding resets movement and
  survival fall distance.
- Landing damage uses Java 26.3's
  `floor((fallDistance + 1e-6 - 3) * (double)0.2f)`, shared with hay.
  The block is detected below the supporting feet despite its inset top.
- Landing emits ten honey terrain particles and the slide sound; sliding
  independently rolls 1/5 for five particles and 1/5 for sound. The shared
  movement feedback route covers host, controller and CLI players; particle
  requests carry their feedback through existing LAN serialization too.
  Block break/place/steps, slide and drinking sounds are original procedural
  squelches.
- Honey bottles stack to 16, drink in exactly 40 simulation ticks, restore
  6 food and 1.2 saturation, can be drunk at full hunger, and remove poison
  (including hidden poison instances) while retaining other effects.
  Survival returns a glass bottle to the hand/inventory, dropping overflow;
  Creative retains the honey stack and produces no extra bottle.
- Recipes match the built-in data: shaped 2×2 honey bottles to a honey block;
  shapeless honey block and four glass bottles to four honey bottles;
  one honey bottle to three sugar. Every consumed honey bottle returns a
  glass bottle, including stacked ingredients in the shared desktop,
  controller and LAN crafting screen and CLI crafting. If the result fits
  but remainder bottles do not, both routes drop the bottles.
- Creative catalog/search, CLI `give`/`craft`/`drink`, placement item
  conversion, mining loot, full-cube pick-block and inventory/chunk saves
  share the existing registries/session actions.

## Allocations

- Block state **8128**: honey block; **8129–8143** reserved and unused.
- Item **1072**: honey block placing item; item **1073**: honey bottle.
  The explicit block-item mapping lives in `Item::from_block`/`Item::block`.
- Highest base block texture layer **L=1762** (`SOUL_LANTERN` at `d17fb37`).
  Honey side/top/bottom use **1771–1773**, or L+9..L+11.
  **1774** (L+12) remains reserved and unused. Other agents' L+1..L+8
  are untouched. The bottle uses an original procedural item sprite in
  the separate item icon array.

## Checks

Checks and their final results are recorded below after execution.

Reference-derived cases in `src/player/travel/tests.rs` cover the speed
factor's actual tick placement and terminal walking recurrence, 0.5 jump
impulse and jump height, sustained honey wall sliding and leaving the wall,
slide thresholds/horizontal reduction, reset fall distance, inset collision
and no bounce.

`tests/honey_integrations.rs` covers registration/IDs, light, collision and
full outline, loot/pick-block, 40-tick drinking, hunger/saturation, poison-only
removal, stacked/last/overflow/Creative bottles, all three crafting recipes
and stacked remainders, CLI give/drink/craft, actual movement-to-survival
landing damage/feedback, and honey block plus both inventory items persisted
through two independent writer/reader processes.

## Remaining and differences

- **No survival source until bees and hives.** Bee nests, beehives, bees and
  harvesting are intentionally not introduced in this slice.
- Slime-like piston stickiness is out of scope until **v0.17 pistons**.
- Original procedural textures, bottle sprite and audio replace Mojang art
  and recordings. Slide effect probabilities/counts match; their exact
  random sequence uses VoxelCraft's existing world RNG, not Java's RNG.
- Non-player entities' honey movement/fall behavior and the deferred
  advancement/statistics criteria are not implemented by this player slice.
- Ordinary mouse/controller play, listening acceptance and representative
  frame-time/performance acceptance remain unverified unless recorded below.
  This slice is **Implemented, unverified**, not Verified.

## Files changed in this slice

The existing uncommitted movement port was retained. Honey changes are in:

- Content: `src/world/block.rs`, `src/world/shape.rs`, `src/physics.rs`,
  `src/item.rs`, `src/crafting.rs`, `src/content/consumption.rs`,
  `src/session/consumption.rs`.
- Movement/survival: `src/player/mod.rs`, `src/player/travel.rs`,
  `src/player/travel/honey.rs`, `src/player/travel/tests.rs`,
  `src/simulation/mod.rs`, `src/simulation/survival.rs`.
- Presentation and adapters: `src/render/textures.rs`, `src/audio/sounds.rs`,
  `src/particles/mod.rs`, `src/particles/types.rs`, `src/world/honey.rs`,
  `src/world/mod.rs`, `src/app/particles.rs`, `src/app/lan.rs`,
  `src/app/hands.rs`, `src/app/tick.rs`, `src/app/screen.rs`,
  `src/app/split.rs`, `src/agent.rs`.
- Evidence: `tests/honey_integrations.rs`, `docs/architecture.md` and this report.

## Integration — 2026-10-21

Merged into `t3/e2f55514` with the movement port and wall posts. The
integrator fixed what the delegated run left failing: the sound normaliser
(`audio::dsp::finish`) now clamps to its peak, since `max * (peak / max)`
rounded one ulp past 0.95 once the new honey sounds shifted the bank's
seeds; the CLI test drinks with `eat` (the CLI has no `drink` command); and
the always-edible fixture now expects the registered honey bottle (Java's
`Foods.HONEY_BOTTLE` is `alwaysEdible`). The full chain passes on the merged
tree.
