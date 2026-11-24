//! v0.18 Track 2: the shared component style of [the art and UI
//! guide](../../../docs/art-and-ui.md), used by menus, container and
//! workstation screens, the HUD and dialogs.
//!
//! Colours are written as the sRGB bytes a capture shows (the guide's
//! tokens) and turned into vertex colours in one place, [`srgb`]. The UI
//! blends in linear space on an sRGB surface while Java blends sRGB values,
//! so translucent veils go through [`veil`] to darken the way Java's do.
//!
//! The drawings are original: rectangles and small pixel masks on the
//! guide's grid, with Java's sizes and colours, never its sprites.

use crate::render::ui::{Color, Ui};

/// The guide's palette, as sRGB `0xRRGGBB`.
pub(in crate::app) mod token {
    /// Container panel fill.
    pub const PANEL: u32 = 0xC6C6C6;
    /// Panel highlight edge (top, left).
    pub const PANEL_LIGHT: u32 = 0xFFFFFF;
    /// Panel shadow edge (bottom, right).
    pub const PANEL_SHADE: u32 = 0x555555;
    pub const OUTLINE: u32 = 0x000000;
    pub const SLOT: u32 = 0x8B8B8B;
    /// Slot inner shadow (top, left).
    pub const SLOT_SHADE: u32 = 0x373737;
    /// Slot inner highlight (bottom, right).
    pub const SLOT_LIGHT: u32 = 0xFFFFFF;
    /// Titles and labels on a panel, drawn without a shadow.
    pub const TITLE_TEXT: u32 = 0x404040;
    /// Button and widget text, with a shadow.
    pub const TEXT: u32 = 0xFFFFFF;
    pub const DISABLED_TEXT: u32 = 0xA0A0A0;
    /// Secondary text on a dark background (tooltips' detail lines).
    pub const GREY_TEXT: u32 = 0xAAAAAA;
    /// Java's `ChatFormatting.YELLOW` (the death screen's score).
    pub const YELLOW_TEXT: u32 = 0xFFFF55;
    /// The anvil's cost line: green, red when it can't be paid.
    pub const COST_TEXT: u32 = 0x80FF20;
    pub const COST_TEXT_SHORT: u32 = 0xFF6060;
    /// Button: fill, top-left light and bottom-right shade, normal and
    /// hovered or focused (which also gets a white outline), and disabled.
    pub const BUTTON: u32 = 0x6F6F6F;
    pub const BUTTON_LIGHT: u32 = 0xAAAAAA;
    pub const BUTTON_SHADE: u32 = 0x565656;
    pub const BUTTON_HOT: u32 = 0x757575;
    pub const BUTTON_HOT_LIGHT: u32 = 0xAFAFAF;
    pub const BUTTON_HOT_SHADE: u32 = 0x5C5C5C;
    pub const BUTTON_DISABLED: u32 = 0x2C2C2C;
    /// A slider's track (the disabled button's fill).
    pub const TRACK: u32 = 0x2B2B2B;
    /// Tooltip fill (at [`super::TOOLTIP_ALPHA`]) and its border's two ends.
    pub const TOOLTIP: u32 = 0x100010;
    pub const TOOLTIP_BORDER_TOP: u32 = 0x5000FF;
    pub const TOOLTIP_BORDER_BOTTOM: u32 = 0x28007F;
    /// Status effect backgrounds: fill, frame and an ambient effect's frame.
    pub const EFFECT: u32 = 0x212121;
    pub const EFFECT_FRAME: u32 = 0x555555;
    pub const EFFECT_AMBIENT_FRAME: u32 = 0x00A8A8;
    /// Progress art: an empty sprite's shape on a panel, and the fills.
    pub const PROGRESS_EMPTY: u32 = 0x8B8B8B;
    pub const PROGRESS: u32 = 0xFFFFFF;
    pub const FLAME_OUTER: u32 = 0xF07A12;
    pub const FLAME_INNER: u32 = 0xFFD84A;
    pub const FLAME_EDGE: u32 = 0xA33A06;
    /// Experience bar: background, its outline and the fill's light,
    /// middle and dark rows.
    pub const XP_BACK: u32 = 0x28332D;
    pub const XP_EDGE: u32 = 0x09100C;
    pub const XP_LIGHT: u32 = 0x80FF20;
    pub const XP_FILL: u32 = 0x5C9A2C;
    /// Experience level text (Java's `0x80FF20`).
    pub const XP_TEXT: u32 = 0x80FF20;
    /// The hotbar's selected slot frame.
    pub const SELECTION: u32 = 0xFFFFFF;
}

pub(in crate::app) const TOOLTIP_ALPHA: f32 = 240.0 / 255.0;
/// The tooltip border's alpha (`tooltip/frame`'s 80 of 255).
const TOOLTIP_BORDER_ALPHA: f32 = 80.0 / 255.0;

/// An sRGB byte, 0..255, as the linear value the UI's vertex colours take.
///
/// This is the seam with v0.18 Track 1's colour helper: when it lands,
/// this body becomes that helper and every token follows.
fn linear(byte: u32) -> f32 {
    (byte as f32 / 255.0).powf(2.2)
}

/// An opaque sRGB colour (`0xRRGGBB`) as a vertex colour.
pub(in crate::app) fn srgb(hex: u32) -> Color {
    srgba(hex, 1.0)
}

/// An sRGB colour with a (linear) alpha, for opaque-looking art and text.
pub(in crate::app) fn srgba(hex: u32, alpha: f32) -> Color {
    [linear(hex >> 16 & 255), linear(hex >> 8 & 255), linear(hex & 255), alpha]
}

/// A translucent fill that looks like Java's `0xAARRGGBB` over a scene:
/// Java blends sRGB values, the UI blends linear ones, so the alpha is
/// raised until the scene behind darkens by Java's factor, `1 - alpha`
/// in sRGB, and the colour is picked so the result over a mid-grey scene
/// matches Java's too. Exact for black veils; tinted ones (the death
/// screen's red) match at mid-grey and run a little strong over black.
pub(in crate::app) fn veil(hex: u32, alpha: f32) -> Color {
    let a = alpha.clamp(0.0, 1.0);
    let la = 1.0 - (1.0 - a).powf(2.2);
    if la <= 0.0 {
        return [0.0; 4];
    }
    const MID: f32 = 0.5;
    let c = |byte: u32| {
        let java = (byte as f32 / 255.0 * a + MID * (1.0 - a)).powf(2.2);
        ((java - MID.powf(2.2) * (1.0 - la)) / la).clamp(0.0, 1.0)
    };
    [c(hex >> 16 & 255), c(hex >> 8 & 255), c(hex & 255), la]
}

/// [`veil`] from Java's packed `0xAARRGGBB`.
pub(in crate::app) fn argb(packed: u32) -> Color {
    veil(packed & 0xFF_FFFF, (packed >> 24) as f32 / 255.0)
}

/// Java's in-world menu backdrop (`inworld_menu_background`, black at
/// 64 of 255), over the blurred world.
pub(in crate::app) const MENU_VEIL: u32 = 0x4000_0000;
/// `Screen.extractTransparentBackground`: behind container screens.
pub(in crate::app) const CONTAINER_VEIL_TOP: u32 = 0xC010_1010;
pub(in crate::app) const CONTAINER_VEIL_BOTTOM: u32 = 0xD010_1010;
/// `DeathScreen.extractDeathBackground`.
pub(in crate::app) const DEATH_VEIL_TOP: u32 = 0x6050_0000;
pub(in crate::app) const DEATH_VEIL_BOTTOM: u32 = 0xA080_3030;

/// A full-screen vertical gradient of two Java `0xAARRGGBB` colours.
pub(in crate::app) fn backdrop(ui: &mut Ui, top: u32, bottom: u32) {
    let (sw, sh) = ui.size();
    ui.gradient(0.0, 0.0, sw, sh, argb(top), argb(bottom));
}

/// The darkening behind a container or workstation screen.
pub(in crate::app) fn container_backdrop(ui: &mut Ui) {
    backdrop(ui, CONTAINER_VEIL_TOP, CONTAINER_VEIL_BOTTOM);
}

/// Java's container panel: a black outline with its corners cut, a
/// 2-pixel white highlight on the top and left, a 2-pixel shade on the
/// bottom and right, and the grey fill.
pub(in crate::app) fn panel(ui: &mut Ui, x: f32, y: f32, w: f32, h: f32) {
    let (black, light, shade, fill) =
        (srgb(token::OUTLINE), srgb(token::PANEL_LIGHT), srgb(token::PANEL_SHADE), srgb(token::PANEL));
    // Outline: straight edges with the corners stepped in.
    ui.rect(x + 2.0, y, w - 4.0, 1.0, black);
    ui.rect(x + 2.0, y + h - 1.0, w - 4.0, 1.0, black);
    ui.rect(x, y + 2.0, 1.0, h - 4.0, black);
    ui.rect(x + w - 1.0, y + 2.0, 1.0, h - 4.0, black);
    for (cx, cy) in [(x + 1.0, y + 1.0), (x + w - 2.0, y + 1.0), (x + 1.0, y + h - 2.0), (x + w - 2.0, y + h - 2.0)] {
        ui.rect(cx, cy, 1.0, 1.0, black);
    }
    ui.rect(x + 1.0, y + 2.0, w - 2.0, h - 4.0, fill);
    ui.rect(x + 2.0, y + 1.0, w - 4.0, h - 2.0, fill);
    // Light on the top and left, shade on the bottom and right.
    ui.rect(x + 2.0, y + 1.0, w - 5.0, 2.0, light);
    ui.rect(x + 1.0, y + 2.0, 2.0, h - 5.0, light);
    ui.rect(x + 3.0, y + h - 3.0, w - 5.0, 2.0, shade);
    ui.rect(x + w - 3.0, y + 3.0, 2.0, h - 5.0, shade);
}

/// A recessed area with a 1-pixel frame: shade on the top and left,
/// light on the bottom and right (slots, list boxes, text boxes).
pub(in crate::app) fn inset(ui: &mut Ui, x: f32, y: f32, w: f32, h: f32, fill: u32) {
    ui.rect(x, y, w, h, srgb(fill));
    ui.rect(x, y, w - 1.0, 1.0, srgb(token::SLOT_SHADE));
    ui.rect(x, y, 1.0, h - 1.0, srgb(token::SLOT_SHADE));
    ui.rect(x + 1.0, y + h - 1.0, w - 1.0, 1.0, srgb(token::SLOT_LIGHT));
    ui.rect(x + w - 1.0, y + 1.0, 1.0, h - 1.0, srgb(token::SLOT_LIGHT));
}

/// An 18 x 18 slot frame at `(x, y)`, its 16 x 16 item area one pixel in.
pub(in crate::app) fn slot(ui: &mut Ui, x: f32, y: f32) {
    inset(ui, x, y, 18.0, 18.0, token::SLOT);
}

/// A result slot: Java's 26 x 26 frame around an 18 x 18 slot at `(x, y)`.
pub(in crate::app) fn big_slot(ui: &mut Ui, x: f32, y: f32) {
    inset(ui, x - 4.0, y - 4.0, 26.0, 26.0, token::SLOT);
}

/// The hover highlight over a slot's item area (white at half alpha).
pub(in crate::app) fn slot_highlight(ui: &mut Ui, x: f32, y: f32) {
    ui.rect(x + 1.0, y + 1.0, 16.0, 16.0, veil(0xFFFFFF, 0.5));
}

/// A 1-pixel outline just inside `[x, y, w, h]`.
pub(in crate::app) fn outline(ui: &mut Ui, [x, y, w, h]: [f32; 4], colour: Color) {
    ui.rect(x, y, w, 1.0, colour);
    ui.rect(x, y + h - 1.0, w, 1.0, colour);
    ui.rect(x, y + 1.0, 1.0, h - 2.0, colour);
    ui.rect(x + w - 1.0, y + 1.0, 1.0, h - 2.0, colour);
}

/// A widget's state, as the guide's component table has it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::app) enum State {
    Normal,
    /// Hovered, focused or pressed: a white outline.
    Hot,
    Disabled,
}

/// Java's button: a black outline (white when hot), a light top-left
/// edge, a 2-pixel shade at the bottom and right, or a flat dark fill when
/// disabled. The state never shows through colour alone: hot buttons gain
/// the white outline and disabled ones lose their bevel.
pub(in crate::app) fn button(ui: &mut Ui, [x, y, w, h]: [f32; 4], state: State) {
    let (edge, fill, light, shade) = match state {
        State::Normal => (token::OUTLINE, token::BUTTON, token::BUTTON_LIGHT, token::BUTTON_SHADE),
        State::Hot => (token::PANEL_LIGHT, token::BUTTON_HOT, token::BUTTON_HOT_LIGHT, token::BUTTON_HOT_SHADE),
        State::Disabled => (token::OUTLINE, token::BUTTON_DISABLED, token::BUTTON_DISABLED, token::BUTTON_DISABLED),
    };
    ui.rect(x, y, w, h, srgb(edge));
    ui.rect(x + 1.0, y + 1.0, w - 2.0, h - 2.0, srgb(fill));
    if state != State::Disabled {
        ui.rect(x + 1.0, y + 1.0, w - 3.0, 1.0, srgb(light));
        ui.rect(x + 1.0, y + 2.0, 1.0, h - 5.0, srgb(light));
        ui.rect(x + 1.0, y + h - 3.0, w - 2.0, 2.0, srgb(shade));
        ui.rect(x + w - 2.0, y + 1.0, 1.0, h - 2.0, srgb(shade));
    }
}

/// A slider's track: a dark bar with a black outline, white when hot.
pub(in crate::app) fn track(ui: &mut Ui, [x, y, w, h]: [f32; 4], hot: bool) {
    ui.rect(x, y, w, h, srgb(if hot { token::PANEL_LIGHT } else { token::OUTLINE }));
    ui.rect(x + 1.0, y + 1.0, w - 2.0, h - 2.0, srgb(token::TRACK));
}

/// The text colour for a widget in `state`.
pub(in crate::app) fn text_colour(state: State) -> Color {
    srgb(if state == State::Disabled { token::DISABLED_TEXT } else { token::TEXT })
}

/// A tooltip's backing for content `w` x `h` at `(x, y)`: Java's dark
/// fill one pixel past a purple-to-dark border, 3 pixels from the content.
pub(in crate::app) fn tooltip_box(ui: &mut Ui, x: f32, y: f32, w: f32, h: f32) {
    let fill = srgba(token::TOOLTIP, TOOLTIP_ALPHA);
    ui.rect(x - 4.0, y - 3.0, w + 8.0, h + 6.0, fill);
    ui.rect(x - 3.0, y - 4.0, w + 6.0, 1.0, fill);
    ui.rect(x - 3.0, y + h + 3.0, w + 6.0, 1.0, fill);
    let top = srgba(token::TOOLTIP_BORDER_TOP, TOOLTIP_BORDER_ALPHA);
    let bottom = srgba(token::TOOLTIP_BORDER_BOTTOM, TOOLTIP_BORDER_ALPHA);
    ui.rect(x - 2.0, y - 3.0, w + 4.0, 1.0, top);
    ui.rect(x - 2.0, y + h + 2.0, w + 4.0, 1.0, bottom);
    ui.gradient(x - 3.0, y - 3.0, 1.0, h + 6.0, top, bottom);
    ui.gradient(x + w + 2.0, y - 3.0, 1.0, h + 6.0, top, bottom);
}

/// A status effect's background (24 x 24 on the HUD, 120 x 32 or 32 x 32
/// beside the inventory): a dark fill in a 1-pixel frame with its corners
/// cut, the frame cyan for an ambient effect (a beacon's or conduit's).
pub(in crate::app) fn effect_background(ui: &mut Ui, (x, y, w, h): (f32, f32, f32, f32), ambient: bool, alpha: f32) {
    let edge = srgba(if ambient { token::EFFECT_AMBIENT_FRAME } else { token::EFFECT_FRAME }, alpha);
    let black = srgba(token::OUTLINE, alpha);
    ui.rect(x + 1.0, y, w - 2.0, 1.0, black);
    ui.rect(x + 1.0, y + h - 1.0, w - 2.0, 1.0, black);
    ui.rect(x, y + 1.0, 1.0, h - 2.0, black);
    ui.rect(x + w - 1.0, y + 1.0, 1.0, h - 2.0, black);
    outline(ui, [x + 1.0, y + 1.0, w - 2.0, h - 2.0], edge);
    ui.rect(x + 2.0, y + 2.0, w - 4.0, h - 4.0, srgba(token::EFFECT, alpha));
}

/// A pixel mask drawn as runs of rectangles: each row's characters pick a
/// colour by `pick` (`None` leaves the pixel clear). Only rows `from..`
/// draw, for sprites that fill from the bottom.
fn mask(ui: &mut Ui, x: f32, y: f32, rows: &[&str], from: usize, pick: impl Fn(u8) -> Option<Color>) {
    for (r, row) in rows.iter().enumerate().skip(from) {
        let bytes = row.as_bytes();
        let mut c = 0;
        while c < bytes.len() {
            let Some(colour) = pick(bytes[c]) else {
                c += 1;
                continue;
            };
            let start = c;
            while c < bytes.len() && bytes[c] == bytes[start] {
                c += 1;
            }
            ui.rect(x + start as f32, y + r as f32, (c - start) as f32, 1.0, colour);
        }
    }
}

/// An original 14 x 14 flame: `o` outline, `#` outer fire, `*` the core.
const FLAME: [&str; 14] = [
    "......o.......",
    ".....o#o......",
    ".....o#o..o...",
    "....o##o.o#o..",
    "...o###oo##o..",
    "...o##*###o...",
    "..o##**###o...",
    "..o#***##o#o..",
    ".o##****###o..",
    ".o#******##o..",
    ".o#******##o..",
    "..o#****##o...",
    "...oo###oo....",
    ".....oooo.....",
];

/// The furnace's fuel flame: the empty flame's shape, with `left` (0..1)
/// of it burning, shrinking from the top in 13 pixel steps as Java's does.
pub(in crate::app) fn flame(ui: &mut Ui, x: f32, y: f32, left: f32) {
    let empty = srgb(token::PROGRESS_EMPTY);
    mask(ui, x, y, &FLAME, 0, |c| (c != b'.').then_some(empty));
    if left <= 0.0 {
        return;
    }
    // `Mth.ceil(litProgress * 13) + 1` rows of the 14 show.
    let lit = ((left.clamp(0.0, 1.0) * 13.0).ceil() as usize + 1).min(14);
    let (edge, outer, inner) = (srgb(token::FLAME_EDGE), srgb(token::FLAME_OUTER), srgb(token::FLAME_INNER));
    mask(ui, x, y, &FLAME, 14 - lit, |c| match c {
        b'o' => Some(edge),
        b'#' => Some(outer),
        b'*' => Some(inner),
        _ => None,
    });
}

/// The progress arrow's shape (`w` x `h`) at column `c`, row `r`: a
/// shaft about three eighths of the height and a head the full height,
/// its tip a couple of pixels short of the right edge.
fn arrow_at(c: usize, r: usize, w: usize, h: usize) -> bool {
    let d = (r as f32 - (h as f32 - 1.0) / 2.0).abs();
    let half = (h / 2) as f32;
    let start = w as f32 - half - 2.0;
    let shaft = d < h as f32 * 3.0 / 16.0 && c >= 1 && (c as f32) <= start;
    let head = c as f32 >= start && (c as f32) < start + half + 1.0 - d - 0.5;
    shaft || head
}

/// A furnace's or crafting screen's arrow, 24 x 16: the empty arrow, white
/// from the left as `progress` (0..1) grows, in whole pixels of 24.
pub(in crate::app) fn arrow(ui: &mut Ui, x: f32, y: f32, progress: f32) {
    arrow_sized(ui, (x, y, 24, 16), progress);
}

/// The arrow at any size (the survival inventory's is 16 x 13).
pub(in crate::app) fn arrow_sized(ui: &mut Ui, (x, y, w, h): (f32, f32, usize, usize), progress: f32) {
    let lit = (progress.clamp(0.0, 1.0) * w as f32).ceil() as usize;
    let (empty, full) = (srgb(token::PROGRESS_EMPTY), srgb(token::PROGRESS));
    for r in 0..h {
        let mut c = 0;
        while c < w {
            if !arrow_at(c, r, w, h) {
                c += 1;
                continue;
            }
            let start = c;
            let filled = c < lit;
            while c < w && arrow_at(c, r, w, h) && (c < lit) == filled {
                c += 1;
            }
            ui.rect(x + start as f32, y + r as f32, (c - start) as f32, 1.0, if filled { full } else { empty });
        }
    }
}

/// The brewing stand's 9 x 28 arrow pointing down, white from the top
/// with `progress` (0..1).
pub(in crate::app) fn arrow_down(ui: &mut Ui, x: f32, y: f32, progress: f32) {
    let lit = (progress.clamp(0.0, 1.0) * 28.0).floor();
    let (empty, full) = (srgb(token::PROGRESS_EMPTY), srgb(token::PROGRESS));
    for r in 0..28 {
        // A 3-pixel shaft, widening into a head over the last 5 rows.
        let half = if r >= 23 { (27 - r) as f32 } else { 1.0 };
        let colour = if (r as f32) < lit { full } else { empty };
        ui.rect(x + 4.0 - half, y + r as f32, 2.0 * half + 1.0, 1.0, colour);
    }
}

/// A horizontal gauge (the brewing stand's blaze fuel): a dark inset with
/// `fill` (0..1) of its inside in `colour`.
pub(in crate::app) fn gauge(ui: &mut Ui, x: f32, y: f32, w: f32, h: f32, fill: f32, colour: u32) {
    ui.rect(x, y, w, h, srgb(token::SLOT_SHADE));
    let inner = ((w - 2.0) * fill.clamp(0.0, 1.0)).ceil();
    if inner > 0.0 {
        ui.rect(x + 1.0, y + 1.0, inner, h - 2.0, srgb(colour));
    }
}

/// The anvil's 13 x 13 plus between its inputs.
pub(in crate::app) fn plus(ui: &mut Ui, x: f32, y: f32) {
    let c = srgb(token::PROGRESS_EMPTY);
    ui.rect(x, y + 5.0, 13.0, 3.0, c);
    ui.rect(x + 5.0, y, 3.0, 13.0, c);
}

/// Java's error mark over an arrow that can't make anything: a red cross
/// with a dark outline, 28 x 21, so it reads without its colour.
pub(in crate::app) fn error_cross(ui: &mut Ui, x: f32, y: f32) {
    let (dark, red) = (srgb(0x3F0000), srgb(0xE03030));
    for i in 0..15 {
        let (cx, cy) = (x + 6.0 + i as f32, y + 3.0 + i as f32);
        let mx = x + 6.0 + 14.0 - i as f32;
        for (px, py) in [(cx, cy), (mx, cy)] {
            ui.rect(px - 1.0, py - 1.0, 4.0, 4.0, dark);
        }
    }
    for i in 0..15 {
        let (cx, cy) = (x + 6.0 + i as f32, y + 3.0 + i as f32);
        let mx = x + 6.0 + 14.0 - i as f32;
        ui.rect(cx, cy, 2.0, 2.0, red);
        ui.rect(mx, cy, 2.0, 2.0, red);
    }
}

/// A panel title or label: dark grey without a shadow.
pub(in crate::app) fn title(ui: &mut Ui, x: f32, y: f32, text: &str) {
    ui.text_flat(x, y, text, srgb(token::TITLE_TEXT));
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The on-screen sRGB byte a linear value shows as.
    fn shown(v: f32) -> u32 {
        (v.powf(1.0 / 2.2) * 255.0).round() as u32
    }

    #[test]
    fn tokens_show_as_written() {
        for hex in [token::PANEL, token::SLOT, token::TITLE_TEXT, token::PANEL_SHADE, token::DISABLED_TEXT] {
            let [r, g, b, a] = srgb(hex);
            assert_eq!((shown(r) << 16) | (shown(g) << 8) | shown(b), hex, "{hex:06x}");
            assert_eq!(a, 1.0);
        }
    }

    #[test]
    fn black_veils_darken_like_java() {
        // Java: 25% black leaves 75% of an sRGB value.
        for (alpha, under) in [(0.25f32, 200u32), (0.75, 120), (0.5, 255)] {
            let [_, _, _, la] = veil(0, alpha);
            let scene = (under as f32 / 255.0).powf(2.2);
            let blended = shown(scene * (1.0 - la));
            let java = (under as f32 * (1.0 - alpha)).round() as u32;
            assert!(blended.abs_diff(java) <= 1, "{alpha} over {under}: {blended} vs {java}");
        }
        // Over a mid-grey scene, a tinted veil shows Java's result.
        let [r, g, _, la] = argb(DEATH_VEIL_BOTTOM);
        let grey = 0.5f32.powf(2.2);
        let java = |c: f32| ((c / 255.0 * 160.0 / 255.0 + 0.5 * (1.0 - 160.0 / 255.0)) * 255.0).round() as u32;
        assert!(shown(r * la + grey * (1.0 - la)).abs_diff(java(128.0)) <= 1);
        assert!(shown(g * la + grey * (1.0 - la)).abs_diff(java(48.0)) <= 1);
    }

    /// Runs of one non-clear character in a mask row.
    fn runs(row: &str) -> usize {
        let b = row.as_bytes();
        (0..b.len()).filter(|&i| b[i] != b'.' && (i == 0 || b[i - 1] != b[i])).count()
    }

    #[test]
    fn progress_icons_fill_in_whole_pixels() {
        // Java's furnace: `ceil(p * 13) + 1` rows of the flame show, so a
        // nearly spent flame keeps its bottom two rows.
        let quads = |p: f32| {
            let mut ui = Ui::with_scale(320.0, 240.0, 1.0);
            flame(&mut ui, 0.0, 0.0, p);
            ui.verts.len() / 6
        };
        let empty = quads(0.0);
        assert!(quads(0.01) > empty && quads(0.5) > quads(0.01) && quads(1.0) > quads(0.5));
        // Lit rows draw over the empty shape: both full and empty give one
        // quad per run, so a full flame draws the empty runs twice.
        assert_eq!(quads(1.0) - empty, FLAME.iter().map(|r| runs(r)).sum::<usize>());
        // The arrow's shape stays inside its box: the shaft from column 1,
        // the head across every row, the tip short of the right edge.
        for (w, h) in [(24, 16), (16, 13)] {
            assert!((0..h).all(|r| (0..w).any(|c| arrow_at(c, r, w, h))), "{w}x{h} head spans every row");
            assert!((0..h).all(|r| !arrow_at(0, r, w, h) && !arrow_at(w - 1, r, w, h)), "{w}x{h} in bounds");
            assert!(arrow_at(1, h / 2, w, h) && arrow_at(w - 3, h / 2, w, h));
            assert!(!arrow_at(1, 0, w, h), "{w}x{h}: the shaft is narrower than the head");
        }
        for (r, row) in FLAME.iter().enumerate() {
            assert_eq!(row.len(), 14, "flame row {r}");
        }
    }
}
