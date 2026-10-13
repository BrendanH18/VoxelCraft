# Shared block placement — 2026-10-13

[Roadmap](../roadmap.md) · [Restructuring](../restructuring.md) · [Reference fixtures](../../tests/fixtures/java/26.3/README.md)

Scope: STR-02/03/05, PLY-04 and BLD-02/03, based on `7b049db` on
`work/v0.7-assessment`. This record accompanies the implementation commit.
Reference: official Java Edition 26.3 server SHA-1
`33680f5f2ac32864d6d7cf5e56a705fdb3e05f4c`. Checks ran on the M5 MacBook Pro.

## Delivered behavior

`PlayerSession::place_held` owns block placement from a selected hotbar stack.
Desktop, controller/LAN hands and CLI adapters call the same transaction;
adapters retain other item uses, sounds, hand animation and repeat delays.
The transaction checks actor state, mode, loaded terrain, finite view/player
coordinates, held-stack validity, engine-computed reach, support, destination
and player intersections before editing or spending inventory. Outcomes carry
the actor identity. Existing world gamerules remain authoritative.

The shared path covers ordinary blocks, all 13 existing door families, all
16 bed colors, ladders, supported plants and the retained bottom-slab
combination. Paired beds, doors and tall plants validate their partner cells
before the first edit. Survival spends one item after successful placement;
Creative retains it. Rejected placement does not advance vine-age randomness.
Nether water evaporation produces an explicit outcome without a block edit or
item spending, retaining the existing block-item policy.

Horizontal placement now follows yaw even when looking straight up/down.
Raycasting respects the existing low-cube top offset rather than targeting a
bottom slab's empty upper half. Axis-parallel rays on integer boundaries handle
zero and negative-zero components without a NaN boundary distance. Invalid or
zero-length rays are rejected. Saved identifiers and formats are unchanged.

## Evidence

The official reference's `SlabBlock` defines its bottom shape from Y=0 to 8
in sixteenth-block units. `AbstractBedBlock` and `DoorBlock` use the placement
context's horizontal direction. These bytecode facts informed the scoped
geometry/yaw corrections; they do not establish complete state/support parity.

Nine integration tests compare shared execution and real CLI actions for
door families, ladders, colored beds, ordinary/new building materials,
orientation, plant replacement and slab combination. They check rejection
without partial edits or spending, player/partner obstruction, Creative,
Nether water, invalid stack/slot/view, vine randomness and vertical-view bed
direction. Direct ray cases check low-cube hit position, reach and parallel
boundary conditions. A two-process test places a bamboo door and cyan bed,
saves actual chunk/inventory formats, and reloads both partners and spent
stacks in a new process.

The required formatting, strict release Clippy and release tests passed:
**905 passed, zero failed, three ignored**. The device-independent
configuration (`--no-default-features`) passed strict Clippy and **823 tests,
zero failed, two ignored**. The real desktop LAN harness (`tools/lan_check.py`)
passed **13/13** checks on the final source, including block/chest races,
death/respawn, reconnect, host restart, PvP and portal following.

## Remaining acceptance

State: **Partial** for STR-03, PLY-04 and BLD-02/03. This extracts block
placement, not the complete item-use transaction. Other item-use precedence,
cooldowns, combat, container/workstation actions and `Game::puppet` removal
remain open. Adapter preambles can still trigger other interactions before
the placement transaction. Full shape targeting, top/double slab states,
waterlogging, door hinges/facing and Java support/replacement rules remain
unverified or incomplete; retained behavior is not an approved fidelity
exception.

Ordinary keyboard/controller, hand/audio/visual and cross-machine LAN
acceptance remains unverified. The earlier native-input attempt returned
`cgWindowNotFound`; no native input was performed. No new placement frame-time
comparison was run. The separate [M5 baseline](2026-10-13-m5-baseline.md)
predates this slice and fails the 240 FPS gate. No version bump, release,
main merge or completion claim is included.
