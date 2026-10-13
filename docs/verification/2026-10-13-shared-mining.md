# Shared mining transactions — 2026-10-13

[Roadmap](../roadmap.md) · [Restructuring](../restructuring.md) · [Reference fixtures](../../tests/fixtures/java/26.3/README.md)

Scope: STR-02/03/05, PLY-04 and BLD-03, based on `bdaf95e` on
`work/v0.7-assessment`. This record accompanies the implementation commit.
Reference: official Java Edition 26.3 server SHA-1
`33680f5f2ac32864d6d7cf5e56a705fdb3e05f4c`. Tests ran on the M5 MacBook Pro.

## Delivered behavior

`PlayerSession` owns fixed-tick mining progress and the validated destruction
transaction. Desktop, controller/LAN hands and CLI mining call this entry point;
adapters retain sound and hand animation. Validation
checks the actor, mode, life/sleep state, hotbar stack, loaded terrain and an
engine-computed raycast before inventory or world mutation. Outcomes identify
the actor. Release/interruption restarts progress; an idle controller rules
tick neither cancels nor advances the hands' clock.

> **Corrected by the [review fixes](2026-10-13-review-mining-fixes.md):** progress
> is keyed on position and held item (not slot or block state), the session owns
> Java's five-tick destroy delay for every adapter, Creative destroys the dragon
> egg without a drop instead of teleporting it, iron doors and Creative double
> plants suppress the lower-half drop, exhaustion requires the correct tool,
> Creative swords cannot break blocks, and Haste is implemented.

The transaction centralizes harvest eligibility, drops/XP, tool wear, exhaustion,
piglin response, tripwire disarming, ice replacement and bed/door partners.
Mining the upper door or bed head removes both halves and drops one item.
Creative can destroy bedrock and destroys a dragon egg without a drop; a
Survival attack on the egg invokes the retained teleport behavior.

Reference-backed corrections include:

- Block targeting uses the default 4.5-block range, with Creative's 0.5-block
  bonus, instead of the previous six-block client/CLI range.
- Destruction progress uses Java's float operation order and accumulation.
  Mining Fatigue uses `0.3^(amplifier + 1)` with the reference double-to-float
  conversion; all 256 amplifier values are checked.
- Destroyed ice becomes water only over a member of the reference support tag
  or an actual liquid, without Silk Touch, in Survival outside the Nether.
  The End's default water-evaporation setting is also checked.

The expanded ice tag is embedded at build time. A bounded, once-initialized
state lookup resolves the existing registry and documented legacy name aliases;
actions perform no JSON parsing or per-cell allocation. Numeric IDs, saved enum
ordering and persistence formats are unchanged.

## Evidence

`tools/extract_java_mining_fixtures.py` verifies the official artifact hash,
expands nested reference tags and inspects the ice, egg, destruction-progress,
player-modifier and interaction-range methods. It reads no Rust output. The
committed fixture reproduced byte-for-byte. Its nine explicit stone/tool/water/
ground scenarios compare exact float bits; the tag check covered **458**
currently registered reference names, plus reviewed exclusions and aliases.

Nine integration tests exercise the common client entry point and real CLI
ticks, target/slot/release interruption, invalid-action atomicity, no-tile-drop
rules, partner removal, ice outcomes, Creative bedrock and egg attacks. The real
two-process persistence test saves interrupted mining without changing the
block or wearing the tool. A new process reloads that state and requires a
complete new mining sequence before destruction and wear.

The required formatting, strict release Clippy and release-test chain passed:
**880 passed, zero failed, three ignored**. Strict headless release Clippy/tests
passed: **798 passed, zero failed, two ignored**. The real desktop LAN harness
passed **13/13** join, item-race, death/reconnect/restart, busy-world, PvP and
portal checks on one Mac. These protocol checks do not prove normal input play
or cross-machine behavior.

## Remaining acceptance

State: **Partial** for STR-03 and PLY-04. Placement, combat and container actions,
other reach checks and removal of `Game::puppet` remain open. The raycast still
needs full reference block-shape fidelity. Complete tool/enchantment metadata
requires further assessment; the retained egg teleport algorithm is not newly
verified. Creative tool restrictions, Haste and destroy-delay timing are
addressed in the [review fixes](2026-10-13-review-mining-fixes.md), which list
their remaining differences.

Normal keyboard/controller, hand/audio and visual acceptance remains unverified.
The isolated native playtest could not be accessed: Computer Use returned
`cgWindowNotFound` by both application path and bundle ID. No native input was
performed, and the owned test process was closed. Representative frame-time
comparisons and full M5 1080p/native acceptance remain open. No version bump,
release or main merge is included.
