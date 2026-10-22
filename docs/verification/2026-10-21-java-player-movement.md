# Java player movement, poses and currents — 2026-10-21

[Roadmap](../roadmap.md#release-plan) · [Requirement inventory](../requirements.md) · [Block shapes](2026-10-14-block-shapes-light-interaction.md) · [Simulation activity](2026-10-14-simulation-activity-save-integrity.md)

Scope: PLY-02a (walking, sprinting, sneaking, jumping, step-up), PLY-02b
(poses and hitboxes) and the fluid, climbing and stuck-block parts of
PLY-02c, on `t3/e2f55514` after `d17fb37`. Reference: Java Edition 26.3,
the server classes (`server-26.3-classes.jar`) and the client jar
(`client-26.3.jar`, SHA-1 `e877b6a0…`, from the 26.3 version manifest) for
the client-only input classes, decompiled with Vineflower 1.11.1 outside the
repository. Classes read:

- **Client input:** `LocalPlayer` (`aiStep`, `applyInput`, `modifyInput`,
  `modifyInputSpeedForSquareMovement`, `canStartSprinting`,
  `shouldStopRunSprinting`, `shouldStopSwimSprinting`,
  `isHorizontalCollisionMinor`, `moveTowardsClosestSpace`),
  `KeyboardInput`, `ClientInput`;
- **Travel:** `LivingEntity` (`aiStep`, `travel`, `travelInAir`,
  `travelInWater`, `travelInLava`, `handleRelativeFrictionAndCalculateMovement`,
  `handleOnClimbable`, `onClimbable`, `getFrictionInfluencedSpeed`,
  `jumpFromGround`, `getEffectiveGravity`, `getFluidFallingAdjustedMovement`),
  `Player` (`travel`, `aiStep`, `updatePlayerPose`, `maybeBackOffFromEdge`,
  `getFlyingSpeed`), `Avatar.POSES`;
- **Movement and collision:** `Entity` (`move`, `collide`,
  `collideWithShapes`, `collectCandidateStepUpHeights`,
  `restituteMovementAfterCollisions`, `checkSupportingBlock`, `getOnPos`,
  `getBlockSpeedFactor`, `getBlockJumpFactor`, `baseTick`,
  `updateSwimming`, `makeStuckInBlock`), `Direction.axisStepOrder`,
  `VoxelShape.collideX`;
- **Fluids:** `EntityFluidInteraction` and its current accumulator,
  `FlowingFluid` (`getFlow`, `getHeight`, `getOwnHeight`, `isSolidFace`),
  `FluidState.getHeightForCamera`;
- **Blocks:** `Blocks` (friction, speed and jump factors, bounce
  restitution), `WebBlock`, `SweetBerryBushBlock`, and the `climbable`,
  `blocks_fluid_flow`, `fall_damage_resetting` and `suppresses_bounce` tags.

## Delivered behavior

- [`player/travel.rs`](../../src/player/travel.rs) replaces the old
  per-frame smoothing (walk 4.4 blocks/s, a "grip" of 18, jump 8.6) with
  one Java tick per 20 Hz step, `Player::tick`. Velocity stays in blocks per
  second at the API (`deltaMovement` times 20); Java's float constants are
  widened where Java widens them. The order follows `LocalPlayer.tick`:
  fluid contact and currents, swimming, crouching, input, sprinting,
  sinking and creative flight, small-motion cut-offs, jumping, travel,
  stuck blocks, then `updatePlayerPose`.
- **Input:** `KeyboardInput` unit vectors, the 0.98 key scale, the 0.2 item
  use factor (eating, drinking, drawing a bow or crossbow) and the 0.3
  sneaking factor while crouching or crawling, then Java's square-input
  correction (diagonals reach full length).
- **Sprinting** is now Java's latched flag (`Player::sprinting`): the
  sprint key or a double tap of forward within 7 ticks starts it; releasing
  forward, hunger at 6 or less, blindness, a head-on wall, sneaking or
  item use stop it, and sprint-swimming has its own stop rule. Attacks,
  exhaustion and the field of view read the flag. Sprint knockback clears
  it for the host as well as agents.
- **Ground and air:** block friction (ice and packed ice 0.98, blue ice
  0.989), `0.216 / friction³` acceleration, 0.91 air drag, 0.98 vertical
  drag, 0.08 gravity (Slow Falling caps a fall's gravity at 0.01), 0.42
  jump with Jump Boost, the 0.2 sprint-jump push, the 10-tick hold-jump
  delay, soul sand's 0.4 speed factor through the supporting block.
- **Collision:** Java's per-axis sweep (Y, then the larger of X and Z) with
  its `1.0E-7` tolerance against block shapes, step-up to 0.6 by Java's
  candidate heights, the sneaking edge back-off, the supporting block for
  friction (`getBlockPosBelowThatAffectsMyMovement`, so a slab on ice is
  slippery), collided axes stopping, minor collisions keeping a sprint, beds
  bouncing at 0.75 unless sneaking, and the push out of suffocating blocks.
- **Climbing:** ladders, vines and the other climbable blocks, open
  trapdoors over a matching ladder; 0.15 sideways and falling caps,
  climbing at 0.2 when pushing into a wall or holding jump, and sneaking
  holding on.
- **Fluids** ([`player/fluid.rs`](../../src/player/fluid.rs)): Java fluid
  amounts and heights (VoxelCraft's three lava levels are Java's 2, 4 and 6),
  eyes-in-water with the camera height, water and lava travel (0.8 and 0.5
  drag, Depth Strider's efficiency, Dolphin's Grace, the fall adjustment,
  jumping out at a wall), sinking with sneak, and **water currents**: the
  averaged `getFlow` of the touched cells, 0.014 a tick (they did not
  push the player before).
- **Stuck blocks:** cobwebs (0.25, 0.05) and sweet berry bushes (0.8,
  0.75), which also reset falls.
- **Poses (PLY-02b):** `Avatar.POSES` sizes and eye heights as Java's
  floats, `canPlayerFitWithinBlocksAndEntitiesWhen` with its `1.0E-7`
  deflation, crouching forced under 1.5 blocks without the sneak key (and
  `LocalPlayer.crouching` following a tick later), crawling in 1-block
  gaps at the sneaking speed, swimming when sprinting under water.
- The field of view follows `getFieldOfViewModifier` (speed attribute and
  flight), smoothed by `GameRenderer.tickFov` and interpolated per frame.
- Every input path calls the same tick: the desktop host, controller seats
  and CLI agents (through `Agent::tick`), LAN client prediction and its
  replays, and `simulation::tick_player`.

## Checks

`src/player/travel/tests.rs` (fixtures derived from the Java code above,
plus the published speeds they produce):

| Case | Expected (Java 26.3) | Result |
|---|---|---|
| First and second walking ticks | `0.1 × 0.216/0.6³ × 0.98`, then `× (1 + 0.546)` | Pass, 1e-9 |
| Top walking, sprinting, sneaking, crawling, 1.5-gap crouching speed | 4.317, 5.612, 1.295, 1.295, 1.295 m/s | Pass, 0.001 |
| Standing jump | 0.42, 0.7532, 1.00134, 1.16611, 1.24919, 1.25220; lands on tick 12 | Pass, 1e-9 |
| Held jump | at least 10 ticks between take-offs | Pass |
| Sprint latch, double tap, hunger stop | latched after key release; tap inside 7 ticks; `sprint_blocked` stops | Pass |
| Square input | 0.98 straight, 1.0 diagonal; ×0.3 sneaking, ×0.2 item use | Pass |
| Slab step-up, full block | steps 0.5 in one move; stops flush at `2 − 0.3f` | Pass |
| Sneaking on a 1×1 pillar | stays on, overhangs to just under 1.3 | Pass |
| Ladder | climbs 0.1176/tick (2.35 m/s), slides 0.15, sneak holds | Pass |
| Soul sand | sinks to 14/16; `a / (1 − 0.4 × 0.546)` per tick | Pass, 1e-9 |
| Ice glide | 35–40 ticks to stop from walking speed; stone ≤ 8 | Pass |
| Cobweb fall | more than 40 ticks to fall 4 blocks through one web | Pass |
| Still water | sinks 0.025/tick; holding jump rises 0.175/tick | Pass |
| Flowing water | carries a standing player downstream | Pass |
| Bed | bounces a falling player; never while sneaking | Pass |
| Creative flight | 0.375/tick up, `0.049 / 0.09` level | Pass |
| Sprint-swimming | swims under water with the 0.6 pose, stands when forward is released | Pass |

Existing tests updated to the Java values: depth strider efficiency
(`agent.rs`), the frame-rate fixture in `tests/headless_simulation.rs`
(4.9 blocks in a second's sprint from rest, motion kept after friction),
the bed stand-up fixture (the sleeper now rests exactly on the bed), and
sprint-dependent combat tests (the flag latches on a movement tick).

The full chain (`cargo fmt`, `clippy -D warnings`, `cargo test --release`),
the `--no-default-features` clippy and tests, and the headless example pass.

## Differences and remaining work

- Java's `Mth.sin` uses a 65,536-entry table; VoxelCraft uses `f32` sine,
  so directions differ in the last bits.
- Entity collisions (boats, shulkers, minecarts) and the world border are not
  in the player's collision; `applyEffectsFromBlocks` checks the box after
  the move, not Java's swept path.
- Nether lava currents use the Overworld scale (0.0023); Java's `FAST_LAVA`
  dimensions push at 0.007.
- Creative flight still toggles with the desktop's 280 ms double tap (or F)
  and the controller's button, not Java's 7-tick `jumpTriggerTime`; auto-jump
  is not implemented.
- Fall damage is still computed by the vitals from height; the player's
  Java `fallDistance` only drives the sneaking back-off.
- Elytra, riptide, levitation, Soul Speed, Swift Sneak, bubble columns and
  slime blocks arrive with their blocks and items. Honey, scaffolding and
  powder snow movement shipped in v0.9 and are recorded in their own reports
  ([honey](2026-10-21-honey-block.md),
  [scaffolding and powder snow](2026-10-21-scaffolding-powder-snow.md)).
- LAN clients receive the host's latched sprint and carried movement state
  (jump delay, sprint double-tap window, last tick's keys) with each
  snapshot, and predict item use from the use button and the held item;
  the rest of the client's prediction state is recomputed each tick.
- Review fixes (2026-10-22, Codex): lava halves the vitals' fall distance
  before each tick's move, landing included (`Entity.baseTick`); a hit with extra knockback clears the sprint on
  every input path; eating, drinking and drawing slow controller and LAN
  players too (`PlayerSession::using_item`); the shared headless step
  honours hunger and blindness; host footsteps count distance walked, since
  velocity is now kept after friction.
- Not yet played by hand: feel on keyboard and mouse, a physical controller
  and a LAN client, and the frame-pacing check at 120 and 240 Hz.
