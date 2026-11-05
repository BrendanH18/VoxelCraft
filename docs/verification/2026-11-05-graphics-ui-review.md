# Graphics and UI review — 2026-11-05

[Roadmap scope](../roadmap.md#v020-scope-presentation-and-performance) · [Requirements](../requirements.md#ux--presentation-usability-and-distribution) · [Documentation index](../README.md)

## Scope and evidence

GPT-6 Astra reviewed the project documentation, rendering/UI source and fresh
scripted captures from `057798f582b098d5bb918b7e614f269cc2780524` on the owner's
M5 Mac. The owner requested that the visual review be performed directly,
then asked to add all eight recommendations to the existing plan and have
Opus 5.5 review that documentation change for project fit.

The release build succeeded using existing dependencies with
`cargo build --release --offline --locked --bin voxelcraft`, after the
shared-target package clean and `build.rs` refresh required by AGENTS.md.
This is visual/source evidence, not Java parity or ordinary-play acceptance.
The project reference remains Java 26.3, with FE-01's original terrain noise.

Most captures used `--seed 42 --new --rd 8 --size 1920x1080`, the built-in
`--screenshot` mode and isolated `--data-dir target/design-review/data`.
The settings comparison also used 1280 × 720. Camera/time/weather, placement,
mob-spawn, inventory and menu overrides produced the individual studies.
One forest comparison used seed 12345. Scripted runs ignore saved settings;
these images do not establish coverage of every GUI scale or saved option.

Reviewed scenes included title, empty world selection, creation, video,
accessibility, sound and key-binding screens; creative and survival
inventories; furnace and smithing screens; common materials and mobs;
Overworld day/dusk/night, ground/elevated views and rain; water, torch-lit
interior, Nether and End views. Material/mob studies used an elevated
sandstone platform to isolate appearance from terrain. The corrected water
study used a supporting sandstone foundation beneath the sand.

Local PNGs, logs and scene argument files remain under ignored
`target/design-review/`. They are disposable supporting artifacts, not links
or runtime inputs required by a fresh clone. Future implementations must
record reproducible scene specifications and results for their own commits
using the [scripted-scene tools](../development.md#screenshots-and-scripted-scenes).

Desktop automation could not attach to the standalone executable. Live
navigation, animation feel, sound quality, persistence and the performance
gate were not verified. A water fixture with unsupported falling sand was
discarded as evidence of a game defect; it was replaced with the supported
fixture. Input-contaminated pause/death captures were not treated as evidence
of default UI behavior; the death-screen recommendation is backed by source.
The displayed FPS counter in any image is not a valid benchmark result.

## Findings

| Area | Observation or source finding | Planned response / requirement |
|---|---|---|
| Material art | Plain stone reads as repeating inset panels; ore backgrounds inherit the pattern. Several greens dominate the scene. | Natural-stone/ore pass followed by common materials; evaluate tiling, icons and held/dropped art. UX-01, BLD-05c |
| Typography and contrast | Large text appears heavy; ordinary video labels overflow at both tested sizes. The toolkit intentionally scrolls overflow. Flat UI colors and the sRGB surface need a consistent palette convention; no conversion defect was proved. | Font metrics, label fit and contrast work, with a color-path audit before retuning. UX-01, UX-03f |
| Inventory and recipes | Creative browsing has three rows plus search. The recipe browser has previous/next navigation and previews only. Survival equipment grouping and blank space weaken the hierarchy. | Java creative/discovery layout, recipe transactions and persistence, stronger equipment grouping/player preview. UX-03c |
| Mobs | Recognizable cuboid animals have uneven surface detail and weak ground contact. Screenshots cannot assess animation timing or feel. | Original faces/markings and terrain-aware shadows, followed by motion/state review using existing systems. MOB-06, UX-01, BLD-05c |
| Atmosphere | Elevated Overworld views lose substantial middle-distance detail to pale fog; rain streaks compete with terrain. Water's existing effects need visual refinement. The Nether has a strong, distinct palette. | Reference-backed fog/cloud behavior, readable rain/water/interiors and dimension-specific comparison scenes. UX-01/02 |
| Title and worlds | Title uses enlarged UI text over sky/clouds; world rows have placeholder icons and the empty list gives little guidance. | Original wordmark, terrain/structure panorama, thumbnails and empty-state guidance. UX-03e |
| HUD and secondary screens | Slots, equipment hints, progress graphics and panel treatments vary; the default FPS counter competes with normal play. Source shows a "Click to respawn" death hint. | Shared component styling, Java death actions and a default-off FPS counter for absent preferences. UX-03c/g |
| Accessibility | GUI scaling/motion controls exist; subtitles remain missing. UI text entry is ASCII-only and several renderers replace accented text with `?`. | Captions, background/focus cues and consistent text entry/rendering/persistence across relevant surfaces. UX-03d/f |

Source anchors for these findings:

- [UI font/rendering](../../src/render/ui.rs), [shared toolkit](../../src/app/gui/mod.rs),
  [text entry](../../src/app/gui/text_field.rs) and
  [UI shader](../../src/render/shaders/overlay.wgsl).
- [HUD/inventory/workstations](../../src/app/hud.rs),
  [recipe browser](../../src/app/recipe_book.rs), [search](../../src/app/search.rs)
  and [settings defaults/persistence](../../src/app/settings.rs).
- [Title/world screens](../../src/app/title/), [fog setup](../../src/app/split.rs),
  [chunk/water shader](../../src/render/shaders/chunk.wgsl) and
  [entity models](../../src/entity/model/).
- [Sign renderer](../../src/render/signs.rs), [sign editing](../../src/app/sign_edit.rs)
  and [entity-label text](../../src/entity/model/labels.rs).
- [Original asset formats](../../assets/README.md),
  [stone art description](../../assets/textures/stone.source.json) and
  [texture generation/mipmaps](../../src/render/textures.rs).

Existing mipmaps, shape-aware icons, water waves/Fresnel/sky reflections,
animation systems, menu navigation and embedded assets are useful foundations.
Their presence was retained in the plan; the review does not propose rebuilding
them or changing gameplay IDs, terrain algorithms, collision or gameplay light.

## Plan review and disposition

Opus 5.5 performed the requested independent, read-only plan review and found
no blocking project-fit issues. It checked source claims at the same commit;
it did not edit, build or run the game. Its three medium and six low findings
were assessed by the primary agent and incorporated as follows:

| Finding | Disposition |
|---|---|
| Shipped row status versus expanded scope | Restore UX-03a/b to their existing v0.11 state. Track new title, typography/options and secondary-screen work in UX-03e/f/g. |
| Visual targets need Java behavior and real options | Require selected-client fixtures for creative layout, fog, shadows, clouds, death actions, panorama/world icons and background behavior. Add corresponding functioning options as their features land; verify names/defaults and the applicability of loading-screen monochrome branding. |
| Recipe state/rules and release exit | Require stable recipe identifiers, per-player saved discovery/book state, `limited_crafting` and save/restart evidence. Legacy players start with Java's initial book and re-evaluate unlocks from existing inventory/supported progression evidence; do not fabricate history, and preserve explicit rules and playable acquisition. Add a v0.20 exit distinct from M7/1.0. This is a planned migration policy, not an implemented result. |
| FPS default ambiguity | Default off only when the option is absent; preserve saved true/false values. |
| Text outside the main UI | Include sign and entity-label paths, relevant map/held-item text consumers, and preservation of saved sign strings when metrics change. The map pixel serializer's ASCII hex validation is not itself a text defect. |
| Missing atmospheric benchmarks | Plan weather, underwater and Nether fixtures before atmosphere work; update the title benchmark for the panorama. Existing scenes do not already cover those cases. |
| Missing capture controls | Add GUI-scale and missing recipe/Hardcore state controls. Reuse `--open-block`: workstation capture already exists, contrary to that part of the suggestion. |
| Evidence placement and guide destination | Put observations/limitations in this record; keep tasks in the roadmap. Name `docs/art-and-ui.md` as a future deliverable. |
| Entity rebuild ordering | Resolve the measured rebuild bottleneck before adding per-frame detail/shadow cost, or record evidence that it no longer applies; do not require an unmeasured rewrite. |

Reference follow-up corrected two potentially stale assumptions in the review.
The official [Java 1.21.9 release notes](https://www.minecraft.net/en-us/article/minecraft-java-edition-1-21-9)
renamed the accessibility controls to **Hide Sky Flashes** and **Closed
Captions**; implementation must check the selected 26.3 client's labels and
behavior rather than use older names. The official
[26.4 Snapshot 2 notes](https://www.minecraft.net/en-us/article/minecraft-26-4-snapshot-2)
describe changed fog/sky/cloud blending. Those snapshot changes do not replace
the project's stable reference. The plan therefore requires 26.3 fog fixtures,
not an assumed historical "near the render-distance edge" formula.

The roadmap remains the only active plan. No release is split or rescheduled
by this review, no new fidelity exception is approved, and no requirement is
marked Verified. The final wording after these corrections was checked by
the primary agent; Opus did not perform a second review.
