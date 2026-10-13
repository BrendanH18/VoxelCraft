# Turtle equipment and potion assessment — 2026-10-12

Scope: PRO-01/02, MOB-03 and PLY-04 equipment/use integration, on
`work/v0.7-assessment` after `b79fd9d`; implementation and this report are
committed together. Reference: Minecraft Java Edition 26.3, official server
SHA-1 `33680f5f2ac32864d6d7cf5e56a705fdb3e05f4c`.

## Implemented behavior

- Five scutes craft a turtle helmet. It has 275 durability, two armor points,
  enchantability nine, helmet enchantments and scute repair. Scute acquisition
  remains the retained v0.7 turtle lifecycle implementation.
- A worn helmet renews 200 ticks of Water Breathing while the player's eyes are
  dry. The effect counts down underwater and does not shorten an existing
  longer Water Breathing effect. Host, controller/LAN player and CLI simulation
  supply the same equipment flag to survival logic.
- Turtle helmets brew awkward bottles into Turtle Master. Redstone extends it;
  glowstone strengthens it. Normal and splash bottles support all three types.
  Both Slowness and Resistance apply, display in tooltips and survive saves.
- Resistance reduces eligible player/mob damage by 20% per level, including
  existing environmental mob damage paths. Player starvation and void damage
  bypass it. Slowness reaches shared walking, swimming, controlled-mount and
  hopping movement. The existing broader damage/AI rules are not declared Java
  parity by this change.
- Java 26.3 splash duration defaults to one. Splash attenuation now measures
  between the potion box and recipient box with the reference age-dependent
  margin and candidate-query bounds. Player width preserves Java's float-to-double
  AABB conversion, which affects duration rounding at exact boundaries. Both Turtle Master effects reach players
  and mobs; existing instant healing/harming and zombie-villager Weakness remain.
- Held-armor equipment validation is shared by desktop/controller and CLI
  adapters, preserving existing usable-block priority.

## Save compatibility

The old 36 normal potion IDs (400..435), splash IDs (436..471), armor material
ordinals and leather overlay layers remain unchanged. New IDs are appended:
helmet 1040, normal Turtle Master variants 1041..1043, splash variants
1044..1046. Potion semantic indices are 36..38; Resistance and Turtle armor
kinds are appended. New effects are optional fields in persistent mob records;
old records load empty effects. Runtime armor sheets grow by two 64×64 RGBA
layers (32 KiB); original leather overlay layers remain 13/14.

## Evidence

`tools/extract_java_turtle_fixtures.py` checks the exact artifact hash and
independently extracts the fixture's recipe, repair tag, material/type
constants, helmet renewal and potion registrations. It checks splash duration
and distance bytecode forms. The reproduced normalized fixture matches the
tracked file. No Java binary or inspection dump is shipped.

The five `tests/turtle_integrations.rs` scenarios cover crafting/consumption,
legacy decoding, brewing all six variants, effect levels/durations, damage,
equipment adapters, breathing expiry and persistent animal/villager/golem
formats. Private splash/physics regressions cover both effects, attenuation,
vertical bounds, hopping Slowness and snow-golem Resistance.
`tests/process_persistence.rs` adds an equipped helmet, new potion ID, partially
elapsed animal effects and interrupted splash Turtle Master brewing to its
separate writer/reader process scenario using real disk storage.

On the M5/macOS assessment machine, the required `cargo fmt && cargo clippy
--release --all-targets -- -D warnings && cargo test --release` chain passed:
**862 passed, zero failed, three ignored**. Strict headless release Clippy and
all headless tests passed: **777 passed, zero failed, two ignored**. Logs are
local ignored `target/release-turtle.log` and `target/headless-turtle.log` in
the configured shared Cargo target directory.

A 1280×720 scripted inventory capture showed the green helmet and normal/splash
bottle icons, with no atlas corruption. The release executable also validated
all three embedded resources from a temporary directory outside the checkout.
A local test bundle opened successfully, but native automation could not bind
its game window: `Computer Use server error -10005: cgWindowNotFound` on both
path and bundle-ID attempts. This is not a completed ordinary-input playtest;
no physical keyboard/controller evidence is claimed.

## Remaining acceptance

State: **Implemented, unverified** for the recorded turtle equipment slice.
Ordinary keyboard/controller play, visual/audio inspection and representative
performance checks remain necessary. This does not establish every potion,
projectile or effect mechanic: other non-instant mob splash effects, exact
projectile collision/motion, hidden lower-priority effect restoration and
broader damage-type tagging still need their own parity assessment. Turtle
breeding/nesting and complete MOB-03 acceptance remain separately tracked.
