# Shared mining review fixes — 2026-10-13

[Roadmap](../roadmap.md) · [Restructuring](../restructuring.md) · [Shared mining record](2026-10-13-shared-mining.md)

Scope: STR-03 and PLY-04 corrections to the shared mining transaction
(`src/session/mining.rs`), based on `ab568fc`. Reference: official Java Edition
26.3 server, SHA-1 `33680f5f2ac32864d6d7cf5e56a705fdb3e05f4c`, inspected with
`javap -c -p`. No jar or decompiled source is committed. Tests ran on the M5
MacBook Pro.

## Findings and evidence

Each finding was checked against server bytecode before it was changed.

1. **Iron door drop without a pickaxe (fixed).** `DoorBlock.playerWillDestroy`
   calls `DoublePlantBlock.preventDropFromBottomPart` when
   `Player.preventsBlockDrops()` (`abilities.instabuild`) is true or
   `Player.hasCorrectToolForDrops(state)` is false. That method removes the
   lower half with flags 35 (no drop) when an upper half is broken. The
   transaction now removes the lower half silently in these cases. If the
   correct tool is used, the lower half still drops one door. The registry has
   no copper doors. Beds have no tool requirement:
   `AbstractBedBlock.playerWillDestroy` suppresses the head drop only for a
   Creative foot break, which the existing partner rule already handled.
2. **Creative dragon egg (fixed).** The instabuild branch of
   `ServerPlayerGameMode.handleBlockBreakAction` calls `destroyAndAck` and
   returns before `BlockState.attack`. `DragonEggBlock.attack` is the only
   teleport call. Creative therefore destroys the egg without a drop. Survival
   attacks the egg when breaking starts. If teleporting fails, the egg remains
   and can be mined normally.
3. **Mining exhaustion (fixed).** `ServerPlayerGameMode.destroyBlock` calls
   `ItemStack.mineBlock`, so tool wear does not depend on the tool being
   correct. It calls `Block.playerDestroy` only when the block was removed and
   `hasCorrectToolForDrops` is true. `playerDestroy` applies
   `causeFoodExhaustion(0.005f)` before `dropResources`. Exhaustion now requires
   the correct tool and is independent of `doTileDrops`.
4. **Creative double plants (fixed).** `DoublePlantBlock.playerWillDestroy`
   calls `preventDropFromBottomPart` when `preventsBlockDrops()` is true.
   Breaking an upper sunflower, lilac, rose bush, peony, large fern or tall grass
   in Creative now removes the lower half without a drop. Survival behavior is
   unchanged.
5. **Destroy delay and timing (fixed).** The server jar does not contain
   `MultiPlayerGameMode`, and `ServerPlayerGameMode` has no `destroyDelay`
   field. The server does not enforce this delay; it uses its own
   elapsed-tick/0.7 tolerance check. The values used here come from the vanilla
   client logic in `MultiPlayerGameMode`/`Minecraft.startAttack` and
   `Minecraft.continueAttack`, which is not in this artifact:
   - `startDestroyBlock` destroys immediately in Creative and sets
     `destroyDelay = 5`.
   - In Survival, `startDestroyBlock` destroys an instant block (progress ≥ 1)
     without a delay, or starts timed progress at zero.
   - Each held tick runs `continueDestroyBlock`. It first decrements a pending
     delay. Otherwise, Creative destroys and sets the delay to 5. Survival adds
     progress for the same target and sets the delay to 5 when progress reaches
     1. A changed target restarts without adding progress in that tick.
   - A click runs start and continue in the same tick.

   The session now owns this state in ticks
   (`DESTROY_DELAY_TICKS = 5`). The CLI's 0.15 s cooldown and its gating by the
   shared action cooldown were removed. The desktop Creative `ACTION_REPEAT`
   path and Survival `BREAK_DELAY` were also removed. Desktop presses in every
   mode are processed on the next fixed tick, so controller hands, which
   use the desktop path through `Game::puppet`, share the same timing. Placement keeps
   its existing repeat. Expected destroy ticks, where tick 0 is the press:
   Creative row at 0, 5 and 11; Survival instant blocks at 0, 1 and 2; iron
   pickaxe on stone at 7, 21 and 35; stone followed by two TNT blocks at 7, 13
   and 14.
6. **Progress key (fixed).** In Java, `sameDestroyTarget` compares the position
   and uses `ItemStack.isSameItemSameComponents`. The key is now
   `(position, held stack ignoring count)`. Progress survives block-state
   changes at the same position, such as a furnace lighting, and continues at
   the new state's rate. An identical stack in another hotbar slot keeps
   progress. Another item, or the same slot after an enchantment change,
   restarts. Cracks disappear when the cell becomes air. Desktop hotbar
   selection and CLI `select` no longer cancel progress directly.
7. **Creative tool restriction and Haste (fixed).**
   - **Creative tools:** `ServerPlayerGameMode.destroyBlock` first checks
     `ItemStack.canDestroyBlock` → `Item.canDestroyBlock`. That fails when the
     item's `Tool` component has `canDestroyBlocksInCreative` false and the
     player has instabuild. `ToolMaterial.applySwordProperties` creates
     `Tool(..., 2, false)`, while `applyToolProperties` uses `(1, true)`.
     `TridentItem`/`MaceItem.createToolProperties` also use false, but those
     items are not registered. `Item.Properties.spear` adds no tool component,
     so spears can break blocks. Swords are rejected in Creative before any
     mutation.
   - **Haste:** Effects are saved and synchronized by their Java string ID
     (`Effects::serialize`/`deserialize`), not by enum ordinal. Icon layers are
     derived from `Effect::ALL`, so appending `Effect::Haste` is save-safe;
     older builds skip the unknown ID. `Player.getDestroySpeed` multiplies by
     `1.0f + (float)(amp + 1) * 0.2f` after efficiency and before Mining
     Fatigue. `MobEffectUtil.getDigSpeedAmplification` takes the maximum of
     Haste and Conduit Power. Conduit Power is not registered. Haste is
     available through `/effect`.
8. **Tests (done; desktop limitation recorded).** The desktop `Game` requires a
   window and GPU and has no headless constructor, so no automated test drives
   `Game::act`. CLI and session timing are tested against the Java tick counts
   listed above. The vine-age test now includes a late rejection: slot, stack,
   target and destination pass, and support fails with "block cannot survive
   here". The stack is unchanged, the RNG does not advance, and the later valid
   placement has the same age as the control.

## Tests

New or rewritten in `tests/shared_mining.rs`:

- `held_attack_timing_matches_java_destroy_delay_through_session_and_cli`
- `iron_doors_drop_only_with_a_pickaxe_from_either_half_and_never_in_creative`
- `creative_upper_double_plant_removes_the_lower_half_without_a_drop`
- `mining_exhaustion_needs_the_correct_tool_but_wear_does_not`
- `creative_swords_cannot_break_blocks_but_other_items_can`, including
  Survival sword wear of 2
- `haste_multiplies_dig_speed_before_fatigue_in_reference_float_order_and_persists`,
  which covers all 256 amplifiers bit-for-bit, persistence, and Haste II
  reducing iron-pickaxe stone from 8 ticks to 6
- `creative_destroys_bedrock_and_the_dragon_egg_while_a_survival_attack_teleports_it`,
  which replaces the old egg test
- `progress_follows_position_and_held_item_not_slot_or_block_state`, which
  replaces the old slot/target test

Updated unit tests in `src/agent.rs`:

- `replacing_a_targeted_block_keeps_progress_at_the_new_rate_until_it_is_air`
- `mining_speed_changes_only_affect_future_progress`: an enchantment change now
  restarts progress

Strengthened: `tests/shared_placement.rs`
`vine_age_comes_from_shared_world_rng_and_failed_requests_do_not_advance_it`.

Totals:

- `cargo fmt && cargo clippy --release --all-targets -- -D warnings && cargo test --release`:
  **918 passed, 0 failed, 3 ignored**.
- `cargo clippy --release --no-default-features --all-targets -- -D warnings && cargo test --release --no-default-features`:
  **836 passed, 0 failed, 2 ignored**.

The existing two-process persistence test still saves interrupted mining and
requires a full new sequence after reload. The destroy delay is transient, like
progress. Save formats and block, item and state IDs are unchanged. The effect
enum was appended, and effects persist by string ID.

## Remaining differences

- **Input paths:**
  - Normal keyboard, mouse and controller play, hand swing and audio were not
    checked by hand.
  - No automated test drives the desktop `Game` adapter.
- **Releases and aim:**
  - Implemented with automated evidence: adapter errors preserve held input and
    progress; only an explicit release resets them.
  - Implemented with automated evidence: re-aiming at a different block while
    held starts at zero, including a first press that missed; re-aiming at the
    same block continues. CLI look no longer synthesizes a release.
  - Implemented with automated evidence: pick-block, dropping from the selected
    slot and mode changes no longer release mining; progress is keyed on the
    position and the held item and components, so a changed stack restarts and
    a partly dropped stack continues. Opening a menu and dying still release
    mining, which matches Java's key release when a screen opens.
- **Server rules:**
  - The server's tolerance check is not modeled. This includes
    `STOP_DESTROY_BLOCK` requiring ≥ 0.7 and delayed destruction.
  - Single-player accumulation follows the client.
  - Spawn protection, `mayInteract`, `blockActionRestricted` and game-master
    blocks are not applicable or not registered.
- **Effects:**
  - Haste's attack-speed attribute modifier (+10% per level) is not applied.
    Mining Fatigue's modifier is not applied either.
  - Conduit Power is not registered.
  - No beacon is available to grant Haste.
- **Block and tool details:**
  - Implemented with automated evidence: lit redstone ore is no longer
    rescheduled by each touch and no longer expires after a fixed 30 ticks; it
    goes dark on a random tick (`RedStoneOreBlock.randomTick`), pinned in
    `tests/fixtures/java/26.3/building.json`.
  - Neighbor drops are suppressed while an upper half is removed silently.
    Java suppresses only the lower half. No registered block is affected.
  - Survival double-plant loot with shears and tool context in neighbor
    destruction are unchanged and not newly verified.
  - Full `Tool` component rules, such as per-block speeds and correct-tool
    tags, still use the engine's tool tables.
- **Not yet verified:** the retained egg teleport algorithm and full raycast
  shape fidelity.

## Proposed roadmap updates

Proposed only; `docs/roadmap.md` is not edited here:

- STR-03/PLY-04 mining: note Java destroy-delay timing shared through the
  session for every adapter, position-and-item progress keys, Creative sword
  restriction, Creative egg destruction, door and double-plant lower-drop
  suppression, correct-tool exhaustion and Haste. Keep the requirement
  **Partial** until manual input, audio and visual acceptance is complete.
- Add gaps: effect attack-speed modifiers for Haste and Mining Fatigue, Conduit
  Power and beacons, server-side destroy tolerance, a headless desktop-adapter
  test harness, and remaining cancel-on-item-change sites in desktop pick-block
  and drop flows.
