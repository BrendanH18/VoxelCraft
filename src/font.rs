//! The text font every screen, sign, name tag and book shares: VoxelCraft's
//! own pixel font (`assets/font/glyphs.txt`), its glyph bitmaps and the one
//! width measurement for text.
//!
//! The glyphs are original. Their advances match Java 26.3's default font
//! (`tests/fixtures/java/26.3/font_metrics.json`) and the line metrics are
//! Java's: a 9-pixel line, capitals 7 pixels tall with the baseline under
//! row 6, descenders on row 7, accents above capitals up to 3 rows over the
//! line's top, and a shadow 1 pixel down and right. The font covers ASCII,
//! Latin-1 and Latin Extended-A; any other character draws as a box.

use std::sync::OnceLock;

/// Bumped whenever a glyph's shape or advance changes, so caches of laid
/// out text (sign text geometry) can key on it. 1 was the `font8x8` font
/// used before v0.18.
pub const VERSION: u32 = 2;
/// Java's `Font.lineHeight`.
pub const LINE_HEIGHT: i32 = 9;
/// Java's `GlyphInfo.getShadowOffset`.
pub const SHADOW_OFFSET: i32 = 1;
/// The first row a glyph can draw on, relative to the line's top (accents
/// over capitals, as Java's 12-pixel accented glyphs with ascent 10).
pub const TOP: i32 = -3;
/// Rows a glyph can use: `TOP` to 8, the row under the descender.
pub const ROWS: usize = 12;
/// Glyph cells in the atlas: 16 by 12 pixels, 16 to a row.
pub const CELL_W: u32 = 16;
pub const CELL_H: u32 = ROWS as u32;
pub const ATLAS_COLUMNS: u32 = 16;
/// Advance of the space and the no-break space (Java's `include/space.json`).
pub const SPACE_ADVANCE: i32 = 4;

/// One glyph: its ink as rows of bits (bit `x` is column `x`, row 0 is
/// [`TOP`]) and its ink width.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Glyph {
    rows: [u16; ROWS],
    width: u8,
    blank_advance: u8,
}

impl Glyph {
    /// Pixels the pen moves past this glyph: its ink and a 1-pixel gap.
    pub fn advance(&self) -> i32 {
        if self.width == 0 { self.blank_advance as i32 } else { self.width as i32 + 1 }
    }

    /// Ink width in pixels (0 for a space).
    pub fn width(&self) -> i32 {
        self.width as i32
    }

    /// The ink rows as (row relative to the line's top, bits), empty rows skipped.
    pub fn rows(&self) -> impl Iterator<Item = (i32, u16)> + '_ {
        self.rows.iter().enumerate().filter(|(_, r)| **r != 0).map(|(i, &r)| (i as i32 + TOP, r))
    }

    /// Whether the pixel at column `x`, row `y` (relative to the line's top) is ink.
    pub fn ink(&self, x: i32, y: i32) -> bool {
        let row = y - TOP;
        (0..ROWS as i32).contains(&row) && (0..16).contains(&x) && self.rows[row as usize] >> x & 1 == 1
    }

    /// Runs of ink in each row, as (x, y, length), for drawing a glyph out
    /// of rectangles.
    pub fn runs(&self) -> impl Iterator<Item = (i32, i32, i32)> + '_ {
        self.rows().flat_map(|(y, bits)| {
            let mut x = 0;
            std::iter::from_fn(move || {
                while x < 16 && bits >> x & 1 == 0 {
                    x += 1;
                }
                if x >= 16 {
                    return None;
                }
                let start = x;
                while x < 16 && bits >> x & 1 == 1 {
                    x += 1;
                }
                Some((start, y, x - start))
            })
        })
    }
}

/// The parsed font: every glyph in atlas order and the characters' indices.
pub struct Font {
    glyphs: Vec<Glyph>,
    /// Glyph index of each character below [`DIRECT`], or `u16::MAX`.
    direct: Vec<u16>,
    missing: u16,
}

/// Characters looked up by index: everything up to the end of Latin Extended-A.
const DIRECT: u32 = 0x180;

const SOURCE: &str = include_str!("../assets/font/glyphs.txt");

/// The font, parsed on first use.
pub fn font() -> &'static Font {
    static FONT: OnceLock<Font> = OnceLock::new();
    FONT.get_or_init(|| Font::parse(SOURCE).expect("assets/font/glyphs.txt parses"))
}

/// The glyph for `c` ([`Font::glyph`]).
pub fn glyph(c: char) -> &'static Glyph {
    font().glyph(c)
}

/// Advance of `c` in pixels.
pub fn advance(c: char) -> i32 {
    font().glyph(c).advance()
}

/// Java's `Font.width`: the sum of the characters' advances, so the
/// trailing 1-pixel gap counts. Sign, book and edit box limits use this.
pub fn width(s: &str) -> i32 {
    let f = font();
    s.chars().map(|c| f.glyph(c).advance()).sum()
}

/// The width of the ink: [`width`] without the last character's gap (0
/// for an empty string). Centring and boxes around text use this.
pub fn ink_width(s: &str) -> i32 {
    (width(s) - 1).max(0)
}

/// Whether the font draws `c` (a space counts) instead of the missing-glyph box.
pub fn covers(c: char) -> bool {
    font().index(c).is_some()
}

/// Characters typed text may hold (signs, books, names, edit boxes): Java's
/// `StringUtil.isAllowedChatCharacter` (no controls, no `§`), limited to
/// what the font draws.
pub fn allowed(c: char) -> bool {
    c != '§' && !c.is_control() && covers(c)
}

/// The longest prefix of `s` whose [`width`] is at most `max` (Java's
/// `Font.plainSubstrByWidth`).
pub fn prefix_by_width(s: &str, max: i32) -> &str {
    let mut width = 0;
    for (i, c) in s.char_indices() {
        width += advance(c);
        if width > max {
            return &s[..i];
        }
    }
    s
}

/// The first line of `s` wrapped to `max` pixels, as Java's
/// `StringSplitter.splitLines` finds it (`LineBreakFinder`): the whole of
/// `s` if it fits, else up to the last space before the first character
/// that overflows (the space dropped), else up to that character. At
/// least one character is kept.
pub fn first_line(s: &str, max: i32) -> &str {
    let mut width = 0;
    let mut last_space = None;
    for (i, c) in s.char_indices() {
        if c == ' ' {
            last_space = Some(i);
        }
        width += advance(c);
        if width > max && i > 0 {
            return &s[..last_space.unwrap_or(i)];
        }
    }
    s
}

impl Font {
    /// Index of `c`'s glyph in [`Font::glyphs`] and the atlas, if the font has one.
    pub fn index(&self, c: char) -> Option<usize> {
        let i = *self.direct.get(c as usize)?;
        (i != u16::MAX).then_some(i as usize)
    }

    /// Atlas index of `c`'s glyph, the missing-glyph box for characters the
    /// font lacks.
    pub fn atlas_index(&self, c: char) -> usize {
        self.index(c).unwrap_or(self.missing as usize)
    }

    pub fn glyph(&self, c: char) -> &Glyph {
        &self.glyphs[self.atlas_index(c)]
    }

    /// Every glyph, in atlas order.
    pub fn glyphs(&self) -> &[Glyph] {
        &self.glyphs
    }

    /// Atlas size in pixels for [`Font::atlas`].
    pub fn atlas_size(&self) -> (u32, u32) {
        let rows = (self.glyphs.len() as u32).div_ceil(ATLAS_COLUMNS);
        (ATLAS_COLUMNS * CELL_W, rows * CELL_H)
    }

    /// The glyphs rasterised as R8 coverage, glyph `i` in cell
    /// (`i % 16`, `i / 16`) with its ink from the cell's top left.
    pub fn atlas(&self) -> Vec<u8> {
        let (w, h) = self.atlas_size();
        let mut px = vec![0u8; (w * h) as usize];
        for (i, g) in self.glyphs.iter().enumerate() {
            let (cx, cy) = ((i as u32 % ATLAS_COLUMNS) * CELL_W, (i as u32 / ATLAS_COLUMNS) * CELL_H);
            for (row, bits) in g.rows.iter().enumerate() {
                for x in 0..16 {
                    if bits >> x & 1 == 1 {
                        px[((cy + row as u32) * w + cx + x) as usize] = 255;
                    }
                }
            }
        }
        px
    }

    /// Parses the font source (format in `assets/font/glyphs.txt`).
    pub fn parse(source: &str) -> Result<Font, String> {
        #[derive(Clone, Copy, PartialEq)]
        enum Place {
            Above,
            Touch,
            Below,
            Ogonek,
        }
        type Pixels = Vec<(i32, i32)>;
        let mut glyphs: Vec<(String, Pixels)> = Vec::new();
        let mut marks: Vec<(String, Place, Pixels)> = Vec::new();
        let named = |glyphs: &[(String, Pixels)], name: &str| glyphs.iter().position(|(n, _)| n == name);
        let key = |token: &str| -> Result<String, String> {
            match token.strip_prefix("U+") {
                Some(hex) => u32::from_str_radix(hex, 16)
                    .ok()
                    .and_then(char::from_u32)
                    .map(String::from)
                    .ok_or_else(|| format!("bad code point {token}")),
                None => Ok(token.to_string()),
            }
        };
        let extent = |px: &Pixels| {
            px.iter().fold((i32::MAX, i32::MIN, i32::MAX, i32::MIN), |(x0, x1, y0, y1), &(x, y)| {
                (x0.min(x), x1.max(x), y0.min(y), y1.max(y))
            })
        };
        // Glyphs and marks first, then the composed letters, which may use
        // a base drawn further down.
        for compose_pass in [false, true] {
            let mut lines = source.lines().enumerate().peekable();
            while let Some((n, line)) = lines.next() {
                let line = line.trim_end();
                if line.is_empty() || line.starts_with(';') {
                    continue;
                }
                let parts: Vec<&str> = line.split_whitespace().collect();
                let err = |m: &str| format!("line {}: {m}: {line}", n + 1);
                match parts.as_slice() {
                    ["glyph", name, top] | ["mark", name, top] => {
                        let mut px = Vec::new();
                        let mut row = 0;
                        while let Some(&(_, l)) = lines.peek().filter(|(_, l)| !l.trim().is_empty()) {
                            for (x, ch) in l.trim_end().chars().enumerate() {
                                match ch {
                                    '#' => px.push((x as i32, row)),
                                    '.' => {}
                                    _ => return Err(err("rows hold only '#' and '.'")),
                                }
                            }
                            row += 1;
                            lines.next();
                        }
                        if compose_pass {
                            continue;
                        }
                        if parts[0] == "glyph" {
                            let top: i32 = top.parse().map_err(|_| err("bad top row"))?;
                            glyphs.push((key(name)?, px.into_iter().map(|(x, y)| (x, y + top)).collect()));
                        } else {
                            let place = match *top {
                                "above" => Place::Above,
                                "touch" => Place::Touch,
                                "below" => Place::Below,
                                "ogonek" => Place::Ogonek,
                                _ => return Err(err("unknown mark placement")),
                            };
                            marks.push((name.to_string(), place, px));
                        }
                    }
                    ["compose", ..] if !compose_pass => {}
                    ["compose", name, base, rest @ ..] if !rest.is_empty() => {
                        let base = named(&glyphs, &key(base)?).ok_or_else(|| err("unknown base"))?;
                        let px = glyphs[base].1.clone();
                        let (_, bx1, by0, by1) = extent(&px);
                        let base_w = bx1 + 1;
                        let mut placed = Vec::new();
                        for m in rest {
                            let (name, at) = match m.split_once('@') {
                                Some((name, at)) => (name, Some(at.parse::<i32>().map_err(|_| err("bad column"))?)),
                                None => (*m, None),
                            };
                            let (_, place, mark) =
                                marks.iter().find(|(n, _, _)| n == name).ok_or_else(|| err("unknown mark"))?;
                            let (_, mx1, _, my1) = extent(mark);
                            let (mark_w, mark_h) = (mx1 + 1, my1 + 1);
                            let y = match place {
                                Place::Above => by0 - 1 - mark_h,
                                Place::Touch => by0 - mark_h,
                                Place::Below | Place::Ogonek => by1 + 1,
                            };
                            let x = match (at, place) {
                                (Some(at), _) => at,
                                (None, Place::Ogonek) => base_w - mark_w,
                                (None, _) => (base_w - mark_w + 1).div_euclid(2),
                            };
                            placed.push((x, y, mark));
                        }
                        // A mark wider than its base moves the base right instead
                        // of starting left of the pen.
                        let shift = -placed.iter().map(|(x, _, _)| *x).min().unwrap_or(0).min(0);
                        let mut out: Pixels = px.iter().map(|&(x, y)| (x + shift, y)).collect();
                        for (x, y, mark) in placed {
                            out.extend(mark.iter().map(|&(mx, my)| (mx + x + shift, my + y)));
                        }
                        glyphs.push((key(name)?, out));
                    }
                    _ => return Err(err("unknown line")),
                }
            }
        }
        let mut font = Font { glyphs: Vec::new(), direct: vec![u16::MAX; DIRECT as usize], missing: 0 };
        for blank in [' ', '\u{a0}'] {
            font.direct[blank as usize] = font.glyphs.len() as u16;
            font.glyphs.push(Glyph { blank_advance: SPACE_ADVANCE as u8, ..Glyph::default() });
        }
        for (name, px) in &glyphs {
            let mut g = Glyph::default();
            for &(x, y) in px {
                let row = y - TOP;
                if !(0..ROWS as i32).contains(&row) || !(0..16).contains(&x) {
                    return Err(format!("glyph {name:?} draws outside its cell at ({x}, {y})"));
                }
                g.rows[row as usize] |= 1 << x;
                g.width = g.width.max(x as u8 + 1);
            }
            let index = font.glyphs.len() as u16;
            let mut chars = name.chars();
            match (chars.next(), chars.next()) {
                (Some(c), None) if (c as u32) < DIRECT => {
                    if font.direct[c as usize] != u16::MAX {
                        return Err(format!("glyph {name:?} is drawn twice"));
                    }
                    font.direct[c as usize] = index;
                }
                (Some(c), None) => return Err(format!("glyph {c:?} is past Latin Extended-A")),
                _ if name == "missing" => font.missing = index,
                _ => {}
            }
            font.glyphs.push(g);
        }
        if font.missing == 0 {
            return Err("no missing glyph".into());
        }
        Ok(font)
    }
}

/// Advances of printable ASCII (`' '` to `'~'`) in the font used before
/// v0.18 (`font8x8`'s legacy set, measured from each glyph's ink plus a
/// 1-pixel gap). Signs written then were cut to 90 pixels by these
/// widths, so the sign renderer keeps showing whatever they allowed.
/// Extracted from `font8x8` 0.3.1's `legacy::BASIC_LEGACY` (the crate
/// checksum in `Cargo.lock` before v0.18) by the rule `render::ui` used.
#[rustfmt::skip]
const LEGACY_ASCII: [u8; 95] = [
    4, 5, 6, 8, 7, 8, 8, 4, 5, 5, 9, 7, 4, 7, 3, 8, 8, 7, 7, 7, 8, 7, 7, 7, 7, 7, 3, 4, 6, 7, 6, 7,
    8, 7, 8, 8, 8, 8, 8, 8, 7, 5, 8, 8, 8, 8, 8, 8, 8, 7, 8, 7, 7, 7, 7, 8, 8, 7, 8, 5, 8, 5, 8, 9,
    4, 8, 8, 7, 8, 7, 7, 8, 8, 5, 7, 8, 5, 8, 7, 7, 8, 8, 8, 7, 6, 8, 7, 8, 8, 7, 7, 7, 3, 7, 8,
];

/// Whether `line` fitted `max` pixels under the pre-v0.18 font, by its
/// measurement then (the advances' sum less the last gap). Lines from then
/// were ASCII; any other character never fitted.
pub fn legacy_prefix(line: &str, max: i32) -> &str {
    let mut width = -1;
    for (i, c) in line.char_indices() {
        let Some(&a) = (c as usize).checked_sub(0x20).and_then(|k| LEGACY_ASCII.get(k)) else { return &line[..i] };
        width += a as i32;
        if width > max {
            return &line[..i];
        }
    }
    line
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_font_parses_and_covers_latin() {
        let f = font();
        for c in (0x20..0x7F).chain(0xA0..0x180) {
            let c = char::from_u32(c).unwrap();
            assert!(f.index(c).is_some(), "{c:?} ({:04X}) has no glyph", c as u32);
        }
        assert!(f.index('\u{7F}').is_none() && f.index('\u{3B1}').is_none());
        assert_eq!(f.glyph('α'), &f.glyphs[f.missing as usize], "others draw the box");
        let (w, h) = f.atlas_size();
        assert_eq!(f.atlas().len(), (w * h) as usize);
    }

    #[test]
    fn metrics_follow_the_guide() {
        assert_eq!(advance(' '), 4);
        assert_eq!(advance('\u{a0}'), 4);
        assert_eq!(advance('A'), 6, "ink plus one");
        assert_eq!(width("Hi"), 6 + 2);
        assert_eq!(ink_width("Hi"), 7);
        assert_eq!(ink_width(""), 0);
        // Capitals stand on rows 0..=6, x-height letters on 2..=6, descenders reach 7.
        let rows = |c: char| {
            let r: Vec<i32> = glyph(c).rows().map(|(y, _)| y).collect();
            (r[0], *r.last().unwrap())
        };
        assert_eq!(rows('H'), (0, 6));
        assert_eq!(rows('x'), (2, 6));
        for c in ['g', 'j', 'p', 'q', 'y'] {
            assert_eq!(rows(c).1, 7, "{c}");
        }
        // Accents sit over the letter, inside the 3 rows above the line.
        assert_eq!(rows('É').0, -3);
        assert_eq!(rows('é').0, -1);
        assert_eq!(rows('Ç').1, 8);
        for g in font().glyphs() {
            assert!(g.width() <= 9, "no glyph is wider than Java's widest (Æ, Œ)");
        }
    }

    #[test]
    fn runs_cover_the_ink() {
        for c in ['A', 'é', '@', 'Œ', 'ŉ'] {
            let g = glyph(c);
            let count: i32 = g.runs().map(|(_, _, n)| n).sum();
            let ink = (TOP..TOP + ROWS as i32).flat_map(|y| (0..16).map(move |x| (x, y))).filter(|&(x, y)| g.ink(x, y));
            assert_eq!(count, ink.count() as i32, "{c}");
        }
    }

    #[test]
    fn typed_text_takes_what_the_font_draws() {
        for c in ['a', 'É', 'ß', 'ł', 'Ž', ' ', '\u{a0}', '~'] {
            assert!(allowed(c), "{c:?}");
        }
        for c in ['§', '\n', '\u{7f}', '\u{85}', 'α', '中', '😀'] {
            assert!(!allowed(c), "{c:?}");
        }
    }

    #[test]
    fn prefixes_are_cut_by_width_at_character_boundaries() {
        assert_eq!(prefix_by_width("Žluťoučký", 12), "Žl");
        assert_eq!(prefix_by_width("Žluťoučký", 1000), "Žluťoučký");
        assert_eq!(prefix_by_width("", 0), "");
        // 15 'W's are 90 pixels, Java's sign line.
        assert_eq!(prefix_by_width(&"W".repeat(20), 90).len(), 15);
    }

    #[test]
    fn the_first_line_breaks_like_javas_splitter() {
        assert_eq!(first_line("Hello world", 1000), "Hello world");
        assert_eq!(first_line("Hello world", 40), "Hello", "at the space");
        assert_eq!(first_line("Helloworld", 20), "Hell", "by character without a space");
        assert_eq!(first_line("W", 0), "W", "never empty");
        assert_eq!(first_line("ab  cd", 15), "ab", "a space that overflows is the break");
        assert_eq!(first_line("ab  cd", 16), "ab ", "the last space reached");
    }

    #[test]
    fn the_old_fonts_line_rule_is_kept_for_old_signs() {
        // 'W' was 8 wide then (sum less the last gap: 11 fit in 90), 'i' 5.
        assert_eq!(legacy_prefix(&"W".repeat(20), 90).len(), 11);
        assert_eq!(legacy_prefix(&"i".repeat(30), 90).len(), 18);
        assert_eq!(legacy_prefix("Hello", 90), "Hello");
        assert_eq!(legacy_prefix("Héllo", 90), "H", "no old line held other characters");
    }

    #[test]
    fn parse_errors_name_the_line() {
        assert!(Font::parse("glyph A 0\n#x#\n").err().unwrap().contains("line 1"));
        assert!(Font::parse("glyph A 0\n#\n\nglyph A 0\n#\n").is_err(), "drawn twice");
        assert!(Font::parse("glyph A -4\n#\n").is_err(), "outside the cell");
        assert!(Font::parse("compose É E acute\n").is_err(), "unknown base");
    }
}
