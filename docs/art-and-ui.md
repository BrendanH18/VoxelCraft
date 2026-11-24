# Art and UI guide

[Roadmap scope](roadmap.md#v018-scope-presentation-and-performance) · [Visual review](verification/2026-11-05-graphics-ui-review.md) · [Comparison scenes](development.md#comparison-scenes) · [Documentation index](README.md)

The style contract for v0.18's visual work (written 2026-11-23, v0.18
track 0). It records decisions and rules you can measure in a capture or
in code. It doesn't describe what is already built: where today's code
differs, the **Now** notes say so and name the track that changes it.

Values marked **tunable (track N)** are starting targets that the
typography (1), shared UI (2) or material (3) track may change after
comparing captures. A track that changes one updates this page in the same
work. Everything else here is a decision. Changing one needs a reason
recorded in that track's verification record.

Java numbers below are Java 26.3's layout (GUI sprite sizes, offsets and
colours). Read them from the selected client for behaviour and numbers
only. No Mojang texture, sprite or font is copied or traced. A track that
depends on an exact Java value checks it against a fixture it extracts
([fidelity rule](../AGENTS.md#engineering-and-acceptance)).

## Checking your work

- Capture the [comparison scenes](development.md#comparison-scenes) before
  and after a change with `scripts/boat-capture.sh --scene <name or group>`,
  and compare the PNGs under `target/captures/<commit>/`.
- Software captures show appearance only. Their frame rate means nothing,
  and the FPS counter that shows in UI scenes is not a measurement.
- Sample colours from the PNGs, not from the code. The surface is sRGB, so
  a colour in the code may not be the colour on screen (see below).

## Colour pipeline

The swapchain and every block, item and entity texture are sRGB
(`Rgba8UnormSrgb` textures, an `*Srgb` surface format). Texture bytes
therefore show exactly as authored.

**Decision:** write every UI and HUD colour as an sRGB byte value (a hex
such as `#C6C6C6`, the number you would see in a capture), and convert it
to linear in one shared helper before it becomes a vertex colour. Alpha
stays linear. Texture art is already authored in sRGB bytes and needs no
change.

**Done (v0.18 track 1):** every colour given to the UI, flat or a
texture tint, is sRGB. `Ui::quad` converts it once with
[`render::ui::linear`](../src/render/ui.rs) (the exact sRGB curve, so a
byte value shows on screen as written; alpha untouched), and
`render::ui::rgb(0xC6C6C6)` writes a constant as its hex. Nothing else
converts: a track adding a colour writes the sRGB value and passes it to
the UI as is. Before the helper, the values below were written as sRGB
but sent as linear, so they showed lighter:

| Constant | Written | Showed before (sRGB) | Shows now | Java's counterpart |
|---|---|---|---|---|
| Inventory panel `[0.78; 3]` | 199 | 229 (`#E5E5E5`) | 199 | `#C6C6C6` (198) |
| Panel title text `[0.25; 3]` | 64 | 137 (`#898989`) | 64 | `#404040` (64) |
| Button `BUTTON` | 107 | 173 (`#ADADAD`) | 107 (`#6B6B6B`) | the button sprite's grey |
| Disabled `DISABLED` | 51 | 124 (`#7C7C7C`) | 74 (`#4A4A4A`, raised for contrast) | the disabled button sprite |
| Panel shadow edge `[0.33; 3]` | 84 | 155 (`#9B9B9B`) | 84 | `#555555` (85) |

The [typography record](verification/2026-11-23-v0.18-typography.md)
has the before and after samples. Translucent colours still blend in
linear light on the sRGB surface, where Java blends sRGB values, so a
dimming overlay at a given alpha looks a little lighter than Java's.

## Pixel grid

- Block and item textures are 16 × 16 texels. No track adds a higher
  resolution. Mipmaps (5 levels, 16 to 1) stay.
- One UI pixel is `GUI scale` physical pixels: Java's rule, already in
  `Ui::gui_scale` (Auto is the largest scale that leaves 320 × 240 UI
  pixels: 4 at 1920 × 1080, 3 at 1280 × 720).
- Every UI rectangle, glyph, icon and text position is a whole number of
  UI pixels. `Ui::ndc` rounds to physical pixels, but a half-pixel origin
  still rounds unevenly at odd scales, so layout code uses whole numbers
  (`floor` when centring).
- Icons draw at their native size or a whole multiple of it: items and
  blocks 16 UI pixels, HUD icons (hearts, hunger, armour, bubbles) 9 UI
  pixels, status effect icons 18. Never scale art by a fraction.
- Text is 1× in the UI. Titles that need emphasis use colour or position
  first. `text_scaled` exists for the title screen's version and splash
  only, at whole factors.

## Palette and contrast

Luminance below is relative luminance (WCAG) of sRGB values from captures.

**UI text** (decision):

- Ordinary labels and values reach a contrast ratio of at least 4.5:1
  against what is directly behind them, including the drop shadow's
  backing. Large titles (2× and over) need 3:1.
- Disabled text stays readable (at least 2.5:1) but sits visibly below
  enabled text: at least 1.5 times less contrast than the same label
  enabled.
- Text on world views (HUD, chat, subtitles, F3) has a backing panel or
  shadow. Text-background opacity follows the Text Background Opacity
  option once it exists (UX-03f).

**UI palette** (Java's values, **tunable (track 2)** only to fix contrast):

| Role | sRGB |
|---|---|
| Container panel fill | `#C6C6C6` |
| Panel highlight edge (top, left) | `#FFFFFF` |
| Panel shadow edge (bottom, right) | `#555555` |
| Panel outline | `#000000` |
| Slot fill | `#8B8B8B` |
| Slot inner shadow (top, left) | `#373737` |
| Slot inner highlight (bottom, right) | `#FFFFFF` |
| Title text on a panel | `#404040`, no shadow |
| Button and widget text | `#FFFFFF` with shadow |
| Disabled widget text | `#A0A0A0` with shadow |
| Hovered slot overlay | white at 50 % alpha over the 16 × 16 item area |
| Screen dimming behind a menu in a world | `#101010` from 75 % alpha at the top to 82 % at the bottom |

**Block materials** (**tunable (track 3)**, checked per tile and in the
`block-palette`, `grass-hillside`, `forest` and `stone-ore-wall` scenes):

- Natural stone: mean relative luminance 0.17–0.24. 90 % of texels lie
  within ±25 % of the tile mean. Ores keep the stone's background exactly,
  with their mineral clusters the only change.
- Grass and leaves: at the plains tint, grass tops have HSV saturation of
  at most 0.55 and leaves are 10–25 % darker than grass tops. Biome tints
  stay distinct (the review found too many strong greens).
- Dirt, sand and gravel: lower texel contrast than stone (90 % within
  ±18 % of the mean), so walls of stone read rougher than the ground.
- Every tile keeps its family recognisable from 8 blocks away at 1080p:
  the 2 × 2 mip level must still separate stone, cobblestone, andesite,
  dirt and gravel by mean colour (no two within 4 % luminance and the same
  hue).

## Natural and manufactured patterns

Natural materials (stone, ores, dirt, sand, gravel, leaves, bark, terrain
of the Nether and End):

- Irregular clusters of 2–5 texels. No straight run of one colour longer
  than 4 texels in a row or column, except deliberate strata (deepslate,
  sandstone layers, which run horizontally).
- No frame, inset, bevel or darker border at the tile edge. The review
  found plain stone reading as repeating inset panels. A 3 × 3 repeat of
  the tile should show no grid.
- Tileable in both directions. Logs tile vertically. Leaves keep their
  cut-out holes (8–15 % transparent texels).

Manufactured materials (planks, bricks, cut and polished stone, metal,
glass, workstations):

- Straight edges on the 16-texel grid: planks in 4-texel boards, bricks in
  4-texel courses with a 1-texel mortar line, tiles at 8 or 16.
- Light from the top left: 1-texel highlights on the top and left edges of
  each board, brick or tile, and shade on the bottom and right. The UI
  bevels use the same direction.
- Joints and seams are at most 1 texel wide.

## Typography

Targets for the typography track (1); the metrics are Java's.

| Metric | Target |
|---|---|
| Line height | 9 UI pixels (8-pixel glyph cell plus 1) |
| Cap height and baseline | 7 pixels; baseline under row 7 of the cell |
| x-height | 5 pixels, **tunable (track 1)** |
| Descenders | 1 pixel below the baseline (g, j, p, q, y), inside the 9-pixel line |
| Advance | glyph ink width plus 1 pixel |
| Space | 4 pixels |
| Shadow | offset (1, 1), the text colour at 25 % brightness |
| Characters | ASCII plus Latin-1 and Latin Extended-A in user text: world names, signs, name tags, book text. No `?` substitution for those |

**Done (v0.18 track 1):** VoxelCraft's own pixel font
([`assets/font/glyphs.txt`](../assets/font/glyphs.txt), read by
[`src/font.rs`](../src/font.rs)) covers ASCII, Latin-1 and Latin
Extended-A with these metrics, accents composed from shared marks. Each
character's advance equals Java 26.3's (checked against
`tests/fixtures/java/26.3/font_metrics.json`), so Java's layouts and line
limits fit the same text. x-height is 5. Anything else draws as a 5 × 7
box and can't be typed. Every measurement goes through the font:
`font::width` is Java's `Font.width` (the advances, trailing gap
included) for limits such as a sign's 90 pixels, and `Ui::text_width`
(`font::ink_width`) is the ink for centring and boxes. Sign text from the
old font still shows whole (see the record). `font::VERSION` changes
whenever a glyph does, for caches of laid-out text.

## Spacing and layout

Java's GUI layout in UI pixels, already used by the
[menu toolkit](../src/app/gui/mod.rs) for menus:

- Buttons are 20 tall: 200 wide in single columns, 150 wide in two-column
  option lists 10 apart, on 24-pixel rows (25 in option lists).
- Screens have a 33-pixel header with the title centred 15 pixels down and
  a 33-pixel footer holding Done.
- Containers are 176 pixels wide. Slots are on an 18-pixel pitch (a
  16-pixel item area with a 1-pixel frame). The grid starts 8 pixels from
  the panel's left edge. The title is at (8, 6), and the player inventory's
  label sits 11 pixels above its first row. The hotbar row is 4 pixels below
  the main inventory.
- Between unrelated groups inside a panel (crafting grid and armour, input
  and output), leave at least one slot pitch or a drawn divider. Don't pad
  with empty grey.

**Now:** container panels are 9 × 18 + 14 = 176 wide with slots 7 pixels in
and 18 apart, matching. The panel has a 1-pixel bevel and no black outline.

## Components and states

Every interactive element has these states. Each differs from the normal
state by more than colour alone (scope item 8):

| State | Rule |
|---|---|
| Normal | Panel or button sprite, white text with shadow |
| Hovered or focused (mouse, keyboard or controller) | A 1-pixel white outline around the widget, or the 50 % white overlay in a slot. Keyboard and controller focus look the same as hover. Menu buttons (track 1): `#646E9C` fill, `#FFFFA0` text (4.8:1), white frame |
| Selected (current hotbar slot, chosen tab, toggled option) | A 24 × 24 hotbar frame, or a raised tab joined to its panel, or "ON" in the label. Never colour only |
| Pressed or active | As hovered, for the frame while held |
| Disabled | Darker sprite with `#A0A0A0` text. It takes no focus with the mouse, but the controller may still land on it to read its tooltip. Menu buttons (track 1): flat `#4A4A4A`, 3.4:1 against 5.3:1 for white on the normal `#6B6B6B` |
| Error or warning | Red (`#FF5555`) text or a red-tinted 1-pixel outline, with the message in words |

Slots draw their frame even when empty. Equipment slots show an outline
silhouette of the expected piece at 50 % alpha when empty. Tooltips are a
dark panel (`#100010` at 94 % alpha) with a 1-pixel purple-to-dark
gradient border and 3 pixels of padding, the item name first in its rarity
colour. **Tunable (track 2):** the tooltip colours, if Java 26.3's
sprites differ.

## Progress and status icons

Pixel-art sprites, never flat rectangles. Each fills in whole UI pixels:

| Icon | Size | Fill |
|---|---|---|
| Furnace burn time (flame) | 14 × 14 | Shrinks from the top; 13 pixels of range |
| Furnace cooking progress (arrow) | 24 × 16 | Grows left to right; 24 pixels of range |
| Brewing time (arrow) / blaze fuel bar | 9 × 28 / 18 × 4 | Downwards / left to right |
| Experience bar | 182 × 5 | Left to right, level number centred above |
| Hearts, hunger, armour, air | 9 × 9 on an 8-pixel pitch | Half icons for odd values |
| Status effect | 18 × 18 icon on a 24 × 24 or 120 × 32 background | Blinks in the last 10 seconds |

Each sprite has a 1-pixel dark outline or an empty-state sprite behind the
fill, so progress reads against both the light panel and the world.
**Now:** the furnace flame and arrow, the experience bar and the effect
backgrounds are drawn as flat rectangles (`src/app/hud.rs`). Track 2 owns
the change.

## Where the code is

- UI geometry, font and icon drawing: [`src/render/ui.rs`](../src/render/ui.rs).
- Menu toolkit and its layout constants: [`src/app/gui/`](../src/app/gui/mod.rs).
- HUD, inventory and workstation drawing: [`src/app/hud.rs`](../src/app/hud.rs).
- UI shader: [`src/render/shaders/overlay.wgsl`](../src/render/shaders/overlay.wgsl).
- Procedural block textures: [`src/render/textures.rs`](../src/render/textures.rs)
  and the `src/render/*_textures.rs` generators, plus the authored
  [stone asset](../assets/textures/stone.source.json) ([asset formats](../assets/README.md)).
