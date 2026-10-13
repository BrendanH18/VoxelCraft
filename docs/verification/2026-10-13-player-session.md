# Shared player sessions and consumption — 2026-10-13

[Roadmap](../roadmap.md) · [Restructuring](../restructuring.md) · [Reference fixtures](../../tests/fixtures/java/26.3/README.md)

Scope: STR-02/03/05, PLY-03/04 and FOO-01, after `e237f6b` on
`work/v0.7-assessment`. Implementation and this record are committed together.
Reference: official Minecraft Java Edition 26.3 server SHA-1
`33680f5f2ac32864d6d7cf5e56a705fdb3e05f4c`. Platform: Apple M5 MacBook Pro,
16 GB, macOS; release configurations.

## Ownership and delivered behavior

The device-independent `PlayerSession` owns player identity, movement, inventory,
vitals, mode, the three workstation input slots, sleeping and personal bed/command
spawn data. Both desktop `Game` and hosted/headless `Agent` use it. Camera,
screen navigation, sounds and hand animation stay in adapters. Existing saved
keys, stack IDs and enum ordering are unchanged. `Agent` retains field access
through dereferencing; callers borrowing multiple fields together should borrow
`agent.session` explicitly.

The transitional controller/LAN bridge swaps the entire session. Previously it
swapped beds but left the host's command-set spawn point behind; a controller
respawn could consequently use the host's point. Personal spawn selection now
uses the current actor's session through the same helper in client and CLI.
Existing world-spawn fallbacks remain in adapters; this is not a claim of full
Java safe-standing-position or spawn-radius parity.

Food/drink validation, fixed-tick progress, inventory exchange, hunger and
effects now run through `PlayerSession::tick_consumption`. Requests validate
mode, life/sleep state, hotbar range, held stack and hunger before mutation or
RNG use. Completion returns the actor identity and magic damage, which adapters
deliver through their existing damage/death routes after returning the bottle.
The old client timer and CLI bite count are presentation mirrors.

Two correctness changes accompany the extraction:

- Dried kelp takes **16 ticks / 0.8 seconds**, rather than the old shared
  32-tick food duration. Other currently registered food/drinks take 32 ticks.
- A CLI player killed by a harming potion now follows death/drop rules after
  the bottle exchange and cannot collect items or XP later in that same tick.

Release/use interruption, changed held items, mode changes, death, menus and
controller disconnects reset the applicable clock. An idle agent-rules tick
after controller/LAN hands does not cancel or advance the hands' consumption.
Gameplay still advances at the existing fixed 20 Hz; adapters do not submit
arbitrary consumption durations.

## Independent expectations and regression evidence

`tools/extract_java_consumption_fixtures.py` hashes the official artifact and
inspects `Consumables`, `Item$Properties.food` and `Items` associations. It reads
no Rust implementation. Representative apple/stew, dried-kelp, milk and potion
durations reproduce `consumption.json` byte-for-byte; a different artifact is
rejected. The fixture establishes duration facts, not every food mechanic.

Eight integration tests cover reference completion timing through the shared
entry point and real CLI ticks; replaced/interrupted food; held-use release;
CLI waiting; controller hands followed by idle agent rules; invalid requests
without inventory/effect/clock/RNG changes; distinct actors, modes, milk and
Turtle Master remainders/effects; harming-potion death; and personal respawn data.
The process-restart test saves an interrupted 31-tick apple use to the real
storage format. A new process reloads the unchanged stack and needs a full new
bite. Existing furnace/brewing/entity restart checks remain in that test.

The required formatting, strict release Clippy and release-test chain passed:
**873 passed, zero failed, three ignored**. Strict headless release Clippy/tests
passed: **789 passed, zero failed, two ignored**. The headless Overworld smoke
completed 200 ticks with 444 loaded chunks, 20 mobs and no render meshes.

The initial real desktop LAN run failed because clients could join before the
asynchronous scripted platform was placed, fall to terrain and die before the
chest scenario. The harness now honors `CARGO_TARGET_DIR` and waits for the new
run's logged placement-readiness marker, rejecting placement failures and early
host exits. Its gameplay checks are unchanged. The rerun passed **13/13 checks**:
joining, chest/block item races, death/respawn, reconnect, host restart, busy
fluid/fire/TNT joining, PvP on/off and host-controlled portal following. This
used a real desktop host and protocol clients on one Mac. The failed attempt
and rerun remain local in ignored logs; no cross-machine result is implied.

## Remaining acceptance and scope

State: **Partial** for STR-03 and PLY-03/04. Mining, placement, combat and
workstation/container transactions still need shared execution; `Game::puppet`
still swaps session, actions and presentation fields. Its removal is unfinished.
Crafting-screen state also remains outside the core session.

This extraction retains existing food/effect policies except the documented
timing and death fixes. Always-edible food rules, complete food side effects
(including all pufferfish effects), Creative food use and unregistered reference
consumables still require separate fidelity work. It does not mark FOO-01 complete.

Physical keyboard/controller interaction, hand/audio review, cross-machine LAN,
representative frame-time comparisons and full M5 1080p/native performance
acceptance remain open. Automated adapter/rule tests and a protocol harness do
not replace ordinary player acceptance. No version bump, release, main merge or
discard of the preserved v0.7 spawning patch is part of this change.
